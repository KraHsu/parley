use super::vocabulary::{cached, check_revision, fingerprint, get_entry, remember};
use super::{Storage, error, now};
use crate::review::*;
use crate::vocabulary::{Entry, Result, bounded, normalized};
use rusqlite::{Connection, OptionalExtension, Row, params};

pub(super) const CARD_COLUMNS: &str =
    "id,entry_id,direction,stage,due_at,last_reviewed_at,suspended,schedule_version,revision";
pub(super) fn card_row(r: &Row<'_>) -> rusqlite::Result<Card> {
    Ok(Card {
        id: r.get(0)?,
        entry_id: r.get(1)?,
        direction: r.get(2)?,
        stage: r.get(3)?,
        due_at: r.get(4)?,
        last_reviewed_at: r.get(5)?,
        suspended: r.get(6)?,
        schedule_version: r.get(7)?,
        revision: r.get(8)?,
    })
}
pub(super) fn get_card(db: &Connection, id: &str) -> Result<Card> {
    db.query_row(
        &format!("SELECT {CARD_COLUMNS} FROM vocabulary_cards WHERE id=?1"),
        [id],
        card_row,
    )
    .optional()
    .map_err(error)?
    .ok_or("复习卡片已不存在。".into())
}
pub(super) fn get_cards(db: &Connection, entry_id: &str) -> Result<Vec<Card>> {
    db.prepare(&format!(
        "SELECT {CARD_COLUMNS} FROM vocabulary_cards WHERE entry_id=?1 ORDER BY direction"
    ))
    .map_err(error)?
    .query_map([entry_id], card_row)
    .map_err(error)?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(error)
}
pub(super) fn get_tags(db: &Connection, entry_id: &str) -> Result<Vec<String>> {
    db.prepare("SELECT t.name FROM vocabulary_tags t JOIN vocabulary_entry_tags et ON et.tag_id=t.id WHERE et.entry_id=?1 ORDER BY t.name").map_err(error)?.query_map([entry_id],|r|r.get(0)).map_err(error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(error)
}
pub(super) fn put_card(db: &Connection, card: &Card) -> Result<()> {
    db.execute("UPDATE vocabulary_cards SET stage=?2,due_at=?3,last_reviewed_at=?4,suspended=?5,schedule_version=?6,revision=?7 WHERE id=?1",params![card.id,card.stage,card.due_at,card.last_reviewed_at,card.suspended,card.schedule_version,card.revision]).map_err(error)?;
    Ok(())
}
pub(super) fn set_tags(db: &Connection, id: &str, tags: &[String]) -> Result<()> {
    if tags.len() > 20 {
        return Err("每条词句最多 20 个标签。".into());
    }
    let mut names = std::collections::BTreeSet::new();
    for tag in tags {
        bounded(tag, 50, "标签")?;
        if !tag.trim().is_empty() {
            names.insert(normalized(tag.trim()));
        }
    }
    db.execute("DELETE FROM vocabulary_entry_tags WHERE entry_id=?1", [id])
        .map_err(error)?;
    for name in names {
        db.execute(
            "INSERT INTO vocabulary_tags(id,name) VALUES(?1,?2) ON CONFLICT(name) DO NOTHING",
            params![uuid::Uuid::new_v4().to_string(), name],
        )
        .map_err(error)?;
        db.execute(
            "INSERT INTO vocabulary_entry_tags SELECT ?1,id FROM vocabulary_tags WHERE name=?2",
            params![id, name],
        )
        .map_err(error)?;
    }
    Ok(())
}
impl Storage {
    pub fn vocabulary_tags_save(&self, request: &TagsRequest) -> Result<Entry> {
        let hash = fingerprint(request)?;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        if cached::<String>(&tx, &request.request_id, "tags", &hash)?.is_some() {
            return get_entry(&tx, &request.entry_id);
        }
        let entry = get_entry(&tx, &request.entry_id)?;
        check_revision(&entry, request.expected_revision)?;
        if entry.deleted_at.is_some() {
            return Err("请先恢复词句。".into());
        }
        set_tags(&tx, &entry.id, &request.tags)?;
        tx.execute(
            "UPDATE vocabulary_entries SET revision=revision+1,updated_at=?2 WHERE id=?1",
            params![entry.id, now()],
        )
        .map_err(error)?;
        remember(&tx, &request.request_id, "tags", &hash, &entry.id)?;
        let result = get_entry(&tx, &entry.id)?;
        tx.commit().map_err(error)?;
        Ok(result)
    }
    pub fn vocabulary_card_save(&self, request: &CardRequest) -> Result<Card> {
        if !["recognition", "production"].contains(&request.direction.as_str()) {
            return Err("未知的复习方向。".into());
        }
        let hash = fingerprint(request)?;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        if let Some(id) = cached::<String>(&tx, &request.request_id, "card", &hash)? {
            return get_card(&tx, &id);
        }
        let entry = get_entry(&tx, &request.entry_id)?;
        if entry.deleted_at.is_some() || entry.fields.meaning.trim().is_empty() {
            return Err("请先恢复词句并补充释义，再加入复习。".into());
        }
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM vocabulary_cards WHERE entry_id=?1 AND direction=?2",
                params![entry.id, request.direction],
                |r| r.get(0),
            )
            .optional()
            .map_err(error)?;
        let card = if let Some(id) = existing {
            let mut card = get_card(&tx, &id)?;
            if request.expected_revision != Some(card.revision) {
                return Err("复习卡已有更新，请刷新后再操作。".into());
            }
            card.suspended = request.suspended;
            card.revision += 1;
            if request.reset {
                card.stage = 0;
                card.due_at = now();
                card.last_reviewed_at = None;
            }
            put_card(&tx, &card)?;
            card
        } else {
            if request.expected_revision.is_some() {
                return Err("复习卡已不存在，请刷新。".into());
            }
            let id = uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO vocabulary_cards(id,entry_id,direction,due_at,suspended) VALUES(?1,?2,?3,?4,?5)",params![id,entry.id,request.direction,now(),request.suspended]).map_err(error)?;
            get_card(&tx, &id)?
        };
        remember(&tx, &request.request_id, "card", &hash, &card.id)?;
        tx.commit().map_err(error)?;
        Ok(card)
    }
    pub fn vocabulary_review_queue(
        &self,
        limit: u32,
        new_limit: u32,
        day_start: i64,
    ) -> Result<ReviewQueue> {
        self.review_queue_at(limit, new_limit, day_start, now())
    }
    fn review_queue_at(
        &self,
        limit: u32,
        new_limit: u32,
        day_start: i64,
        time: i64,
    ) -> Result<ReviewQueue> {
        if !(1..=200).contains(&limit) || new_limit > limit {
            return Err("每轮复习 1–200 张，新卡不能超过本轮总数。".into());
        }
        if day_start > time || day_start < time - 27 * 60 * 60 * 1000 {
            return Err("今日起点无效，请刷新本地时间。".into());
        }
        let db = self.db.lock().unwrap();
        let eligible = "FROM vocabulary_cards c JOIN vocabulary_entries e ON e.id=c.entry_id WHERE e.deleted_at IS NULL AND length(trim(e.meaning))>0 AND c.suspended=0";
        let due_count = db
            .query_row(
                &format!(
                    "SELECT count(*) {eligible} AND c.last_reviewed_at IS NOT NULL AND c.due_at<=?1"
                ),
                [time],
                |r| r.get(0),
            )
            .map_err(error)?;
        let new_count = db
            .query_row(
                &format!("SELECT count(*) {eligible} AND c.last_reviewed_at IS NULL"),
                [],
                |r| r.get(0),
            )
            .map_err(error)?;
        let next_due_at=db.query_row(&format!("SELECT min(c.due_at) {eligible} AND c.last_reviewed_at IS NOT NULL AND c.due_at>?1"),[time],|r|r.get(0)).map_err(error)?;
        let mut stmt=db.prepare(&format!("SELECT c.id {eligible} AND c.last_reviewed_at IS NOT NULL AND c.due_at<=?1 ORDER BY c.due_at,c.id LIMIT ?2")).map_err(error)?;
        let mut ids = stmt
            .query_map(params![time, limit], |r| r.get::<_, String>(0))
            .map_err(error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(error)?;
        let remaining = (limit as usize - ids.len()).min(new_limit as usize);
        let mut stmt=db.prepare(&format!("SELECT c.id {eligible} AND c.last_reviewed_at IS NULL ORDER BY c.due_at,c.id LIMIT ?1")).map_err(error)?;
        ids.extend(
            stmt.query_map([remaining as u32], |r| r.get::<_, String>(0))
                .map_err(error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(error)?,
        );
        let items = ids
            .into_iter()
            .map(|id| {
                let card = get_card(&db, &id)?;
                Ok(ReviewItem {
                    entry: get_entry(&db, &card.entry_id)?,
                    card,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let completed_today=db.query_row("SELECT count(*) FROM vocabulary_reviews WHERE reviewed_at>=?1 AND reviewed_at<=?2 AND undone_at IS NULL",params![day_start,time],|r|r.get(0)).map_err(error)?;
        let last_review_id=db.query_row("SELECT id FROM vocabulary_reviews WHERE undone_at IS NULL ORDER BY reviewed_at DESC,rowid DESC LIMIT 1",[],|r|r.get(0)).optional().map_err(error)?;
        Ok(ReviewQueue {
            items,
            due_count,
            new_count,
            next_due_at,
            completed_today,
            last_review_id,
        })
    }
    pub fn vocabulary_review_grade(&self, request: &GradeRequest) -> Result<GradeResult> {
        self.grade_at(request, now())
    }
    fn grade_at(&self, request: &GradeRequest, time: i64) -> Result<GradeResult> {
        let hash = fingerprint(request)?;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        if let Some(result) = cached(&tx, &request.request_id, "grade", &hash)? {
            return Ok(result);
        }
        let card = get_card(&tx, &request.card_id)?;
        if card.revision != request.expected_revision {
            return Err("这张卡片已经更新或评分，请刷新复习队列。".into());
        }
        let entry = get_entry(&tx, &card.entry_id)?;
        if card.suspended
            || entry.deleted_at.is_some()
            || entry.fields.meaning.trim().is_empty()
            || card.due_at > time
        {
            return Err("这张卡片当前不在可复习队列中。".into());
        }
        let next = schedule(&card, &request.rating, time)?;
        let review_id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO vocabulary_reviews(id,card_id,request_id,rating,reviewed_at,before_state,after_state) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![review_id,card.id,request.request_id,request.rating,time,serde_json::to_string(&card).map_err(error)?,serde_json::to_string(&next).map_err(error)?]).map_err(error)?;
        put_card(&tx, &next)?;
        let result = GradeResult {
            card: next,
            review_id,
        };
        remember(&tx, &request.request_id, "grade", &hash, &result)?;
        tx.commit().map_err(error)?;
        Ok(result)
    }
    pub fn vocabulary_review_undo(&self, request: &UndoRequest) -> Result<Card> {
        let hash = fingerprint(request)?;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        if let Some(id) = cached::<String>(&tx, &request.request_id, "undo", &hash)? {
            return get_card(&tx, &id);
        }
        let recent:Option<(String,String,String)>=tx.query_row("SELECT id,before_state,after_state FROM vocabulary_reviews WHERE undone_at IS NULL ORDER BY reviewed_at DESC,rowid DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(error)?;
        let (id, before, after) = recent.ok_or("没有可以撤销的评分。")?;
        if id != request.review_id {
            return Err("只能撤销最近一次评分。".into());
        }
        let mut before: Card = serde_json::from_str(&before).map_err(error)?;
        let after: Card = serde_json::from_str(&after).map_err(error)?;
        let current = get_card(&tx, &before.id)?;
        if current != after {
            return Err("评分后卡片已有变化，不能覆盖较新的排程。".into());
        }
        before.revision = current.revision + 1;
        put_card(&tx, &before)?;
        tx.execute(
            "UPDATE vocabulary_reviews SET undone_at=?2 WHERE id=?1",
            params![id, now()],
        )
        .map_err(error)?;
        remember(&tx, &request.request_id, "undo", &hash, &before.id)?;
        tx.commit().map_err(error)?;
        Ok(before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocabulary::{EntryFields, EntryMutation, SaveRequest};
    fn id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
    fn fixture(storage: &Storage) -> (Entry, Card) {
        let entry = storage
            .vocabulary_save(&SaveRequest {
                request_id: id(),
                id: None,
                expected_revision: None,
                fields: EntryFields {
                    language: "en".into(),
                    language_label: "English".into(),
                    kind: "word".into(),
                    text: "hello".into(),
                    meaning: "你好".into(),
                    meaning_language: "zh-CN".into(),
                    note: String::new(),
                },
                occurrence: None,
                draft_id: None,
                allow_duplicate: false,
                tags: vec![],
            })
            .unwrap()
            .entry;
        let card = storage
            .vocabulary_card_save(&CardRequest {
                request_id: id(),
                entry_id: entry.id.clone(),
                direction: "recognition".into(),
                suspended: false,
                reset: false,
                expected_revision: None,
            })
            .unwrap();
        (entry, card)
    }
    #[test]
    fn fixed_clock_schedule_covers_every_step_and_lapses() {
        let storage = Storage::memory();
        let (_, mut card) = fixture(&storage);
        let time = now();
        let day = 86_400_000;
        for days in [1, 3, 7, 14, 30, 30] {
            card = schedule(&card, "remembered", time).unwrap();
            assert_eq!(card.due_at, time + days * day)
        }
        assert_eq!(card.stage, 5);
        let hard = schedule(&card, "hard", time).unwrap();
        assert_eq!(hard.stage, 5);
        assert_eq!(hard.due_at, time + day);
        let forgot = schedule(&card, "forgot", time).unwrap();
        assert_eq!(forgot.stage, 0);
        assert_eq!(forgot.due_at, time + 600_000);
        assert_eq!(
            schedule(&forgot, "remembered", time).unwrap().due_at,
            time + day
        );
        assert!(schedule(&card, "invalid", time).is_err());
        assert!(schedule(&card, "remembered", i64::MAX).is_err());
    }
    #[test]
    fn duplicate_grade_and_undo_do_not_repeat_history_or_overwrite_newer_card_state() {
        let storage = Storage::memory();
        let (_, card) = fixture(&storage);
        let time = now() + 1;
        let request = GradeRequest {
            request_id: id(),
            card_id: card.id.clone(),
            expected_revision: 1,
            rating: "remembered".into(),
        };
        let result = storage.grade_at(&request, time).unwrap();
        let repeat = storage.grade_at(&request, time + 1).unwrap();
        assert_eq!(result.review_id, repeat.review_id);
        let queue = storage.review_queue_at(20, 5, time - 1000, time).unwrap();
        assert_eq!(queue.completed_today, 1);
        assert_eq!(queue.due_count, 0);
        let undo = UndoRequest {
            request_id: id(),
            review_id: result.review_id,
        };
        let restored = storage.vocabulary_review_undo(&undo).unwrap();
        assert_eq!(restored.stage, 0);
        assert_eq!(restored.last_reviewed_at, None);
        assert_eq!(restored.revision, 3);
        assert_eq!(storage.vocabulary_review_undo(&undo).unwrap(), restored);
        let stale = GradeRequest {
            request_id: id(),
            ..request
        };
        assert!(storage.grade_at(&stale, time).is_err());
        let next = storage
            .grade_at(
                &GradeRequest {
                    request_id: id(),
                    card_id: card.id.clone(),
                    expected_revision: 3,
                    rating: "hard".into(),
                },
                time,
            )
            .unwrap();
        storage
            .vocabulary_card_save(&CardRequest {
                request_id: id(),
                entry_id: card.entry_id,
                direction: card.direction,
                suspended: true,
                reset: false,
                expected_revision: Some(next.card.revision),
            })
            .unwrap();
        assert!(
            storage
                .vocabulary_review_undo(&UndoRequest {
                    request_id: id(),
                    review_id: next.review_id
                })
                .unwrap_err()
                .contains("已有变化")
        );
    }
    #[test]
    fn queue_prioritizes_due_limits_new_cards_and_isolates_directions() {
        let storage = Storage::memory();
        let (entry, card) = fixture(&storage);
        let production = storage
            .vocabulary_card_save(&CardRequest {
                request_id: id(),
                entry_id: entry.id,
                direction: "production".into(),
                suspended: false,
                reset: false,
                expected_revision: None,
            })
            .unwrap();
        for _ in 0..7 {
            fixture(&storage);
        }
        let time = now() + 1;
        storage
            .grade_at(
                &GradeRequest {
                    request_id: id(),
                    card_id: card.id.clone(),
                    expected_revision: 1,
                    rating: "forgot".into(),
                },
                time,
            )
            .unwrap();
        let queue = storage.review_queue_at(3, 1, time, time + 600_000).unwrap();
        assert_eq!(queue.items.len(), 2);
        assert_eq!(queue.items[0].card.id, card.id);
        assert_eq!(queue.due_count, 1);
        assert_eq!(queue.new_count, 8);
        assert_eq!(
            get_card(&storage.db.lock().unwrap(), &production.id)
                .unwrap()
                .revision,
            1
        );
        assert!(storage.review_queue_at(3, 4, time, time).is_err());
    }
    #[test]
    fn missing_meaning_trash_and_suspension_stay_out_of_queue_and_purge_cascades() {
        let storage = Storage::memory();
        let (entry, card) = fixture(&storage);
        let time = now() + 1;
        let result = storage
            .grade_at(
                &GradeRequest {
                    request_id: id(),
                    card_id: card.id.clone(),
                    expected_revision: 1,
                    rating: "forgot".into(),
                },
                time,
            )
            .unwrap();
        let op = EntryMutation {
            request_id: id(),
            id: entry.id.clone(),
            expected_revision: 1,
        };
        storage.vocabulary_mutate(&op, "trash").unwrap();
        assert_eq!(
            storage
                .review_queue_at(20, 5, time, time + 600_000)
                .unwrap()
                .due_count,
            0
        );
        assert!(
            storage
                .grade_at(
                    &GradeRequest {
                        request_id: id(),
                        card_id: card.id,
                        expected_revision: result.card.revision,
                        rating: "remembered".into()
                    },
                    time + 600_000
                )
                .is_err()
        );
        storage
            .vocabulary_mutate(
                &EntryMutation {
                    request_id: id(),
                    id: entry.id,
                    expected_revision: 2,
                },
                "purge",
            )
            .unwrap();
        let db = storage.db.lock().unwrap();
        for table in ["vocabulary_cards", "vocabulary_reviews"] {
            assert_eq!(
                db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            )
        }
    }
    #[test]
    fn tags_normalize_and_revision_protect_edits() {
        let storage = Storage::memory();
        let (entry, _) = fixture(&storage);
        let req = TagsRequest {
            request_id: id(),
            entry_id: entry.id.clone(),
            expected_revision: 1,
            tags: vec![" cafe\u{301} ".into(), "café".into(), "会话".into()],
        };
        let saved = storage.vocabulary_tags_save(&req).unwrap();
        assert_eq!(saved.tags, vec!["café", "会话"]);
        assert_eq!(saved.revision, 2);
        assert_eq!(storage.vocabulary_tags_save(&req).unwrap().revision, 2);
        assert!(
            storage
                .vocabulary_tags_save(&TagsRequest {
                    request_id: id(),
                    ..req
                })
                .is_err()
        );
        let page = storage
            .vocabulary_list(&crate::vocabulary::ListQuery {
                tag: "会话".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.total, 1);
    }
}
