#![cfg(target_os = "linux")]
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn script(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}
fn wait_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(Instant::now() < deadline, "missing {}", path.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn reopen_gui_keeps_native_pid_and_collects_events_while_window_is_closed() {
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("native cli");
    let gui = dir.path().join("grammar gui");
    let launcher = env!("CARGO_BIN_EXE_parley-cli");
    script(
        &gui,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$PARLEY_TEST_GUI_ARGS\"\n",
    );
    script(
        &native,
        r#"#!/bin/sh
printf '%s' "$$" > "$PARLEY_TEST_PID"
printf '%s' '{"session_id":"fixture","hook_event_name":"UserPromptSubmit","prompt":"Bonjour?"}' | "$PARLEY_TEST_LAUNCHER" --terminal-event "$2"
printf '%s' ready > "$PARLEY_TEST_READY"
read -r reply
printf '%s' '{"session_id":"fixture","hook_event_name":"Stop","last_assistant_message":"Bonjour!"}' | "$PARLEY_TEST_LAUNCHER" --terminal-event "$2"
printf '%s' finished > "$PARLEY_TEST_DONE"
read -r finish
exit 0
"#,
    );
    let command = || {
        let mut c = Command::new(launcher);
        c.current_dir(dir.path())
            .env("XDG_CACHE_HOME", dir.path().join("cache"))
            .env("XDG_DATA_HOME", dir.path().join("data"))
            .env("PARLEY_TEST_GUI_ARGS", dir.path().join("gui-args"))
            .env("PARLEY_TEST_PID", dir.path().join("pid"))
            .env("PARLEY_TEST_READY", dir.path().join("ready"))
            .env("PARLEY_TEST_DONE", dir.path().join("done"))
            .env("PARLEY_TEST_LAUNCHER", launcher);
        c
    };
    let mut child = command()
        .args(["--backend", "claude-code", "--claude"])
        .arg(&native)
        .arg("--gui-bin")
        .arg(&gui)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_file(&dir.path().join("ready"));
    wait_file(&dir.path().join("gui-args"));
    let pid = std::fs::read_to_string(dir.path().join("pid")).unwrap();
    let args = std::fs::read_to_string(dir.path().join("gui-args")).unwrap();
    let directory = args
        .lines()
        .skip_while(|line| *line != "--terminal-session")
        .nth(1)
        .unwrap();
    let context = Path::new(directory).join("context.json");
    assert!(
        std::fs::read_to_string(&context)
            .unwrap()
            .contains("Bonjour?")
    );
    let list = command().arg("--list-companions").output().unwrap();
    assert!(String::from_utf8_lossy(&list.stdout).contains("运行中"));
    let id = Path::new(directory).file_name().unwrap();
    assert!(
        !command()
            .args(["--clean-companions", "--session"])
            .arg(id)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(context.is_file());
    // The fake GUI has exited. The same native CLI emits another hook event.
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    wait_file(&dir.path().join("done"));
    assert!(
        std::fs::read_to_string(&context)
            .unwrap()
            .contains("Bonjour!")
    );
    std::fs::remove_file(dir.path().join("gui-args")).unwrap();
    assert!(
        command()
            .arg("--reconnect")
            .arg("--gui-bin")
            .arg(&gui)
            .status()
            .unwrap()
            .success()
    );
    wait_file(&dir.path().join("gui-args"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("gui-args")).unwrap(),
        args
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("pid")).unwrap(),
        pid
    );
    assert!(child.try_wait().unwrap().is_none());
    child.stdin.as_mut().unwrap().write_all(b"exit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("连接"));
    let list = command().arg("--list-companions").output().unwrap();
    assert!(String::from_utf8_lossy(&list.stdout).contains("已结束"));
    // Implicit reconnect must never silently select an ended session.
    assert!(
        !command()
            .arg("--reconnect")
            .arg("--gui-bin")
            .arg(&gui)
            .output()
            .unwrap()
            .status
            .success()
    );
    // Explicit historical browsing is still available.
    assert!(
        command()
            .arg("--reconnect")
            .arg("--session")
            .arg(id)
            .arg("--gui-bin")
            .arg(&gui)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        command()
            .arg("--clean-companions")
            .status()
            .unwrap()
            .success()
    );
    assert!(!Path::new(directory).exists());
}
