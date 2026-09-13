mod codex;
mod launcher;
mod storage;
pub use launcher::run_cli;
use tauri::Manager;
mod commands;

pub fn run() {
    let options = launcher::LaunchOptions::from_environment();
    tauri::Builder::default()
        .manage(options)
        .manage(codex::CodexState::default())
        .setup(|app| {
            if app.state::<launcher::LaunchOptions>().tutor_only {
                let window = app
                    .get_webview_window("main")
                    .ok_or("Missing main window")?;
                window.set_title("Parley · 语法助手")?;
                window.set_min_size(Some(tauri::LogicalSize::new(420.0, 540.0)))?;
                window.set_size(tauri::LogicalSize::new(520.0, 820.0))?;
            }
            let path = app
                .path()
                .app_local_data_dir()
                .map(|dir| dir.join("parley.sqlite3"))
                .map_err(|e| e.to_string());
            app.manage(storage::StorageState::new(path));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_runtime_info,
            launcher::get_launch_options,
            codex::codex_connect,
            codex::codex_disconnect,
            codex::codex_status,
            codex::codex_terminal_context,
            codex::codex_login,
            codex::codex_cancel_login,
            codex::codex_open_login,
            codex::codex_send,
            codex::codex_stop,
            codex::codex_reset,
            storage::storage_load,
            storage::storage_save,
            storage::storage_create,
            storage::storage_list,
            storage::storage_read,
            storage::storage_delete
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Parley")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                app.state::<codex::CodexState>().shutdown();
            }
        });
}
