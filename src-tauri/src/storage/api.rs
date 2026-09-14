use super::*;
use crate::backends::{credentials::CredentialReference, types::BackendProfile};

#[derive(Clone)]
pub struct ApiTurn {
    pub id: String,
    pub conversation_id: String,
    pub profile: BackendProfile,
    pub auth_scope: String,
    pub model: String,
    pub user_id: String,
    pub assistant_id: String,
    pub text: String,
    pub input: String,
    pub target: String,
    pub native: String,
    pub mode: String,
    pub signature: String,
}

#[derive(diesel::Queryable, diesel::Selectable)]
#[diesel(table_name = super::schema::backend_credentials)]
struct CredentialRow {
    slot_id: String,
    scope: String,
    endpoint: String,
}
impl From<CredentialRow> for CredentialReference {
    fn from(row: CredentialRow) -> Self {
        Self {
            slot_id: row.slot_id,
            scope: row.scope,
            endpoint: row.endpoint,
        }
    }
}
impl Storage {
    pub fn check_profile_revision(&self, id: &str, revision: i64) -> Result<()> {
        let profile = self.backend_profile(id)?;
        if profile.revision != revision || !profile.config.enabled {
            return Err("服务配置已改变，请重新读取后重试。".into());
        }
        Ok(())
    }
    pub fn credential_reference(&self, profile: &str) -> Result<Option<CredentialReference>> {
        use super::schema::backend_credentials as k;
        use diesel::prelude::*;
        self.typed(|db| {
            Ok(k::table
                .find(profile)
                .select(CredentialRow::as_select())
                .first::<CredentialRow>(db)
                .optional()?
                .map(Into::into))
        })
    }
    pub fn replace_credential(
        &self,
        profile: &BackendProfile,
        value: Option<&CredentialReference>,
    ) -> Result<Option<CredentialReference>> {
        use super::schema::backend_credentials as k;
        use diesel::prelude::*;
        self.typed_transaction(|db| {
            let current = super::backends::typed_profile(db, &profile.id)?;
            if current.revision != profile.revision {
                return Err("服务配置已改变，请重新读取后重试。".into());
            }
            let old = k::table
                .find(&profile.id)
                .select(CredentialRow::as_select())
                .first::<CredentialRow>(db)
                .optional()?
                .map(Into::into);
            if let Some(value) = value {
                let fields = (
                    k::slot_id.eq(&value.slot_id),
                    k::scope.eq(&value.scope),
                    k::endpoint.eq(&value.endpoint),
                );
                diesel::insert_into(k::table)
                    .values((k::profile_id.eq(&profile.id), fields))
                    .on_conflict(k::profile_id)
                    .do_update()
                    .set(fields)
                    .execute(db)?;
            } else {
                diesel::delete(k::table.find(&profile.id)).execute(db)?;
            }
            Ok(old)
        })
    }
    // Serialize profile edits with keystore removal so revision validation cannot race
    // a changed endpoint. The caller runs this operation on a blocking worker.
    pub fn remove_credential_with(
        &self,
        profile: &BackendProfile,
        remove: impl FnOnce(Option<&CredentialReference>) -> Result<()>,
    ) -> Result<()> {
        use super::schema::backend_credentials as k;
        use diesel::prelude::*;
        self.typed_transaction(|db| {
            if super::backends::typed_profile(db, &profile.id)?.revision != profile.revision {
                return Err("服务配置已改变，请重新读取后重试。".into());
            }
            let reference = k::table
                .find(&profile.id)
                .select(CredentialRow::as_select())
                .first::<CredentialRow>(db)
                .optional()?
                .map(CredentialReference::from);
            remove(reference.as_ref())?;
            diesel::delete(k::table.find(&profile.id)).execute(db)?;
            Ok(())
        })
    }
    pub fn begin_api_turn(&self, turn: &ApiTurn) -> Result<()> {
        use super::schema::{
            conversation_backends as b, conversations as c, messages as m, model_turns as t,
        };
        use diesel::prelude::*;
        self.typed_transaction(|db| {
            let current = super::backends::typed_profile(db, &turn.profile.id)?;
            if current.revision != turn.profile.revision || !current.config.enabled {
                return Err("服务配置已改变，请重新连接。".into());
            }
            let binding = b::table
                .find(&turn.conversation_id)
                .select((b::profile_id, b::profile_revision))
                .first::<(String, i64)>(db)?;
            if binding != (turn.profile.id.clone(), turn.profile.revision) {
                return Err("会话属于其他服务配置版本，请新建对话。".into());
            }
            let old_scope = t::table
                .filter(t::conversation_id.eq(&turn.conversation_id))
                .order((t::created_at.desc(), t::id.desc()))
                .select(t::auth_scope)
                .first::<String>(db)
                .optional()?;
            if old_scope.is_some_and(|s| s != turn.auth_scope) {
                return Err(
                    "该会话的 API 凭据已改变。为避免向其他账号发送历史，请新建对话。".into(),
                );
            }
            let (signature, pane, title) = c::table
                .find(&turn.conversation_id)
                .select((c::signature, c::pane, c::title))
                .first::<(String, String, String)>(db)?;
            if !signature.is_empty() && signature != turn.signature {
                return Err("模型或语言设置已改变，请新建对话。".into());
            }
            let busy = diesel::select(diesel::dsl::exists(
                c::table
                    .filter(c::status.eq("running"))
                    .filter(c::pane.eq(pane)),
            ))
            .get_result::<bool>(db)?;
            if busy {
                return Err("该面板仍在生成，请先停止回复。".into());
            }
            let title = if title == "新的对话" {
                turn.text.chars().take(36).collect()
            } else {
                title
            };
            if diesel::update(
                c::table
                    .find(&turn.conversation_id)
                    .filter(c::status.ne("running")),
            )
            .set((
                c::status.eq("running"),
                c::title.eq(title),
                c::model.eq(&turn.model),
                c::target_language.eq(&turn.target),
                c::native_language.eq(&turn.native),
                c::mode.eq(&turn.mode),
                c::signature.eq(&turn.signature),
                c::updated_at.eq(now()),
            ))
            .execute(db)?
                != 1
            {
                return Err("会话仍在生成或已删除。".into());
            }
            diesel::insert_into(m::table)
                .values((
                    m::id.eq(&turn.user_id),
                    m::conversation_id.eq(&turn.conversation_id),
                    m::role.eq("user"),
                    m::text.eq(&turn.text),
                    m::status.eq("complete"),
                    m::turn_id.eq(&turn.id),
                ))
                .execute(db)?;
            diesel::insert_into(m::table)
                .values((
                    m::id.eq(&turn.assistant_id),
                    m::conversation_id.eq(&turn.conversation_id),
                    m::role.eq("assistant"),
                    m::text.eq(""),
                    m::status.eq("streaming"),
                    m::turn_id.eq(&turn.id),
                ))
                .execute(db)?;
            diesel::insert_into(t::table)
                .values((
                    t::id.eq(&turn.id),
                    t::conversation_id.eq(&turn.conversation_id),
                    t::profile_id.eq(&turn.profile.id),
                    t::profile_revision.eq(turn.profile.revision),
                    t::auth_scope.eq(&turn.auth_scope),
                    t::model.eq(&turn.model),
                    t::user_message_id.eq(&turn.user_id),
                    t::assistant_message_id.eq(&turn.assistant_id),
                    t::provider_input.eq(&turn.input),
                    t::status.eq("streaming"),
                    t::created_at.eq(now()),
                ))
                .execute(db)?;
            Ok(())
        })
    }
    pub fn update_api_turn(
        &self,
        turn: &ApiTurn,
        sequence: i64,
        text: &str,
        status: &str,
        usage: Option<&Value>,
        output: Option<&Value>,
    ) -> Result<bool> {
        use super::schema::{conversations as c, messages as m, model_turns as t};
        use diesel::prelude::*;
        if !["streaming", "complete", "interrupted", "failed"].contains(&status) {
            return Err("无效的回复状态。".into());
        }
        self.typed_transaction(|db| {
            let changed = diesel::update(
                t::table
                    .find(&turn.id)
                    .filter(t::status.eq("streaming"))
                    .filter(t::sequence.lt(sequence)),
            )
            .set((t::sequence.eq(sequence), t::status.eq(status)))
            .execute(db)?;
            if changed == 0 {
                return Ok(false);
            }
            if let Some(usage) = usage {
                diesel::update(t::table.find(&turn.id))
                    .set(t::usage.eq(usage.to_string()))
                    .execute(db)?;
            }
            if let Some(output) = output {
                diesel::update(t::table.find(&turn.id))
                    .set(t::provider_output.eq(output.to_string()))
                    .execute(db)?;
            }
            diesel::update(
                m::table
                    .filter(m::conversation_id.eq(&turn.conversation_id))
                    .filter(m::id.eq(&turn.assistant_id)),
            )
            .set((m::text.eq(text), m::status.eq(status)))
            .execute(db)?;
            if status != "streaming" {
                diesel::update(c::table.find(&turn.conversation_id))
                    .set((
                        c::status.eq(if status == "complete" { "idle" } else { status }),
                        c::updated_at.eq(now()),
                    ))
                    .execute(db)?;
            }
            Ok(true)
        })
    }
    pub fn api_history(&self, conversation: &str) -> Result<Vec<(String, String, Option<Value>)>> {
        use super::schema::{messages as m, model_turns as t};
        use diesel::prelude::*;
        self.typed(|db| {
            let rows = t::table
                .inner_join(
                    m::table.on(m::conversation_id
                        .eq(t::conversation_id)
                        .and(m::id.eq(t::assistant_message_id))),
                )
                .filter(t::conversation_id.eq(conversation))
                .filter(t::status.eq("complete"))
                .order(m::sequence)
                .select((t::provider_input, m::text, t::provider_output))
                .load::<(String, String, Option<String>)>(db)?;
            rows.into_iter()
                .map(|(user, assistant, output)| {
                    Ok((
                        user,
                        assistant,
                        output
                            .map(|s| serde_json::from_str(&s).map_err(error))
                            .transpose()?,
                    ))
                })
                .collect()
        })
    }
}
