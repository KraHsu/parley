use super::api::ApiTurn;
use super::*;
use crate::backends::{
    credentials::CredentialReference,
    types::{BackendKind, ProfileConfig, Provider, SaveProfile},
};
use serde_json::json;

pub(crate) fn turn(storage: &Storage, conversation: &str, pane: &str) -> ApiTurn {
    let profile = storage
        .save_backend_profile(SaveProfile {
            id: None,
            expected_revision: None,
            config: ProfileConfig {
                name: "API fixture".into(),
                kind: BackendKind::OpenaiResponses,
                provider: Provider::Openai,
                endpoint: "https://api.example.invalid/v1".into(),
                binary_path: String::new(),
                enabled: true,
            },
        })
        .unwrap();
    storage
        .create_for_backend(conversation, pane, &profile.id)
        .unwrap();
    ApiTurn {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: conversation.into(),
        profile,
        auth_scope: "account-one".into(),
        model: "fixture-model".into(),
        user_id: "user-one".into(),
        assistant_id: "assistant-one".into(),
        text: "What does this mean?".into(),
        input: "Quoted terminal context\nWhat does this mean?".into(),
        target: "en".into(),
        native: "zh-CN".into(),
        mode: "conversation".into(),
        signature: "immutable-config".into(),
    }
}
fn next(turn: &ApiTurn) -> ApiTurn {
    ApiTurn {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: uuid::Uuid::new_v4().to_string(),
        assistant_id: uuid::Uuid::new_v4().to_string(),
        ..turn.clone()
    }
}

#[test]
fn api_updates_are_ordered_final_once_and_preserve_provider_context() {
    let s = Storage::memory();
    let turn = turn(&s, "main", "main");
    s.begin_api_turn(&turn).unwrap();
    assert!(
        s.update_api_turn(&turn, 2, "你好", "streaming", None, None)
            .unwrap()
    );
    assert!(
        !s.update_api_turn(&turn, 1, "stale", "streaming", None, None)
            .unwrap()
    );
    let output = json!([{"type":"reasoning","encrypted_content":"opaque fixture"},{"type":"message","content":[{"type":"output_text","text":"你好！"}]}]);
    assert!(
        s.update_api_turn(
            &turn,
            3,
            "你好！",
            "complete",
            Some(&json!({"total_tokens":12})),
            Some(&output)
        )
        .unwrap()
    );
    assert!(
        !s.update_api_turn(&turn, 4, "late", "interrupted", None, None)
            .unwrap()
    );
    let saved = s.read("main").unwrap();
    assert_eq!(saved.status, "idle");
    assert_eq!(saved.messages[1].text, "你好！");
    assert_eq!(
        s.api_history("main").unwrap(),
        vec![(turn.input.clone(), "你好！".into(), Some(output))]
    );
    let follow = next(&turn);
    s.begin_api_turn(&follow).unwrap();
    s.update_api_turn(&follow, 1, "partial", "failed", None, None)
        .unwrap();
    assert_eq!(s.api_history("main").unwrap().len(), 1);
    assert_eq!(s.read("main").unwrap().messages[3].text, "partial");
}
#[test]
fn duplicate_turn_rolls_back_messages_and_conversation_state() {
    let s = Storage::memory();
    let turn = turn(&s, "main", "main");
    s.begin_api_turn(&turn).unwrap();
    s.update_api_turn(&turn, 1, "answer", "complete", None, None)
        .unwrap();
    // Failure after inserting the user row must roll the entire transaction back.
    let duplicate = ApiTurn {
        user_id: "new-user".into(),
        ..next(&turn)
    };
    let duplicate = ApiTurn {
        assistant_id: turn.assistant_id.clone(),
        ..duplicate
    };
    assert!(s.begin_api_turn(&duplicate).is_err());
    let saved = s.read("main").unwrap();
    assert_eq!(saved.status, "idle");
    assert_eq!(saved.messages.len(), 2);
    assert!(s.begin_api_turn(&turn).is_err());
}
#[test]
fn account_binding_and_pane_exclusivity_prevent_cross_backend_history() {
    let s = Storage::memory();
    let a = turn(&s, "api-main", "main");
    let b = turn(&s, "api-tutor", "tutor");
    s.create("codex", "main").unwrap();
    s.begin_api_turn(&a).unwrap();
    s.begin_api_turn(&b).unwrap();
    assert!(
        s.begin(
            "codex",
            "u",
            "hello",
            ConversationConfig {
                model: "test",
                target: "en",
                native: "zh-CN",
                mode: "conversation"
            }
        )
        .is_err()
    );
    s.update_api_turn(&a, 1, "done", "complete", None, None)
        .unwrap();
    let changed = ApiTurn {
        auth_scope: "different-account".into(),
        ..next(&a)
    };
    assert!(
        s.begin_api_turn(&changed)
            .unwrap_err()
            .contains("凭据已改变")
    );
    let wrong = ApiTurn {
        profile: b.profile.clone(),
        ..next(&a)
    };
    assert!(s.begin_api_turn(&wrong).is_err());
    assert_eq!(s.read("api-main").unwrap().messages.len(), 2);
    assert_eq!(s.read("api-tutor").unwrap().status, "running");
    s.begin(
        "codex",
        "u",
        "hello",
        ConversationConfig {
            model: "test",
            target: "en",
            native: "zh-CN",
            mode: "conversation",
        },
    )
    .unwrap();
    assert!(s.begin_api_turn(&next(&a)).is_err());
}
#[test]
fn credential_removal_checks_revision_before_external_work_and_retains_failed_removal() {
    let s = Storage::memory();
    let a = turn(&s, "api", "main");
    let reference = CredentialReference {
        slot_id: "not-a-real-system-key".into(),
        scope: "scope".into(),
        endpoint: a.profile.config.endpoint.clone(),
    };
    s.replace_credential(&a.profile, Some(&reference)).unwrap();
    assert!(
        s.remove_credential_with(&a.profile, |_| Err("locked fixture".into()))
            .is_err()
    );
    assert_eq!(
        s.credential_reference(&a.profile.id)
            .unwrap()
            .unwrap()
            .slot_id,
        reference.slot_id
    );
    let mut config = a.profile.config.clone();
    config.name = "renamed".into();
    let changed = s
        .save_backend_profile(SaveProfile {
            id: Some(a.profile.id.clone()),
            expected_revision: Some(1),
            config,
        })
        .unwrap();
    assert!(
        s.remove_credential_with(&a.profile, |_| panic!("stale removal reached keystore"))
            .is_err()
    );
    s.remove_credential_with(&changed, |_| Ok(())).unwrap();
    assert!(s.credential_reference(&a.profile.id).unwrap().is_none());
}
#[test]
fn api_restart_keeps_partial_text_and_excludes_it_from_continuation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("api.sqlite3");
    {
        let s = Storage::open(&path).unwrap();
        let a = turn(&s, "api", "main");
        s.begin_api_turn(&a).unwrap();
        s.update_api_turn(&a, 1, "unfinished 🌍", "streaming", None, None)
            .unwrap();
    }
    let s = Storage::open(&path).unwrap();
    let saved = s.read("api").unwrap();
    assert_eq!(saved.status, "interrupted");
    assert_eq!(saved.messages[1].text, "unfinished 🌍");
    assert_eq!(saved.messages[1].status, "interrupted");
    assert!(s.api_history("api").unwrap().is_empty());
}
