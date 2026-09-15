use super::*;
use crate::{
    backends::native::tests::claude_events,
    storage::{Storage, api_tests::turn},
};

fn init(id: &str) -> Value {
    json!({"type":"system","subtype":"init","session_id":id,"apiKeySource":"ANTHROPIC_API_KEY","permissionMode":"dontAsk","tools":[],"mcp_servers":[],"skills":[],"plugins":[],"slash_commands":[]})
}
pub(crate) fn events(id: &str) -> Vec<Value> {
    let mut values = vec![init(id)];
    values.extend(claude_events().into_iter().map(
        |e| json!({"type":"stream_event","session_id":id,"event":e,"parent_tool_use_id":null}),
    ));
    values.push(json!({"type":"result","session_id":id,"subtype":"success","is_error":false,"result":"Bonjour 🌍","usage":{"input_tokens":11,"output_tokens":7},"permission_denials":[]}));
    values
}
#[test]
fn lifecycle_requires_isolated_init_matching_session_and_final_result() {
    let id = uuid::Uuid::new_v4().to_string();
    let mut protocol = Protocol::default();
    let mut output = Output::default();
    for event in events(&id) {
        protocol.accept(event, &id, &mut output).unwrap();
    }
    assert!(protocol.result);
    assert!(!output.complete, "process exit must still be checked");
    assert_eq!(output.text, "Bonjour 🌍");
    assert_eq!(output.continuation.unwrap()["session_id"], id);
    assert_eq!(output.usage.unwrap()["output_tokens"], 7);
    for (field, value) in [
        ("tools", json!(["Bash"])),
        ("mcp_servers", json!([{"name":"remote"}])),
        ("apiKeySource", json!("none")),
        ("permissionMode", json!("bypassPermissions")),
        ("session_id", json!("other")),
    ] {
        let mut event = init(&id);
        event[field] = value;
        assert!(
            Protocol::default()
                .accept(event, &id, &mut Output::default())
                .is_err()
        );
    }
    for change in [
        json!({"is_error":true}),
        json!({"permission_denials":[{"tool_name":"Bash"}]}),
        json!({"result":"different"}),
    ] {
        let mut protocol = Protocol::default();
        let mut output = Output::default();
        let mut fixture = events(&id);
        fixture
            .last_mut()
            .unwrap()
            .as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        let error = fixture
            .into_iter()
            .try_for_each(|e| protocol.accept(e, &id, &mut output));
        assert!(error.is_err());
        assert!(!protocol.result);
        assert_eq!(output.text, "Bonjour 🌍");
    }
}

#[test]
fn cli_retry_and_additional_model_round_are_rejected() {
    let id = uuid::Uuid::new_v4().to_string();
    for retry in [true, false] {
        let mut protocol = Protocol::default();
        let mut output = Output::default();
        let mut fixture = events(&id);
        fixture.pop();
        for event in fixture {
            protocol.accept(event, &id, &mut output).unwrap();
        }
        let extra = if retry {
            json!({"type":"system","subtype":"api_retry","session_id":id})
        } else {
            json!({"type":"stream_event","session_id":id,"event":{"type":"message_start"}})
        };
        assert!(protocol.accept(extra, &id, &mut output).is_err());
        assert!(!output.complete);
        assert_eq!(output.text, "Bonjour 🌍");
    }
}

#[cfg(unix)]
fn script(directory: &Path, contents: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let file = directory.join("claude-fixture");
    std::fs::write(&file, format!("#!/bin/sh\n{contents}\n")).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
    file
}
#[cfg(unix)]
#[test]
fn command_keeps_prompt_off_argv_and_forks_only_explicit_completed_session() {
    let directory = tempfile::tempdir().unwrap();
    let binary = script(directory.path(), "exit 0");
    let storage = Storage::memory();
    let mut turn = turn(&storage, "c", "main");
    turn.profile.config.binary_path = binary.to_str().unwrap().into();
    turn.input = "learner text `$(echo untrusted)`".into();
    let old = uuid::Uuid::new_v4().to_string();
    let request = Request::new(
        directory.path().into(),
        &[("q".into(), "a".into(), Some(json!({"session_id":old})))],
    )
    .unwrap();
    let credential = Credential {
        key: zeroize::Zeroizing::new("fixture-key".into()),
        scope: "fixture".into(),
        endpoint: "".into(),
    };
    let command = command(&turn, &credential, &request).unwrap();
    let command = command.as_std();
    let args: Vec<_> = command
        .get_args()
        .map(|v| v.to_string_lossy().into_owned())
        .collect();
    assert!(
        !args
            .iter()
            .any(|v| v.contains("learner text") || v.contains("fixture-key"))
    );
    assert!(args.windows(2).any(|v| v == ["--tools", ""]));
    assert!(args.windows(2).any(|v| v == ["--resume", old.as_str()]));
    assert!(
        args.windows(2)
            .any(|v| v == ["--session-id", turn.id.as_str()])
    );
    assert!(args.contains(&"--fork-session".into()));
    assert!(!args.contains(&"--continue".into()));
    let env: std::collections::HashMap<_, _> = command.get_envs().collect();
    assert_eq!(
        env.get(std::ffi::OsStr::new("ANTHROPIC_API_KEY"))
            .unwrap()
            .unwrap(),
        "fixture-key"
    );
    for name in [
        "CLAUDE_CODE_OAUTH_TOKEN",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
        "CLAUDE_CODE_USE_BEDROCK",
        "NODE_OPTIONS",
        "LD_PRELOAD",
    ] {
        assert!(!env.contains_key(std::ffi::OsStr::new(name)));
    }
    assert!(Request::new(directory.path().into(), &[("q".into(), "a".into(), None)]).is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn nonzero_exit_or_missing_result_keeps_partial_text_and_never_completes() {
    let storage = Storage::memory();
    let turn = turn(&storage, "c", "main");
    for (complete, exit) in [(true, 1), (false, 0), (true, 0)] {
        let directory = tempfile::tempdir().unwrap();
        let mut fixture = events(&turn.id);
        if !complete {
            fixture.pop();
        }
        let wire = fixture.iter().map(|v| format!("{v}\n")).collect::<String>();
        let path = script(
            directory.path(),
            &format!("cat >/dev/null\ncat <<'PARLEY_FIXTURE'\n{wire}PARLEY_FIXTURE\nexit {exit}"),
        );
        let mut command = base_command(&path).unwrap();
        command.stdin(Stdio::piped());
        let (_cancel, rx) = watch::channel(false);
        let mut output = Output::default();
        let result = run(command, &turn, &mut output, rx, |_| {
            std::future::ready(Ok(()))
        })
        .await;
        assert_eq!(result.is_ok(), complete && exit == 0);
        assert_eq!(output.complete, complete && exit == 0);
        assert_eq!(output.text, "Bonjour 🌍");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_reaps_process_and_descendants_with_partial_output() {
    let storage = Storage::memory();
    let turn = turn(&storage, "c", "main");
    let directory = tempfile::tempdir().unwrap();
    let wire = events(&turn.id)
        .into_iter()
        .take(8)
        .map(|v| format!("{v}\n"))
        .collect::<String>();
    let path = script(
        directory.path(),
        &format!(
            "cat >/dev/null\nsleep 60 &\necho \"$$ $!\" >owned-pids\ncat <<'PARLEY_FIXTURE'\n{wire}PARLEY_FIXTURE\nwait"
        ),
    );
    let mut command = base_command(&path).unwrap();
    command.stdin(Stdio::piped()).current_dir(directory.path());
    let (cancel, rx) = watch::channel(false);
    let mut output = Output::default();
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        run(command, &turn, &mut output, rx, |out| {
            if !out.text.is_empty() {
                cancel.send(true).unwrap();
            }
            std::future::ready(Ok(()))
        }),
    )
    .await
    .unwrap();
    assert!(result.unwrap_err().contains("停止"));
    assert_eq!(output.text, "Bonjour 🌍");
    assert!(!output.complete);
    #[cfg(target_os = "linux")]
    for pid in std::fs::read_to_string(directory.path().join("owned-pids"))
        .unwrap()
        .split_whitespace()
    {
        // SIGKILL has been sent to the entire group. The kernel may still need a
        // scheduler tick to finish descendants after the direct child is reaped.
        tokio::time::timeout(Duration::from_secs(1), async {
            while let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                let state = status.rsplit_once(") ").unwrap().1.chars().next().unwrap();
                if matches!(state, 'Z' | 'X') {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("owned descendant must stop after group cancellation");
    }
}

#[tokio::test]
#[ignore = "requires PARLEY_TEST_CLAUDE_BIN; uses only loopback fixtures, no real API or login"]
async fn live_cli_initialization_stream_and_explicit_session_fork() {
    use tokio::net::TcpListener;
    let binary = std::env::var("PARLEY_TEST_CLAUDE_BIN").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let mut posts = 0;
        while posts < 3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 8192];
            let end = loop {
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
                assert!(bytes.len() < 200_000);
            };
            let headers = String::from_utf8(bytes[..end].to_vec())
                .unwrap()
                .to_lowercase();
            if headers.starts_with("head ") {
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await
                    .unwrap();
                continue;
            }
            assert!(headers.starts_with("post /v1/messages"));
            assert!(headers.contains("x-api-key: fixture-key"));
            let length = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .unwrap()
                .parse::<usize>()
                .unwrap();
            assert!(length < 200_000);
            while bytes.len() < end + length {
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let body: Value = serde_json::from_slice(&bytes[end..end + length]).unwrap();
            assert!(
                body.get("tools")
                    .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
            );
            assert_eq!(body["stream"], true);
            if posts == 1 {
                let messages = body["messages"].as_array().unwrap();
                assert!(
                    messages.iter().any(|m| m["role"] == "assistant"
                        && m["content"].to_string().contains("Bonjour")),
                    "resume must actually carry prior model text"
                );
            }
            if posts >= 2 {
                let error =
                    json!({"type":"error","error":{"type":"overloaded_error","message":"fixture"}})
                        .to_string();
                socket.write_all(format!("HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{error}",error.len()).as_bytes()).await.unwrap();
                posts += 1;
                continue;
            }
            let fixture = [
                json!({"type":"message_start","message":{"id":format!("msg_{posts}"),"type":"message","role":"assistant","model":"claude-sonnet-4-6","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":1}}}),
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Bonjour 🌍"}}),
                json!({"type":"content_block_stop","index":0}),
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":2}}),
                json!({"type":"message_stop"}),
            ];
            let wire = fixture
                .iter()
                .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
                .collect::<String>();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{wire}",wire.len()).as_bytes()).await.unwrap();
            posts += 1;
        }
    });
    let directory = tempfile::tempdir().unwrap();
    let storage = Storage::memory();
    let mut turn = turn(&storage, "c", "main");
    turn.id = uuid::Uuid::new_v4().to_string();
    turn.profile.config.kind = BackendKind::ClaudeCode;
    turn.profile.config.binary_path = binary;
    turn.model = "claude-sonnet-4-6".into();
    check(&turn.profile).await.unwrap();
    let credential = Credential {
        key: zeroize::Zeroizing::new("fixture-key".into()),
        scope: "fixture".into(),
        endpoint: "".into(),
    };
    let mut request = Request::new(directory.path().into(), &[]).unwrap();
    for round in 0..2 {
        if round == 1 {
            turn.id = uuid::Uuid::new_v4().to_string();
            turn.input = "How do I reply?".into();
        }
        let mut cmd = command(&turn, &credential, &request).unwrap();
        // This override exists only inside the test; production never inherits a base URL.
        cmd.env("ANTHROPIC_BASE_URL", &endpoint);
        let mut output = Output::default();
        let (_cancel, rx) = watch::channel(false);
        tokio::time::timeout(
            Duration::from_secs(30),
            run(cmd, &turn, &mut output, rx, |_| std::future::ready(Ok(()))),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(output.complete);
        assert_eq!(output.text, "Bonjour 🌍");
        assert_eq!(output.continuation.as_ref().unwrap()["session_id"], turn.id);
        request = Request::new(
            directory.path().into(),
            &[(turn.input.clone(), output.text, output.continuation)],
        )
        .unwrap();
    }
    turn.id = uuid::Uuid::new_v4().to_string();
    let mut cmd = command(&turn, &credential, &request).unwrap();
    cmd.env("ANTHROPIC_BASE_URL", &endpoint);
    let mut output = Output::default();
    let (_cancel, rx) = watch::channel(false);
    assert!(
        tokio::time::timeout(
            Duration::from_secs(5),
            run(cmd, &turn, &mut output, rx, |_| std::future::ready(Ok(())))
        )
        .await
        .unwrap()
        .is_err()
    );
    assert!(!output.complete);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}

#[test]
fn diagnostics_are_bounded_and_never_return_raw_stderr() {
    let mut diagnostic = Diagnostic::default();
    diagnostic.push(&vec![b'x'; 100_000]);
    diagnostic.push(b"unknown option --private-learner-text api-key=fixture-secret");
    assert_eq!(diagnostic.0.len(), 8192);
    let detail = diagnostic.summary().unwrap();
    assert!(detail.contains("启动参数"));
    assert!(!detail.contains("fixture-secret"));
    assert!(!detail.contains("private-learner"));
}
