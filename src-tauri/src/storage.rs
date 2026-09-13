use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::State;

pub struct StorageState {
    path: std::result::Result<PathBuf, String>,
    value: Mutex<Option<Storage>>,
}
impl StorageState {
    pub fn new(path: std::result::Result<PathBuf, String>) -> Self {
        Self {
            path,
            value: Mutex::new(None),
        }
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
    db: Arc<Mutex<Connection>>,
    path: PathBuf,
    _lock: Option<Arc<File>>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
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
fn migrate(db: &mut Connection) -> Result<()> {
    let version: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(error)?;
    if version > 1 {
        return Err("数据库来自更新版本的 Parley。请升级应用；原始数据未改动。".into());
    }
    if version == 0 {
        let tx = db.transaction().map_err(error)?;
        tx.execute_batch(include_str!("../migrations/001_workspace.sql"))
            .map_err(error)?;
        tx.commit().map_err(error)?;
    }
    Ok(())
}
pub struct ConversationConfig<'a> {
    pub model: &'a str,
    pub target: &'a str,
    pub native: &'a str,
    pub mode: &'a str,
}
fn conversation_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: r.get(0)?,
        pane: r.get(1)?,
        title: r.get(2)?,
        model: r.get(3)?,
        target_language: r.get(4)?,
        native_language: r.get(5)?,
        mode: r.get(6)?,
        draft: r.get(7)?,
        thread_id: r.get(8)?,
        account: r.get(9)?,
        signature: r.get(10)?,
        status: r.get(11)?,
        updated_at: r.get(12)?,
        messages: Vec::new(),
    })
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
        let mut db = Connection::open(path).map_err(error)?;
        db.busy_timeout(Duration::from_secs(3)).map_err(error)?;
        // Version compatibility is checked before modifying an existing database.
        migrate(&mut db)?;
        db.execute_batch(
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
    pub fn interrupt_all(&self) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        tx.execute(
            "UPDATE conversations SET status='interrupted' WHERE status='running'",
            [],
        )
        .map_err(error)?;
        tx.execute(
            "UPDATE messages SET status='interrupted' WHERE status IN ('streaming','pending')",
            [],
        )
        .map_err(error)?;
        tx.commit().map_err(error)
    }
    pub fn preferences(&self) -> Result<Preferences> {
        let db = self.db.lock().unwrap();
        let value: Option<String> = db
            .query_row("SELECT value FROM preferences WHERE id=1", [], |r| r.get(0))
            .optional()
            .map_err(error)?;
        value
            .map(|s| serde_json::from_str(&s).map_err(error))
            .unwrap_or_else(|| Ok(Preferences::default()))
    }
    pub fn save(&self, p: &Preferences, drafts: &[Draft]) -> Result<()> {
        if p.target_language.is_empty()
            || p.native_language.is_empty()
            || p.target_language.len() > 100
            || p.native_language.len() > 100
            || p.main_model.len() > 200
            || p.tutor_model.len() > 200
        {
            return Err("语言或模型设置无效。".into());
        }
        if !["express", "explain", "translate"].contains(&p.tutor_mode.as_str())
            || !["conversation", "vocabulary"].contains(&p.active_view.as_str())
            || !["main", "tutor"].contains(&p.mobile_pane.as_str())
        {
            return Err("工作区设置无效。".into());
        }
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        for (id, pane) in [(&p.main_id, "main"), (&p.tutor_id, "tutor")] {
            if let Some(id) = id {
                let exists: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1 AND pane=?2)",
                        params![id, pane],
                        |r| r.get(0),
                    )
                    .map_err(error)?;
                if !exists {
                    return Err("所选会话不存在。".into());
                }
            }
        }
        tx.execute("INSERT INTO preferences VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET value=excluded.value",[serde_json::to_string(p).map_err(error)?]).map_err(error)?;
        for draft in drafts {
            if draft.text.len() > 128000 {
                return Err("草稿不能超过 128 KB。".into());
            }
            if tx
                .execute(
                    "UPDATE conversations SET draft=?1 WHERE id=?2",
                    params![draft.text, draft.id],
                )
                .map_err(error)?
                != 1
            {
                return Err("草稿所属会话不存在。".into());
            }
        }
        tx.commit().map_err(error)
    }
    pub fn create(&self, id: &str, pane: &str) -> Result<Conversation> {
        if id.is_empty() || id.len() > 100 || !["main", "tutor"].contains(&pane) {
            return Err("会话标识无效。".into());
        }
        self.db.lock().unwrap().execute("INSERT INTO conversations(id,pane,mode,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)",params![id,pane,if pane=="main" {"conversation"} else {"express"},now()]).map_err(error)?;
        self.read(id)
    }
    pub fn list(&self) -> Result<Vec<Conversation>> {
        let db = self.db.lock().unwrap();
        let mut query=db.prepare("SELECT id,pane,title,model,target_language,native_language,mode,draft,thread_id,account,signature,status,updated_at FROM conversations ORDER BY updated_at DESC,rowid DESC").map_err(error)?;
        query
            .query_map([], conversation_row)
            .map_err(error)?
            .collect::<rusqlite::Result<_>>()
            .map_err(error)
    }
    fn read_metadata(&self, id: &str) -> Result<Conversation> {
        self.db.lock().unwrap().query_row("SELECT id,pane,title,model,target_language,native_language,mode,draft,thread_id,account,signature,status,updated_at FROM conversations WHERE id=?1",[id],conversation_row).map_err(error)
    }
    pub fn read(&self, id: &str) -> Result<Conversation> {
        let mut c = self.read_metadata(id)?;
        let db = self.db.lock().unwrap();
        let mut q=db.prepare("SELECT id,role,text,status FROM messages WHERE conversation_id=?1 ORDER BY sequence").map_err(error)?;
        c.messages = q
            .query_map([id], |r| {
                Ok(Message {
                    id: r.get(0)?,
                    role: r.get(1)?,
                    text: r.get(2)?,
                    status: r.get(3)?,
                })
            })
            .map_err(error)?
            .collect::<rusqlite::Result<_>>()
            .map_err(error)?;
        Ok(c)
    }
    pub fn bind(
        &self,
        id: &str,
        thread: &str,
        account: Option<&str>,
        signature: &str,
    ) -> Result<()> {
        self.db
            .lock()
            .unwrap()
            .execute(
                "UPDATE conversations SET thread_id=?1,account=?2,signature=?3 WHERE id=?4",
                params![thread, account, signature, id],
            )
            .map_err(error)?;
        Ok(())
    }
    pub fn begin(
        &self,
        id: &str,
        message_id: &str,
        text: &str,
        config: ConversationConfig<'_>,
    ) -> Result<()> {
        let ConversationConfig {
            model,
            target,
            native,
            mode,
        } = config;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        if tx.execute("UPDATE conversations SET status='running',title=CASE WHEN title='新的对话' THEN ?1 ELSE title END,model=?2,target_language=?3,native_language=?4,mode=?5,draft='',updated_at=?6 WHERE id=?7 AND status!='running'",params![text.chars().take(36).collect::<String>(),model,target,native,mode,now(),id]).map_err(error)?!=1 { return Err("会话不存在或仍在回复中。".into()); }
        // A request ID cannot be submitted twice, including after a crash.
        tx.execute("INSERT INTO messages(id,conversation_id,role,text,status) VALUES(?1,?2,'user',?3,'pending')",params![message_id,id,text]).map_err(error)?;
        tx.commit().map_err(error)
    }
    pub fn event(&self, id: &str, method: &str, p: &Value) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        match method {
            "item/agentMessage/delta" => {
                if let (Some(item), Some(delta)) = (p["itemId"].as_str(), p["delta"].as_str()) {
                    tx.execute("INSERT INTO messages(id,conversation_id,role,text,status,turn_id) VALUES(?1,?2,'assistant',?3,'streaming',?4) ON CONFLICT(conversation_id,id) DO UPDATE SET text=messages.text||excluded.text WHERE messages.status='streaming'",params![item,id,delta,p["turnId"].as_str()]).map_err(error)?;
                }
            }
            "item/completed" if p["item"]["type"] == "agentMessage" => {
                if let (Some(item), Some(text)) =
                    (p["item"]["id"].as_str(), p["item"]["text"].as_str())
                {
                    tx.execute("INSERT INTO messages(id,conversation_id,role,text,status,turn_id) VALUES(?1,?2,'assistant',?3,'complete',?4) ON CONFLICT(conversation_id,id) DO UPDATE SET text=excluded.text,status='complete'",params![item,id,text,p["turnId"].as_str()]).map_err(error)?;
                }
            }
            "turn/started" => {
                tx.execute("UPDATE messages SET status='complete' WHERE conversation_id=?1 AND role='user' AND status='pending'",[id]).map_err(error)?;
            }
            "turn/completed" => {
                let status = match p["turn"]["status"].as_str() {
                    Some("completed") => "idle",
                    Some("interrupted") => "interrupted",
                    _ => "failed",
                };
                tx.execute(
                    "UPDATE conversations SET status=?1,updated_at=?2 WHERE id=?3",
                    params![status, now(), id],
                )
                .map_err(error)?;
                tx.execute("UPDATE messages SET status=?1 WHERE conversation_id=?2 AND status IN ('streaming','pending')",params![if status=="idle" {"complete"} else {status},id]).map_err(error)?;
            }
            _ => {}
        }
        tx.commit().map_err(error)
    }
    pub fn fail(&self, id: &str) -> Result<()> {
        self.event(
            id,
            "turn/completed",
            &serde_json::json!({"turn":{"status":"failed"}}),
        )
    }
    pub fn delete(&self, id: &str) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        if tx
            .query_row("SELECT status FROM conversations WHERE id=?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .map_err(error)?
            == "running"
        {
            return Err("请先停止当前回复。".into());
        }
        tx.execute("DELETE FROM conversations WHERE id=?1", [id])
            .map_err(error)?;
        let saved: Option<String> = tx
            .query_row("SELECT value FROM preferences WHERE id=1", [], |r| r.get(0))
            .optional()
            .map_err(error)?;
        if let Some(saved) = saved {
            let mut p: Preferences = serde_json::from_str(&saved).map_err(error)?;
            if p.main_id.as_deref() == Some(id) {
                p.main_id = None;
            }
            if p.tutor_id.as_deref() == Some(id) {
                p.tutor_id = None;
            }
            tx.execute(
                "UPDATE preferences SET value=?1 WHERE id=1",
                [serde_json::to_string(&p).map_err(error)?],
            )
            .map_err(error)?;
        }
        tx.commit().map_err(error)
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
) -> Result<Conversation> {
    let s = storage.get()?;
    tauri::async_runtime::spawn_blocking(move || s.create(&id, &pane))
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
    use super::*;
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
        {
            let s = Storage::open(&path).unwrap();
            s.create("c", "main").unwrap();
            s.create("t", "tutor").unwrap();
            let p = Preferences {
                main_id: Some("c".into()),
                tutor_id: Some("t".into()),
                target_language: "ja".into(),
                main_model: "saved-model".into(),
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
            s.db.lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn future_schema_and_failed_migration_do_not_destroy_old_data() {
        let mut future = Connection::open_in_memory().unwrap();
        future.execute_batch("CREATE TABLE original(value TEXT);INSERT INTO original VALUES('preserve');PRAGMA user_version=99;").unwrap();
        assert!(migrate(&mut future).is_err());
        assert_eq!(
            future
                .query_row("SELECT value FROM original", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "preserve"
        );
        let mut conflict = Connection::open_in_memory().unwrap();
        conflict
            .execute_batch("CREATE TABLE conversations(original TEXT);")
            .unwrap();
        assert!(migrate(&mut conflict).is_err());
        assert_eq!(
            conflict
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name='preferences'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
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
