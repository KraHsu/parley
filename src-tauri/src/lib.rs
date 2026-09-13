mod codex;
mod storage;
use tauri::Manager;
mod commands;

pub fn run() {
    tauri::Builder::default()
        .manage(codex::CodexState::default())
        .setup(|app| {
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
            codex::codex_connect,
            codex::codex_disconnect,
            codex::codex_status,
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
