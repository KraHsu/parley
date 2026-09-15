//! Versioned word-list exchange. The WebView never receives an arbitrary file API.
use crate::review::Card;
use crate::vocabulary::{EntryFields, Occurrence, Result, bounded, uuid};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Mutex};

pub const MAX_BYTES: u64 = 512 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Backup {
    pub format: String,
    pub version: u32,
    pub dataset_id: String,
    pub exported_at: i64,
    pub entries: Vec<BackupEntry>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupEntry {
    pub id: String,
    pub fields: EntryFields,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub occurrences: Vec<Occurrence>,
    pub tags: Vec<String>,
    pub cards: Vec<Card>,
    pub reviews: Vec<BackupReview>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupReview {
    pub id: String,
    pub card_id: String,
    pub rating: String,
    pub reviewed_at: i64,
    pub before_state: Card,
    pub after_state: Card,
    pub undone_at: Option<i64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub token: String,
    pub entries: usize,
    pub added: usize,
    pub duplicates: usize,
    pub conflicts: usize,
    pub conflict_examples: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: usize,
    pub duplicates: usize,
    pub skipped: usize,
}
#[derive(Clone)]
pub struct PendingImport {
    pub token: String,
    pub backup: Backup,
    pub signature: String,
}
#[derive(Default)]
pub struct ImportState(pub Mutex<Option<PendingImport>>);
fn valid_card(card: &Card, entry_id: &str) -> Result<()> {
    uuid(&card.id)?;
    if card.entry_id != entry_id
        || !["recognition", "production"].contains(&card.direction.as_str())
        || card.stage > if card.schedule_version == 1 { 5 } else { 6 }
        || ![1, 2].contains(&card.schedule_version)
        || card.revision < 1
        || card.revision == i64::MAX
        || card.due_at < 0
        || card.last_reviewed_at.is_some_and(|time| time < 0)
    {
        return Err("复习卡片包含不支持或无效的数据。".into());
    }
    Ok(())
}
impl Backup {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() as u64 > MAX_BYTES {
            return Err("导入文件不能超过 512 MiB。".into());
        }
        let backup: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("无法读取词句备份：{e}"))?;
        backup.validate()?;
        Ok(backup)
    }
    pub fn validate(&self) -> Result<()> {
        if self.format != "parley-vocabulary" || ![1, 2, 3].contains(&self.version) {
            return Err("不支持的词句备份格式或版本，请升级 Parley 后重试。".into());
        }
        uuid(&self.dataset_id)?;
        if self.exported_at < 0 {
            return Err("备份时间无效。".into());
        }
        if self.entries.len() > 50000 {
            return Err("一次最多导入 50,000 条词句。".into());
        }
        let mut ids = HashSet::new();
        let mut total_sources = 0;
        let mut total_reviews = 0;
        for entry in &self.entries {
            uuid(&entry.id)?;
            if !ids.insert(entry.id.clone()) {
                return Err("备份中存在重复标识。".into());
            }
            entry.fields.validate(false)?;
            if entry.created_at < 0
                || entry.updated_at < 0
                || entry.deleted_at.is_some_and(|time| time < 0)
            {
                return Err("词句时间无效。".into());
            }
            if entry.occurrences.len() > 100 || entry.tags.len() > 30 || entry.cards.len() > 2 {
                return Err("单条词句超过来源、标签或卡片数量上限。".into());
            }
            for tag in &entry.tags {
                bounded(tag, 100, "标签")?
            }
            let mut source_keys = HashSet::new();
            for source in &entry.occurrences {
                uuid(&source.id)?;
                if !ids.insert(source.id.clone()) {
                    return Err("备份中存在重复来源标识。".into());
                }
                source.source.validate()?;
                if self.version == 1 && source.source.backend.is_some() {
                    return Err("v1 备份不能包含 v2 后端来源字段。".into());
                }
                if self.version >= 2
                    && source.source.source_kind == "terminal"
                    && source.source.backend.is_none()
                {
                    return Err("v2 终端来源缺少后端信息。".into());
                }
                if self.version >= 2 && source.source.source_kind == "terminal" {
                    let claude =
                        source.source.backend.as_ref().is_some_and(|b| {
                            b.kind == crate::backends::types::BackendKind::ClaudeCode
                        });
                    let namespaced = source
                        .source
                        .thread_id
                        .as_ref()
                        .is_some_and(|s| s.starts_with("claude-code:"));
                    if claude != namespaced {
                        return Err("终端来源命名空间与后端不匹配。".into());
                    }
                }
                if !source_keys.insert(source.source.fingerprint()) {
                    return Err("备份包含重复来源。".into());
                }
                if source.snapshot_hash != crate::vocabulary::digest(&source.source.snapshot) {
                    return Err("来源快照校验失败。".into());
                }
            }
            let mut directions = HashSet::new();
            let mut cards = HashSet::new();
            for card in &entry.cards {
                valid_card(card, &entry.id)?;
                if !ids.insert(card.id.clone()) || !directions.insert(&card.direction) {
                    return Err("备份中存在重复复习卡片。".into());
                }
                cards.insert(&card.id);
            }
            for review in &entry.reviews {
                uuid(&review.id)?;
                if !ids.insert(review.id.clone()) || !cards.contains(&review.card_id) {
                    return Err("复习记录重复或缺少对应卡片。".into());
                }
                valid_card(&review.before_state, &entry.id)?;
                valid_card(&review.after_state, &entry.id)?;
                if crate::review::schedule(
                    &review.before_state,
                    &review.rating,
                    review.reviewed_at,
                )? != review.after_state
                {
                    return Err("复习前后状态不一致。".into());
                }
                if review.before_state.id != review.card_id
                    || review.after_state.id != review.card_id
                    || !["forgot", "hard", "remembered"].contains(&review.rating.as_str())
                    || review.reviewed_at < 0
                    || review.undone_at.is_some_and(|t| t < 0)
                {
                    return Err("复习记录无效。".into());
                }
            }
            total_sources += entry.occurrences.len();
            total_reviews += entry.reviews.len();
        }
        if total_sources > 50000 || total_reviews > 100000 {
            return Err("备份超过 50,000 条来源或 100,000 条复习记录的导入上限。".into());
        }
        Ok(())
    }
}

use crate::storage::StorageState;
use std::io::{Read, Write};
use tauri::State;
fn read_backup(path: &std::path::Path) -> Result<Backup> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("导入文件不能超过 512 MiB。".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Backup::parse(&bytes)
}
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let directory = path.parent().ok_or("无法确定导出文件夹。")?;
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.as_file().sync_all())
        .map_err(|e| format!("导出未完成：{e}"))?;
    file.persist(path)
        .map_err(|e| format!("无法保存导出文件：{e}"))?;
    Ok(())
}
#[tauri::command]
pub async fn vocabulary_export(
    state: State<'_, StorageState>,
    format: String,
    include_trash: bool,
) -> Result<Option<String>> {
    if !["json", "csv"].contains(&format.as_str()) {
        return Err("不支持的导出格式。".into());
    }
    let storage = state.get()?;
    let file = rfd::AsyncFileDialog::new()
        .set_title("导出词句学习数据")
        .set_file_name(format!("parley-words.{format}"))
        .add_filter("词句数据", &[format.as_str()])
        .save_file()
        .await;
    let Some(file) = file else { return Ok(None) };
    let path = file.path().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let backup = storage.vocabulary_backup(include_trash)?;
        backup.validate()?;
        let content = if format == "json" {
            serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?
        } else {
            crate::storage::exchange::csv(&backup).into_bytes()
        };
        if content.len() as u64 > MAX_BYTES {
            return Err("导出超过 512 MiB，请分批整理词句后重试。".into());
        }
        write_atomic(&path, &content)?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn vocabulary_preview_import(
    storage: State<'_, StorageState>,
    state: State<'_, ImportState>,
) -> Result<Option<ImportPreview>> {
    let storage = storage.get()?;
    let file = rfd::AsyncFileDialog::new()
        .set_title("选择 Parley JSON 词句备份")
        .add_filter("Parley 词句备份", &["json"])
        .pick_file()
        .await;
    let Some(file) = file else { return Ok(None) };
    let path = file.path().to_owned();
    let (preview, pending) = tauri::async_runtime::spawn_blocking(move || {
        let backup = read_backup(&path)?;
        storage.vocabulary_preview(backup)
    })
    .await
    .map_err(|e| e.to_string())??;
    *state.0.lock().unwrap() = Some(pending);
    Ok(Some(preview))
}
#[tauri::command]
pub async fn vocabulary_import(
    storage: State<'_, StorageState>,
    state: State<'_, ImportState>,
    token: String,
    request_id: String,
    copy_conflicts: bool,
) -> Result<ImportResult> {
    let storage = storage.get()?;
    let pending = state
        .0
        .lock()
        .unwrap()
        .as_ref()
        .filter(|p| p.token == token)
        .cloned()
        .ok_or("导入预览已过期，请重新选择文件。")?;
    tauri::async_runtime::spawn_blocking(move || {
        storage.vocabulary_import(&pending, &request_id, copy_conflicts)
    })
    .await
    .map_err(|e| e.to_string())?
}
