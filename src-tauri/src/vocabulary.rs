//! Typed vocabulary DTOs. All mutations are executed by the shared Storage repository.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

pub type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryFields {
    pub language: String,
    #[serde(default)]
    pub language_label: String,
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub meaning: String,
    pub meaning_language: String,
    #[serde(default)]
    pub note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceBackend {
    pub kind: crate::backends::types::BackendKind,
    pub provider: crate::backends::types::Provider,
    pub model: Option<String>,
}
impl SourceBackend {
    pub fn validate(&self) -> Result<()> {
        use crate::backends::types::{BackendKind as K, Provider as P};
        if !matches!(
            (self.kind, self.provider),
            (K::Codex | K::OpenaiResponses, P::Openai)
                | (K::ClaudeCode | K::AnthropicMessages, P::Anthropic)
                | (K::GeminiInteractions, P::Google)
                | (K::OpenaiCompatible, _)
        ) {
            return Err("来源厂商与协议不匹配。".into());
        }
        if let Some(model) = &self.model {
            bounded(model, 200, "来源模型")?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub source_kind: String,
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub item_id: Option<String>,
    pub role: String,
    pub selected_text: String,
    pub snapshot: String,
    pub start: usize,
    pub end: usize,
    pub locator_version: u32,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<SourceBackend>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    pub id: String,
    #[serde(flatten)]
    pub source: Source,
    pub snapshot_hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    #[serde(flatten)]
    pub fields: EntryFields,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub occurrences: Vec<Occurrence>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub cards: Vec<crate::review::Card>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveRequest {
    pub request_id: String,
    pub id: Option<String>,
    pub expected_revision: Option<i64>,
    pub fields: EntryFields,
    pub occurrence: Option<Source>,
    pub draft_id: Option<String>,
    #[serde(default)]
    pub allow_duplicate: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub entry: Entry,
    pub duplicate: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditDraft {
    pub id: String,
    pub entry_id: Option<String>,
    pub base_revision: Option<i64>,
    pub fields: EntryFields,
    pub occurrence: Option<Source>,
    #[serde(default)]
    pub allow_duplicate: bool,
    pub request_id: String,
    #[serde(default)]
    pub tag_text: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListQuery {
    pub search: String,
    pub language: String,
    pub kind: String,
    pub has_meaning: Option<bool>,
    pub trash: bool,
    pub sort: String,
    pub offset: u32,
    pub tag: String,
    pub review_status: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPage {
    pub entries: Vec<Entry>,
    pub total: i64,
    pub languages: Vec<String>,
    pub tags: Vec<String>,
    pub active_count: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryMutation {
    pub request_id: String,
    pub id: String,
    pub expected_revision: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddOccurrence {
    pub request_id: String,
    pub id: String,
    pub expected_revision: i64,
    pub source: Source,
    pub draft_id: Option<String>,
}
pub fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
pub fn normalized(text: &str) -> String {
    text.nfc().collect()
}
pub fn uuid(value: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| "无效的记录标识".into())
}
pub fn bounded(value: &str, limit: usize, label: &str) -> Result<()> {
    if value.contains('\0') || value.chars().count() > limit {
        return Err(format!(
            "{label}最多允许 {limit} 个字符，且不能包含空字符。"
        ));
    }
    Ok(())
}
pub fn language(value: &str) -> Result<String> {
    bounded(value, 100, "语言代码")?;
    language_tags::LanguageTag::parse(value.trim())
        .map(|tag| tag.into_string())
        .map_err(|_| "请输入有效语言代码，例如 en、pt-BR 或 zh-Hant。".into())
}
impl EntryFields {
    pub fn validate(&self, draft: bool) -> Result<()> {
        bounded(&self.text, 2000, "词句")?;
        bounded(&self.meaning, 8000, "释义")?;
        bounded(&self.note, 16000, "注释")?;
        bounded(&self.language_label, 100, "语言名称")?;
        bounded(&self.language, 100, "语言代码")?;
        bounded(&self.meaning_language, 100, "释义语言")?;
        if !["word", "phrase", "sentence"].contains(&self.kind.as_str()) {
            return Err("请选择词、短语或句子。".into());
        }
        if !draft {
            if self.text.trim().is_empty() {
                return Err("请填写要收藏的词句。".into());
            }
            language(&self.language)?;
            language(&self.meaning_language)?;
        }
        Ok(())
    }
    pub fn search_text(&self) -> String {
        normalized(&format!("{}\n{}\n{}", self.text, self.meaning, self.note))
    }
}
impl Source {
    pub fn validate(&self) -> Result<()> {
        if let Some(backend) = &self.backend {
            backend.validate()?;
            if ["manual", "import"].contains(&self.source_kind.as_str()) {
                return Err("手动或导入材料不能附带模型后端。".into());
            }
            if self.source_kind == "terminal" && !backend.kind.is_cli() {
                return Err("终端来源必须属于本机 CLI。".into());
            }
        }
        if !["main", "tutor", "terminal", "manual", "import"].contains(&self.source_kind.as_str())
            || !["user", "assistant", "manual"].contains(&self.role.as_str())
            || self.locator_version != 1
        {
            return Err("来源类型或选区格式不支持。".into());
        }
        for id in [
            &self.conversation_id,
            &self.message_id,
            &self.thread_id,
            &self.turn_id,
            &self.item_id,
        ]
        .into_iter()
        .flatten()
        {
            bounded(id, 200, "来源标识")?;
        }
        if self.conversation_id.is_some() != self.message_id.is_some() {
            return Err("来源会话和消息必须同时提供。".into());
        }
        if self.source_kind == "terminal" && (self.thread_id.is_none() || self.item_id.is_none()) {
            return Err("终端来源缺少会话或消息标识。".into());
        }
        bounded(&self.snapshot, 16000, "原句")?;
        bounded(&self.selected_text, 2000, "选中文字")?;
        let chars: Vec<char> = self.snapshot.chars().collect();
        if self.start >= self.end
            || self.end > chars.len()
            || chars[self.start..self.end].iter().collect::<String>() != self.selected_text
        {
            return Err("选区与原句不匹配，请重新选择。".into());
        }
        let mut boundaries = vec![0];
        let mut offset = 0;
        for grapheme in self.snapshot.graphemes(true) {
            offset += grapheme.chars().count();
            boundaries.push(offset);
        }
        if !boundaries.contains(&self.start) || !boundaries.contains(&self.end) {
            return Err("请选择完整字符，不能拆开重音或组合字符。".into());
        }
        Ok(())
    }
    // CLI identity already includes its thread namespace; preserve v1 import hashes.
    pub fn legacy_identity(&self) -> Self {
        let mut source = self.clone();
        if source.backend.as_ref().is_some_and(|b| b.kind.is_cli()) {
            source.backend = None;
        }
        source
    }
    pub fn legacy_fingerprint(&self) -> String {
        let mut source = self.clone();
        source.backend = None;
        digest(&serde_json::to_string(&source).expect("source serializes"))
    }
    pub fn fingerprint(&self) -> String {
        digest(&serde_json::to_string(&self.legacy_identity()).expect("source serializes"))
    }
}

use crate::storage::StorageState;
use tauri::State;
#[tauri::command]
pub async fn vocabulary_list(
    state: State<'_, StorageState>,
    query: ListQuery,
) -> Result<EntryPage> {
    state
        .run(move |storage| storage.vocabulary_list(&query))
        .await
}
#[tauri::command]
pub async fn vocabulary_get(state: State<'_, StorageState>, id: String) -> Result<Entry> {
    state.run(move |storage| storage.vocabulary_get(&id)).await
}
#[tauri::command]
pub async fn vocabulary_save(
    state: State<'_, StorageState>,
    request: SaveRequest,
) -> Result<SaveResult> {
    state
        .run(move |storage| storage.vocabulary_save(&request))
        .await
}
#[tauri::command]
pub async fn vocabulary_add_occurrence(
    state: State<'_, StorageState>,
    request: AddOccurrence,
) -> Result<Entry> {
    state
        .run(move |storage| storage.vocabulary_add_occurrence(&request))
        .await
}
#[tauri::command]
pub async fn vocabulary_trash(
    state: State<'_, StorageState>,
    request: EntryMutation,
) -> Result<()> {
    state
        .run(move |storage| storage.vocabulary_mutate(&request, "trash"))
        .await
}
#[tauri::command]
pub async fn vocabulary_restore(
    state: State<'_, StorageState>,
    request: EntryMutation,
) -> Result<()> {
    state
        .run(move |storage| storage.vocabulary_mutate(&request, "restore"))
        .await
}
#[tauri::command]
pub async fn vocabulary_purge(
    state: State<'_, StorageState>,
    request: EntryMutation,
) -> Result<()> {
    state
        .run(move |storage| storage.vocabulary_mutate(&request, "purge"))
        .await
}
#[tauri::command]
pub async fn vocabulary_save_draft(state: State<'_, StorageState>, draft: EditDraft) -> Result<()> {
    state
        .run(move |storage| storage.vocabulary_save_draft(&draft))
        .await
}
#[tauri::command]
pub async fn vocabulary_load_drafts(state: State<'_, StorageState>) -> Result<Vec<EditDraft>> {
    state
        .run(move |storage| storage.vocabulary_load_drafts())
        .await
}
#[tauri::command]
pub async fn vocabulary_discard_draft(state: State<'_, StorageState>, id: String) -> Result<()> {
    state
        .run(move |storage| storage.vocabulary_discard_draft(&id))
        .await
}
