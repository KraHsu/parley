use super::{Storage, error, now};
use crate::vocabulary::*;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Serialize, de::DeserializeOwned};

pub(super) const ENTRY_COLUMNS: &str = "id,language,language_label,kind,text,meaning,meaning_language,note,revision,created_at,updated_at,deleted_at";
pub(super) fn entry_row(row: &Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        fields: EntryFields {
            language: row.get(1)?,
            language_label: row.get(2)?,
            kind: row.get(3)?,
            text: row.get(4)?,
            meaning: row.get(5)?,
            meaning_language: row.get(6)?,
            note: row.get(7)?,
        },
        revision: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        deleted_at: row.get(11)?,
        occurrences: vec![],
        tags: vec![],
        cards: vec![],
    })
}
pub(super) fn get_entry(db: &Connection, id: &str) -> Result<Entry> {
    let mut entry = db
        .query_row(
            &format!("SELECT {ENTRY_COLUMNS} FROM vocabulary_entries WHERE id=?1"),
            [id],
            entry_row,
        )
        .optional()
        .map_err(error)?
        .ok_or("词句已不存在，请刷新列表。")?;
    let mut query = db.prepare("SELECT id,source_kind,conversation_id,message_id,thread_id,turn_id,item_id,role,selected_text,snapshot,start,end,locator_version,truncated,snapshot_hash FROM vocabulary_occurrences WHERE entry_id=?1 ORDER BY rowid").map_err(error)?;
    entry.occurrences = query
        .query_map([id], |r| {
            Ok(Occurrence {
                id: r.get(0)?,
                source: Source {
                    source_kind: r.get(1)?,
                    conversation_id: r.get(2)?,
                    message_id: r.get(3)?,
                    thread_id: r.get(4)?,
                    turn_id: r.get(5)?,
                    item_id: r.get(6)?,
                    role: r.get(7)?,
                    selected_text: r.get(8)?,
                    snapshot: r.get(9)?,
                    start: r.get::<_, u32>(10)? as usize,
                    end: r.get::<_, u32>(11)? as usize,
                    locator_version: r.get(12)?,
                    truncated: r.get(13)?,
                },
                snapshot_hash: r.get(14)?,
            })
        })
        .map_err(error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(error)?;
    entry.tags = super::review::get_tags(db, id)?;
    entry.cards = super::review::get_cards(db, id)?;
    Ok(entry)
}
pub(super) fn fingerprint(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value)
        .map(|value| digest(&value))
        .map_err(error)
}
pub(super) fn cached<T: DeserializeOwned>(
    db: &Connection,
    id: &str,
    operation: &str,
    hash: &str,
) -> Result<Option<T>> {
    uuid(id)?;
    let previous: Option<(String, String, String)> = db
        .query_row(
            "SELECT operation,fingerprint,result FROM vocabulary_mutations WHERE request_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(error)?;
    if let Some((op, fingerprint, result)) = previous {
        if op != operation || fingerprint != hash {
            return Err("这次操作的内容已改变，请使用新的请求标识。".into());
        }
        return serde_json::from_str(&result).map(Some).map_err(error);
    }
    Ok(None)
}
pub(super) fn remember(
    db: &Connection,
    id: &str,
    operation: &str,
    hash: &str,
    result: &impl Serialize,
) -> Result<()> {
    db.execute(
        "INSERT INTO vocabulary_mutations VALUES(?1,?2,?3,?4,?5)",
        params![
            id,
            operation,
            hash,
            serde_json::to_string(result).map_err(error)?,
            now()
        ],
    )
    .map_err(error)?;
    Ok(())
}
pub(super) fn check_revision(entry: &Entry, revision: i64) -> Result<()> {
    if entry.revision != revision {
        return Err("词句已有更新，草稿已保留。请重新加载并比较后再保存。".into());
    }
    Ok(())
}
pub(super) fn insert_entry(
    db: &Connection,
    id: &str,
    fields: &EntryFields,
    time: i64,
) -> Result<()> {
    db.execute("INSERT INTO vocabulary_entries(id,language,language_label,kind,text,lookup_key,meaning,meaning_language,note,search_text,revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,1,?11,?11)", params![id,language(&fields.language)?,fields.language_label,fields.kind,fields.text,normalized(&fields.text),fields.meaning,language(&fields.meaning_language)?,fields.note,fields.search_text(),time]).map_err(error)?;
    Ok(())
}
pub(super) fn insert_source(db: &Connection, entry_id: &str, source: &Source) -> Result<()> {
    source.validate()?;
    let mut source = source.clone();
    let hash = source.fingerprint();
    if let (Some(conversation), Some(message)) = (&source.conversation_id, &source.message_id) {
        let found: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM messages WHERE conversation_id=?1 AND id=?2)",
                params![conversation, message],
                |r| r.get(0),
            )
            .map_err(error)?;
        if !found {
            source.conversation_id = None;
            source.message_id = None;
        }
    }
    db.execute("INSERT INTO vocabulary_occurrences(id,entry_id,source_kind,conversation_id,message_id,thread_id,turn_id,item_id,role,selected_text,snapshot,snapshot_hash,start,end,locator_version,truncated,fingerprint) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17) ON CONFLICT(entry_id,fingerprint) DO NOTHING", params![uuid::Uuid::new_v4().to_string(),entry_id,source.source_kind,source.conversation_id,source.message_id,source.thread_id,source.turn_id,source.item_id,source.role,source.selected_text,source.snapshot,digest(&source.snapshot),source.start as i64,source.end as i64,source.locator_version,source.truncated,hash]).map_err(error)?;
    Ok(())
}
fn clear_draft(db: &Connection, id: &Option<String>) -> Result<()> {
    if let Some(id) = id {
        db.execute("DELETE FROM vocabulary_drafts WHERE id=?1", [id])
            .map_err(error)?;
    }
    Ok(())
}
impl Storage {
    pub fn vocabulary_get(&self, id: &str) -> Result<Entry> {
        get_entry(&self.db.lock().unwrap(), id)
    }
    pub fn vocabulary_list(&self, query: &ListQuery) -> Result<EntryPage> {
        bounded(&query.search, 2000, "搜索文字")?;
        bounded(&query.language, 100, "语言代码")?;
        let filter = "WHERE ((?1=1 AND deleted_at IS NOT NULL) OR (?1=0 AND deleted_at IS NULL)) AND (?2='' OR language=?2) AND (?3='' OR kind=?3) AND (?4 IS NULL OR (length(trim(meaning))>0)=?4) AND (?5='' OR instr(search_text,?5)>0) AND (?7='' OR EXISTS(SELECT 1 FROM vocabulary_entry_tags et JOIN vocabulary_tags t ON t.id=et.tag_id WHERE et.entry_id=vocabulary_entries.id AND t.name=?7)) AND (?8='' OR (?8='none' AND NOT EXISTS(SELECT 1 FROM vocabulary_cards c WHERE c.entry_id=vocabulary_entries.id)) OR EXISTS(SELECT 1 FROM vocabulary_cards c WHERE c.entry_id=vocabulary_entries.id AND ((?8='due' AND c.suspended=0 AND c.due_at<=?9 AND length(trim(meaning))>0) OR (?8='suspended' AND c.suspended=1))))";
        let order = if query.sort == "created" {
            "created_at"
        } else {
            "updated_at"
        };
        let search = normalized(query.search.trim());
        let db = self.db.lock().unwrap();
        let total = db
            .query_row(
                &format!("SELECT count(*) FROM vocabulary_entries {filter}"),
                params![
                    query.trash,
                    query.language,
                    query.kind,
                    query.has_meaning,
                    search,
                    query.offset,
                    query.tag,
                    query.review_status,
                    now()
                ],
                |r| r.get(0),
            )
            .map_err(error)?;
        let mut stmt = db.prepare(&format!("SELECT {ENTRY_COLUMNS} FROM vocabulary_entries {filter} ORDER BY {order} DESC,id LIMIT 50 OFFSET ?6")).map_err(error)?;
        let entries = stmt
            .query_map(
                params![
                    query.trash,
                    query.language,
                    query.kind,
                    query.has_meaning,
                    search,
                    query.offset,
                    query.tag,
                    query.review_status,
                    now()
                ],
                entry_row,
            )
            .map_err(error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(error)?;
        let mut stmt = db
            .prepare("SELECT DISTINCT language FROM vocabulary_entries ORDER BY language")
            .map_err(error)?;
        let languages = stmt
            .query_map([], |r| r.get(0))
            .map_err(error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(error)?;
        Ok(EntryPage {
            entries,
            total,
            languages,
            tags: db
                .prepare("SELECT name FROM vocabulary_tags ORDER BY name")
                .map_err(error)?
                .query_map([], |r| r.get(0))
                .map_err(error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(error)?,
            active_count: db
                .query_row(
                    "SELECT count(*) FROM vocabulary_entries WHERE deleted_at IS NULL",
                    [],
                    |r| r.get(0),
                )
                .map_err(error)?,
        })
    }
    pub fn vocabulary_save(&self, request: &SaveRequest) -> Result<SaveResult> {
        request.fields.validate(false)?;
        if let Some(source) = &request.occurrence {
            source.validate()?;
        }
        if let Some(id) = &request.id {
            uuid(id)?;
        }
        if let Some(id) = &request.draft_id {
            uuid(id)?;
        }
        let hash = fingerprint(request)?;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        if let Some(previous) = cached::<SaveResult>(&tx, &request.request_id, "save", &hash)? {
            return Ok(SaveResult {
                entry: get_entry(&tx, &previous.entry.id)?,
                duplicate: previous.duplicate,
            });
        }
        if request.id.is_none()
            && !request.allow_duplicate
            && let Some(source) = &request.occurrence
        {
            let existing: Option<String> = tx.query_row("SELECT e.id FROM vocabulary_entries e JOIN vocabulary_occurrences o ON o.entry_id=e.id WHERE o.fingerprint=?1 AND e.language=?2 AND e.deleted_at IS NULL ORDER BY e.created_at LIMIT 1", params![source.fingerprint(),language(&request.fields.language)?], |r| r.get(0)).optional().map_err(error)?;
            if let Some(id) = existing {
                // Leave the form draft intact so a new meaning is never silently discarded.
                return Ok(SaveResult {
                    entry: get_entry(&tx, &id)?,
                    duplicate: true,
                });
            }
        }
        let id = request
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if request.id.is_some() {
            let current = get_entry(&tx, &id)?;
            check_revision(
                &current,
                request
                    .expected_revision
                    .ok_or("缺少编辑版本，请重新打开词句。")?,
            )?;
            if current.deleted_at.is_some() {
                return Err("请先从回收站恢复词句。".into());
            }
            let f = &request.fields;
            tx.execute("UPDATE vocabulary_entries SET language=?2,language_label=?3,kind=?4,text=?5,lookup_key=?6,meaning=?7,meaning_language=?8,note=?9,search_text=?10,revision=revision+1,updated_at=?11 WHERE id=?1",params![id,language(&f.language)?,f.language_label,f.kind,f.text,normalized(&f.text),f.meaning,language(&f.meaning_language)?,f.note,f.search_text(),now()]).map_err(error)?;
        } else {
            insert_entry(&tx, &id, &request.fields, now())?;
        }
        if let Some(source) = &request.occurrence {
            insert_source(&tx, &id, source)?;
        }
        super::review::set_tags(&tx, &id, &request.tags)?;
        clear_draft(&tx, &request.draft_id)?;
        let result = SaveResult {
            entry: get_entry(&tx, &id)?,
            duplicate: false,
        };
        remember(&tx, &request.request_id, "save", &hash, &result)?;
        tx.commit().map_err(error)?;
        Ok(result)
    }
    pub fn vocabulary_add_occurrence(&self, request: &AddOccurrence) -> Result<Entry> {
        request.source.validate()?;
        let hash = fingerprint(request)?;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        if cached::<String>(&tx, &request.request_id, "occurrence", &hash)?.is_some() {
            return get_entry(&tx, &request.id);
        }
        let entry = get_entry(&tx, &request.id)?;
        check_revision(&entry, request.expected_revision)?;
        if entry.deleted_at.is_some() {
            return Err("请先恢复词句。".into());
        }
        insert_source(&tx, &request.id, &request.source)?;
        tx.execute(
            "UPDATE vocabulary_entries SET revision=revision+1,updated_at=?2 WHERE id=?1",
            params![request.id, now()],
        )
        .map_err(error)?;
        clear_draft(&tx, &request.draft_id)?;
        remember(&tx, &request.request_id, "occurrence", &hash, &request.id)?;
        let result = get_entry(&tx, &request.id)?;
        tx.commit().map_err(error)?;
        Ok(result)
    }
    pub fn vocabulary_mutate(&self, request: &EntryMutation, operation: &str) -> Result<()> {
        let hash = fingerprint(request)?;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        if cached::<bool>(&tx, &request.request_id, operation, &hash)?.is_some() {
            return Ok(());
        }
        let entry = get_entry(&tx, &request.id)?;
        check_revision(&entry, request.expected_revision)?;
        match operation {
            "trash" => {
                tx.execute("UPDATE vocabulary_entries SET deleted_at=?2,updated_at=?2,revision=revision+1 WHERE id=?1",params![request.id,now()]).map_err(error)?;
            }
            "restore" => {
                tx.execute("UPDATE vocabulary_entries SET deleted_at=NULL,updated_at=?2,revision=revision+1 WHERE id=?1",params![request.id,now()]).map_err(error)?;
            }
            "purge" => {
                if entry.deleted_at.is_none() {
                    return Err("请先将词句移入回收站。".into());
                }
                tx.execute("DELETE FROM vocabulary_entries WHERE id=?1", [&request.id])
                    .map_err(error)?;
            }
            _ => return Err("未知词句操作。".into()),
        }
        remember(&tx, &request.request_id, operation, &hash, &true)?;
        tx.commit().map_err(error)
    }
    pub fn vocabulary_save_draft(&self, draft: &EditDraft) -> Result<()> {
        uuid(&draft.id)?;
        uuid(&draft.request_id)?;
        draft.fields.validate(true)?;
        bounded(&draft.tag_text, 1200, "标签草稿")?;
        if let Some(source) = &draft.occurrence {
            source.validate()?;
        }
        let db = self.db.lock().unwrap();
        // A late autosave for a committed form must not recreate its draft.
        let committed: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM vocabulary_mutations WHERE request_id=?1)",
                [&draft.request_id],
                |r| r.get(0),
            )
            .map_err(error)?;
        if committed {
            return Ok(());
        }
        db.execute("INSERT INTO vocabulary_drafts(id,entry_id,payload,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET entry_id=excluded.entry_id,payload=excluded.payload,updated_at=excluded.updated_at",params![draft.id,draft.entry_id,serde_json::to_string(draft).map_err(error)?,now()]).map_err(error)?;
        Ok(())
    }
    pub fn vocabulary_load_drafts(&self) -> Result<Vec<EditDraft>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db
            .prepare("SELECT payload FROM vocabulary_drafts ORDER BY updated_at DESC")
            .map_err(error)?;
        let data = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(error)?;
        data.iter()
            .map(|s| serde_json::from_str(s).map_err(error))
            .collect()
    }
    pub fn vocabulary_discard_draft(&self, id: &str) -> Result<()> {
        self.db
            .lock()
            .unwrap()
            .execute("DELETE FROM vocabulary_drafts WHERE id=?1", [id])
            .map_err(error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
    fn fields() -> EntryFields {
        EntryFields {
            language: "en".into(),
            language_label: "English".into(),
            kind: "phrase".into(),
            text: "have you been".into(),
            meaning: "最近过得怎么样".into(),
            meaning_language: "zh-CN".into(),
            note: "自己的理解 café العربية".into(),
        }
    }
    fn source() -> Source {
        Source {
            source_kind: "terminal".into(),
            conversation_id: None,
            message_id: None,
            thread_id: Some("thread".into()),
            turn_id: Some("turn".into()),
            item_id: Some("item".into()),
            role: "assistant".into(),
            selected_text: "have you been".into(),
            snapshot: "How have you been?".into(),
            start: 4,
            end: 17,
            locator_version: 1,
            truncated: false,
        }
    }
    fn request() -> SaveRequest {
        SaveRequest {
            request_id: id(),
            id: None,
            expected_revision: None,
            fields: fields(),
            occurrence: Some(source()),
            draft_id: None,
            allow_duplicate: false,
            tags: vec![],
        }
    }
    fn draft(req: &SaveRequest) -> EditDraft {
        EditDraft {
            id: req.draft_id.clone().unwrap(),
            entry_id: req.id.clone(),
            base_revision: req.expected_revision,
            fields: req.fields.clone(),
            occurrence: req.occurrence.clone(),
            allow_duplicate: req.allow_duplicate,
            tag_text: String::new(),
            request_id: req.request_id.clone(),
        }
    }
    #[test]
    fn confirmed_save_survives_reopen_and_does_not_recreate_a_late_draft() {
        let dir = std::env::temp_dir().join(id());
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("workspace.sqlite3");
        let mut req = request();
        req.draft_id = Some(id());
        let entry;
        {
            let storage = Storage::open(&path).unwrap();
            storage.vocabulary_save_draft(&draft(&req)).unwrap();
            assert_eq!(storage.vocabulary_load_drafts().unwrap().len(), 1);
            entry = storage.vocabulary_save(&req).unwrap().entry;
            storage.vocabulary_save_draft(&draft(&req)).unwrap();
            assert!(storage.vocabulary_load_drafts().unwrap().is_empty());
        }
        {
            let storage = Storage::open(&path).unwrap();
            assert_eq!(storage.vocabulary_get(&entry.id).unwrap().fields, fields());
            let again = storage.vocabulary_save(&req).unwrap();
            assert_eq!(again.entry.id, entry.id);
            assert_eq!(
                storage
                    .vocabulary_list(&ListQuery::default())
                    .unwrap()
                    .total,
                1
            );
            assert_eq!(again.entry.occurrences[0].source, source());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn duplicate_source_keeps_new_meaning_draft_and_explicit_second_meaning_is_allowed() {
        let storage = Storage::memory();
        let first = storage.vocabulary_save(&request()).unwrap().entry;
        let mut req = request();
        req.draft_id = Some(id());
        req.fields.meaning = "另一种理解".into();
        storage.vocabulary_save_draft(&draft(&req)).unwrap();
        let duplicate = storage.vocabulary_save(&req).unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.entry.id, first.id);
        assert_eq!(storage.vocabulary_load_drafts().unwrap().len(), 1);
        req.allow_duplicate = true;
        let second = storage.vocabulary_save(&req).unwrap();
        assert!(!second.duplicate);
        assert_ne!(second.entry.id, first.id);
        assert_eq!(
            storage
                .vocabulary_list(&ListQuery::default())
                .unwrap()
                .total,
            2
        );
    }
    #[test]
    fn edit_conflict_and_invalid_source_do_not_partially_write() {
        let storage = Storage::memory();
        let first = storage.vocabulary_save(&request()).unwrap().entry;
        let mut req = request();
        req.id = Some(first.id.clone());
        req.expected_revision = Some(1);
        req.fields.note = "new note".into();
        let second = storage.vocabulary_save(&req).unwrap().entry;
        assert_eq!(second.revision, 2);
        req.request_id = id();
        req.fields.note = "stale note".into();
        assert!(
            storage
                .vocabulary_save(&req)
                .unwrap_err()
                .contains("已有更新")
        );
        req.expected_revision = Some(2);
        req.occurrence.as_mut().unwrap().end = 999;
        assert!(storage.vocabulary_save(&req).is_err());
        assert_eq!(
            storage.vocabulary_get(&first.id).unwrap().fields.note,
            "new note"
        );
        req = request();
        req.request_id = id();
        req.occurrence.as_mut().unwrap().end = 999;
        assert!(storage.vocabulary_save(&req).is_err());
        assert_eq!(
            storage
                .vocabulary_list(&ListQuery::default())
                .unwrap()
                .total,
            1
        );
    }
    #[test]
    fn message_composite_key_and_chat_deletion_preserve_the_right_snapshot() {
        let storage = Storage::memory();
        storage.create("first", "main").unwrap();
        storage.create("second", "main").unwrap();
        storage.db.lock().unwrap().execute_batch("INSERT INTO messages(id,conversation_id,role,text,status) VALUES('same','first','assistant','How have you been?','complete'),('same','second','assistant','Different context','complete');").unwrap();
        let mut req = request();
        let src = req.occurrence.as_mut().unwrap();
        src.source_kind = "main".into();
        src.conversation_id = Some("first".into());
        src.message_id = Some("same".into());
        let entry = storage.vocabulary_save(&req).unwrap().entry;
        storage.delete("second").unwrap();
        assert_eq!(
            storage.vocabulary_get(&entry.id).unwrap().occurrences[0]
                .source
                .conversation_id
                .as_deref(),
            Some("first")
        );
        storage.delete("first").unwrap();
        let saved = storage.vocabulary_get(&entry.id).unwrap();
        assert!(saved.occurrences[0].source.conversation_id.is_none());
        assert!(saved.occurrences[0].source.message_id.is_none());
        assert_eq!(saved.occurrences[0].source.snapshot, "How have you been?");
    }
    #[test]
    fn trash_restore_purge_and_retries_are_transactional() {
        let storage = Storage::memory();
        let entry = storage.vocabulary_save(&request()).unwrap().entry;
        let mut op = EntryMutation {
            request_id: id(),
            id: entry.id.clone(),
            expected_revision: 1,
        };
        assert!(storage.vocabulary_mutate(&op, "purge").is_err());
        storage.vocabulary_mutate(&op, "trash").unwrap();
        storage.vocabulary_mutate(&op, "trash").unwrap();
        assert_eq!(
            storage
                .vocabulary_list(&ListQuery::default())
                .unwrap()
                .total,
            0
        );
        assert_eq!(
            storage
                .vocabulary_list(&ListQuery {
                    trash: true,
                    ..Default::default()
                })
                .unwrap()
                .total,
            1
        );
        op.request_id = id();
        op.expected_revision = 2;
        storage.vocabulary_mutate(&op, "restore").unwrap();
        assert_eq!(
            storage.vocabulary_get(&entry.id).unwrap().occurrences.len(),
            1
        );
        op.request_id = id();
        op.expected_revision = 3;
        storage.vocabulary_mutate(&op, "trash").unwrap();
        op.request_id = id();
        op.expected_revision = 4;
        storage.vocabulary_mutate(&op, "purge").unwrap();
        storage.vocabulary_mutate(&op, "purge").unwrap();
        assert!(storage.vocabulary_get(&entry.id).is_err());
        let count: i64 = storage
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM vocabulary_occurrences", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn nfc_search_preserves_case_and_language_filter_and_paginates() {
        let storage = Storage::memory();
        for i in 0..55 {
            let mut req = request();
            req.occurrence = None;
            req.fields.text = format!("cafe\u{301} {i}");
            req.fields.language = "fr".into();
            storage.vocabulary_save(&req).unwrap();
        }
        let query = ListQuery {
            search: "café".into(),
            language: "fr".into(),
            ..Default::default()
        };
        let page = storage.vocabulary_list(&query).unwrap();
        assert_eq!(page.total, 55);
        assert_eq!(page.entries.len(), 50);
        assert_eq!(
            storage
                .vocabulary_list(&ListQuery {
                    offset: 50,
                    ..query
                })
                .unwrap()
                .entries
                .len(),
            5
        );
        assert_eq!(
            storage
                .vocabulary_list(&ListQuery {
                    search: "Café".into(),
                    ..Default::default()
                })
                .unwrap()
                .total,
            0
        );
        assert_eq!(
            storage
                .vocabulary_list(&ListQuery {
                    search: "%_' OR 1=1 --".into(),
                    ..Default::default()
                })
                .unwrap()
                .total,
            0
        );
    }
    #[test]
    fn migration_upgrades_legacy_workspace_and_rolls_back_failed_additions() {
        let mut legacy = Connection::open_in_memory().unwrap();
        legacy
            .execute_batch(include_str!("../../migrations/001_workspace.sql"))
            .unwrap();
        legacy
            .execute(
                "INSERT INTO preferences VALUES(1,?1)",
                [r#"{"targetLanguage":"ja"}"#],
            )
            .unwrap();
        super::super::migrate(&mut legacy).unwrap();
        assert_eq!(
            legacy
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            legacy
                .query_row("SELECT value FROM preferences", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            r#"{"targetLanguage":"ja"}"#
        );
        let mut broken = Connection::open_in_memory().unwrap();
        broken
            .execute_batch(include_str!("../../migrations/001_workspace.sql"))
            .unwrap();
        broken.execute_batch("CREATE TABLE vocabulary_occurrences(original TEXT);INSERT INTO vocabulary_occurrences VALUES('preserve');").unwrap();
        assert!(super::super::migrate(&mut broken).is_err());
        assert_eq!(
            broken
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(broken.prepare("SELECT * FROM vocabulary_entries").is_err());
        assert_eq!(
            broken
                .query_row("SELECT original FROM vocabulary_occurrences", [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap(),
            "preserve"
        );
    }
    #[test]
    fn unicode_selection_requires_complete_graphemes_and_exact_occurrence() {
        let mut src = source();
        src.snapshot = "cafe\u{301} 🌍 你好 مرحبا cafe\u{301}".into();
        src.start = 0;
        src.end = 4;
        src.selected_text = "cafe".into();
        assert!(src.validate().is_err());
        src.end = 5;
        src.selected_text = "cafe\u{301}".into();
        src.validate().unwrap();
        src.start = 6;
        src.end = 7;
        src.selected_text = "🌍".into();
        src.validate().unwrap();
        src.start = src.snapshot.chars().count() - 5;
        src.end = src.snapshot.chars().count();
        src.selected_text = "cafe\u{301}".into();
        src.validate().unwrap();
        src.selected_text = "wrong".into();
        assert!(src.validate().is_err());
    }
    #[test]
    #[ignore = "local benchmark with a temporary on-disk database"]
    fn vocabulary_search_benchmark_10000() {
        let directory = tempfile::tempdir().unwrap();
        let storage = Storage::open(&directory.path().join("benchmark.sqlite3")).unwrap();
        {
            let mut db = storage.db.lock().unwrap();
            let tx = db.transaction().unwrap();
            for i in 0..10000 {
                let mut fields = fields();
                fields.text = format!("expression {i} café 日本語 العربية");
                fields.meaning =
                    format!("Meaning {i}: a realistic short explanation with context.");
                insert_entry(&tx, &id(), &fields, i).unwrap();
            }
            tx.commit().unwrap();
        }
        let mut elapsed = Vec::new();
        for i in 0..60 {
            let search = match i % 4 {
                0 => "café".to_owned(),
                1 => format!("expression {}", i * 100),
                2 => "absent phrase".into(),
                _ => "日本語".into(),
            };
            let start = std::time::Instant::now();
            let page = storage
                .vocabulary_list(&ListQuery {
                    search,
                    ..Default::default()
                })
                .unwrap();
            assert!(page.entries.len() <= 50);
            elapsed.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        elapsed.sort_by(f64::total_cmp);
        let p95 = elapsed[56];
        println!(
            "10,000 entries, on-disk SQLite, 60 mixed searches: P95={p95:.2} ms, max={:.2} ms",
            elapsed[59]
        );
        assert!(p95 < 300.0, "search P95 exceeded 300 ms");
    }
}
