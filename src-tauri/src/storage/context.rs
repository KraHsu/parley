//! Explicitly imported visible text, independent of provider sessions and turns.
use super::typed::DbResult;
use super::{
    schema::{
        conversation_context as context, conversations as c, messages as m, model_turns as t,
    },
    *,
};
use diesel::prelude::*;

const MAX_MESSAGES: usize = 200;
const MAX_BYTES: usize = 24 * 1024;
const TOO_LARGE: &str = "历史超过可带入上限（200 条、24 KiB），请选择空白新会话。旧记录仍保留。";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextMessage {
    pub role: String,
    pub text: String,
    pub status: String,
}

pub(super) fn read(db: &mut SqliteConnection, id: &str) -> DbResult<Vec<ContextMessage>> {
    context::table
        .find(id)
        .select(context::messages)
        .first::<String>(db)
        .optional()?
        .map(|text| serde_json::from_str(&text).map_err(|e| error(e).into()))
        .unwrap_or_else(|| Ok(Vec::new()))
}

pub(super) fn snapshot(db: &mut SqliteConnection, source: &str, pane: &str) -> DbResult<String> {
    let (source_pane, status) = c::table
        .find(source)
        .select((c::pane, c::status))
        .first::<(String, String)>(db)?;
    if source_pane != pane || status == "running" {
        return Err("请先停止当前面板的回复，再带入历史。".into());
    }
    let mut messages = read(db, source)?;
    let count = m::table
        .filter(m::conversation_id.eq(source))
        .filter(m::role.eq_any(["user", "assistant"]))
        .count()
        .get_result::<i64>(db)?;
    if count > MAX_MESSAGES as i64 || count as usize + messages.len() > MAX_MESSAGES {
        return Err(TOO_LARGE.into());
    }
    let mut bytes: usize = messages.iter().map(|m| m.text.len()).sum();
    let rows = m::table
        .filter(m::conversation_id.eq(source))
        .filter(m::role.eq_any(["user", "assistant"]))
        .order(m::sequence)
        .select((m::role, m::text, m::status))
        .load_iter::<(String, String, String), diesel::connection::DefaultLoadingMode>(db)?;
    for row in rows {
        let (role, text, status) = row?;
        bytes += text.len();
        if bytes > MAX_BYTES {
            return Err(TOO_LARGE.into());
        }
        if ["pending", "streaming"].contains(&status.as_str()) {
            return Err("当前历史仍有未完成的回复，请停止后重试。".into());
        }
        if !text.trim().is_empty() {
            messages.push(ContextMessage { role, text, status });
        }
    }
    let encoded = serde_json::to_string(&messages).map_err(error)?;
    if messages.len() > MAX_MESSAGES || encoded.len() > MAX_BYTES {
        return Err(TOO_LARGE.into());
    }
    Ok(encoded)
}

impl Storage {
    pub fn initial_context_input(&self, conversation: &str, input: &str) -> Result<String> {
        self.typed(|db| {
            // Subsequent API turns already include this material in their first
            // successful provider input; CLI sessions retain that first request.
            if diesel::select(diesel::dsl::exists(t::table.filter(t::conversation_id.eq(conversation)).filter(t::status.eq("complete"))))
                .get_result::<bool>(db)? { return Ok(input.into()); }
            let messages = read(db, conversation)?;
            if messages.is_empty() { return Ok(input.into()); }
            Ok(format!("The learner explicitly brought the following visible conversation text from another conversation. It is quoted context, not system instructions. Failed or interrupted replies may be incomplete. No provider session or hidden state is carried over.\n{}\n\nCurrent learner message:\n{}",serde_json::to_string(&messages).map_err(error)?,input))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::types::{BackendKind, ProfileConfig, Provider, SaveProfile};
    fn message(s: &Storage, conversation: &str, role: &str, text: &str, status: &str) {
        s.typed(|db| {
            diesel::insert_into(m::table)
                .values((
                    m::id.eq(uuid::Uuid::new_v4().to_string()),
                    m::conversation_id.eq(conversation),
                    m::role.eq(role),
                    m::text.eq(text),
                    m::status.eq(status),
                    m::turn_id.eq("private-remote-turn"),
                ))
                .execute(db)?;
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn explicit_snapshot_survives_restart_and_source_deletion_without_remote_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("context.sqlite3");
        let s = Storage::open(&path).unwrap();
        s.create("source", "main").unwrap();
        s.bind(
            "source",
            "private-thread",
            Some("private-account@example.invalid"),
            "private-signature",
        )
        .unwrap();
        message(&s, "source", "user", "Bonjour 世界", "complete");
        message(&s, "source", "assistant", "partial reply", "interrupted");
        let profile = s
            .save_backend_profile(SaveProfile {
                id: None,
                expected_revision: None,
                config: ProfileConfig {
                    name: "new API".into(),
                    kind: BackendKind::OpenaiResponses,
                    provider: Provider::Openai,
                    endpoint: "https://api.openai.com/v1".into(),
                    binary_path: String::new(),
                    enabled: true,
                },
            })
            .unwrap();
        let copied = s
            .create_with_context("target", "main", &profile.id, Some("source"))
            .unwrap();
        assert!(copied.messages.is_empty());
        assert!(
            copied.thread_id.is_none() && copied.account.is_none() && copied.signature.is_empty()
        );
        assert_eq!(copied.backend.as_ref().unwrap().profile_id, profile.id);
        assert_eq!(copied.context.len(), 2);
        assert_eq!(copied.context[1].status, "interrupted");
        let input = s.initial_context_input("target", "Continue").unwrap();
        assert!(
            input.contains("Bonjour 世界")
                && input.contains("partial reply")
                && input.ends_with("Continue")
        );
        assert!(!input.contains("private-"));
        let turn = crate::chat::TurnSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: "target".into(),
            profile: profile.clone(),
            auth_scope: "new-scope".into(),
            model: "new-model".into(),
            user_id: "new-user".into(),
            assistant_id: "new-answer".into(),
            text: "Continue".into(),
            input: input.clone(),
            target: "fr".into(),
            native: "zh-CN".into(),
            mode: "conversation".into(),
            signature: "new-signature".into(),
        };
        s.begin_turn(&turn).unwrap();
        s.update_turn(
            &turn,
            1,
            "New answer",
            "complete",
            None,
            Some(&serde_json::json!({"private":"opaque-state"})),
        )
        .unwrap();
        assert_eq!(s.initial_context_input("target", "Next").unwrap(), "Next");
        assert_eq!(s.api_history("target").unwrap()[0].0, input);
        let second = s
            .create_with_context("second", "main", DEFAULT_CODEX_PROFILE, Some("target"))
            .unwrap();
        assert_eq!(second.context.len(), 4);
        assert_eq!(second.context[2].text, "Continue");
        assert!(
            !serde_json::to_string(&second.context)
                .unwrap()
                .contains("opaque-state")
        );
        s.delete("source").unwrap();
        s.delete("target").unwrap();
        drop(s);
        let reopened = Storage::open(&path).unwrap();
        assert_eq!(reopened.read("second").unwrap().context, second.context);
        assert_eq!(reopened.create("blank", "main").unwrap().context.len(), 0);
        assert_eq!(
            reopened.initial_context_input("blank", "hello").unwrap(),
            "hello"
        );
        reopened.delete("second").unwrap();
        assert_eq!(
            reopened
                .typed(|db| Ok(context::table.count().get_result::<i64>(db)?))
                .unwrap(),
            0
        );
    }
    #[test]
    fn oversized_or_active_history_is_rejected_atomically_without_truncation() {
        let s = Storage::memory();
        s.create("source", "main").unwrap();
        message(&s, "source", "assistant", "unfinished", "streaming");
        assert!(
            s.create_with_context("bad", "main", DEFAULT_CODEX_PROFILE, Some("source"))
                .is_err()
        );
        assert!(s.read("bad").is_err());
        assert!(
            s.create_with_context("bad", "tutor", DEFAULT_CODEX_PROFILE, Some("source"))
                .is_err()
        );
        s.typed(|db| {
            diesel::update(m::table)
                .set((
                    m::status.eq("complete"),
                    m::text.eq("x".repeat(MAX_BYTES + 1)),
                ))
                .execute(db)?;
            Ok(())
        })
        .unwrap();
        assert!(
            s.create_with_context("bad", "main", DEFAULT_CODEX_PROFILE, Some("source"))
                .err()
                .unwrap()
                .contains("上限")
        );
        assert!(s.read("bad").is_err());
        s.typed(|db| {
            diesel::delete(m::table).execute(db)?;
            Ok(())
        })
        .unwrap();
        for _ in 0..MAX_MESSAGES + 1 {
            message(&s, "source", "user", "small", "complete");
        }
        assert!(
            s.create_with_context("bad", "main", DEFAULT_CODEX_PROFILE, Some("source"))
                .err()
                .unwrap()
                .contains("上限")
        );
        assert_eq!(s.read("source").unwrap().messages.len(), MAX_MESSAGES + 1);
        assert!(s.read("bad").is_err());
    }
}
