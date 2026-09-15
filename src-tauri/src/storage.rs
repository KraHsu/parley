pub mod api;
pub mod backends;
mod context;
pub mod exchange;
mod learning_rows;
pub mod review;
pub(crate) mod schema;
mod sources;
mod typed;
pub mod vocabulary;
mod workspace;
use crate::backends::types::{ConversationBackend, DEFAULT_CODEX_PROFILE};
pub use context::ContextMessage;
use diesel::{Connection, RunQueryDsl, SqliteConnection, connection::SimpleConnection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::State;

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/001_workspace.sql"),
    include_str!("../migrations/002_vocabulary.sql"),
    include_str!("../migrations/003_vocabulary_review.sql"),
    include_str!("../migrations/004_model_backends.sql"),
    include_str!("../migrations/005_api_turns.sql"),
    include_str!("../migrations/006_source_backends.sql"),
    include_str!("../migrations/007_conversation_context.sql"),
    include_str!("../migrations/008_credential_identity.sql"),
    include_str!("../migrations/009_learning_exchange.sql"),
];

#[derive(Clone)]
pub struct StorageState {
    path: std::result::Result<PathBuf, String>,
    value: Arc<Mutex<Option<Storage>>>,
}
impl StorageState {
    pub fn new(path: std::result::Result<PathBuf, String>) -> Self {
        Self {
            path,
            value: Arc::new(Mutex::new(None)),
        }
    }
    pub async fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce(Storage) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let state = self.clone();
        tauri::async_runtime::spawn_blocking(move || operation(state.get()?))
            .await
            .map_err(error)?
    }
    pub fn get(&self) -> std::result::Result<Storage, String> {
        let mut value = self.value.lock().unwrap();
        if let Some(storage) = value.as_ref() {
            return Ok(storage.clone());
        }
        let path = self.path.as_ref().map_err(Clone::clone)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(error)?;
        }
        let storage = Storage::open(path)?;
        *value = Some(storage.clone());
        Ok(storage)
    }
}
type Result<T> = std::result::Result<T, String>;
#[derive(Clone)]
pub struct Storage {
    db: Arc<Mutex<SqliteConnection>>,
    path: PathBuf,
    _lock: Option<Arc<File>>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub codex_path: String,
    pub native_language: String,
    pub target_language: String,
    pub main_model: String,
    pub tutor_model: String,
    pub main_id: Option<String>,
    pub tutor_id: Option<String>,
    pub tutor_mode: String,
    pub active_view: String,
    pub mobile_pane: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            codex_path: String::new(),
            native_language: "zh-CN".into(),
            target_language: "en".into(),
            main_model: String::new(),
            tutor_model: String::new(),
            main_id: None,
            tutor_id: None,
            tutor_mode: "express".into(),
            active_view: "conversation".into(),
            mobile_pane: "main".into(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    pub id: String,
    pub role: String,
    pub text: String,
    pub status: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub pane: String,
    pub title: String,
    pub model: String,
    pub target_language: String,
    pub native_language: String,
    pub mode: String,
    pub draft: String,
    pub thread_id: Option<String>,
    #[serde(skip_serializing)]
    pub account: Option<String>,
    pub signature: String,
    pub status: String,
    pub updated_at: i64,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub context: Vec<ContextMessage>,
    pub backend: Option<ConversationBackend>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    preferences: Preferences,
    history: Vec<Conversation>,
    main: Option<Conversation>,
    tutor: Option<Conversation>,
    path: String,
}
#[derive(Deserialize)]
pub struct Draft {
    pub id: String,
    pub text: String,
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn error(e: impl std::fmt::Display) -> String {
    format!("本地数据操作失败：{e}")
}
#[derive(diesel::QueryableByName)]
struct SchemaVersion {
    #[diesel(sql_type=diesel::sql_types::BigInt)]
    user_version: i64,
}
fn schema_version(db: &mut SqliteConnection) -> Result<i64> {
    diesel::sql_query("PRAGMA user_version")
        .get_result::<SchemaVersion>(db)
        .map(|r| r.user_version)
        .map_err(error)
}
fn migrate(db: &mut SqliteConnection) -> Result<()> {
    let version = schema_version(db)?;
    if version < 0 || version as usize > MIGRATIONS.len() {
        return Err("数据库来自更新版本的 Parley。请升级应用；原始数据未改动。".into());
    }
    if (version as usize) < MIGRATIONS.len() {
        db.transaction::<(), typed::DbError, _>(|db| {
            for migration in &MIGRATIONS[version as usize..] {
                db.batch_execute(migration)?;
            }
            if version < 6 {
                sources::migrate(db)?;
            }
            Ok(())
        })
        .map_err(error)?;
    }
    Ok(())
}
#[cfg(test)]
pub struct ConversationConfig<'a> {
    pub model: &'a str,
    pub target: &'a str,
    pub native: &'a str,
    pub mode: &'a str,
}
impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        let lock = if path != Path::new(":memory:") {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path.with_extension("lock"))
                .map_err(error)?;
            file.try_lock()
                .map_err(|_| "学习数据正在被另一个 Parley 窗口使用，请先关闭该窗口。".to_owned())?;
            Some(Arc::new(file))
        } else {
            None
        };
        let database_url = path.to_str().ok_or("数据目录路径不是有效 UTF-8。")?;
        let mut db = typed::connect(database_url)?;
        let version = schema_version(&mut db)?;
        if path != Path::new(":memory:") && version > 0 && (version as usize) < MIGRATIONS.len() {
            // VACUUM INTO includes committed WAL contents and leaves the source untouched.
            let backup = path.with_file_name(format!(
                "parley-before-v{}-{}.sqlite3",
                MIGRATIONS.len(),
                uuid::Uuid::new_v4()
            ));
            diesel::sql_query("VACUUM INTO ?1")
                .bind::<diesel::sql_types::Text, _>(backup.to_string_lossy().as_ref())
                .execute(&mut db)
                .map_err(|_| {
                    "升级前的数据库备份失败，原始数据未迁移。请检查磁盘空间和目录权限。".to_owned()
                })?;
        }
        // Version compatibility is checked before modifying an existing database.
        migrate(&mut db)?;
        db.batch_execute(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )
        .map_err(error)?;
        let storage = Self {
            db: Arc::new(Mutex::new(db)),
            path: path.to_owned(),
            _lock: lock,
        };
        storage.interrupt_all()?;
        Ok(storage)
    }
    #[cfg(test)]
    pub fn memory() -> Self {
        Self::open(Path::new(":memory:")).unwrap()
    }
    pub fn load(&self) -> Result<Workspace> {
        let p = self.preferences()?;
        Ok(Workspace {
            main: p.main_id.as_ref().map(|id| self.read(id)).transpose()?,
            tutor: p.tutor_id.as_ref().map(|id| self.read(id)).transpose()?,
            history: self.list()?,
            preferences: p,
            path: self.path.to_string_lossy().into_owned(),
        })
    }
}
#[tauri::command]
pub async fn storage_load(storage: State<'_, StorageState>) -> Result<Workspace> {
    let s = storage.get()?;
    tauri::async_runtime::spawn_blocking(move || s.load())
        .await
        .map_err(error)?
}
#[tauri::command]
pub async fn storage_save(
    storage: State<'_, StorageState>,
    preferences: Preferences,
    drafts: Vec<Draft>,
) -> Result<()> {
    let s = storage.get()?;
    tauri::async_runtime::spawn_blocking(move || s.save(&preferences, &drafts))
        .await
        .map_err(error)?
}
#[tauri::command]
pub async fn storage_create(
    storage: State<'_, StorageState>,
    id: String,
    pane: String,
    profile_id: Option<String>,
    source_id: Option<String>,
) -> Result<Conversation> {
    let s = storage.get()?;
    tauri::async_runtime::spawn_blocking(move || {
        s.create_with_context(
            &id,
            &pane,
            profile_id.as_deref().unwrap_or(DEFAULT_CODEX_PROFILE),
            source_id.as_deref(),
        )
    })
    .await
    .map_err(error)?
}
#[tauri::command]
pub async fn storage_read(storage: State<'_, StorageState>, id: String) -> Result<Conversation> {
    let s = storage.get()?;
    tauri::async_runtime::spawn_blocking(move || s.read(&id))
        .await
        .map_err(error)?
}
#[tauri::command]
pub async fn storage_list(storage: State<'_, StorageState>) -> Result<Vec<Conversation>> {
    let s = storage.get()?;
    tauri::async_runtime::spawn_blocking(move || s.list())
        .await
        .map_err(error)?
}
#[tauri::command]
pub async fn storage_delete(storage: State<'_, StorageState>, id: String) -> Result<()> {
    let s = storage.get()?;
    tauri::async_runtime::spawn_blocking(move || s.delete(&id))
        .await
        .map_err(error)?
}

#[cfg(test)]
mod tests {
    use super::test_support::{integer, text};
    use super::*;
    use diesel::prelude::*;
    fn config() -> ConversationConfig<'static> {
        ConversationConfig {
            model: "test",
            target: "ja",
            native: "zh-CN",
            mode: "conversation",
        }
    }
    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "parley-storage-{name}-{}-{}.sqlite3",
            std::process::id(),
            now()
        ))
    }
    #[test]
    fn restart_restores_settings_drafts_messages_and_marks_partial_reply() {
        let path = path("restart");
        let codex_path = path
            .parent()
            .unwrap()
            .join("custom tools")
            .join("codex")
            .to_string_lossy()
            .into_owned();
        {
            let s = Storage::open(&path).unwrap();
            s.create("c", "main").unwrap();
            s.create("t", "tutor").unwrap();
            let p = Preferences {
                main_id: Some("c".into()),
                tutor_id: Some("t".into()),
                target_language: "ja".into(),
                main_model: "saved-model".into(),
                codex_path: codex_path.clone(),
                ..Default::default()
            };
            s.save(
                &p,
                &[Draft {
                    id: "t".into(),
                    text: "مرحبا café 日本語".into(),
                }],
            )
            .unwrap();
            s.bind(
                "c",
                "upstream-thread",
                Some("test@example.invalid"),
                "signature",
            )
            .unwrap();
            s.begin("c", "u", "こんにちは", config()).unwrap();
            s.event(
                "c",
                "item/agentMessage/delta",
                &serde_json::json!({"itemId":"a","delta":"你好 🌍"}),
            )
            .unwrap();
        }
        let s = Storage::open(&path).unwrap();
        let w = s.load().unwrap();
        assert_eq!(w.preferences.main_model, "saved-model");
        assert_eq!(w.preferences.codex_path, codex_path);
        assert_eq!(w.tutor.unwrap().draft, "مرحبا café 日本語");
        let c = w.main.unwrap();
        assert_eq!(c.thread_id.as_deref(), Some("upstream-thread"));
        assert_eq!(c.status, "interrupted");
        assert_eq!(c.messages.len(), 2);
        assert_eq!(c.messages[1].text, "你好 🌍");
        assert_eq!(c.messages[1].status, "interrupted");
    }
    #[test]
    fn final_items_are_idempotent_and_duplicate_submission_rolls_back() {
        let s = Storage::memory();
        s.create("c", "main").unwrap();
        s.begin("c", "u", "hello", config()).unwrap();
        let item = serde_json::json!({"item":{"id":"a","type":"agentMessage","text":"你好"}});
        s.event("c", "item/completed", &item).unwrap();
        s.event("c", "item/completed", &item).unwrap();
        s.event(
            "c",
            "turn/completed",
            &serde_json::json!({"turn":{"status":"completed"}}),
        )
        .unwrap();
        assert!(s.begin("c", "u", "duplicate", config()).is_err());
        let c = s.read("c").unwrap();
        assert_eq!(c.messages.len(), 2);
        assert_eq!(c.status, "idle");
        assert_eq!(c.messages[0].text, "hello");
    }
    #[test]
    fn invalid_draft_transaction_preserves_previous_preferences() {
        let s = Storage::memory();
        s.save(&Preferences::default(), &[]).unwrap();
        let p = Preferences {
            target_language: "ar".into(),
            ..Default::default()
        };
        assert!(
            s.save(
                &p,
                &[Draft {
                    id: "missing".into(),
                    text: "text".into()
                }]
            )
            .is_err()
        );
        assert_eq!(s.preferences().unwrap().target_language, "en");
    }
    #[test]
    fn deletion_cascades_messages_and_clears_selected_id_but_rejects_active_turn() {
        let s = Storage::memory();
        s.create("c", "main").unwrap();
        s.save(
            &Preferences {
                main_id: Some("c".into()),
                ..Default::default()
            },
            &[],
        )
        .unwrap();
        s.begin("c", "u", "hello", config()).unwrap();
        assert!(s.delete("c").is_err());
        s.interrupt_all().unwrap();
        s.delete("c").unwrap();
        assert!(s.preferences().unwrap().main_id.is_none());
        assert_eq!(
            schema::messages::table
                .count()
                .get_result::<i64>(&mut *s.db.lock().unwrap())
                .unwrap(),
            0
        );
    }
    #[test]
    fn future_schema_and_failed_migration_do_not_destroy_old_data() {
        let mut future = typed::connect(":memory:").unwrap();
        future.batch_execute("CREATE TABLE original(value TEXT);INSERT INTO original VALUES('preserve');PRAGMA user_version=99;").unwrap();
        assert!(migrate(&mut future).is_err());
        assert_eq!(text(&mut future, "SELECT value FROM original"), "preserve");
        let mut conflict = typed::connect(":memory:").unwrap();
        conflict
            .batch_execute("CREATE TABLE conversations(original TEXT);")
            .unwrap();
        assert!(migrate(&mut conflict).is_err());
        assert_eq!(
            integer(
                &mut conflict,
                "SELECT COUNT(*) AS value FROM sqlite_master WHERE name='preferences'"
            ),
            0
        );
    }
    #[test]
    fn two_process_handles_cannot_write_same_workspace() {
        let path = path("lock");
        let first = Storage::open(&path).unwrap();
        let second = StorageState::new(Ok(path.clone()));
        assert!(second.get().is_err());
        drop(first);
        assert!(second.get().is_ok());
    }
}

#[cfg(test)]
pub(crate) mod api_tests;

#[cfg(test)]
pub(crate) mod test_support;
