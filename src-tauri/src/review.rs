use crate::vocabulary::{Entry, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Card {
    pub id: String,
    pub entry_id: String,
    pub direction: String,
    pub stage: u32,
    pub due_at: i64,
    pub last_reviewed_at: Option<i64>,
    pub suspended: bool,
    pub schedule_version: u32,
    pub revision: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardRequest {
    pub request_id: String,
    pub entry_id: String,
    pub direction: String,
    pub suspended: bool,
    pub reset: bool,
    pub expected_revision: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GradeRequest {
    pub request_id: String,
    pub card_id: String,
    pub expected_revision: i64,
    pub rating: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradeResult {
    pub card: Card,
    pub review_id: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewItem {
    pub card: Card,
    pub entry: Entry,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewQueue {
    pub items: Vec<ReviewItem>,
    pub due_count: i64,
    pub new_count: i64,
    pub next_due_at: Option<i64>,
    pub completed_today: i64,
    pub last_review_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UndoRequest {
    pub request_id: String,
    pub review_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TagsRequest {
    pub request_id: String,
    pub entry_id: String,
    pub expected_revision: i64,
    pub tags: Vec<String>,
}

pub fn schedule(card: &Card, rating: &str, now: i64) -> Result<Card> {
    if ![1, 2].contains(&card.schedule_version)
        || card.stage > if card.schedule_version == 1 { 5 } else { 6 }
    {
        return Err("不支持的复习排程版本。".into());
    }
    let mut next = card.clone();
    let day = 24 * 60 * 60 * 1000;
    let delay = match rating {
        "forgot" => {
            next.stage = 0;
            10 * 60 * 1000
        }
        "hard" => day,
        "remembered" => {
            let max = if card.schedule_version == 1 { 5 } else { 6 };
            next.stage = (card.stage + 1).min(max);
            [1, 3, 7, 14, 30, 60][card.stage.min(max - 1) as usize] * day
        }
        _ => return Err("请选择忘记、吃力或记住。".into()),
    };
    next.due_at = now.checked_add(delay).ok_or("复习日期超出范围。")?;
    next.last_reviewed_at = Some(now);
    next.revision = card.revision.checked_add(1).ok_or("复习版本超出范围。")?;
    Ok(next)
}

use crate::storage::StorageState;
use tauri::State;
#[tauri::command]
pub async fn vocabulary_card_save(
    state: State<'_, StorageState>,
    request: CardRequest,
) -> Result<Card> {
    state
        .run(move |storage| storage.vocabulary_card_save(&request))
        .await
}
#[tauri::command]
pub async fn vocabulary_review_queue(
    state: State<'_, StorageState>,
    limit: u32,
    new_limit: u32,
    day_start: i64,
) -> Result<ReviewQueue> {
    state
        .run(move |storage| storage.vocabulary_review_queue(limit, new_limit, day_start))
        .await
}
#[tauri::command]
pub async fn vocabulary_review_grade(
    state: State<'_, StorageState>,
    request: GradeRequest,
) -> Result<GradeResult> {
    state
        .run(move |storage| storage.vocabulary_review_grade(&request))
        .await
}
#[tauri::command]
pub async fn vocabulary_review_undo(
    state: State<'_, StorageState>,
    request: UndoRequest,
) -> Result<Card> {
    state
        .run(move |storage| storage.vocabulary_review_undo(&request))
        .await
}
#[tauri::command]
pub async fn vocabulary_tags_save(
    state: State<'_, StorageState>,
    request: TagsRequest,
) -> Result<Entry> {
    state
        .run(move |storage| storage.vocabulary_tags_save(&request))
        .await
}
