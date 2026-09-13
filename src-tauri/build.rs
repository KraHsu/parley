fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["get_runtime_info"])),
    )
    .expect("failed to generate Tauri build metadata");
}
