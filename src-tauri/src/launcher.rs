//! Launch the user's official native CLI alongside the grammar GUI.
use crate::storage::Preferences;
use crate::storage::schema::preferences as pref;
use diesel::prelude::*;
use serde::Serialize;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(crate) fn codex_command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    // Desktop launchers don't load shell startup files. Keep an npm-installed
    // Codex shim able to find the Node runtime beside the selected executable.
    if let Some(parent) = binary.parent() {
        let mut paths = vec![parent.to_path_buf()];
        if let Some(inherited) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&inherited));
        }
        if let Ok(path) = std::env::join_paths(paths) {
            command.env("PATH", path);
        }
    }
    command
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalBackend {
    #[default]
    Codex,
    ClaudeCode,
}
impl TerminalBackend {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "codex" => Ok(Self::Codex),
            "claude-code" => Ok(Self::ClaudeCode),
            _ => Err("--backend 只能选择 codex 或 claude-code。".into()),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
        }
    }
}

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    pub tutor_only: bool,
    pub codex_path: Option<String>,
    pub terminal_cwd: Option<String>,
    pub terminal_started_at: u64,
    pub terminal_backend: TerminalBackend,
    #[serde(skip)]
    pub terminal_ready: Option<PathBuf>,
}
impl LaunchOptions {
    pub fn from_environment() -> Self {
        let mut result = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--tutor-only" => result.tutor_only = true,
                "--codex-path" => result.codex_path = args.next(),
                "--terminal-cwd" => result.terminal_cwd = args.next(),
                "--terminal-backend" => {
                    result.terminal_backend = args
                        .next()
                        .and_then(|v| TerminalBackend::parse(&v).ok())
                        .unwrap_or_default()
                }
                "--terminal-ready" => result.terminal_ready = args.next().map(PathBuf::from),
                "--terminal-started-at" => {
                    result.terminal_started_at =
                        args.next().and_then(|v| v.parse().ok()).unwrap_or(0)
                }
                _ => {}
            }
        }
        result
    }
}
#[tauri::command]
pub fn get_launch_options(options: tauri::State<'_, LaunchOptions>) -> LaunchOptions {
    options.inner().clone()
}

#[derive(Debug, Default)]
struct CliOptions {
    backend: TerminalBackend,
    claude: Option<PathBuf>,
    codex: Option<PathBuf>,
    gui: Option<PathBuf>,
    no_gui: bool,
    help: bool,
    args: Vec<OsString>,
}
fn parse(args: impl IntoIterator<Item = OsString>) -> Result<CliOptions, String> {
    let mut options = CliOptions::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--backend") => {
                options.backend = TerminalBackend::parse(
                    args.next()
                        .ok_or("--backend 需要后端名称")?
                        .to_str()
                        .ok_or("后端名称无效")?,
                )?
            }
            Some("--claude") => {
                options.claude = Some(args.next().ok_or("--claude 需要绝对路径")?.into())
            }
            Some("--codex") => {
                options.codex = Some(args.next().ok_or("--codex 需要绝对路径")?.into())
            }
            Some("--gui-bin") => {
                options.gui = Some(args.next().ok_or("--gui-bin 需要路径")?.into())
            }
            Some("--no-gui") => options.no_gui = true,
            Some("--help" | "-h") => options.help = true,
            Some("--") => {
                options.args.extend(args);
                break;
            }
            _ => {
                options.args.push(arg);
                options.args.extend(args);
                break;
            }
        }
    }
    if (options.backend == TerminalBackend::Codex && options.claude.is_some())
        || (options.backend == TerminalBackend::ClaudeCode && options.codex.is_some())
    {
        return Err(
            "CLI 路径参数与 --backend 不匹配。Claude 请使用 --backend claude-code --claude PATH。"
                .into(),
        );
    }
    Ok(options)
}
fn data_file() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .map(|dir| dir.join("org.parley.desktop/parley.sqlite3"))
        .ok_or_else(|| "无法确定 Parley 数据目录，请使用 --codex 指定路径。".into())
}
fn saved_codex(path: &Path) -> Result<PathBuf, String> {
    if !path.is_file() {
        return Err("请先在 Parley 设置中填写 Codex 路径，或使用 --codex /完整路径/codex。".into());
    }
    // Read existing settings without migrations, writer locks or changing the workspace.
    let absolute = path.canonicalize().map_err(|e| e.to_string())?;
    let mut uri = url::Url::from_file_path(absolute).map_err(|_| "数据路径无效。")?;
    uri.set_query(Some("mode=ro"));
    let mut db = SqliteConnection::establish(uri.as_str()).map_err(|e| e.to_string())?;
    let value = pref::table
        .find(1_i64)
        .select(pref::value)
        .first::<String>(&mut db)
        .optional()
        .map_err(|e| format!("读取 Codex 路径失败：{e}"))?;
    let preferences: Preferences =
        serde_json::from_str(value.as_deref().unwrap_or("{}")).map_err(|e| e.to_string())?;
    if preferences.codex_path.trim().is_empty() {
        return Err("请先在 Parley 设置中填写 Codex 路径，或使用 --codex /完整路径/codex。".into());
    }
    Ok(PathBuf::from(preferences.codex_path.trim()))
}
fn saved_claude(path: &Path) -> Result<PathBuf, String> {
    use crate::{
        backends::types::{BackendKind, ProfileConfig},
        storage::schema::backend_profiles as profiles,
    };
    let absolute = path
        .canonicalize()
        .map_err(|_| "请使用 --claude 指定 Claude Code 路径，或先在设置中保存配置。")?;
    let mut uri = url::Url::from_file_path(absolute).map_err(|_| "数据路径无效")?;
    uri.set_query(Some("mode=ro"));
    let mut db = SqliteConnection::establish(uri.as_str())
        .map_err(|_| "无法读取后端配置，请使用 --claude 指定路径。")?;
    let rows = profiles::table
        .select(profiles::config)
        .load::<String>(&mut db)
        .map_err(|_| "无法读取后端配置，请使用 --claude 指定路径。")?;
    let mut paths = rows
        .into_iter()
        .filter_map(|v| serde_json::from_str::<ProfileConfig>(&v).ok())
        .filter(|p| p.kind == BackendKind::ClaudeCode && p.enabled && !p.binary_path.is_empty())
        .map(|p| PathBuf::from(p.binary_path))
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    if paths.len() != 1 {
        return Err(
            "未找到唯一的 Claude Code 路径，请使用 --claude /完整路径/claude 指定。".into(),
        );
    }
    Ok(paths.remove(0))
}

fn validate_binary(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || !path.is_file() {
        return Err(format!(
            "请指定存在的 CLI 可执行文件绝对路径：{}",
            path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path
            .metadata()
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            return Err(format!("文件没有执行权限：{}", path.display()));
        }
    }
    Ok(())
}
fn gui_running(path: &Path) -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .is_ok_and(|file| file.try_lock().is_err())
}
fn start_gui(
    gui: &Path,
    binary: &Path,
    cwd: &Path,
    backend: TerminalBackend,
    ready: Option<&Path>,
) -> Result<std::process::Child, String> {
    if !gui.is_file() {
        return Err(format!(
            "未找到语法助手程序：{}。请先运行 npm run cli:build。",
            gui.display()
        ));
    }
    let mut command = Command::new(gui);
    command
        .arg("--tutor-only")
        .arg("--terminal-backend")
        .arg(backend.key())
        .arg("--terminal-cwd")
        .arg(cwd)
        .arg("--terminal-started-at")
        .arg(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .to_string(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if backend == TerminalBackend::Codex {
        command.arg("--codex-path").arg(binary);
    }
    if let Some(ready) = ready {
        command.arg("--terminal-ready").arg(ready);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000200 | 0x00000008);
    }
    command
        .spawn()
        .map_err(|e| format!("无法启动语法助手：{e}"))
}
fn terminal_cwd(args: &[OsString]) -> Result<PathBuf, String> {
    let mut cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        if arg == "-C" || arg == "--cd" {
            if let Some(path) = args.next() {
                cwd = cwd.join(path);
            }
        } else if let Some(path) = arg.to_str().and_then(|a| {
            a.strip_prefix("--cd=")
                .or_else(|| a.strip_prefix("-C").filter(|path| !path.is_empty()))
        }) {
            cwd = cwd.join(path);
        }
    }
    Ok(cwd.canonicalize().unwrap_or(cwd))
}
fn run(options: CliOptions) -> Result<i32, String> {
    let backend = options.backend;
    let binary = match backend {
        TerminalBackend::Codex => options
            .codex
            .clone()
            .map(Ok)
            .unwrap_or_else(|| saved_codex(&data_file()?)),
        TerminalBackend::ClaudeCode => options
            .claude
            .clone()
            .map(Ok)
            .unwrap_or_else(|| saved_claude(&data_file()?)),
    }?;
    validate_binary(&binary)?;
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    if binary.canonicalize().ok() == current.canonicalize().ok() {
        return Err("CLI 路径不能指向 Parley 启动器自身。".into());
    }
    let mut plugin = None;
    if !options.no_gui {
        if data_file().is_ok_and(|path| gui_running(&path)) {
            return Err("Parley 工作区已经打开。请先正常关闭已有窗口，再启动终端伴随模式；只启动官方 CLI 可使用 --no-gui。".into());
        }
        let gui = options.gui.unwrap_or_else(|| {
            current.with_file_name(if cfg!(windows) {
                "parley.exe"
            } else {
                "parley"
            })
        });
        let cwd = if backend == TerminalBackend::Codex {
            terminal_cwd(&options.args)?
        } else {
            std::env::current_dir().map_err(|e| e.to_string())?
        };
        let ready = if backend == TerminalBackend::ClaudeCode {
            Some(
                tempfile::Builder::new()
                    .prefix("parley-ready-")
                    .tempfile()
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let mut child = start_gui(
            &gui,
            &binary,
            &cwd,
            backend,
            ready.as_ref().map(|f| f.path()),
        )?;
        if let Some(ready) = ready {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            while std::time::Instant::now() < deadline {
                if let Ok(bytes) = std::fs::read(ready.path())
                    && bytes.len() <= 8192
                    && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
                {
                    plugin = value["pluginPath"]
                        .as_str()
                        .map(PathBuf::from)
                        .filter(|p| p.is_absolute() && p.join("hooks/hooks.json").is_file());
                    if plugin.is_some() {
                        break;
                    }
                }
                if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            let _ = ready.close();
            if plugin.is_none() {
                eprintln!(
                    "Parley 自动同步接收器未就绪；Claude Code 仍可正常使用。请检查语法窗口中的同步状态。"
                );
            }
        }
    }
    let mut command = codex_command(&binary);
    if let Some(plugin) = plugin {
        command.arg("--plugin-dir").arg(plugin);
    }
    // The official CLI owns the terminal, login, settings, signals and native UI.
    command
        .args(options.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(format!("无法启动 {}：{}", backend.name(), command.exec()))
    }
    #[cfg(not(unix))]
    {
        let status = command
            .status()
            .map_err(|e| format!("无法启动 {}：{e}", backend.name()))?;
        Ok(status.code().unwrap_or(1))
    }
}
pub fn run_cli() -> i32 {
    let options = match parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("Parley: {error}");
            return 2;
        }
    };
    if options.help {
        println!(
            "Parley — 官方 Codex / Claude Code TUI + 语法助手 GUI\n\n用法：parley-cli [--backend codex|claude-code] [--codex PATH|--claude PATH] [--no-gui] [--gui-bin PATH] [-- NATIVE_ARGS...]\n\n默认运行 Codex。CLI 路径读取设置，或通过参数显式指定。原生参数从 -- 之后或第一个非 Parley 参数开始原样传递。\n\n示例：\n  parley-cli\n  parley-cli --codex /path/to/codex -- resume --last\n  parley-cli --backend claude-code --claude /path/to/claude -- --resume SESSION_ID\n  parley-cli --backend claude-code --no-gui -- --help\n\nClaude 自动同步通过本次启动的局部 hooks 插件接收新轮次。关闭语法窗口不会结束终端会话。"
        );
        return 0;
    }
    match run(options) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Parley: {error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::connection::SimpleConnection;
    #[test]
    #[cfg(unix)]
    fn selected_shim_finds_its_sibling_runtime_without_shell_startup() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("parley-runtime-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("codex");
        let runtime = dir.join("parley-test-runtime");
        std::fs::write(&binary, "#!/usr/bin/env parley-test-runtime\n").unwrap();
        std::fs::write(&runtime, "#!/bin/sh\nprintf 'runtime found\\n'\n").unwrap();
        for path in [&binary, &runtime] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // Parallel subprocess tests can briefly inherit a just-written script's
        // descriptor before exec closes it, producing ETXTBSY on Linux. No child
        // starts in this case; wait only for that transient fixture condition.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        let output = loop {
            match codex_command(&binary).output() {
                Err(error)
                    if error.raw_os_error() == Some(libc::ETXTBSY)
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                result => break result.unwrap(),
            }
        };
        assert!(output.status.success());
        assert_eq!(output.stdout, b"runtime found\n");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn saved_claude_requires_one_distinct_enabled_binary() {
        use crate::{
            backends::types::{BackendKind, ProfileConfig, Provider, SaveProfile},
            storage::Storage,
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.sqlite3");
        let storage = Storage::open(&path).unwrap();
        assert!(saved_claude(&path).is_err());
        let first = dir.path().join("claude").to_str().unwrap().to_owned();
        let second = dir.path().join("other-claude").to_str().unwrap().to_owned();
        for (name, binary, enabled) in [
            ("one", first.as_str(), true),
            ("same", first.as_str(), true),
            ("disabled", second.as_str(), false),
        ] {
            storage
                .save_backend_profile(SaveProfile {
                    id: None,
                    expected_revision: None,
                    config: ProfileConfig {
                        name: name.into(),
                        kind: BackendKind::ClaudeCode,
                        provider: Provider::Anthropic,
                        endpoint: "".into(),
                        binary_path: binary.into(),
                        enabled,
                    },
                })
                .unwrap();
        }
        assert_eq!(saved_claude(&path).unwrap(), PathBuf::from(first));
        storage
            .save_backend_profile(SaveProfile {
                id: None,
                expected_revision: None,
                config: ProfileConfig {
                    name: "other".into(),
                    kind: BackendKind::ClaudeCode,
                    provider: Provider::Anthropic,
                    endpoint: "".into(),
                    binary_path: second,
                    enabled: true,
                },
            })
            .unwrap();
        assert!(saved_claude(&path).is_err());
    }

    #[test]
    fn backend_selection_preserves_native_settings_and_arguments() {
        let options = parse(
            [
                "--backend",
                "claude-code",
                "--claude",
                "/path/claude",
                "--",
                "--settings",
                "/my/settings.json",
                "--plugin-dir",
                "/existing/plugin",
                "--resume",
                "explicit-session",
                "prompt `$(literal)`",
            ]
            .map(OsString::from),
        )
        .unwrap();
        assert_eq!(options.backend, TerminalBackend::ClaudeCode);
        assert_eq!(options.claude, Some(PathBuf::from("/path/claude")));
        assert_eq!(
            options.args,
            [
                "--settings",
                "/my/settings.json",
                "--plugin-dir",
                "/existing/plugin",
                "--resume",
                "explicit-session",
                "prompt `$(literal)`"
            ]
            .map(OsString::from)
        );
        assert!(parse(["--backend", "unknown"].map(OsString::from)).is_err());
        assert!(
            parse(["--backend", "claude-code", "--codex", "/path/codex"].map(OsString::from))
                .is_err()
        );
    }

    #[test]
    fn arguments_are_forwarded_without_shell_interpretation_or_config_injection() {
        let options = parse(
            [
                "--codex",
                "/My Tools/codex",
                "--",
                "resume",
                "--last",
                "$(literal); hello",
                "--help",
            ]
            .map(OsString::from),
        )
        .unwrap();
        assert_eq!(options.codex, Some(PathBuf::from("/My Tools/codex")));
        assert!(!options.help);
        assert_eq!(
            options.args,
            ["resume", "--last", "$(literal); hello", "--help"].map(OsString::from)
        );
        assert!(parse([OsString::from("--codex")]).is_err());
    }
    #[test]
    fn native_commands_work_without_a_separator() {
        let options = parse(["resume", "--last", "--help"].map(OsString::from)).unwrap();
        assert!(!options.help);
        assert_eq!(
            options.args,
            ["resume", "--last", "--help"].map(OsString::from)
        );
    }
    #[test]
    fn reads_user_selection_without_changing_preferences() {
        let path =
            std::env::temp_dir().join(format!("parley-launcher-{}.sqlite3", std::process::id()));
        let mut db = SqliteConnection::establish(path.to_str().unwrap()).unwrap();
        db.batch_execute("CREATE TABLE preferences(id INTEGER PRIMARY KEY, value TEXT)")
            .unwrap();
        let value = r#"{"codexPath":"/My Tools/codex","targetLanguage":"ja"}"#;
        diesel::insert_into(pref::table)
            .values((pref::id.eq(1_i64), pref::value.eq(value)))
            .execute(&mut db)
            .unwrap();
        assert_eq!(
            saved_codex(&path).unwrap(),
            PathBuf::from("/My Tools/codex")
        );
        assert_eq!(
            pref::table
                .select(pref::value)
                .first::<String>(&mut db)
                .unwrap(),
            value
        );
        drop(db);
        std::fs::remove_file(path).unwrap();
    }
}
