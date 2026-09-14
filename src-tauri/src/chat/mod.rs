//! Stable local turn records and durable publications shared by all adapters.
use crate::{backends::types::BackendProfile, storage::Storage};
use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;

#[derive(Clone)]
pub struct TurnSnapshot {
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnEvent {
    pub profile_id: String,
    pub profile_revision: i64,
    pub conversation_id: String,
    pub pane: String,
    pub turn_id: String,
    pub request_id: String,
    pub message_id: String,
    pub sequence: i64,
    pub status: String,
    pub text: String,
    pub usage: Option<Value>,
    pub error: Option<String>,
    pub notice: Option<String>,
}

pub(crate) struct Publisher {
    pub storage: Storage,
    pub turn: TurnSnapshot,
    pub pane: String,
    pub events: Channel<TurnEvent>,
    pub sequence: i64,
    pub notice: Option<String>,
}
impl Publisher {
    fn prepare(
        &mut self,
        text: &str,
        status: &str,
        usage: Option<&Value>,
        continuation: Option<&Value>,
        error: Option<String>,
    ) -> impl FnOnce() -> Result<(), String> + Send + use<> {
        self.sequence += 1;
        let event = TurnEvent {
            profile_id: self.turn.profile.id.clone(),
            profile_revision: self.turn.profile.revision,
            conversation_id: self.turn.conversation_id.clone(),
            pane: self.pane.clone(),
            turn_id: self.turn.id.clone(),
            request_id: self.turn.user_id.clone(),
            message_id: self.turn.assistant_id.clone(),
            sequence: self.sequence,
            status: status.into(),
            text: text.into(),
            usage: usage.cloned(),
            error,
            notice: self.notice.clone(),
        };
        let storage = self.storage.clone();
        let turn = self.turn.clone();
        let continuation = continuation.cloned();
        let events = self.events.clone();
        move || {
            if storage.update_turn(
                &turn,
                event.sequence,
                &event.text,
                &event.status,
                event.usage.as_ref(),
                continuation.as_ref(),
            )? {
                let _ = events.send(event);
            }
            Ok(())
        }
    }
    pub fn publish(
        &mut self,
        text: &str,
        status: &str,
        usage: Option<&Value>,
        continuation: Option<&Value>,
        error: Option<String>,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send + use<> {
        let operation = self.prepare(text, status, usage, continuation, error);
        async move {
            tauri::async_runtime::spawn_blocking(operation)
                .await
                .map_err(|_| "保存回复任务失败。".to_owned())?
        }
    }
    pub fn publish_blocking(
        &mut self,
        text: &str,
        status: &str,
        usage: Option<&Value>,
        continuation: Option<&Value>,
        error: Option<String>,
    ) -> Result<(), String> {
        self.prepare(text, status, usage, continuation, error)()
    }
}
