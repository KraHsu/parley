//! Public backend configuration. Credentials belong to a separate secret store.
pub mod credentials;
pub mod http;
pub mod manager;
pub mod sse;
pub mod types;

use crate::storage::StorageState;
use tauri::State;
use types::{BackendProfile, SaveProfile};

#[tauri::command]
pub async fn backend_profiles(
    storage: State<'_, StorageState>,
) -> Result<Vec<BackendProfile>, String> {
    let storage = storage.get()?;
    tauri::async_runtime::spawn_blocking(move || storage.backend_profiles())
        .await
        .map_err(|_| "读取模型服务配置失败。".to_owned())?
}

#[tauri::command]
pub async fn backend_save_profile(
    storage: State<'_, StorageState>,
    request: SaveProfile,
) -> Result<BackendProfile, String> {
    let storage = storage.get()?;
    tauri::async_runtime::spawn_blocking(move || storage.save_backend_profile(request))
        .await
        .map_err(|_| "保存模型服务配置失败。".to_owned())?
}
