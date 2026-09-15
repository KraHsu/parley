use super::learning_rows::{CardRow, length, trim};
use super::schema::{
    vocabulary_cards as c, vocabulary_entries as e, vocabulary_entry_tags as et,
    vocabulary_reviews as r, vocabulary_tags as tags,
};
use super::vocabulary::{cached, check_revision, fingerprint, get_entry, remember};
use super::{Storage, error, now};
use crate::review::*;
use crate::vocabulary::{Entry, Result, bounded, normalized};
use diesel::prelude::*;

pub(super) fn get_card(db: &mut SqliteConnection, id: &str) -> Result<Card> {
    c::table
        .find(id)
        .select(CardRow::as_select())
        .first::<CardRow>(db)
        .optional()
        .map_err(error)?
        .ok_or("复习卡片已不存在。")?
        .try_into()
}
pub(super) fn get_cards(db: &mut SqliteConnection, entry_id: &str) -> Result<Vec<Card>> {
    c::table
        .filter(c::entry_id.eq(entry_id))
        .order(c::direction)
        .select(CardRow::as_select())
        .load::<CardRow>(db)
        .map_err(error)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
}
pub(super) fn get_tags(db: &mut SqliteConnection, entry_id: &str) -> Result<Vec<String>> {
    tags::table
        .inner_join(et::table.on(et::tag_id.eq(tags::id)))
        .filter(et::entry_id.eq(entry_id))
        .order(tags::name)
        .select(tags::name)
        .load(db)
        .map_err(error)
}
pub(super) fn put_card(db: &mut SqliteConnection, card: &Card) -> Result<()> {
    diesel::update(c::table.find(&card.id))
        .set((
            c::stage.eq(i64::from(card.stage)),
            c::due_at.eq(card.due_at),
            c::last_reviewed_at.eq(card.last_reviewed_at),
            c::suspended.eq(i64::from(card.suspended)),
            c::schedule_version.eq(i64::from(card.schedule_version)),
            c::revision.eq(card.revision),
        ))
        .execute(db)
        .map_err(error)?;
    Ok(())
}
pub(super) fn set_tags(db: &mut SqliteConnection, id: &str, values: &[String]) -> Result<()> {
    if values.len() > 30 {
        return Err("每条词句最多 30 个标签。".into());
    }
    let mut names = std::collections::BTreeSet::new();
    for tag in values {
        bounded(tag, 100, "标签")?;
        if !tag.trim().is_empty() {
            names.insert(normalized(tag.trim()));
        }
    }
    diesel::delete(et::table.filter(et::entry_id.eq(id)))
        .execute(db)
        .map_err(error)?;
    for name in names {
        diesel::insert_into(tags::table)
            .values((
                tags::id.eq(uuid::Uuid::new_v4().to_string()),
                tags::name.eq(&name),
            ))
            .on_conflict(tags::name)
            .do_nothing()
            .execute(db)
            .map_err(error)?;
        let tag = tags::table
            .filter(tags::name.eq(name))
            .select(tags::id)
            .first::<String>(db)
            .map_err(error)?;
        diesel::insert_into(et::table)
            .values((et::entry_id.eq(id), et::tag_id.eq(tag)))
            .execute(db)
            .map_err(error)?;
    }
    Ok(())
}
impl Storage {
    pub fn vocabulary_tags_save(&self, request: &TagsRequest) -> Result<Entry> {
        let hash = fingerprint(request)?;
        self.repository_transaction(|db| {
            if cached::<String>(db, &request.request_id, "tags", &hash)?.is_some() {
                return get_entry(db, &request.entry_id);
            }
            let entry = get_entry(db, &request.entry_id)?;
            check_revision(&entry, request.expected_revision)?;
            if entry.deleted_at.is_some() {
                return Err("请先恢复词句。".into());
            }
            set_tags(db, &entry.id, &request.tags)?;
            diesel::update(e::table.find(&entry.id))
                .set((e::revision.eq(e::revision + 1_i64), e::updated_at.eq(now())))
                .execute(db)
                .map_err(error)?;
            remember(db, &request.request_id, "tags", &hash, &entry.id)?;
            get_entry(db, &entry.id)
        })
    }
    pub fn vocabulary_card_save(&self, request: &CardRequest) -> Result<Card> {
        if !["recognition", "production"].contains(&request.direction.as_str()) {
            return Err("未知的复习方向。".into());
        }
        let hash = fingerprint(request)?;
        self.repository_transaction(|db| {
            if let Some(id) = cached::<String>(db, &request.request_id, "card", &hash)? {
                return get_card(db, &id);
            }
            let entry = get_entry(db, &request.entry_id)?;
            if entry.deleted_at.is_some() || entry.fields.meaning.trim().is_empty() {
                return Err("请先恢复词句并补充释义，再加入复习。".into());
            }
            let existing = c::table
                .filter(c::entry_id.eq(&entry.id))
                .filter(c::direction.eq(&request.direction))
                .select(c::id)
                .first::<String>(db)
                .optional()
                .map_err(error)?;
            let card = if let Some(id) = existing {
                let mut card = get_card(db, &id)?;
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
                put_card(db, &card)?;
                card
            } else {
                if request.expected_revision.is_some() {
                    return Err("复习卡已不存在，请刷新。".into());
                }
                let id = uuid::Uuid::new_v4().to_string();
                diesel::insert_into(c::table)
                    .values((
                        c::id.eq(&id),
                        c::entry_id.eq(&entry.id),
                        c::direction.eq(&request.direction),
                        c::due_at.eq(now()),
                        c::suspended.eq(i64::from(request.suspended)),
                    ))
                    .execute(db)
                    .map_err(error)?;
                get_card(db, &id)?
            };
            remember(db, &request.request_id, "card", &hash, &card.id)?;
            Ok(card)
        })
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
        self.repository(|db| {
            let eligible = c::table
                .inner_join(e::table.on(e::id.eq(c::entry_id)))
                .filter(e::deleted_at.is_null())
                .filter(length(trim(e::meaning)).gt(0_i64))
                .filter(c::suspended.eq(0_i64));
            let due = eligible
                .filter(c::last_reviewed_at.is_not_null())
                .filter(c::due_at.le(time));
            let fresh = eligible.filter(c::last_reviewed_at.is_null());
            let due_count = due.count().get_result(db).map_err(error)?;
            let new_count = fresh.count().get_result(db).map_err(error)?;
            let next_due_at = eligible
                .filter(c::last_reviewed_at.is_not_null())
                .filter(c::due_at.gt(time))
                .select(diesel::dsl::min(c::due_at))
                .first(db)
                .map_err(error)?;
            let mut ids = due
                .order((c::due_at, c::id))
                .limit(i64::from(limit))
                .select(c::id)
                .load::<String>(db)
                .map_err(error)?;
            let remaining = (limit as usize - ids.len()).min(new_limit as usize);
            ids.extend(
                fresh
                    .order((c::due_at, c::id))
                    .limit(remaining as i64)
                    .select(c::id)
                    .load::<String>(db)
                    .map_err(error)?,
            );
            let items = ids
                .into_iter()
                .map(|id| {
                    let card = get_card(db, &id)?;
                    Ok(ReviewItem {
                        entry: get_entry(db, &card.entry_id)?,
                        card,
                    })
                })
                .collect::<Result<_>>()?;
            let completed_today = r::table
                .filter(r::reviewed_at.ge(day_start))
                .filter(r::reviewed_at.le(time))
                .filter(r::undone_at.is_null())
                .count()
                .get_result(db)
                .map_err(error)?;
            let last_review_id = r::table
                .filter(r::undone_at.is_null())
                .order((r::reviewed_at.desc(), r::rowid.desc()))
                .select(r::id)
                .first::<String>(db)
                .optional()
                .map_err(error)?;
            Ok(ReviewQueue {
                items,
                due_count,
                new_count,
                next_due_at,
                completed_today,
                last_review_id,
            })
        })
    }
    pub fn vocabulary_review_grade(&self, request: &GradeRequest) -> Result<GradeResult> {
        self.grade_at(request, now())
    }
    fn grade_at(&self, request: &GradeRequest, time: i64) -> Result<GradeResult> {
        let hash = fingerprint(request)?;
        self.repository_transaction(|db| {
            if let Some(result) = cached(db, &request.request_id, "grade", &hash)? {
                return Ok(result);
            }
            let card = get_card(db, &request.card_id)?;
            if card.revision != request.expected_revision {
                return Err("这张卡片已经更新或评分，请刷新复习队列。".into());
            }
            let entry = get_entry(db, &card.entry_id)?;
            if card.suspended
                || entry.deleted_at.is_some()
                || entry.fields.meaning.trim().is_empty()
                || card.due_at > time
            {
                return Err("这张卡片当前不在可复习队列中。".into());
            }
            let next = schedule(&card, &request.rating, time)?;
            let review_id = uuid::Uuid::new_v4().to_string();
            diesel::insert_into(r::table)
                .values((
                    r::id.eq(&review_id),
                    r::card_id.eq(&card.id),
                    r::request_id.eq(&request.request_id),
                    r::rating.eq(&request.rating),
                    r::reviewed_at.eq(time),
                    r::before_state.eq(serde_json::to_string(&card).map_err(error)?),
                    r::after_state.eq(serde_json::to_string(&next).map_err(error)?),
                ))
                .execute(db)
                .map_err(error)?;
            put_card(db, &next)?;
            let result = GradeResult {
                card: next,
                review_id,
            };
            remember(db, &request.request_id, "grade", &hash, &result)?;
            Ok(result)
        })
    }
    pub fn vocabulary_review_undo(&self, request: &UndoRequest) -> Result<Card> {
        let hash = fingerprint(request)?;
        self.repository_transaction(|db| {
            if let Some(id) = cached::<String>(db, &request.request_id, "undo", &hash)? {
                return get_card(db, &id);
            }
            let (id, before, after) = r::table
                .filter(r::undone_at.is_null())
                .order((r::reviewed_at.desc(), r::rowid.desc()))
                .select((r::id, r::before_state, r::after_state))
                .first::<(String, String, String)>(db)
                .optional()
                .map_err(error)?
                .ok_or("没有可以撤销的评分。")?;
            if id != request.review_id {
                return Err("只能撤销最近一次评分。".into());
            }
            let mut before: Card = serde_json::from_str(&before).map_err(error)?;
            let after: Card = serde_json::from_str(&after).map_err(error)?;
            let current = get_card(db, &before.id)?;
            if current != after {
                return Err("评分后卡片已有变化，不能覆盖较新的排程。".into());
            }
            before.revision = current.revision + 1;
            put_card(db, &before)?;
            diesel::update(r::table.find(&id))
                .set(r::undone_at.eq(now()))
                .execute(db)
                .map_err(error)?;
            remember(db, &request.request_id, "undo", &hash, &before.id)?;
            Ok(before)
        })
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
    fn equal_timestamp_reviews_undo_in_insertion_order() {
        let storage = Storage::memory();
        let (_, first) = fixture(&storage);
        let (_, second) = fixture(&storage);
        let time = now() + 1;
        let older = storage
            .grade_at(
                &GradeRequest {
                    request_id: id(),
                    card_id: first.id.clone(),
                    expected_revision: 1,
                    rating: "remembered".into(),
                },
                time,
            )
            .unwrap();
        let newer = storage
            .grade_at(
                &GradeRequest {
                    request_id: id(),
                    card_id: second.id.clone(),
                    expected_revision: 1,
                    rating: "remembered".into(),
                },
                time,
            )
            .unwrap();
        let newer_id = "00000000-0000-4000-8000-000000000001";
        storage
            .repository(|db| {
                diesel::update(r::table.find(&older.review_id))
                    .set(r::id.eq("ffffffff-ffff-4fff-8fff-ffffffffffff"))
                    .execute(db)
                    .map_err(error)?;
                diesel::update(r::table.find(&newer.review_id))
                    .set(r::id.eq(newer_id))
                    .execute(db)
                    .map_err(error)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            storage
                .review_queue_at(20, 5, time, time)
                .unwrap()
                .last_review_id
                .as_deref(),
            Some(newer_id)
        );
        let undone = storage
            .vocabulary_review_undo(&UndoRequest {
                request_id: id(),
                review_id: newer_id.into(),
            })
            .unwrap();
        assert_eq!(undone.id, second.id);
        assert!(undone.last_reviewed_at.is_none());
        assert_eq!(
            storage.repository(|db| get_card(db, &first.id)).unwrap(),
            older.card
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
            get_card(&mut storage.db.lock().unwrap(), &production.id)
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
        let mut db = storage.db.lock().unwrap();
        assert_eq!(c::table.count().get_result::<i64>(&mut *db).unwrap(), 0);
        assert_eq!(r::table.count().get_result::<i64>(&mut *db).unwrap(), 0);
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
