//! User-selected Claude Code, isolated API authentication and explicit session forks.
use super::{
    credentials::Credential,
    http::{self, Output},
    types::{BackendKind, BackendProfile},
};
use crate::storage::api::ApiTurn;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    sync::watch,
};

pub struct Request {
    pub directory: PathBuf,
    pub resume: Option<String>,
}
impl Request {
    pub fn new(
        directory: PathBuf,
        history: &[(String, String, Option<Value>)],
    ) -> Result<Self, String> {
        let resume = history
            .last()
            .map(|(_, _, output)| {
                let id = output
                    .as_ref()
                    .and_then(|v| v["session_id"].as_str())
                    .ok_or("Claude Code 续聊状态缺失，请新建对话。")?;
                uuid::Uuid::parse_str(id).map_err(|_| "Claude Code 会话标识无效。")?;
                Ok::<_, String>(id.to_owned())
            })
            .transpose()?;
        Ok(Self { directory, resume })
    }
}

fn base_command(binary: &Path) -> Result<Command, String> {
    if !binary.is_absolute() || !binary.is_file() {
        return Err("请在设置中选择存在的 Claude Code 可执行文件绝对路径。".into());
    }
    let mut cmd = std::process::Command::new(binary);
    // Keep only OS/runtime and proxy settings; no inherited provider keys, OAuth,
    // loaders, CLI routing variables, or apiKeyHelper commands.
    cmd.env_clear();
    for name in [
        "HOME",
        "USERPROFILE",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "PATHEXT",
        "TEMP",
        "TMP",
        "TMPDIR",
        "LANG",
        "LC_ALL",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
    ] {
        if let Some(value) = std::env::var_os(name) {
            cmd.env(name, value);
        }
    }
    let mut paths = vec![binary.parent().ok_or("CLI 路径无效。")?.to_path_buf()];
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    cmd.env(
        "PATH",
        std::env::join_paths(paths).map_err(|_| "CLI 运行路径无效。")?,
    );
    cmd.env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
        .env("DISABLE_AUTOUPDATER", "1")
        .env("DISABLE_TELEMETRY", "1")
        .env("CLAUDE_CODE_MAX_RETRIES", "0")
        .env("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "4096")
        .env("CLAUDE_CODE_MAX_TURNS", "1");
    let mut cmd = Command::from(cmd);
    cmd.kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    cmd.process_group(0);
    Ok(cmd)
}

pub async fn check(profile: &BackendProfile) -> Result<(), String> {
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        base_command(Path::new(&profile.config.binary_path))?
            .arg("--help")
            .output(),
    )
    .await
    .map_err(|_| "Claude Code 能力检查超时。")?
    .map_err(|_| "无法运行所选 Claude Code，请检查文件和执行权限。")?;
    let help = String::from_utf8_lossy(&output.stdout);
    if !output.status.success()
        || ![
            "--bare",
            "--restricted",
            "--tools",
            "--strict-mcp-config",
            "--include-partial-messages",
            "--disable-slash-commands",
            "--setting-sources",
            "--permission-prompts",
            "--fork-session",
            "--session-id",
        ]
        .iter()
        .all(|flag| help.contains(flag))
    {
        return Err("当前 Claude Code 缺少所需的隔离或流式能力。请更新用户安装的 CLI（已验证 2.1.269），或使用 Claude API。".into());
    }
    Ok(())
}

fn command(turn: &ApiTurn, credential: &Credential, request: &Request) -> Result<Command, String> {
    let mut cmd = base_command(Path::new(&turn.profile.config.binary_path))?;
    cmd.current_dir(&request.directory)
        .env("CLAUDE_CONFIG_DIR", request.directory.join("config"))
        .env("ANTHROPIC_API_KEY", credential.key.as_str())
        .args([
            "--bare",
            "--restricted",
            "--print",
            "--verbose",
            "--include-partial-messages",
            "--output-format",
            "stream-json",
            "--input-format",
            "text",
            "--tools",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
            "--disable-slash-commands",
            "--setting-sources",
            "",
            "--settings",
            "{\"disableAllHooks\":true}",
            "--permission-mode",
            "dontAsk",
            "--permission-prompts",
            "none",
            "--no-chrome",
            "--prompt-suggestions",
            "false",
        ])
        .arg("--model")
        .arg(&turn.model)
        .arg("--session-id")
        .arg(&turn.id)
        .arg("--system-prompt")
        .arg(http::instructions(turn))
        .stdin(Stdio::piped());
    if let Some(id) = &request.resume {
        cmd.arg("--resume").arg(id).arg("--fork-session");
    }
    Ok(cmd)
}

#[derive(Default)]
struct Protocol {
    initialized: bool,
    result: bool,
    message: Output,
}
impl Protocol {
    fn accept(&mut self, event: Value, session: &str, output: &mut Output) -> Result<(), String> {
        if self.result {
            return Err("Claude Code 在最终结果后返回额外事件。".into());
        }
        if event["session_id"] != session {
            return Err("Claude Code 返回其他会话的事件，已停止。".into());
        }
        if event
            .get("parent_tool_use_id")
            .is_some_and(|v| !v.is_null())
        {
            return Err("纯聊天后端收到子任务事件，已停止。".into());
        }
        match event["type"].as_str() {
            Some("system") if event["subtype"] == "init" => {
                if self.initialized
                    || event["apiKeySource"] != "ANTHROPIC_API_KEY"
                    || event["permissionMode"] != "dontAsk"
                    || [
                        "tools",
                        "mcp_servers",
                        "skills",
                        "plugins",
                        "slash_commands",
                    ]
                    .iter()
                    .any(|key| event[key].as_array().is_none_or(|a| !a.is_empty()))
                {
                    return Err("Claude Code 的有效认证或纯聊天限制不符合配置，请使用 Claude API 或检查组织策略。".into());
                }
                self.initialized = true;
            }
            _ if !self.initialized => return Err("Claude Code 未确认初始化配置。".into()),
            Some("system") if event["subtype"] == "api_retry" => {
                return Err(
                    "Claude Code 请求失败，已停止自动重试。可检查服务状态后手动重试。".into(),
                );
            }
            Some("stream_event") => {
                if self.message.complete {
                    return Err("Claude Code 在回复完成后尝试继续生成，已停止。".into());
                }
                self.message
                    .accept(BackendKind::AnthropicMessages, &event["event"].to_string())?;
                output.text.clone_from(&self.message.text);
                output.usage.clone_from(&self.message.usage);
            }
            Some("assistant") => {
                let blocks = event["message"]["content"]
                    .as_array()
                    .ok_or("Claude Code 消息格式无效。")?;
                if blocks.iter().any(|b| {
                    !matches!(
                        b["type"].as_str(),
                        Some("text" | "thinking" | "redacted_thinking")
                    )
                }) {
                    return Err("纯聊天后端收到工具调用，已停止。".into());
                }
                if event.get("error").is_some_and(|v| !v.is_null()) {
                    return Err("Claude Code 返回模型错误，已保留部分回复。".into());
                }
            }
            Some("result") => {
                if event["subtype"] != "success"
                    || event["is_error"] != false
                    || !self.message.complete
                    || event["permission_denials"]
                        .as_array()
                        .is_none_or(|a| !a.is_empty())
                    || event["result"].as_str() != Some(output.text.as_str())
                {
                    return Err("Claude Code 未正常完成回答，已保留部分回复。".into());
                }
                output.usage = event.get("usage").cloned();
                output.continuation = Some(json!({"session_id":session}));
                self.result = true;
            }
            Some("system" | "rate_limit_event") => {}
            _ => return Err("Claude Code 返回不支持的事件，已停止。".into()),
        }
        Ok(())
    }
}

#[derive(Default)]
struct Diagnostic(Vec<u8>);
impl Diagnostic {
    fn push(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
        let excess = self.0.len().saturating_sub(8192);
        self.0.drain(..excess);
    }
    fn summary(&self) -> Option<&'static str> {
        let text = String::from_utf8_lossy(&self.0).to_ascii_lowercase();
        if text.contains("unknown option") || text.contains("unrecognized option") {
            Some("CLI 不支持启动参数，请更新所选 Claude Code（已验证 2.1.269）。")
        } else if text.contains("permission denied") || text.contains("exec format") {
            Some("CLI 无法执行，请检查权限及操作系统版本。")
        } else if text.contains("node") && text.contains("not found") {
            Some("CLI 所需的 Node 运行时不可用，请检查安装或选择原生可执行文件。")
        } else if !self.0.is_empty() {
            Some("CLI 错误通道有输出；为保护凭据和学习文本，原始内容未展示。")
        } else {
            None
        }
    }
}

struct Process {
    child: Child,
    pid: u32,
    reaped: bool,
}
impl Process {
    async fn reap(&mut self) -> Result<(), String> {
        #[cfg(unix)]
        {
            // A fresh process group contains only this invocation and its descendants.
            unsafe {
                libc::kill(-(self.pid as i32), libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        {
            let _ = tokio::time::timeout(
                Duration::from_secs(3),
                Command::new("taskkill")
                    .args(["/PID", &self.pid.to_string(), "/T", "/F"])
                    .output(),
            )
            .await;
        }
        let _ = self.child.start_kill();
        tokio::time::timeout(Duration::from_secs(3), self.child.wait())
            .await
            .map_err(|_| "Claude Code 进程未能及时回收。")?
            .map_err(|_| "Claude Code 进程回收失败。")?;
        self.reaped = true;
        Ok(())
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        #[cfg(unix)]
        if !self.reaped {
            unsafe {
                libc::kill(-(self.pid as i32), libc::SIGKILL);
            }
        }
    }
}

pub async fn generate<F, Fut>(
    turn: &ApiTurn,
    credential: &Credential,
    request: &Request,
    output: &mut Output,
    cancel: watch::Receiver<bool>,
    changed: F,
) -> Result<(), String>
where
    F: FnMut(&Output) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    check(&turn.profile).await?;
    if *cancel.borrow() {
        return Err("已停止回复。".into());
    }
    run(
        command(turn, credential, request)?,
        turn,
        output,
        cancel,
        changed,
    )
    .await
}

async fn run<F, Fut>(
    mut command: Command,
    turn: &ApiTurn,
    output: &mut Output,
    mut cancel: watch::Receiver<bool>,
    mut changed: F,
) -> Result<(), String>
where
    F: FnMut(&Output) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let child = command.spawn().map_err(|_| "无法启动所选 Claude Code。")?;
    let mut process = Process {
        pid: child.id().ok_or("Claude Code 进程标识不可用。")?,
        child,
        reaped: false,
    };
    let mut stdin = process
        .child
        .stdin
        .take()
        .ok_or("Claude Code 输入通道不可用。")?;
    let mut stdout = process
        .child
        .stdout
        .take()
        .ok_or("Claude Code 输出通道不可用。")?;
    let mut stderr = process
        .child
        .stderr
        .take()
        .ok_or("Claude Code 错误通道不可用。")?;
    // Retain a bounded diagnostic tail and expose only controlled categories.
    let diagnostic = std::sync::Arc::new(std::sync::Mutex::new(Diagnostic::default()));
    let captured = diagnostic.clone();
    let mut diagnostics = tokio::spawn(async move {
        let mut bytes = [0; 4096];
        while let Ok(n) = stderr.read(&mut bytes).await {
            if n == 0 {
                break;
            }
            captured.lock().unwrap().push(&bytes[..n]);
        }
    });
    let run = async {
        stdin
            .write_all(turn.input.as_bytes())
            .await
            .map_err(|_| "Claude Code 输入已断开。")?;
        stdin
            .shutdown()
            .await
            .map_err(|_| "Claude Code 输入已断开。")?;
        drop(stdin);
        let mut protocol = Protocol::default();
        let mut pending = Vec::new();
        let mut bytes = [0; 8192];
        let mut total = 0;
        let mut dirty = false;
        let mut next_flush = tokio::time::Instant::now();
        loop {
            let n = tokio::select! {
                biased;
                _ = tokio::time::sleep_until(next_flush), if dirty => {
                    changed(output).await?;
                    dirty = false;
                    next_flush = tokio::time::Instant::now() + Duration::from_millis(50);
                    continue;
                }
                n = stdout.read(&mut bytes) => n.map_err(|_| "Claude Code 输出已断开。")?,
            };
            if n == 0 {
                break;
            }
            total += n;
            if total > http::MAX_RESPONSE {
                return Err("Claude Code 输出超出大小限制。".into());
            }
            pending.extend_from_slice(&bytes[..n]);
            while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                let line: Vec<_> = pending.drain(..=end).collect();
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                let event =
                    serde_json::from_slice(&line).map_err(|_| "Claude Code 返回无效 JSONL。")?;
                let before = output.text.len();
                protocol.accept(event, &turn.id, output)?;
                dirty |= output.text.len() != before;
            }
            if pending.len() > 2 * 1024 * 1024 {
                return Err("Claude Code 单条事件过大。".into());
            }
        }
        if !pending.iter().all(u8::is_ascii_whitespace) {
            return Err("Claude Code 输出在事件中途断开。".into());
        }
        if dirty {
            changed(output).await?;
        }
        let status = tokio::time::timeout(Duration::from_secs(3), process.child.wait())
            .await
            .map_err(|_| "Claude Code 输出结束后未退出。")?
            .map_err(|_| "无法读取 Claude Code 退出状态。")?;
        if !status.success() || !protocol.result {
            return Err(format!(
                "Claude Code 已退出（{status}），未收到成功的最终结果。已保留部分回复。"
            ));
        }
        output.complete = true;
        Ok(())
    };
    let result = tokio::select! {
        biased;
        _ = async { if !*cancel.borrow() { let _ = cancel.changed().await; } } => Err("已停止回复。".into()),
        result = tokio::time::timeout(Duration::from_secs(600), run) => result.unwrap_or_else(|_| Err("Claude Code 回复超时，已停止。".into())),
    };
    let cleanup = process.reap().await;
    if cleanup.is_err() {
        output.complete = false;
    }
    if tokio::time::timeout(Duration::from_millis(250), &mut diagnostics)
        .await
        .is_err()
    {
        diagnostics.abort();
        let _ = diagnostics.await;
    }
    cleanup.and(result).map_err(|error| {
        diagnostic
            .lock()
            .unwrap()
            .summary()
            .map_or(error.clone(), |detail| format!("{error} {detail}"))
    })
}

#[cfg(test)]
#[path = "claude_tests.rs"]
pub(crate) mod tests;
