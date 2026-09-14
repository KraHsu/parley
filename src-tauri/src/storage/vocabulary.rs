use super::learning_rows::{EntryRow, OccurrenceRow, instr, length, trim};
use super::schema::{
    messages as m, vocabulary_cards as c, vocabulary_drafts as d, vocabulary_entries as e,
    vocabulary_entry_tags as et, vocabulary_mutations as mutations, vocabulary_occurrences as o,
    vocabulary_tags as tags,
};
use super::{Storage, error, now};
use crate::vocabulary::*;
use diesel::{dsl::exists, prelude::*};
use serde::{Serialize, de::DeserializeOwned};

pub(super) fn get_entry(db: &mut SqliteConnection, id: &str) -> Result<Entry> {
    let mut entry: Entry = e::table
        .find(id)
        .select(EntryRow::as_select())
        .first::<EntryRow>(db)
        .optional()
        .map_err(error)?
        .ok_or("词句已不存在，请刷新列表。")?
        .into();
    entry.occurrences = o::table
        .filter(o::entry_id.eq(id))
        .order(o::rowid)
        .select(OccurrenceRow::as_select())
        .load::<OccurrenceRow>(db)
        .map_err(error)?
        .into_iter()
        .map(TryInto::try_into)
        .collect::<Result<_>>()?;
    entry.tags = super::review::get_tags(db, id)?;
    entry.cards = super::review::get_cards(db, id)?;
    Ok(entry)
}
pub(super) fn fingerprint(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value)
        .map(|s| digest(&s))
        .map_err(error)
}
pub(super) fn cached<T: DeserializeOwned>(
    db: &mut SqliteConnection,
    id: &str,
    operation: &str,
    hash: &str,
) -> Result<Option<T>> {
    uuid(id)?;
    let previous = mutations::table
        .find(id)
        .select((
            mutations::operation,
            mutations::fingerprint,
            mutations::result,
        ))
        .first::<(String, String, String)>(db)
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
    db: &mut SqliteConnection,
    id: &str,
    operation: &str,
    hash: &str,
    result: &impl Serialize,
) -> Result<()> {
    diesel::insert_into(mutations::table)
        .values((
            mutations::request_id.eq(id),
            mutations::operation.eq(operation),
            mutations::fingerprint.eq(hash),
            mutations::result.eq(serde_json::to_string(result).map_err(error)?),
            mutations::created_at.eq(now()),
        ))
        .execute(db)
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
    db: &mut SqliteConnection,
    id: &str,
    f: &EntryFields,
    time: i64,
) -> Result<()> {
    diesel::insert_into(e::table)
        .values((
            e::id.eq(id),
            e::language.eq(language(&f.language)?),
            e::language_label.eq(&f.language_label),
            e::kind.eq(&f.kind),
            e::text.eq(&f.text),
            e::lookup_key.eq(normalized(&f.text)),
            e::meaning.eq(&f.meaning),
            e::meaning_language.eq(language(&f.meaning_language)?),
            e::note.eq(&f.note),
            e::search_text.eq(f.search_text()),
            e::revision.eq(1_i64),
            e::created_at.eq(time),
            e::updated_at.eq(time),
        ))
        .execute(db)
        .map_err(error)?;
    Ok(())
}
pub(super) fn insert_source(
    db: &mut SqliteConnection,
    entry_id: &str,
    source: &Source,
) -> Result<()> {
    source.validate()?;
    let mut source = source.clone();
    let hash = source.fingerprint();
    if let (Some(conversation), Some(message)) = (&source.conversation_id, &source.message_id) {
        let found = diesel::select(exists(
            m::table
                .filter(m::conversation_id.eq(conversation))
                .filter(m::id.eq(message)),
        ))
        .get_result::<bool>(db)
        .map_err(error)?;
        if !found {
            source.conversation_id = None;
            source.message_id = None;
        }
    }
    diesel::insert_into(o::table)
        .values((
            o::id.eq(uuid::Uuid::new_v4().to_string()),
            o::entry_id.eq(entry_id),
            o::source_kind.eq(&source.source_kind),
            o::conversation_id.eq(&source.conversation_id),
            o::message_id.eq(&source.message_id),
            o::thread_id.eq(&source.thread_id),
            o::turn_id.eq(&source.turn_id),
            o::item_id.eq(&source.item_id),
            o::role.eq(&source.role),
            o::selected_text.eq(&source.selected_text),
            o::snapshot.eq(&source.snapshot),
            o::snapshot_hash.eq(digest(&source.snapshot)),
            o::start.eq(source.start as i64),
            o::end.eq(source.end as i64),
            o::locator_version.eq(i64::from(source.locator_version)),
            o::truncated.eq(i64::from(source.truncated)),
            o::fingerprint.eq(hash),
        ))
        .on_conflict((o::entry_id, o::fingerprint))
        .do_nothing()
        .execute(db)
        .map_err(error)?;
    Ok(())
}
fn clear_draft(db: &mut SqliteConnection, id: &Option<String>) -> Result<()> {
    if let Some(id) = id {
        diesel::delete(d::table.find(id))
            .execute(db)
            .map_err(error)?;
    }
    Ok(())
}
fn filtered(query: &ListQuery, time: i64) -> e::BoxedQuery<'_, diesel::sqlite::Sqlite> {
    let mut rows = e::table.into_boxed();
    rows = if query.trash {
        rows.filter(e::deleted_at.is_not_null())
    } else {
        rows.filter(e::deleted_at.is_null())
    };
    if !query.language.is_empty() {
        rows = rows.filter(e::language.eq(&query.language));
    }
    if !query.kind.is_empty() {
        rows = rows.filter(e::kind.eq(&query.kind));
    }
    if let Some(has_meaning) = query.has_meaning {
        rows = rows.filter(length(trim(e::meaning)).gt(0_i64).eq(has_meaning));
    }
    let search = normalized(query.search.trim());
    if !search.is_empty() {
        rows = rows.filter(instr(e::search_text, search).gt(0_i64));
    }
    if !query.tag.is_empty() {
        rows = rows.filter(exists(
            et::table
                .inner_join(tags::table.on(tags::id.eq(et::tag_id)))
                .filter(et::entry_id.eq(e::id))
                .filter(tags::name.eq(&query.tag)),
        ));
    }
    let cards = c::table.filter(c::entry_id.eq(e::id));
    match query.review_status.as_str() {
        "none" => rows = rows.filter(diesel::dsl::not(exists(cards))),
        "due" => {
            rows = rows
                .filter(length(trim(e::meaning)).gt(0_i64))
                .filter(exists(
                    cards
                        .filter(c::suspended.eq(0_i64))
                        .filter(c::due_at.le(time)),
                ))
        }
        "suspended" => rows = rows.filter(exists(cards.filter(c::suspended.eq(1_i64)))),
        "" => {}
        _ => rows = rows.filter(e::id.eq("")),
    }
    rows
}
impl Storage {
    pub fn vocabulary_get(&self, id: &str) -> Result<Entry> {
        self.repository(|db| get_entry(db, id))
    }
    pub fn vocabulary_list(&self, query: &ListQuery) -> Result<EntryPage> {
        bounded(&query.search, 2000, "搜索文字")?;
        bounded(&query.language, 100, "语言代码")?;
        self.repository(|db| {
            let time = now();
            let total = filtered(query, time)
                .count()
                .get_result::<i64>(db)
                .map_err(error)?;
            let rows = filtered(query, time);
            let rows = if query.sort == "created" {
                rows.order((e::created_at.desc(), e::id))
            } else {
                rows.order((e::updated_at.desc(), e::id))
            };
            let entries = rows
                .limit(50)
                .offset(i64::from(query.offset))
                .select(EntryRow::as_select())
                .load::<EntryRow>(db)
                .map_err(error)?
                .into_iter()
                .map(Into::into)
                .collect();
            Ok(EntryPage {
                entries,
                total,
                languages: e::table
                    .select(e::language)
                    .distinct()
                    .order(e::language)
                    .load(db)
                    .map_err(error)?,
                tags: tags::table
                    .select(tags::name)
                    .order(tags::name)
                    .load(db)
                    .map_err(error)?,
                active_count: e::table
                    .filter(e::deleted_at.is_null())
                    .count()
                    .get_result(db)
                    .map_err(error)?,
            })
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
        self.repository_transaction(|db| {
            if let Some(previous) = cached::<SaveResult>(db, &request.request_id, "save", &hash)? {
                return Ok(SaveResult {
                    entry: get_entry(db, &previous.entry.id)?,
                    duplicate: previous.duplicate,
                });
            }
            if request.id.is_none()
                && !request.allow_duplicate
                && let Some(source) = &request.occurrence
            {
                let existing = e::table
                    .inner_join(o::table.on(o::entry_id.eq(e::id)))
                    .filter(o::fingerprint.eq(source.fingerprint()))
                    .filter(e::language.eq(language(&request.fields.language)?))
                    .filter(e::deleted_at.is_null())
                    .order(e::created_at)
                    .select(e::id)
                    .first::<String>(db)
                    .optional()
                    .map_err(error)?;
                // Preserve the draft so a newly entered meaning is never silently discarded.
                if let Some(id) = existing {
                    return Ok(SaveResult {
                        entry: get_entry(db, &id)?,
                        duplicate: true,
                    });
                }
            }
            let id = request
                .id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            if request.id.is_some() {
                let current = get_entry(db, &id)?;
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
                diesel::update(e::table.find(&id))
                    .set((
                        e::language.eq(language(&f.language)?),
                        e::language_label.eq(&f.language_label),
                        e::kind.eq(&f.kind),
                        e::text.eq(&f.text),
                        e::lookup_key.eq(normalized(&f.text)),
                        e::meaning.eq(&f.meaning),
                        e::meaning_language.eq(language(&f.meaning_language)?),
                        e::note.eq(&f.note),
                        e::search_text.eq(f.search_text()),
                        e::revision.eq(e::revision + 1_i64),
                        e::updated_at.eq(now()),
                    ))
                    .execute(db)
                    .map_err(error)?;
            } else {
                insert_entry(db, &id, &request.fields, now())?;
            }
            if let Some(source) = &request.occurrence {
                insert_source(db, &id, source)?;
            }
            super::review::set_tags(db, &id, &request.tags)?;
            clear_draft(db, &request.draft_id)?;
            let result = SaveResult {
                entry: get_entry(db, &id)?,
                duplicate: false,
            };
            remember(db, &request.request_id, "save", &hash, &result)?;
            Ok(result)
        })
    }
    pub fn vocabulary_add_occurrence(&self, request: &AddOccurrence) -> Result<Entry> {
        request.source.validate()?;
        let hash = fingerprint(request)?;
        self.repository_transaction(|db| {
            if cached::<String>(db, &request.request_id, "occurrence", &hash)?.is_some() {
                return get_entry(db, &request.id);
            }
            let entry = get_entry(db, &request.id)?;
            check_revision(&entry, request.expected_revision)?;
            if entry.deleted_at.is_some() {
                return Err("请先恢复词句。".into());
            }
            insert_source(db, &request.id, &request.source)?;
            diesel::update(e::table.find(&request.id))
                .set((e::revision.eq(e::revision + 1_i64), e::updated_at.eq(now())))
                .execute(db)
                .map_err(error)?;
            clear_draft(db, &request.draft_id)?;
            remember(db, &request.request_id, "occurrence", &hash, &request.id)?;
            get_entry(db, &request.id)
        })
    }
    pub fn vocabulary_mutate(&self, request: &EntryMutation, operation: &str) -> Result<()> {
        let hash = fingerprint(request)?;
        self.repository_transaction(|db| {
            if cached::<bool>(db, &request.request_id, operation, &hash)?.is_some() {
                return Ok(());
            }
            let entry = get_entry(db, &request.id)?;
            check_revision(&entry, request.expected_revision)?;
            match operation {
                "trash" | "restore" => {
                    let time = now();
                    diesel::update(e::table.find(&request.id))
                        .set((
                            e::deleted_at.eq((operation == "trash").then_some(time)),
                            e::updated_at.eq(time),
                            e::revision.eq(e::revision + 1_i64),
                        ))
                        .execute(db)
                        .map_err(error)?;
                }
                "purge" => {
                    if entry.deleted_at.is_none() {
                        return Err("请先将词句移入回收站。".into());
                    }
                    diesel::delete(e::table.find(&request.id))
                        .execute(db)
                        .map_err(error)?;
                }
                _ => return Err("未知词句操作。".into()),
            }
            remember(db, &request.request_id, operation, &hash, &true)
        })
    }
    pub fn vocabulary_save_draft(&self, draft: &EditDraft) -> Result<()> {
        uuid(&draft.id)?;
        uuid(&draft.request_id)?;
        draft.fields.validate(true)?;
        bounded(&draft.tag_text, 1200, "标签草稿")?;
        if let Some(source) = &draft.occurrence {
            source.validate()?;
        }
        self.repository_transaction(|db| {
            if diesel::select(exists(mutations::table.find(&draft.request_id)))
                .get_result::<bool>(db)
                .map_err(error)?
            {
                return Ok(());
            }
            let fields = (
                d::entry_id.eq(&draft.entry_id),
                d::payload.eq(serde_json::to_string(draft).map_err(error)?),
                d::updated_at.eq(now()),
            );
            diesel::insert_into(d::table)
                .values((d::id.eq(&draft.id), fields.clone()))
                .on_conflict(d::id)
                .do_update()
                .set(fields)
                .execute(db)
                .map_err(error)?;
            Ok(())
        })
    }
    pub fn vocabulary_load_drafts(&self) -> Result<Vec<EditDraft>> {
        self.repository(|db| {
            d::table
                .select(d::payload)
                .order(d::updated_at.desc())
                .load::<String>(db)
                .map_err(error)?
                .into_iter()
                .map(|s| serde_json::from_str(&s).map_err(error))
                .collect()
        })
    }
    pub fn vocabulary_discard_draft(&self, id: &str) -> Result<()> {
        self.repository(|db| {
            diesel::delete(d::table.find(id))
                .execute(db)
                .map_err(error)?;
            Ok(())
        })
    }
}
#[cfg(test)]
mod tests {
    use super::super::test_support::{integer, text};
    use super::*;
    use diesel::connection::SimpleConnection;
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
        storage.db.lock().unwrap().batch_execute("INSERT INTO messages(id,conversation_id,role,text,status) VALUES('same','first','assistant','How have you been?','complete'),('same','second','assistant','Different context','complete');").unwrap();
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
        let count = o::table
            .count()
            .get_result::<i64>(&mut *storage.db.lock().unwrap())
            .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn source_insertion_order_survives_backup_and_combined_filters() {
        let storage = Storage::memory();
        let mut req = request();
        req.tags = vec!["daily".into()];
        let first = storage.vocabulary_save(&req).unwrap().entry;
        let mut second = source();
        second.snapshot.push_str(" — second example");
        let updated = storage
            .vocabulary_add_occurrence(&AddOccurrence {
                request_id: id(),
                id: first.id.clone(),
                expected_revision: 1,
                source: second.clone(),
                draft_id: None,
            })
            .unwrap();
        storage
            .repository(|db| {
                diesel::update(o::table.find(&updated.occurrences[0].id))
                    .set(o::id.eq("ffffffff-ffff-4fff-8fff-ffffffffffff"))
                    .execute(db)
                    .map_err(error)?;
                diesel::update(o::table.find(&updated.occurrences[1].id))
                    .set(o::id.eq("00000000-0000-4000-8000-000000000001"))
                    .execute(db)
                    .map_err(error)?;
                Ok(())
            })
            .unwrap();
        let backup = storage.vocabulary_backup(false).unwrap();
        let target = Storage::memory();
        let (_, pending) = target.vocabulary_preview(backup.clone()).unwrap();
        target.vocabulary_import(&pending, &id(), false).unwrap();
        let occurrences = target.vocabulary_get(&first.id).unwrap().occurrences;
        assert_eq!(occurrences[0].source, source());
        assert_eq!(occurrences[1].source, second);
        assert_eq!(
            serde_json::to_value(&target.vocabulary_backup(false).unwrap().entries).unwrap(),
            serde_json::to_value(&backup.entries).unwrap()
        );
        let mut filter = ListQuery {
            language: "en".into(),
            kind: "phrase".into(),
            has_meaning: Some(true),
            search: "have you".into(),
            tag: "daily".into(),
            review_status: "none".into(),
            ..Default::default()
        };
        assert_eq!(target.vocabulary_list(&filter).unwrap().total, 1);
        target
            .vocabulary_card_save(&crate::review::CardRequest {
                request_id: id(),
                entry_id: first.id.clone(),
                direction: "recognition".into(),
                suspended: false,
                reset: false,
                expected_revision: None,
            })
            .unwrap();
        assert_eq!(target.vocabulary_list(&filter).unwrap().total, 0);
        filter.review_status = "due".into();
        assert_eq!(target.vocabulary_list(&filter).unwrap().total, 1);
        filter.has_meaning = Some(false);
        assert_eq!(target.vocabulary_list(&filter).unwrap().total, 0);
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
        let mut legacy = super::super::typed::connect(":memory:").unwrap();
        legacy
            .batch_execute(include_str!("../../migrations/001_workspace.sql"))
            .unwrap();
        diesel::insert_into(super::super::schema::preferences::table)
            .values((
                super::super::schema::preferences::id.eq(1_i64),
                super::super::schema::preferences::value.eq(r#"{"targetLanguage":"ja"}"#),
            ))
            .execute(&mut legacy)
            .unwrap();
        super::super::migrate(&mut legacy).unwrap();
        assert_eq!(super::super::schema_version(&mut legacy).unwrap(), 5);
        assert_eq!(
            text(&mut legacy, "SELECT value FROM preferences"),
            r#"{"targetLanguage":"ja"}"#
        );
        let mut broken = super::super::typed::connect(":memory:").unwrap();
        broken
            .batch_execute(include_str!("../../migrations/001_workspace.sql"))
            .unwrap();
        broken.batch_execute("CREATE TABLE vocabulary_occurrences(original TEXT);INSERT INTO vocabulary_occurrences VALUES('preserve');").unwrap();
        assert!(super::super::migrate(&mut broken).is_err());
        assert_eq!(super::super::schema_version(&mut broken).unwrap(), 1);
        assert_eq!(
            integer(
                &mut broken,
                "SELECT count(*) AS value FROM sqlite_master WHERE name='vocabulary_entries'"
            ),
            0
        );
        assert_eq!(
            text(
                &mut broken,
                "SELECT original AS value FROM vocabulary_occurrences"
            ),
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
        storage
            .repository_transaction(|db| {
                for i in 0..10000 {
                    let mut fields = fields();
                    fields.text = format!("expression {i} café 日本語 العربية");
                    fields.meaning =
                        format!("Meaning {i}: a realistic short explanation with context.");
                    insert_entry(db, &id(), &fields, i)?;
                }
                Ok(())
            })
            .unwrap();
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
