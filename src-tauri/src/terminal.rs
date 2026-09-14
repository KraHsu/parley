//! Native Claude terminal hooks. No model calls, transcript reads, or prompt injection.
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::State;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
};

const MAX_BODY: usize = 256 * 1024;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Message {
    id: String,
    thread_id: String,
    turn_id: String,
    role: String,
    text: String,
    truncated: bool,
}
struct Session {
    id: String,
    title: String,
    created_at: u64,
    messages: VecDeque<Message>,
    turn: String,
}
#[derive(Default)]
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
    data: Arc<Mutex<Data>>,
    _plugin: Option<tempfile::TempDir>,
    task: Option<tauri::async_runtime::JoinHandle<()>>,
}
impl Drop for TerminalState {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
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
                "已接收 Claude 终端事件；显示本次启动后收到的最近六轮，不包含启动前历史。"
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
fn plugin(directory: &Path, url: &str, token: &str) -> Result<(), String> {
    std::fs::create_dir_all(directory.join(".claude-plugin")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(directory.join("hooks")).map_err(|e| e.to_string())?;
    let manifest = json!({"name":format!("parley-terminal-{}",uuid::Uuid::new_v4()),"version":"0.1.0","description":"Local language-learning terminal context"});
    std::fs::write(
        directory.join(".claude-plugin/plugin.json"),
        manifest.to_string(),
    )
    .map_err(|e| e.to_string())?;
    let mut hooks = json!({});
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "Stop",
        "StopFailure",
        "SessionEnd",
    ] {
        hooks[event] = json!([{"hooks":[{"type":"http","url":url,"headers":{"Authorization":format!("Bearer {token}")},"timeout":1}]}]);
    }
    std::fs::write(
        directory.join("hooks/hooks.json"),
        json!({"hooks":hooks}).to_string(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
impl TerminalState {
    pub fn start(ready: &Path) -> Result<Self, String> {
        // The launcher creates this private empty file; do not create arbitrary paths.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(ready)
            .map_err(|e| e.to_string())?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let url = format!(
            "http://{}/terminal-hooks",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let directory = tempfile::Builder::new()
            .prefix("parley-terminal-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        plugin(directory.path(), &url, &token)?;
        let data = Arc::new(Mutex::new(Data::default()));
        let state = data.clone();
        let task = tauri::async_runtime::spawn(async move {
            let Ok(listener) = TcpListener::from_std(listener) else {
                return;
            };
            let capacity = Arc::new(Semaphore::new(8));
            while let Ok((socket, peer)) = listener.accept().await {
                if !peer.ip().is_loopback() {
                    continue;
                }
                let Ok(permit) = capacity.clone().try_acquire_owned() else {
                    continue;
                };
                let state = state.clone();
                let token = token.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    let _ = tokio::time::timeout(
                        Duration::from_secs(2),
                        receive(socket, &token, &state),
                    )
                    .await;
                });
            }
        });
        use std::io::Write;
        let written = file
            .write_all(
                json!({"pluginPath":directory.path()})
                    .to_string()
                    .as_bytes(),
            )
            .and_then(|_| file.sync_all());
        if let Err(error) = written {
            task.abort();
            return Err(error.to_string());
        }
        Ok(Self {
            data,
            _plugin: Some(directory),
            task: Some(task),
        })
    }
}
async fn receive(
    mut socket: TcpStream,
    token: &str,
    data: &Arc<Mutex<Data>>,
) -> Result<(), String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let end = loop {
        let n = socket.read(&mut buffer).await.map_err(|_| "读取事件失败")?;
        if n == 0 {
            return Err("事件连接已关闭".into());
        }
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            if i > 8192 {
                return Err("事件头过大".into());
            }
            break i + 4;
        }
        if bytes.len() > 8192 {
            return Err("事件头过大".into());
        }
    };
    let headers = std::str::from_utf8(&bytes[..end]).map_err(|_| "事件头无效")?;
    let mut auth = false;
    let mut length = None;
    if !headers.starts_with("POST /terminal-hooks HTTP/1.1\r\n") {
        return Err("事件路径无效".into());
    }
    for line in headers.lines().skip(1) {
        if let Some((name, value)) = line.split_once(':') {
            match name.to_ascii_lowercase().as_str() {
                "authorization" => auth = value.trim() == format!("Bearer {token}"),
                "content-length" => {
                    if length.is_some() {
                        return Err("重复长度".into());
                    }
                    length = value.trim().parse::<usize>().ok();
                }
                "transfer-encoding" | "origin" => return Err("不接受浏览器或分块事件".into()),
                _ => {}
            }
        }
    }
    if !auth {
        return Err("事件认证无效".into());
    }
    let Some(length) = length.filter(|n| *n <= MAX_BODY) else {
        data.lock().unwrap().error = Some("终端事件过大或长度无效，本次内容未能同步。".into());
        return Err("事件长度无效".into());
    };
    while bytes.len() < end + length {
        let n = socket.read(&mut buffer).await.map_err(|_| "读取事件失败")?;
        if n == 0 {
            return Err("事件正文不完整".into());
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    let event: Value =
        serde_json::from_slice(&bytes[end..end + length]).map_err(|_| "事件 JSON 无效")?;
    let updates_text = matches!(
        event["hook_event_name"].as_str(),
        Some("UserPromptSubmit" | "Stop")
    );
    {
        let mut data = data.lock().unwrap();
        if let Err(error) = data.accept(event) {
            data.error = Some(format!(
                "终端事件未能同步：{error}。请检查 CLI 版本和 hooks 状态。"
            ));
            return Err(error);
        }
        if updates_text {
            data.error = None;
        }
    }
    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.map_err(|_|"事件响应失败")?;
    Ok(())
}
#[tauri::command]
pub fn claude_terminal_context(
    state: State<'_, TerminalState>,
    thread_id: Option<String>,
) -> Snapshot {
    let mut snapshot = state.data.lock().unwrap().snapshot(thread_id.as_deref());
    if state.task.is_none() {
        snapshot.notice = "终端同步接收器未启动，请通过 parley-cli 重新打开伴随窗口。".into();
    }
    snapshot
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
    #[tokio::test]
    async fn loopback_auth_size_limits_and_empty_hook_response() {
        let ready = tempfile::NamedTempFile::new().unwrap();
        let state = TerminalState::start(ready.path()).unwrap();
        let metadata: Value =
            serde_json::from_slice(&std::fs::read(ready.path()).unwrap()).unwrap();
        let hooks: Value = serde_json::from_slice(
            &std::fs::read(
                Path::new(metadata["pluginPath"].as_str().unwrap()).join("hooks/hooks.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let hook = &hooks["hooks"]["Stop"][0]["hooks"][0];
        let url = hook["url"].as_str().unwrap();
        let key = hook["headers"]["Authorization"].as_str().unwrap();
        let client = reqwest::Client::new();
        let event = json!({"session_id":"session","hook_event_name":"Stop","last_assistant_message":"Bonjour 🌍"});
        assert!(client.post(url).json(&event).send().await.is_err());
        assert!(state.data.lock().unwrap().sessions.is_empty());
        let response = client
            .post(url)
            .header("Authorization", key)
            .json(&event)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.json::<Value>().await.unwrap(), json!({}));
        assert_eq!(
            state
                .data
                .lock()
                .unwrap()
                .snapshot(Some("claude-code:session"))
                .messages[0]
                .text,
            "Bonjour 🌍"
        );
        assert!(
            client
                .post(url)
                .header("Authorization", key)
                .body("x".repeat(MAX_BODY + 1))
                .send()
                .await
                .is_err()
        );
        assert_eq!(state.data.lock().unwrap().sessions.len(), 1);
    }
}

#[cfg(all(test, unix))]
#[tokio::test]
#[ignore = "requires PARLEY_TEST_CLAUDE_BIN; real CLI with loopback API and temporary hooks only"]
async fn live_claude_plugin_delivers_new_turn_without_replacing_user_hooks() {
    let binary = std::env::var("PARLEY_TEST_CLAUDE_BIN").unwrap();
    let ready = tempfile::NamedTempFile::new().unwrap();
    let state = TerminalState::start(ready.path()).unwrap();
    let metadata: Value = serde_json::from_slice(&std::fs::read(ready.path()).unwrap()).unwrap();
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
        .arg(metadata["pluginPath"].as_str().unwrap())
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
        let data = state.data.lock().unwrap();
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
