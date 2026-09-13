//! Launch the user's unchanged Codex TUI alongside the grammar GUI.
use crate::storage::Preferences;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
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

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    pub tutor_only: bool,
    pub codex_path: Option<String>,
    pub terminal_cwd: Option<String>,
    pub terminal_started_at: u64,
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
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let value: Option<String> = db
        .query_row("SELECT value FROM preferences WHERE id=1", [], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| format!("读取 Codex 路径失败：{e}"))?;
    let preferences: Preferences =
        serde_json::from_str(value.as_deref().unwrap_or("{}")).map_err(|e| e.to_string())?;
    if preferences.codex_path.trim().is_empty() {
        return Err("请先在 Parley 设置中填写 Codex 路径，或使用 --codex /完整路径/codex。".into());
    }
    Ok(PathBuf::from(preferences.codex_path.trim()))
}
fn validate_binary(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || !path.is_file() {
        return Err(format!(
            "请指定存在的 Codex 可执行文件绝对路径：{}",
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
fn start_gui(gui: &Path, codex: &Path, cwd: &Path) -> Result<(), String> {
    if !gui.is_file() {
        return Err(format!(
            "未找到语法助手程序：{}。请先运行 npm run cli:build。",
            gui.display()
        ));
    }
    let mut command = Command::new(gui);
    command
        .arg("--tutor-only")
        .arg("--codex-path")
        .arg(codex)
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
        .map_err(|e| format!("无法启动语法助手：{e}"))?;
    Ok(())
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
    let codex = match options.codex {
        Some(path) => path,
        None => saved_codex(&data_file()?)?,
    };
    validate_binary(&codex)?;
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    if codex.canonicalize().ok() == current.canonicalize().ok() {
        return Err("Codex 路径不能指向 Parley 启动器自身。".into());
    }
    if !options.no_gui {
        let already_running = data_file().is_ok_and(|path| gui_running(&path));
        if already_running {
            return Err("Parley 工作区已经打开。请先正常关闭已有窗口，再启动终端伴随模式；只启动 Codex 可使用 --no-gui。".into());
        }
        {
            let gui = options.gui.unwrap_or_else(|| {
                current.with_file_name(if cfg!(windows) {
                    "parley.exe"
                } else {
                    "parley"
                })
            });
            start_gui(&gui, &codex, &terminal_cwd(&options.args)?)?;
        }
    }
    let mut command = codex_command(&codex);
    // No prompt/config injection, cwd change, pipe, token handling, or output parsing.
    command
        .args(options.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(format!("无法启动 Codex：{}", command.exec()))
    }
    #[cfg(not(unix))]
    {
        let status = command
            .status()
            .map_err(|e| format!("无法启动 Codex：{e}"))?;
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
            "Parley — 官方 Codex TUI + 语法助手 GUI\n\n用法：parley-cli [--codex PATH] [--no-gui] [--gui-bin PATH] [-- CODEX_ARGS...]\n\n默认读取 Parley 设置中的 Codex 路径，并打开语法助手窗口。\nCodex 参数从 -- 之后或第一个非 Parley 参数开始原样传递。\n\n示例：\n  parley-cli\n  parley-cli --codex /path/to/codex\n  parley-cli -- resume --last\n  parley-cli -- -m MODEL\n  parley-cli --no-gui -- --help\n\n关闭语法窗口不会结束终端会话；退出 Codex 后仍可继续使用语法窗口。"
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
        let output = codex_command(&binary).output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"runtime found\n");
        std::fs::remove_dir_all(dir).unwrap();
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
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE preferences(id INTEGER PRIMARY KEY, value TEXT)")
            .unwrap();
        let value = r#"{"codexPath":"/My Tools/codex","targetLanguage":"ja"}"#;
        db.execute("INSERT INTO preferences VALUES(1, ?1)", [value])
            .unwrap();
        assert_eq!(
            saved_codex(&path).unwrap(),
            PathBuf::from("/My Tools/codex")
        );
        assert_eq!(
            db.query_row("SELECT value FROM preferences", [], |row| row
                .get::<_, String>(0))
                .unwrap(),
            value
        );
        drop(db);
        std::fs::remove_file(path).unwrap();
    }
}
