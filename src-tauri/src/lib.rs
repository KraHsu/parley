mod backends;
mod chat;
mod codex;
mod exchange;
mod launcher;
mod review;
mod storage;
mod terminal;
mod vocabulary;
pub use launcher::run_cli;
use tauri::Manager;
mod commands;

pub fn run() {
    let options = launcher::LaunchOptions::from_environment();
    tauri::Builder::default()
        .manage(options)
        .manage(backends::manager::BackendState::default())
        .manage(exchange::ImportState::default())
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
            let terminal = app
                .state::<launcher::LaunchOptions>()
                .terminal_ready
                .as_ref()
                .map(|path| terminal::TerminalState::start(path))
                .transpose();
            match terminal {
                Ok(value) => {
                    app.manage(value.unwrap_or_default());
                }
                Err(_) => {
                    app.manage(terminal::TerminalState::default());
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_runtime_info,
            backends::backend_profiles,
            backends::backend_save_profile,
            backends::manager::backend_credential_status,
            backends::manager::backend_set_credential,
            backends::manager::backend_remove_credential,
            backends::manager::backend_models,
            backends::manager::backend_send,
            backends::manager::backend_stop,
            backends::manager::backend_disconnect,
            launcher::get_launch_options,
            terminal::claude_terminal_context,
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
            storage::storage_delete,
            vocabulary::vocabulary_list,
            vocabulary::vocabulary_get,
            vocabulary::vocabulary_save,
            vocabulary::vocabulary_add_occurrence,
            vocabulary::vocabulary_trash,
            vocabulary::vocabulary_restore,
            vocabulary::vocabulary_purge,
            vocabulary::vocabulary_save_draft,
            vocabulary::vocabulary_load_drafts,
            vocabulary::vocabulary_discard_draft,
            review::vocabulary_card_save,
            review::vocabulary_review_queue,
            review::vocabulary_review_grade,
            review::vocabulary_review_undo,
            review::vocabulary_tags_save,
            exchange::vocabulary_export,
            exchange::vocabulary_preview_import,
            exchange::vocabulary_import,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Parley")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                app.state::<backends::manager::BackendState>().shutdown();
            }
        });
}
