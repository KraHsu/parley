//! Native Claude terminal hooks. No model calls, transcript reads, or prompt injection.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::State;
const MAX_BODY: usize = 256 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Message {
    id: String,
    thread_id: String,
    turn_id: String,
    role: String,
    text: String,
    truncated: bool,
}
#[derive(Serialize, Deserialize)]
struct Session {
    id: String,
    title: String,
    created_at: u64,
    messages: VecDeque<Message>,
    turn: String,
}
#[derive(Default, Serialize, Deserialize)]
struct Data {
    sessions: VecDeque<Session>,
    received: bool,
    error: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Thread {
    id: String,
    title: String,
    created_at: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    threads: Vec<Thread>,
    messages: Vec<Message>,
    connected: bool,
    notice: String,
}

#[derive(Default)]
pub struct TerminalState {
    directory: Option<std::path::PathBuf>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn clipped(text: &str) -> (String, bool) {
    (
        text.chars().take(16000).collect(),
        text.chars().count() > 16000,
    )
}
impl Data {
    fn accept(&mut self, event: Value) -> Result<(), String> {
        if event.get("agent_id").is_some_and(|v| !v.is_null()) {
            return Ok(());
        }
        let raw = event["session_id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 128)
            .ok_or("会话标识无效")?;
        let name = event["hook_event_name"].as_str().ok_or("事件类型无效")?;
        if ![
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "StopFailure",
            "SessionEnd",
        ]
        .contains(&name)
        {
            return Err("不支持的终端事件".into());
        }
        let id = format!("claude-code:{raw}");
        if !self.sessions.iter().any(|s| s.id == id) {
            if self.sessions.len() >= 20 {
                self.sessions.pop_front();
            }
            self.sessions.push_back(Session {
                id: id.clone(),
                title: "Claude Code 对话".into(),
                created_at: now(),
                messages: VecDeque::new(),
                turn: uuid::Uuid::new_v4().to_string(),
            });
        }
        let session = self.sessions.iter_mut().find(|s| s.id == id).unwrap();
        match name {
            "UserPromptSubmit" => {
                let prompt = event["prompt"].as_str().ok_or("缺少用户文本")?;
                session.turn = uuid::Uuid::new_v4().to_string();
                let (text, truncated) = clipped(prompt);
                session.title = format!(
                    "Claude Code · {}",
                    text.chars().take(40).collect::<String>()
                );
                session.messages.push_back(Message {
                    id: format!("{}:user", session.turn),
                    thread_id: id,
                    turn_id: session.turn.clone(),
                    role: "user".into(),
                    text,
                    truncated,
                });
            }
            "Stop" => {
                let answer = event["last_assistant_message"]
                    .as_str()
                    .ok_or("此 CLI 未提供最终回答文本")?;
                let (text, truncated) = clipped(answer);
                if !text.is_empty()
                    && !session.messages.back().is_some_and(|m| {
                        m.role == "assistant" && m.turn_id == session.turn && m.text == text
                    })
                {
                    session.messages.push_back(Message {
                        id: uuid::Uuid::new_v4().to_string(),
                        thread_id: id,
                        turn_id: session.turn.clone(),
                        role: "assistant".into(),
                        text,
                        truncated,
                    });
                }
            }
            // StopFailure contains rendered API error text, not an assistant answer.
            "StopFailure" | "SessionEnd" | "SessionStart" => {}
            _ => unreachable!(),
        }
        while session.messages.len() > 12 {
            session.messages.pop_front();
        }
        self.received = true;
        Ok(())
    }
    fn snapshot(&self, thread: Option<&str>) -> Snapshot {
        let notice = self.error.clone().unwrap_or_else(|| {
            if self.received {
                "已接收 Claude 终端事件；显示本次终端启动后收到的最近六轮（关闭语法窗口期间也会同步），不包含启动前历史。"
            } else {
                "尚未收到 Claude 终端事件。若已开始对话，请检查 /hooks；bare、安全模式或组织策略可能禁用同步。"
            }.into()
        });
        Snapshot {
            threads: self
                .sessions
                .iter()
                .map(|s| Thread {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    created_at: s.created_at,
                })
                .collect(),
            messages: self
                .sessions
                .iter()
                .find(|s| Some(s.id.as_str()) == thread)
                .map(|s| s.messages.iter().cloned().collect())
                .unwrap_or_default(),
            connected: self.received,
            notice,
        }
    }
}
// The callback is a short-lived native process, independent of the GUI. Only the
// bounded visible-message snapshot is retained; raw hook payloads are never saved.
pub fn plugin(directory: &Path, launcher: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directory.join(".claude-plugin")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(directory.join("hooks")).map_err(|e| e.to_string())?;
    let manifest = json!({"name":format!("parley-terminal-{}",uuid::Uuid::new_v4()),"version":"0.2.0","description":"Local language-learning terminal context"});
    crate::exchange::write_atomic(
        &directory.join(".claude-plugin/plugin.json"),
        manifest.to_string().as_bytes(),
    )?;
    let mut hooks = json!({});
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "Stop",
        "StopFailure",
        "SessionEnd",
    ] {
        // Exec form has no shell interpolation, including on Windows.
        hooks[event] = json!([{"hooks":[{"type":"command","command":launcher,"args":["--terminal-event",directory],"timeout":2}]}]);
    }
    crate::exchange::write_atomic(
        &directory.join("hooks/hooks.json"),
        json!({"hooks":hooks}).to_string().as_bytes(),
    )
}
fn read_data(directory: &Path) -> Result<Data, String> {
    use std::io::Read;
    let file = match std::fs::File::open(directory.join("context.json")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Data::default()),
        Err(e) => return Err(e.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("终端同步记录过大。".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
pub fn record(directory: &Path, input: impl std::io::Read) -> Result<(), String> {
    use std::io::Read;
    // Only attach to a directory created by the launcher. Never create a missing
    // session from a stale hook after the user has removed its cached context.
    if !directory.join("companion.json").is_file() {
        return Ok(());
    }
    let mut bytes = Vec::new();
    input
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("context.lock"))
        .map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while lock.try_lock().is_err() {
        if std::time::Instant::now() >= deadline {
            return Err("终端同步正忙。".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let mut data = read_data(directory)?;
    let result = if bytes.len() > MAX_BODY {
        Err("终端事件超过 256 KiB，本次内容未同步。".into())
    } else {
        serde_json::from_slice(&bytes)
            .map_err(|_| "终端事件 JSON 无效。".to_owned())
            .and_then(|event| data.accept(event))
    };
    data.error = result.err();
    crate::exchange::write_atomic(
        &directory.join("context.json"),
        &serde_json::to_vec(&data).map_err(|e| e.to_string())?,
    )
}
impl TerminalState {
    pub fn attach(directory: &Path) -> Self {
        Self {
            directory: Some(directory.to_owned()),
        }
    }
    fn snapshot(&self, thread: Option<&str>) -> Snapshot {
        let data = self
            .directory
            .as_ref()
            .ok_or_else(|| "请通过 parley-cli 启动或重连语法窗口。".to_owned())
            .and_then(|directory| read_data(directory));
        match data {
            Ok(data) => data.snapshot(thread),
            Err(error) => Data {
                error: Some(error),
                ..Data::default()
            }
            .snapshot(thread),
        }
    }
}
#[tauri::command]
pub fn claude_terminal_context(
    state: State<'_, TerminalState>,
    thread_id: Option<String>,
) -> Snapshot {
    state.snapshot(thread_id.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn events_keep_session_namespaces_recent_turns_and_ignore_errors_and_subagents() {
        let mut data = Data::default();
        for i in 0..8 {
            data.accept(json!({"session_id":"session","hook_event_name":"UserPromptSubmit","prompt":format!("question {i}")})).unwrap();
            data.accept(json!({"session_id":"session","hook_event_name":"Stop","last_assistant_message":format!("answer {i}")})).unwrap();
        }
        let before = data.snapshot(Some("claude-code:session"));
        assert_eq!(before.messages.len(), 12);
        assert_eq!(before.messages[0].text, "question 2");
        let latest = before.messages.last().unwrap().id.clone();
        data.accept(json!({"session_id":"session","hook_event_name":"Stop","last_assistant_message":"answer 7"})).unwrap();
        data.accept(json!({"session_id":"session","hook_event_name":"StopFailure","last_assistant_message":"API secret error"})).unwrap();
        data.accept(json!({"session_id":"subagent","agent_id":"agent","hook_event_name":"Stop","last_assistant_message":"not learner context"})).unwrap();
        let after = data.snapshot(Some("claude-code:session"));
        assert_eq!(after.threads.len(), 1);
        assert_eq!(after.messages.len(), 12);
        assert_eq!(after.messages.last().unwrap().id, latest);
        assert!(data.snapshot(Some("session")).messages.is_empty());
        data.accept(json!({"session_id":"another","hook_event_name":"Stop","last_assistant_message":"x".repeat(20000)})).unwrap();
        assert!(data.snapshot(Some("claude-code:another")).messages[0].truncated);
        assert_eq!(
            after.messages.last().unwrap().thread_id,
            "claude-code:session"
        );
    }
    #[test]
    fn callbacks_continue_without_gui_and_reopened_window_recovers_context() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("companion.json"), "{}").unwrap();
        let event = |name: &str, field: &str, text: &str| {
            json!({"session_id":"session","hook_event_name":name,field:text}).to_string()
        };
        record(
            directory.path(),
            event("UserPromptSubmit", "prompt", "Bonjour?").as_bytes(),
        )
        .unwrap();
        let window = TerminalState::attach(directory.path());
        assert_eq!(
            window.snapshot(Some("claude-code:session")).messages.len(),
            1
        );
        drop(window);
        record(
            directory.path(),
            event("Stop", "last_assistant_message", "Bonjour 🌍").as_bytes(),
        )
        .unwrap();
        let reopened = TerminalState::attach(directory.path());
        let snapshot = reopened.snapshot(Some("claude-code:session"));
        assert_eq!(snapshot.messages.len(), 2);
        assert_eq!(snapshot.messages[1].text, "Bonjour 🌍");
        record(directory.path(), vec![b'x'; MAX_BODY + 1].as_slice()).unwrap();
        assert!(reopened.snapshot(None).notice.contains("256 KiB"));
        assert_eq!(
            reopened
                .snapshot(Some("claude-code:session"))
                .messages
                .len(),
            2
        );
        record(
            directory.path(),
            event("Stop", "last_assistant_message", "Salut").as_bytes(),
        )
        .unwrap();
        assert!(!reopened.snapshot(None).notice.contains("256 KiB"));
    }
    #[test]
    fn plugin_uses_direct_arguments_and_does_not_depend_on_a_listening_window() {
        let directory = tempfile::tempdir().unwrap();
        let launcher = directory.path().join("a ' $weird` launcher");
        plugin(directory.path(), &launcher).unwrap();
        let hooks: Value = serde_json::from_slice(
            &std::fs::read(directory.path().join("hooks/hooks.json")).unwrap(),
        )
        .unwrap();
        let hook = &hooks["hooks"]["Stop"][0]["hooks"][0];
        assert_eq!(hook["type"], "command");
        assert_eq!(hook["command"], launcher.to_str().unwrap());
        assert_eq!(hook["args"], json!(["--terminal-event", directory.path()]));
        assert!(hook.get("url").is_none());
    }
}

#[cfg(all(test, unix))]
#[tokio::test]
#[ignore = "requires PARLEY_TEST_CLAUDE_BIN; real CLI with loopback API and temporary hooks only"]
async fn live_claude_plugin_delivers_new_turn_without_replacing_user_hooks() {
    let binary = std::env::var("PARLEY_TEST_CLAUDE_BIN").unwrap();
    use std::time::Duration;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    let launcher = std::env::var("PARLEY_TEST_LAUNCHER_BIN").unwrap();
    let companion = tempfile::tempdir().unwrap();
    std::fs::write(companion.path().join("companion.json"), "{}").unwrap();
    plugin(companion.path(), Path::new(&launcher)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let api = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", api.local_addr().unwrap());
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = api.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 8192];
            let end = loop {
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
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
            let length = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .unwrap()
                .parse::<usize>()
                .unwrap();
            while bytes.len() < end + length {
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let body: Value = serde_json::from_slice(&bytes[end..end + length]).unwrap();
            assert!(body["messages"].to_string().contains("Language fixture"));
            let events = [
                json!({"type":"message_start","message":{"id":"msg_fixture","type":"message","role":"assistant","model":"claude-sonnet-4-6","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":1}}}),
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Bonjour 🌍"}}),
                json!({"type":"content_block_stop","index":0}),
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":2}}),
                json!({"type":"message_stop"}),
            ];
            let body = events
                .iter()
                .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
                .collect::<String>();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            break;
        }
    });
    let marker = directory.path().join("user-hook-ran");
    let settings = json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/usr/bin/touch","args":[marker]}]}]}});
    let mut command = tokio::process::Command::new(&binary);
    command.env_clear();
    for name in ["HOME", "PATH", "LANG", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .current_dir(directory.path())
        .env("CLAUDE_CONFIG_DIR", directory.path().join("config"))
        .env("ANTHROPIC_API_KEY", "fixture-key")
        .env("ANTHROPIC_BASE_URL", endpoint)
        .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
        .env("CLAUDE_CODE_MAX_RETRIES", "0")
        .args([
            "--print",
            "--verbose",
            "--output-format",
            "stream-json",
            "--tools",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
            "--disable-slash-commands",
            "--setting-sources",
            "",
            "--model",
            "claude-sonnet-4-6",
            "--prompt-suggestions",
            "false",
            "--plugin-dir",
        ])
        .arg(companion.path())
        .arg("--settings")
        .arg(settings.to_string())
        .arg("Language fixture")
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(marker.is_file(), "the user's own hook must still run");
    {
        let data = read_data(companion.path()).unwrap();
        assert!(data.received, "CLI must deliver real hook events");
        assert_eq!(data.sessions.len(), 1);
        let session = &data.sessions[0];
        assert!(session.id.starts_with("claude-code:"));
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].text, "Language fixture");
        assert_eq!(session.messages[1].text, "Bonjour 🌍");
    }
    server.await.unwrap();
}
