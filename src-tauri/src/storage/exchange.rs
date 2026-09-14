use super::learning_rows::CardRow;
use super::review::set_tags;
use super::schema::{
    vocabulary_cards as c, vocabulary_entries as e, vocabulary_import_records as imports,
    vocabulary_metadata as metadata, vocabulary_occurrences as o, vocabulary_reviews as r,
};
use super::vocabulary::{cached, fingerprint, get_entry, insert_entry, insert_source, remember};
use super::{Storage, error, now};
use crate::exchange::*;
use crate::vocabulary::Result;
use diesel::{dsl::exists, prelude::*};
use std::collections::HashMap;

#[derive(Queryable, Selectable)]
#[diesel(table_name=r)]
struct ReviewRow {
    id: String,
    card_id: String,
    rating: String,
    reviewed_at: i64,
    before_state: String,
    after_state: String,
    undone_at: Option<i64>,
}
fn backup_entry(db: &mut SqliteConnection, id: &str) -> Result<BackupEntry> {
    let entry = get_entry(db, id)?;
    let mut occurrences = entry.occurrences;
    for occurrence in &mut occurrences {
        occurrence.source.conversation_id = None;
        occurrence.source.message_id = None;
    }
    let reviews = r::table
        .inner_join(c::table.on(c::id.eq(r::card_id)))
        .filter(c::entry_id.eq(id))
        .order((r::reviewed_at, r::id))
        .select(ReviewRow::as_select())
        .load::<ReviewRow>(db)
        .map_err(error)?
        .into_iter()
        .map(|r| {
            Ok(BackupReview {
                id: r.id,
                card_id: r.card_id,
                rating: r.rating,
                reviewed_at: r.reviewed_at,
                before_state: serde_json::from_str(&r.before_state).map_err(error)?,
                after_state: serde_json::from_str(&r.after_state).map_err(error)?,
                undone_at: r.undone_at,
            })
        })
        .collect::<Result<_>>()?;
    Ok(BackupEntry {
        id: entry.id,
        fields: entry.fields,
        created_at: entry.created_at,
        updated_at: entry.updated_at,
        deleted_at: entry.deleted_at,
        occurrences,
        tags: entry.tags,
        cards: entry.cards,
        reviews,
    })
}
fn entry_fingerprint(entry: &BackupEntry) -> Result<String> {
    let mut entry = entry.clone();
    for occurrence in &mut entry.occurrences {
        occurrence.source = occurrence.source.legacy_identity();
    }
    fingerprint(&entry)
}

#[derive(serde::Serialize)]
struct Decision {
    kind: &'static str,
    hash: String,
    existing: Option<String>,
    current_hash: Option<String>,
}
fn decisions(db: &mut SqliteConnection, backup: &Backup) -> Result<Vec<Decision>> {
    backup
        .entries
        .iter()
        .map(|entry| {
            let hash = entry_fingerprint(entry)?;
            let mapped = imports::table
                .find((&backup.dataset_id, &entry.id, &hash))
                .select(imports::local_id)
                .first::<String>(db)
                .optional()
                .map_err(error)?;
            if let Some(existing) = mapped {
                return Ok(Decision {
                    kind: "duplicate",
                    hash,
                    current_hash: Some(entry_fingerprint(&backup_entry(db, &existing)?)?),
                    existing: Some(existing),
                });
            }
            let previous = imports::table
                .filter(imports::dataset_id.eq(&backup.dataset_id))
                .filter(imports::record_id.eq(&entry.id))
                .order(imports::rowid.desc())
                .select(imports::local_id)
                .first::<String>(db)
                .optional()
                .map_err(error)?;
            let existing = match previous {
                Some(id) => Some(id),
                None => e::table
                    .find(&entry.id)
                    .select(e::id)
                    .first::<String>(db)
                    .optional()
                    .map_err(error)?,
            };
            let current_hash = existing
                .as_ref()
                .map(|id| backup_entry(db, id).and_then(|entry| entry_fingerprint(&entry)))
                .transpose()?;
            let kind = if current_hash.as_deref() == Some(&hash) {
                "duplicate"
            } else if existing.is_some() {
                "conflict"
            } else {
                "add"
            };
            Ok(Decision {
                kind,
                hash,
                existing,
                current_hash,
            })
        })
        .collect()
}
fn free_id(wanted: &str, copy: bool, exists: bool) -> String {
    if copy || exists {
        uuid::Uuid::new_v4().to_string()
    } else {
        wanted.to_owned()
    }
}
fn insert_record(db: &mut SqliteConnection, record: &BackupEntry, copy: bool) -> Result<String> {
    let id = free_id(
        &record.id,
        copy,
        diesel::select(exists(e::table.find(&record.id)))
            .get_result::<bool>(db)
            .map_err(error)?,
    );
    insert_entry(db, &id, &record.fields, record.created_at)?;
    diesel::update(e::table.find(&id))
        .set((
            e::updated_at.eq(record.updated_at),
            e::deleted_at.eq(record.deleted_at),
        ))
        .execute(db)
        .map_err(error)?;
    set_tags(db, &id, &record.tags)?;
    for occurrence in &record.occurrences {
        let mut source = occurrence.source.clone();
        source.conversation_id = None;
        source.message_id = None;
        let source_hash = insert_source(db, &id, &source)?;
        let occurrence_id = free_id(
            &occurrence.id,
            copy,
            diesel::select(exists(o::table.find(&occurrence.id)))
                .get_result::<bool>(db)
                .map_err(error)?,
        );
        diesel::update(
            o::table
                .filter(o::entry_id.eq(&id))
                .filter(o::fingerprint.eq(source_hash)),
        )
        .set(o::id.eq(occurrence_id))
        .execute(db)
        .map_err(error)?;
    }
    let mut cards = HashMap::new();
    for card in &record.cards {
        let card_id = free_id(
            &card.id,
            copy,
            diesel::select(exists(c::table.find(&card.id)))
                .get_result::<bool>(db)
                .map_err(error)?,
        );
        let mut row = CardRow::from(card);
        row.id = card_id.clone();
        row.entry_id = id.clone();
        diesel::insert_into(c::table)
            .values(row)
            .execute(db)
            .map_err(error)?;
        cards.insert(card.id.clone(), card_id);
    }
    for review in &record.reviews {
        let card_id = cards.get(&review.card_id).ok_or("复习记录缺少卡片。")?;
        let mut before = review.before_state.clone();
        before.id = card_id.clone();
        before.entry_id = id.clone();
        let mut after = review.after_state.clone();
        after.id = card_id.clone();
        after.entry_id = id.clone();
        let review_id = free_id(
            &review.id,
            copy,
            diesel::select(exists(r::table.find(&review.id)))
                .get_result::<bool>(db)
                .map_err(error)?,
        );
        diesel::insert_into(r::table)
            .values((
                r::id.eq(review_id),
                r::card_id.eq(card_id),
                r::request_id.eq(uuid::Uuid::new_v4().to_string()),
                r::rating.eq(&review.rating),
                r::reviewed_at.eq(review.reviewed_at),
                r::before_state.eq(serde_json::to_string(&before).map_err(error)?),
                r::after_state.eq(serde_json::to_string(&after).map_err(error)?),
                r::undone_at.eq(review.undone_at),
            ))
            .execute(db)
            .map_err(error)?;
    }
    Ok(id)
}
impl Storage {
    pub fn vocabulary_backup(&self, include_trash: bool) -> Result<Backup> {
        self.repository_transaction(|db| {
            diesel::insert_into(metadata::table)
                .values((
                    metadata::key.eq("dataset_id"),
                    metadata::value.eq(uuid::Uuid::new_v4().to_string()),
                ))
                .on_conflict(metadata::key)
                .do_nothing()
                .execute(db)
                .map_err(error)?;
            let dataset_id = metadata::table
                .find("dataset_id")
                .select(metadata::value)
                .first(db)
                .map_err(error)?;
            let mut rows = e::table.into_boxed();
            if !include_trash {
                rows = rows.filter(e::deleted_at.is_null());
            }
            let ids = rows
                .order((e::created_at, e::id))
                .select(e::id)
                .load::<String>(db)
                .map_err(error)?;
            let entries = ids
                .into_iter()
                .map(|id| backup_entry(db, &id))
                .collect::<Result<_>>()?;
            Ok(Backup {
                format: "parley-vocabulary".into(),
                version: 2,
                dataset_id,
                exported_at: now(),
                entries,
            })
        })
    }
    pub fn vocabulary_preview(&self, backup: Backup) -> Result<(ImportPreview, PendingImport)> {
        backup.validate()?;
        self.repository(|db| {
            let decisions = decisions(db, &backup)?;
            let token = uuid::Uuid::new_v4().to_string();
            let preview = ImportPreview {
                token: token.clone(),
                entries: backup.entries.len(),
                added: decisions.iter().filter(|d| d.kind == "add").count(),
                duplicates: decisions.iter().filter(|d| d.kind == "duplicate").count(),
                conflicts: decisions.iter().filter(|d| d.kind == "conflict").count(),
                conflict_examples: backup
                    .entries
                    .iter()
                    .zip(&decisions)
                    .filter(|(_, d)| d.kind == "conflict")
                    .take(20)
                    .map(|(entry, _)| entry.fields.text.chars().take(100).collect())
                    .collect(),
            };
            Ok((
                preview,
                PendingImport {
                    token,
                    signature: fingerprint(&decisions)?,
                    backup,
                },
            ))
        })
    }
    pub fn vocabulary_import(
        &self,
        pending: &PendingImport,
        request_id: &str,
        copy_conflicts: bool,
    ) -> Result<ImportResult> {
        let hash = fingerprint(&(&pending.token, copy_conflicts))?;
        self.repository_transaction(|db| {
            if let Some(result) = cached(db, request_id, "import", &hash)? {
                return Ok(result);
            }
            let decisions = decisions(db, &pending.backup)?;
            if fingerprint(&decisions)? != pending.signature {
                return Err("词句在预览后已有变化，请重新选择文件并预览。".into());
            }
            let mut result = ImportResult {
                added: 0,
                duplicates: 0,
                skipped: 0,
            };
            for (record, decision) in pending.backup.entries.iter().zip(&decisions) {
                let id = match decision.kind {
                    "duplicate" => {
                        result.duplicates += 1;
                        decision.existing.clone().ok_or("重复记录缺少映射。")?
                    }
                    "conflict" if !copy_conflicts => {
                        result.skipped += 1;
                        continue;
                    }
                    _ => {
                        let id = insert_record(db, record, decision.kind == "conflict")?;
                        result.added += 1;
                        id
                    }
                };
                diesel::insert_into(imports::table)
                    .values((
                        imports::dataset_id.eq(&pending.backup.dataset_id),
                        imports::record_id.eq(&record.id),
                        imports::content_hash.eq(&decision.hash),
                        imports::local_id.eq(id),
                    ))
                    .on_conflict_do_nothing()
                    .execute(db)
                    .map_err(error)?;
            }
            remember(db, request_id, "import", &hash, &result)?;
            Ok(result)
        })
    }
}
fn cell(text: &str) -> String {
    let formula = text
        .trim_start()
        .starts_with(['=', '+', '-', '@', '\t', '\r']);
    let text = if formula {
        format!("'{text}")
    } else {
        text.to_owned()
    };
    format!("\"{}\"", text.replace('"', "\"\""))
}
pub fn csv(backup: &Backup) -> String {
    let mut output = "\u{feff}language,text,meaning,note,sentence,tags\r\n".to_owned();
    for record in &backup.entries {
        let sentence = record
            .occurrences
            .first()
            .map(|o| o.source.snapshot.as_str())
            .unwrap_or("");
        let tags = record.tags.join("; ");
        output.push_str(
            &[
                &record.fields.language,
                &record.fields.text,
                &record.fields.meaning,
                &record.fields.note,
                sentence,
                &tags,
            ]
            .map(cell)
            .join(","),
        );
        output.push_str("\r\n");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::{CardRequest, GradeRequest, TagsRequest};
    use crate::vocabulary::{EntryFields, ListQuery, SaveRequest, Source};
    use diesel::connection::SimpleConnection;
    fn id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
    fn populated() -> Storage {
        let storage = Storage::memory();
        let entry = storage
            .vocabulary_save(&SaveRequest {
                request_id: id(),
                id: None,
                expected_revision: None,
                fields: EntryFields {
                    language: "fr".into(),
                    language_label: "Français".into(),
                    kind: "word".into(),
                    text: "café".into(),
                    meaning: "咖啡馆".into(),
                    meaning_language: "zh-CN".into(),
                    note: "العربية\n带有\"引号\"".into(),
                },
                occurrence: Some(Source {
                    source_kind: "terminal".into(),
                    conversation_id: None,
                    message_id: None,
                    thread_id: Some("external-thread".into()),
                    turn_id: Some("turn".into()),
                    item_id: Some("item".into()),
                    role: "assistant".into(),
                    selected_text: "café".into(),
                    snapshot: "Un café ☕".into(),
                    start: 3,
                    end: 7,
                    backend: None,
                    locator_version: 1,
                    truncated: false,
                }),
                draft_id: None,
                allow_duplicate: false,
                tags: vec![],
            })
            .unwrap()
            .entry;
        storage
            .vocabulary_tags_save(&TagsRequest {
                request_id: id(),
                entry_id: entry.id.clone(),
                expected_revision: 1,
                tags: vec!["日常".into()],
            })
            .unwrap();
        let card = storage
            .vocabulary_card_save(&CardRequest {
                request_id: id(),
                entry_id: entry.id,
                direction: "recognition".into(),
                suspended: false,
                reset: false,
                expected_revision: None,
            })
            .unwrap();
        storage
            .vocabulary_review_grade(&GradeRequest {
                request_id: id(),
                card_id: card.id,
                expected_revision: 1,
                rating: "remembered".into(),
            })
            .unwrap();
        storage
    }
    #[test]
    fn json_roundtrip_restores_sources_tags_cards_and_history_without_credentials() {
        let source = populated();
        let backup = source.vocabulary_backup(false).unwrap();
        let bytes = serde_json::to_vec(&backup).unwrap();
        let parsed = Backup::parse(&bytes).unwrap();
        let target = Storage::memory();
        let (preview, pending) = target.vocabulary_preview(parsed).unwrap();
        assert_eq!(preview.added, 1);
        let request = id();
        assert_eq!(
            target
                .vocabulary_import(&pending, &request, false)
                .unwrap()
                .added,
            1
        );
        assert_eq!(
            target
                .vocabulary_import(&pending, &request, false)
                .unwrap()
                .added,
            1
        );
        let restored = target.vocabulary_backup(false).unwrap();
        assert_eq!(
            serde_json::to_value(&restored.entries).unwrap(),
            serde_json::to_value(&backup.entries).unwrap()
        );
        let (preview, pending) = target.vocabulary_preview(backup.clone()).unwrap();
        assert_eq!(preview.duplicates, 1);
        assert_eq!(
            target
                .vocabulary_import(&pending, &id(), false)
                .unwrap()
                .duplicates,
            1
        );
        let json = String::from_utf8(bytes).unwrap();
        for forbidden in ["codexPath", "account", "email", "auth.json"] {
            assert!(!json.contains(forbidden))
        }
        assert_eq!(
            target.vocabulary_list(&ListQuery::default()).unwrap().total,
            1
        );
    }
    #[test]
    fn conflicts_preserve_local_edits_or_copy_with_independent_card_ids() {
        let source = populated();
        let backup = source.vocabulary_backup(false).unwrap();
        let target = Storage::memory();
        let (_, pending) = target.vocabulary_preview(backup.clone()).unwrap();
        target.vocabulary_import(&pending, &id(), false).unwrap();
        let mut changed = backup.clone();
        changed.entries[0].fields.note = "new backup note".into();
        let (preview, pending) = target.vocabulary_preview(changed.clone()).unwrap();
        assert_eq!(preview.conflicts, 1);
        assert_eq!(
            target
                .vocabulary_import(&pending, &id(), false)
                .unwrap()
                .skipped,
            1
        );
        assert_ne!(
            target
                .vocabulary_get(&backup.entries[0].id)
                .unwrap()
                .fields
                .note,
            "new backup note"
        );
        let (_, pending) = target.vocabulary_preview(changed.clone()).unwrap();
        assert_eq!(
            target
                .vocabulary_import(&pending, &id(), true)
                .unwrap()
                .added,
            1
        );
        let copies = target.vocabulary_backup(false).unwrap();
        assert_eq!(copies.entries.len(), 2);
        assert_ne!(copies.entries[0].cards[0].id, copies.entries[1].cards[0].id);
        let (preview, _) = target.vocabulary_preview(changed).unwrap();
        assert_eq!(preview.duplicates, 1);
    }
    #[test]
    fn rejects_changed_database_after_preview_and_rolls_back_mid_import_failure() {
        let source = populated();
        let backup = source.vocabulary_backup(false).unwrap();
        let target = Storage::memory();
        let (_, pending) = target.vocabulary_preview(backup.clone()).unwrap();
        // Simulate an actual write failure inside the transaction, after entry insertion.
        target.db.lock().unwrap().batch_execute("CREATE TRIGGER fail_source BEFORE INSERT ON vocabulary_occurrences BEGIN SELECT RAISE(ABORT,'disk-like failure'); END;").unwrap();
        assert!(target.vocabulary_import(&pending, &id(), false).is_err());
        assert_eq!(
            target.vocabulary_list(&ListQuery::default()).unwrap().total,
            0
        );
        target
            .db
            .lock()
            .unwrap()
            .batch_execute("DROP TRIGGER fail_source")
            .unwrap();
        target.vocabulary_import(&pending, &id(), false).unwrap();
        let (_, pending) = target.vocabulary_preview(backup).unwrap();
        diesel::update(e::table)
            .set(e::note.eq("changed after preview"))
            .execute(&mut *target.db.lock().unwrap())
            .unwrap();
        assert!(
            target
                .vocabulary_import(&pending, &id(), false)
                .unwrap_err()
                .contains("预览后已有变化")
        );
    }
    #[test]
    fn validates_unknown_versions_oversize_orphans_and_corrupted_sources() {
        let backup = populated().vocabulary_backup(false).unwrap();
        let mut changed = backup.clone();
        changed.version = 999;
        assert!(changed.validate().is_err());
        let mut changed = backup.clone();
        changed.entries[0].occurrences[0].source.end = 900;
        assert!(changed.validate().is_err());
        let mut changed = backup.clone();
        changed.entries[0].reviews[0].card_id = id();
        assert!(changed.validate().is_err());
        let mut changed = backup.clone();
        changed.entries[0].cards[0].schedule_version = 9;
        assert!(changed.validate().is_err());
        assert!(Backup::parse(b"not json").is_err());
        assert!(Backup::parse(&vec![b' '; MAX_BYTES as usize + 1]).is_err());
        let mut value = serde_json::to_value(backup).unwrap();
        value["unexpected"] = "field".into();
        assert!(Backup::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    fn legacy(backup: &Backup) -> Backup {
        let mut backup = backup.clone();
        backup.version = 1;
        for entry in &mut backup.entries {
            for occurrence in &mut entry.occurrences {
                occurrence.source.backend = None;
            }
        }
        backup.validate().unwrap();
        backup
    }

    fn downgrade_to_v5(storage: &Storage) {
        let mut db = storage.db.lock().unwrap();
        // Recreate the exact previous schema, without changing historical import keys.
        db.batch_execute(
            "DROP TABLE conversation_context; ALTER TABLE vocabulary_occurrences DROP COLUMN backend; PRAGMA user_version=5;",
        )
        .unwrap();
    }

    #[test]
    fn v1_import_keys_survive_upgrade_including_conflict_copies() {
        let old = legacy(&populated().vocabulary_backup(false).unwrap());
        // Frozen v1 wire identity; adding public metadata must not change this key.
        assert_eq!(
            old.entries[0].occurrences[0].source.fingerprint(),
            "2d1d5a6fe7027d11a6cd0c60c900f68b17738bd4086b45d9928d8f39647946ba"
        );
        assert_eq!(
            fingerprint(&old.entries[0]).unwrap(),
            entry_fingerprint(&old.entries[0]).unwrap()
        );
        let target = Storage::memory();
        let (_, pending) = target.vocabulary_preview(old.clone()).unwrap();
        target.vocabulary_import(&pending, &id(), false).unwrap();
        let mut changed = old.clone();
        changed.entries[0].fields.note = "updated export".into();
        let (_, pending) = target.vocabulary_preview(changed.clone()).unwrap();
        target.vocabulary_import(&pending, &id(), true).unwrap();
        let before = target.vocabulary_backup(false).unwrap();
        let hashes = o::table
            .select((o::id, o::fingerprint))
            .order(o::rowid)
            .load::<(String, String)>(&mut *target.db.lock().unwrap())
            .unwrap();
        downgrade_to_v5(&target);
        super::super::migrate(&mut target.db.lock().unwrap()).unwrap();
        assert_eq!(
            hashes,
            o::table
                .select((o::id, o::fingerprint))
                .order(o::rowid)
                .load::<(String, String)>(&mut *target.db.lock().unwrap())
                .unwrap()
        );
        assert_eq!(
            serde_json::to_value(before.entries).unwrap(),
            serde_json::to_value(target.vocabulary_backup(false).unwrap().entries).unwrap()
        );
        for backup in [old, changed] {
            let (preview, pending) = target.vocabulary_preview(backup).unwrap();
            assert_eq!(
                (preview.duplicates, preview.added, preview.conflicts),
                (1, 0, 0)
            );
            assert_eq!(
                target
                    .vocabulary_import(&pending, &id(), true)
                    .unwrap()
                    .duplicates,
                1
            );
        }
        let v2 = target.vocabulary_backup(false).unwrap();
        assert_eq!(v2.version, 2);
        let legacy_claude_storage = populated();
        let mut legacy_claude = legacy(&legacy_claude_storage.vocabulary_backup(false).unwrap());
        legacy_claude.entries[0].occurrences[0].source.thread_id =
            Some("claude-code:older-session".into());
        let legacy_hash = fingerprint(&legacy_claude.entries[0]).unwrap();
        let claude_target = Storage::memory();
        let (_, pending) = claude_target
            .vocabulary_preview(legacy_claude.clone())
            .unwrap();
        claude_target
            .vocabulary_import(&pending, &id(), false)
            .unwrap();
        let enriched = claude_target.vocabulary_backup(false).unwrap();
        assert_eq!(
            entry_fingerprint(&enriched.entries[0]).unwrap(),
            legacy_hash
        );
        // Own-dataset imports may have no import map yet; content still matches.
        diesel::delete(imports::table)
            .execute(&mut *claude_target.db.lock().unwrap())
            .unwrap();
        assert_eq!(
            claude_target
                .vocabulary_preview(legacy_claude)
                .unwrap()
                .0
                .duplicates,
            1
        );
        let other = Storage::memory();
        let (_, pending) = other.vocabulary_preview(v2.clone()).unwrap();
        other.vocabulary_import(&pending, &id(), false).unwrap();
        assert_eq!(other.vocabulary_preview(v2).unwrap().0.duplicates, 2);
        assert_eq!(
            target.vocabulary_list(&ListQuery::default()).unwrap().total,
            2
        );
    }

    fn source_request(source: Source) -> SaveRequest {
        SaveRequest {
            request_id: id(),
            id: None,
            expected_revision: None,
            fields: populated()
                .vocabulary_backup(false)
                .unwrap()
                .entries
                .remove(0)
                .fields,
            occurrence: Some(source),
            draft_id: None,
            allow_duplicate: false,
            tags: vec![],
        }
    }

    #[test]
    fn source_namespaces_language_and_trash_do_not_cross_deduplicate() {
        use crate::backends::types::{BackendKind, Provider};
        use crate::vocabulary::SourceBackend;
        let storage = populated();
        let old = storage.vocabulary_backup(false).unwrap().entries[0].occurrences[0]
            .source
            .clone();
        let mut request = source_request(old.clone());
        assert!(storage.vocabulary_save(&request).unwrap().duplicate);
        request.occurrence.as_mut().unwrap().backend = Some(SourceBackend {
            kind: BackendKind::ClaudeCode,
            provider: Provider::Anthropic,
            model: None,
        });
        let claude = storage.vocabulary_save(&request).unwrap();
        assert!(!claude.duplicate);
        assert_eq!(
            claude.entry.occurrences[0].source.thread_id.as_deref(),
            Some("claude-code:external-thread")
        );
        request.request_id = id();
        assert!(storage.vocabulary_save(&request).unwrap().duplicate);
        request.fields.language = "en".into();
        request.request_id = id();
        let english = storage.vocabulary_save(&request).unwrap();
        assert!(!english.duplicate);
        request.request_id = id();
        assert_eq!(
            storage.vocabulary_save(&request).unwrap().entry.id,
            english.entry.id
        );
        diesel::update(e::table.find(&english.entry.id))
            .set(e::deleted_at.eq(Some(now())))
            .execute(&mut *storage.db.lock().unwrap())
            .unwrap();
        request.request_id = id();
        assert!(!storage.vocabulary_save(&request).unwrap().duplicate);
        let backup = storage.vocabulary_backup(true).unwrap();
        let target = Storage::memory();
        let (_, pending) = target.vocabulary_preview(backup.clone()).unwrap();
        target.vocabulary_import(&pending, &id(), false).unwrap();
        assert_eq!(
            serde_json::to_value(&backup.entries).unwrap(),
            serde_json::to_value(target.vocabulary_backup(true).unwrap().entries).unwrap()
        );
    }

    #[test]
    fn source_migration_uses_historical_binding_and_rolls_back_invalid_metadata() {
        use super::super::schema::{
            backend_profile_versions as v, conversations as c, messages as m,
        };
        use crate::backends::types::{BackendKind, ProfileConfig, Provider, SaveProfile};
        let storage = populated();
        let profile = storage
            .save_backend_profile(SaveProfile {
                id: None,
                expected_revision: None,
                config: ProfileConfig {
                    name: "Original".into(),
                    kind: BackendKind::AnthropicMessages,
                    provider: Provider::Anthropic,
                    endpoint: "https://api.anthropic.com/v1".into(),
                    binary_path: String::new(),
                    enabled: true,
                },
            })
            .unwrap();
        storage
            .create_for_backend("local-conversation", "main", &profile.id)
            .unwrap();
        {
            let mut db = storage.db.lock().unwrap();
            diesel::update(c::table.find("local-conversation"))
                .set(c::model.eq("original-model"))
                .execute(&mut *db)
                .unwrap();
            diesel::insert_into(m::table)
                .values((
                    m::id.eq("message"),
                    m::conversation_id.eq("local-conversation"),
                    m::role.eq("assistant"),
                    m::text.eq("Un café ☕"),
                    m::status.eq("complete"),
                ))
                .execute(&mut *db)
                .unwrap();
        }
        let mut source = storage.vocabulary_backup(false).unwrap().entries[0].occurrences[0]
            .source
            .clone();
        source.source_kind = "main".into();
        source.conversation_id = Some("local-conversation".into());
        source.message_id = Some("message".into());
        source.backend = None;
        let request = source_request(source.clone());
        let saved = storage.vocabulary_save(&request).unwrap().entry;
        assert_eq!(
            saved.occurrences[0]
                .source
                .backend
                .as_ref()
                .unwrap()
                .model
                .as_deref(),
            Some("original-model")
        );
        let mut spoofed = request.clone();
        spoofed.request_id = id();
        spoofed.occurrence.as_mut().unwrap().backend = Some(crate::vocabulary::SourceBackend {
            kind: BackendKind::Codex,
            provider: Provider::Openai,
            model: None,
        });
        assert!(storage.vocabulary_save(&spoofed).is_err());
        let mut updated = profile.config.clone();
        updated.name = "Renamed service".into();
        updated.endpoint = "https://api.example.com/v1".into();
        storage
            .save_backend_profile(SaveProfile {
                id: Some(profile.id.clone()),
                expected_revision: Some(1),
                config: updated,
            })
            .unwrap();
        // Old API occurrences had no metadata and used the legacy source fingerprint.
        diesel::update(o::table.filter(o::entry_id.eq(&saved.id)))
            .set(o::fingerprint.eq(source.legacy_fingerprint()))
            .execute(&mut *storage.db.lock().unwrap())
            .unwrap();
        downgrade_to_v5(&storage);
        {
            let mut db = storage.db.lock().unwrap();
            diesel::update(v::table.find((&profile.id, 1_i64)))
                .set(v::config.eq("{}"))
                .execute(&mut *db)
                .unwrap();
            assert!(super::super::migrate(&mut db).is_err());
            assert_eq!(super::super::schema_version(&mut db).unwrap(), 5);
            assert_eq!(
                super::super::test_support::integer(
                    &mut db,
                    "SELECT COUNT(*) AS value FROM pragma_table_info('vocabulary_occurrences') WHERE name='backend'"
                ),
                0
            );
            diesel::update(v::table.find((&profile.id, 1_i64)))
                .set(v::config.eq(serde_json::to_string(&profile.config).unwrap()))
                .execute(&mut *db)
                .unwrap();
            super::super::migrate(&mut db).unwrap();
        }
        let restored = storage.vocabulary_get(&saved.id).unwrap();
        assert_eq!(
            restored.occurrences[0].source.backend,
            saved.occurrences[0].source.backend
        );
        assert!(
            storage
                .vocabulary_save(&SaveRequest {
                    request_id: id(),
                    ..request
                })
                .unwrap()
                .duplicate
        );
        assert_eq!(
            storage.read("local-conversation").unwrap().messages.len(),
            1
        );
        storage.delete("local-conversation").unwrap();
        let detached = storage.vocabulary_get(&saved.id).unwrap();
        assert!(detached.occurrences[0].source.conversation_id.is_none());
        assert_eq!(
            detached.occurrences[0].source.backend,
            restored.occurrences[0].source.backend
        );
        assert_eq!(detached.occurrences[0].source.snapshot, "Un café ☕");
        let backup = storage.vocabulary_backup(false).unwrap();
        let json = serde_json::to_string(&backup).unwrap();
        for forbidden in ["endpoint", "binaryPath", "profileId", "api.anthropic.com"] {
            assert!(!json.contains(forbidden));
        }
        let target = Storage::memory();
        let (_, pending) = target.vocabulary_preview(backup.clone()).unwrap();
        target.vocabulary_import(&pending, &id(), false).unwrap();
        let imported = target.vocabulary_get(&saved.id).unwrap();
        assert_eq!(
            imported.occurrences[0].source.backend,
            detached.occurrences[0].source.backend
        );
        assert_eq!(
            target
                .vocabulary_preview(backup.clone())
                .unwrap()
                .0
                .duplicates,
            2
        );
        let mut value = serde_json::to_value(&backup).unwrap();
        value["entries"][1]["occurrences"][0]["backend"]["apiKey"] = "forbidden".into();
        assert!(Backup::parse(&serde_json::to_vec(&value).unwrap()).is_err());
        let mut unknown = legacy(&backup);
        unknown.entries.retain(|e| e.id == saved.id);
        let unknown_target = Storage::memory();
        let (_, pending) = unknown_target.vocabulary_preview(unknown).unwrap();
        unknown_target
            .vocabulary_import(&pending, &id(), false)
            .unwrap();
        let unknown_backup = unknown_target.vocabulary_backup(false).unwrap();
        unknown_backup.validate().unwrap();
        assert!(
            unknown_backup.entries[0].occurrences[0]
                .source
                .backend
                .is_none()
        );
        let mut v1 = backup;
        v1.version = 1;
        assert!(v1.validate().is_err());
    }

    #[test]
    fn csv_quotes_multiline_unicode_and_neutralizes_formulas() {
        let mut backup = populated().vocabulary_backup(false).unwrap();
        backup.entries[0].fields.text = "=1+1".into();
        let output = csv(&backup);
        assert!(output.starts_with('\u{feff}'));
        assert!(output.contains("\"'=1+1\""));
        assert!(output.contains("带有\"\"引号\"\""));
        assert!(output.contains("咖啡馆"));
        assert!(output.contains("العربية\n"));
    }
    #[test]
    fn atomic_export_replaces_only_after_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.json");
        std::fs::write(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        let blocked = dir.path().join("directory");
        std::fs::create_dir(&blocked).unwrap();
        assert!(write_atomic(&blocked, b"new").is_err());
        assert!(blocked.is_dir());
    }
}
