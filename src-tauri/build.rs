fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_runtime_info",
            "codex_connect",
            "codex_disconnect",
            "codex_status",
            "codex_login",
            "codex_cancel_login",
            "codex_open_login",
            "codex_send",
            "codex_stop",
            "codex_reset",
            "storage_load",
            "storage_save",
            "storage_create",
            "storage_list",
            "storage_read",
            "storage_delete",
        ]),
    ))
    .expect("failed to generate Tauri build metadata");
}
