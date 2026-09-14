use super::schema::{
    backend_profile_versions as v, conversation_backends as b, conversations as c, messages as m,
    model_turns as t, preferences as pref,
};
use super::typed::DbResult;
use super::*;
use crate::backends::types::ProfileConfig;
use diesel::prelude::*;
#[cfg(test)]
use diesel::upsert::excluded;

#[derive(Queryable, Selectable)]
#[diesel(table_name = c)]
struct ConversationRow {
    id: String,
    pane: String,
    title: String,
    model: String,
    target_language: String,
    native_language: String,
    mode: String,
    draft: String,
    thread_id: Option<String>,
    account: Option<String>,
    signature: String,
    status: String,
    updated_at: i64,
}
impl ConversationRow {
    fn into_conversation(self, binding: Option<(String, i64, String)>) -> DbResult<Conversation> {
        let backend = binding
            .map(|(profile_id, profile_revision, config)| {
                let config: ProfileConfig = serde_json::from_str(&config).map_err(error)?;
                Ok::<_, super::typed::DbError>(ConversationBackend {
                    profile_id,
                    profile_revision,
                    kind: config.kind,
                })
            })
            .transpose()?;
        Ok(Conversation {
            id: self.id,
            pane: self.pane,
            title: self.title,
            model: self.model,
            target_language: self.target_language,
            native_language: self.native_language,
            mode: self.mode,
            draft: self.draft,
            thread_id: self.thread_id,
            account: self.account,
            signature: self.signature,
            status: self.status,
            updated_at: self.updated_at,
            messages: Vec::new(),
            context: Vec::new(),
            backend,
        })
    }
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = m)]
struct MessageRow {
    id: String,
    role: String,
    text: String,
    status: String,
}
impl TryFrom<(MessageRow, Option<String>)> for Message {
    type Error = super::typed::DbError;
    fn try_from((row, usage): (MessageRow, Option<String>)) -> DbResult<Self> {
        Ok(Self {
            usage: usage
                .map(|s| serde_json::from_str(&s).map_err(error))
                .transpose()?,
            id: row.id,
            role: row.role,
            text: row.text,
            status: row.status,
        })
    }
}

impl Storage {
    pub fn interrupt_all(&self) -> Result<()> {
        self.typed_transaction(|db| {
            diesel::update(c::table.filter(c::status.eq("running")))
                .set(c::status.eq("interrupted"))
                .execute(db)?;
            diesel::update(m::table.filter(m::status.eq_any(["streaming", "pending"])))
                .set(m::status.eq("interrupted"))
                .execute(db)?;
            diesel::update(t::table.filter(t::status.eq("streaming")))
                .set(t::status.eq("interrupted"))
                .execute(db)?;
            Ok(())
        })
    }
    pub fn preferences(&self) -> Result<Preferences> {
        self.typed(|db| {
            let saved = pref::table
                .find(1_i64)
                .select(pref::value)
                .first::<String>(db)
                .optional()?;
            saved
                .map(|s| serde_json::from_str(&s).map_err(|e| error(e).into()))
                .unwrap_or_else(|| Ok(Preferences::default()))
        })
    }
    pub fn save(&self, p: &Preferences, drafts: &[Draft]) -> Result<()> {
        if p.target_language.is_empty()
            || p.native_language.is_empty()
            || p.target_language.len() > 100
            || p.native_language.len() > 100
            || p.main_model.len() > 200
            || p.tutor_model.len() > 200
        {
            return Err("语言或模型设置无效。".into());
        }
        if !["express", "explain", "translate"].contains(&p.tutor_mode.as_str())
            || !["conversation", "vocabulary"].contains(&p.active_view.as_str())
            || !["main", "tutor"].contains(&p.mobile_pane.as_str())
        {
            return Err("工作区设置无效。".into());
        }
        self.typed_transaction(|db| {
            for (id, pane) in [(&p.main_id, "main"), (&p.tutor_id, "tutor")] {
                if let Some(id) = id
                    && !diesel::select(diesel::dsl::exists(
                        c::table.find(id).filter(c::pane.eq(pane)),
                    ))
                    .get_result::<bool>(db)?
                {
                    return Err("所选会话不存在。".into());
                }
            }
            backends::sync_codex_path(db, &p.codex_path)?;
            let value = serde_json::to_string(p).map_err(error)?;
            diesel::insert_into(pref::table)
                .values((pref::id.eq(1_i64), pref::value.eq(&value)))
                .on_conflict(pref::id)
                .do_update()
                .set(pref::value.eq(&value))
                .execute(db)?;
            for draft in drafts {
                if draft.text.len() > 128000 {
                    return Err("草稿不能超过 128 KB。".into());
                }
                if diesel::update(c::table.find(&draft.id))
                    .set(c::draft.eq(&draft.text))
                    .execute(db)?
                    != 1
                {
                    return Err("草稿所属会话不存在。".into());
                }
            }
            Ok(())
        })
    }
    #[cfg(test)]
    pub fn create(&self, id: &str, pane: &str) -> Result<Conversation> {
        self.create_for_backend(id, pane, DEFAULT_CODEX_PROFILE)
    }
    pub fn list(&self) -> Result<Vec<Conversation>> {
        self.typed(|db| {
            c::table
                .left_join(b::table.on(b::conversation_id.eq(c::id)))
                .left_join(
                    v::table.on(v::profile_id
                        .eq(b::profile_id)
                        .and(v::revision.eq(b::profile_revision))),
                )
                .select((
                    ConversationRow::as_select(),
                    (b::profile_id, b::profile_revision, v::config).nullable(),
                ))
                .order((c::updated_at.desc(), c::rowid.desc()))
                .load::<(ConversationRow, Option<(String, i64, String)>)>(db)?
                .into_iter()
                .map(|(row, binding)| row.into_conversation(binding))
                .collect()
        })
    }
    fn read_metadata(&self, id: &str) -> Result<Conversation> {
        self.typed(|db| {
            let (row, binding) = c::table
                .left_join(b::table.on(b::conversation_id.eq(c::id)))
                .left_join(
                    v::table.on(v::profile_id
                        .eq(b::profile_id)
                        .and(v::revision.eq(b::profile_revision))),
                )
                .filter(c::id.eq(id))
                .select((
                    ConversationRow::as_select(),
                    (b::profile_id, b::profile_revision, v::config).nullable(),
                ))
                .first::<(ConversationRow, Option<(String, i64, String)>)>(db)?;
            row.into_conversation(binding)
        })
    }
    pub fn read(&self, id: &str) -> Result<Conversation> {
        let mut conversation = self.read_metadata(id)?;
        conversation.context = self.typed(|db| super::context::read(db, id))?;
        conversation.messages = self.typed(|db| {
            m::table
                .left_join(
                    t::table.on(t::conversation_id
                        .eq(m::conversation_id)
                        .and(t::assistant_message_id.eq(m::id))
                        .and(m::role.eq("assistant"))),
                )
                .filter(m::conversation_id.eq(id))
                .order(m::sequence)
                .select((MessageRow::as_select(), t::usage.nullable()))
                .load::<(MessageRow, Option<String>)>(db)?
                .into_iter()
                .map(TryInto::try_into)
                .collect()
        })?;
        Ok(conversation)
    }
    pub fn bind(
        &self,
        id: &str,
        thread: &str,
        account: Option<&str>,
        signature: &str,
    ) -> Result<()> {
        self.typed(|db| {
            diesel::update(c::table.find(id))
                .set((
                    c::thread_id.eq(thread),
                    c::account.eq(account),
                    c::signature.eq(signature),
                ))
                .execute(db)?;
            Ok(())
        })
    }
    #[cfg(test)]
    pub fn begin(
        &self,
        id: &str,
        message_id: &str,
        text: &str,
        config: ConversationConfig<'_>,
    ) -> Result<()> {
        self.typed_transaction(|db| {
            let (pane, title, status) = c::table
                .find(id)
                .select((c::pane, c::title, c::status))
                .first::<(String, String, String)>(db)?;
            if status == "running"
                || diesel::select(diesel::dsl::exists(
                    c::table
                        .filter(c::pane.eq(pane))
                        .filter(c::status.eq("running")),
                ))
                .get_result::<bool>(db)?
            {
                return Err("该面板仍在生成，请先停止回复。".into());
            }
            let title = if title == "新的对话" {
                text.chars().take(36).collect()
            } else {
                title
            };
            diesel::update(c::table.find(id))
                .set((
                    c::status.eq("running"),
                    c::title.eq(title),
                    c::model.eq(config.model),
                    c::target_language.eq(config.target),
                    c::native_language.eq(config.native),
                    c::mode.eq(config.mode),
                    c::draft.eq(""),
                    c::updated_at.eq(now()),
                ))
                .execute(db)?;
            diesel::insert_into(m::table)
                .values((
                    m::id.eq(message_id),
                    m::conversation_id.eq(id),
                    m::role.eq("user"),
                    m::text.eq(text),
                    m::status.eq("pending"),
                ))
                .execute(db)?;
            Ok(())
        })
    }
    #[cfg(test)]
    pub fn event(&self, id: &str, method: &str, p: &Value) -> Result<()> {
        self.typed_transaction(|db| {
            match method {
                "item/agentMessage/delta" => {
                    if let (Some(item), Some(delta)) = (p["itemId"].as_str(), p["delta"].as_str()) {
                        // Read and append under the same transaction to preserve idempotent final items.
                        let old = m::table
                            .filter(m::conversation_id.eq(id))
                            .filter(m::id.eq(item))
                            .select((m::text, m::status))
                            .first::<(String, String)>(db)
                            .optional()?;
                        if let Some((mut text, status)) = old {
                            if status == "streaming" {
                                text.push_str(delta);
                                diesel::update(
                                    m::table
                                        .filter(m::conversation_id.eq(id))
                                        .filter(m::id.eq(item)),
                                )
                                .set(m::text.eq(text))
                                .execute(db)?;
                            }
                        } else {
                            diesel::insert_into(m::table)
                                .values((
                                    m::id.eq(item),
                                    m::conversation_id.eq(id),
                                    m::role.eq("assistant"),
                                    m::text.eq(delta),
                                    m::status.eq("streaming"),
                                    m::turn_id.eq(p["turnId"].as_str()),
                                ))
                                .execute(db)?;
                        }
                    }
                }
                "item/completed" if p["item"]["type"] == "agentMessage" => {
                    if let (Some(item), Some(text)) =
                        (p["item"]["id"].as_str(), p["item"]["text"].as_str())
                    {
                        diesel::insert_into(m::table)
                            .values((
                                m::id.eq(item),
                                m::conversation_id.eq(id),
                                m::role.eq("assistant"),
                                m::text.eq(text),
                                m::status.eq("complete"),
                                m::turn_id.eq(p["turnId"].as_str()),
                            ))
                            .on_conflict((m::conversation_id, m::id))
                            .do_update()
                            .set((m::text.eq(excluded(m::text)), m::status.eq("complete")))
                            .execute(db)?;
                    }
                }
                "turn/started" => {
                    diesel::update(
                        m::table
                            .filter(m::conversation_id.eq(id))
                            .filter(m::role.eq("user"))
                            .filter(m::status.eq("pending")),
                    )
                    .set(m::status.eq("complete"))
                    .execute(db)?;
                }
                "turn/completed" => {
                    let status = match p["turn"]["status"].as_str() {
                        Some("completed") => "idle",
                        Some("interrupted") => "interrupted",
                        _ => "failed",
                    };
                    diesel::update(c::table.find(id))
                        .set((c::status.eq(status), c::updated_at.eq(now())))
                        .execute(db)?;
                    diesel::update(
                        m::table
                            .filter(m::conversation_id.eq(id))
                            .filter(m::status.eq_any(["streaming", "pending"])),
                    )
                    .set(m::status.eq(if status == "idle" { "complete" } else { status }))
                    .execute(db)?;
                }
                _ => {}
            }
            Ok(())
        })
    }
    pub fn delete(&self, id: &str) -> Result<()> {
        self.typed_transaction(|db| {
            if c::table.find(id).select(c::status).first::<String>(db)? == "running" {
                return Err("请先停止当前回复。".into());
            }
            diesel::delete(c::table.find(id)).execute(db)?;
            if let Some(saved) = pref::table
                .find(1_i64)
                .select(pref::value)
                .first::<String>(db)
                .optional()?
            {
                let mut p: Preferences = serde_json::from_str(&saved).map_err(error)?;
                if p.main_id.as_deref() == Some(id) {
                    p.main_id = None;
                }
                if p.tutor_id.as_deref() == Some(id) {
                    p.tutor_id = None;
                }
                diesel::update(pref::table.find(1_i64))
                    .set(pref::value.eq(serde_json::to_string(&p).map_err(error)?))
                    .execute(db)?;
            }
            Ok(())
        })
    }
}
