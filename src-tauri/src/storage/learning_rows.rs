//! Named Diesel records shared by vocabulary, review and backup repositories.
use super::schema::{vocabulary_cards as c, vocabulary_entries as e, vocabulary_occurrences as o};
use crate::{
    review::Card,
    vocabulary::{Entry, EntryFields, Occurrence, Result, Source},
};
use diesel::prelude::*;

#[derive(Queryable, Selectable)]
#[diesel(table_name=e)]
pub(super) struct EntryRow {
    pub id: String,
    pub language: String,
    pub language_label: String,
    pub kind: String,
    pub text: String,
    pub meaning: String,
    pub meaning_language: String,
    pub note: String,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}
impl From<EntryRow> for Entry {
    fn from(r: EntryRow) -> Self {
        Self {
            id: r.id,
            fields: EntryFields {
                language: r.language,
                language_label: r.language_label,
                kind: r.kind,
                text: r.text,
                meaning: r.meaning,
                meaning_language: r.meaning_language,
                note: r.note,
            },
            revision: r.revision,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
            occurrences: vec![],
            tags: vec![],
            cards: vec![],
        }
    }
}
#[derive(Queryable, Selectable)]
#[diesel(table_name=o)]
pub(super) struct OccurrenceRow {
    id: String,
    source_kind: String,
    conversation_id: Option<String>,
    message_id: Option<String>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    role: String,
    selected_text: String,
    snapshot: String,
    start: i64,
    end: i64,
    locator_version: i64,
    truncated: i64,
    snapshot_hash: String,
    backend: Option<String>,
}
impl TryFrom<OccurrenceRow> for Occurrence {
    type Error = String;
    fn try_from(r: OccurrenceRow) -> Result<Self> {
        Ok(Self {
            id: r.id,
            source: Source {
                source_kind: r.source_kind,
                conversation_id: r.conversation_id,
                message_id: r.message_id,
                thread_id: r.thread_id,
                turn_id: r.turn_id,
                item_id: r.item_id,
                role: r.role,
                selected_text: r.selected_text,
                snapshot: r.snapshot,
                start: u32::try_from(r.start).map_err(super::error)? as usize,
                end: u32::try_from(r.end).map_err(super::error)? as usize,
                locator_version: u32::try_from(r.locator_version).map_err(super::error)?,
                truncated: r.truncated != 0,
                backend: r
                    .backend
                    .map(|v| serde_json::from_str(&v).map_err(super::error))
                    .transpose()?,
            },
            snapshot_hash: r.snapshot_hash,
        })
    }
}
#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name=c)]
pub(super) struct CardRow {
    pub id: String,
    pub entry_id: String,
    pub direction: String,
    pub stage: i64,
    pub due_at: i64,
    pub last_reviewed_at: Option<i64>,
    pub suspended: i64,
    pub schedule_version: i64,
    pub revision: i64,
}
impl TryFrom<CardRow> for Card {
    type Error = String;
    fn try_from(r: CardRow) -> Result<Self> {
        Ok(Self {
            id: r.id,
            entry_id: r.entry_id,
            direction: r.direction,
            stage: u32::try_from(r.stage).map_err(super::error)?,
            due_at: r.due_at,
            last_reviewed_at: r.last_reviewed_at,
            suspended: r.suspended != 0,
            schedule_version: u32::try_from(r.schedule_version).map_err(super::error)?,
            revision: r.revision,
        })
    }
}
impl From<&Card> for CardRow {
    fn from(c: &Card) -> Self {
        Self {
            id: c.id.clone(),
            entry_id: c.entry_id.clone(),
            direction: c.direction.clone(),
            stage: c.stage.into(),
            due_at: c.due_at,
            last_reviewed_at: c.last_reviewed_at,
            suspended: i64::from(c.suspended),
            schedule_version: c.schedule_version.into(),
            revision: c.revision,
        }
    }
}
// SQLite built-ins exposed as typed expressions, with all arguments bound by Diesel.
diesel::define_sql_function! { fn trim(value: diesel::sql_types::Text) -> diesel::sql_types::Text; }
diesel::define_sql_function! { fn length(value: diesel::sql_types::Text) -> diesel::sql_types::BigInt; }
diesel::define_sql_function! { fn instr(haystack: diesel::sql_types::Text, needle: diesel::sql_types::Text) -> diesel::sql_types::BigInt; }
