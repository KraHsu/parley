-- Preserve existing version-1 schedules; version 2 adds the Web 60-day stage.
CREATE TABLE vocabulary_cards_next (
 id TEXT PRIMARY KEY,entry_id TEXT NOT NULL REFERENCES vocabulary_entries(id) ON DELETE CASCADE,
 direction TEXT NOT NULL CHECK(direction IN ('recognition','production')),
 stage INTEGER NOT NULL DEFAULT 0 CHECK(stage BETWEEN 0 AND 6),
 due_at INTEGER NOT NULL,last_reviewed_at INTEGER,suspended INTEGER NOT NULL DEFAULT 0,
 schedule_version INTEGER NOT NULL DEFAULT 1 CHECK(schedule_version IN (1,2)),revision INTEGER NOT NULL DEFAULT 1,
 UNIQUE(entry_id,direction), CHECK(schedule_version=2 OR stage<=5)
);
INSERT INTO vocabulary_cards_next SELECT * FROM vocabulary_cards;
CREATE TABLE vocabulary_reviews_next (
 id TEXT PRIMARY KEY,card_id TEXT NOT NULL REFERENCES vocabulary_cards_next(id) ON DELETE CASCADE,
 request_id TEXT NOT NULL UNIQUE,rating TEXT NOT NULL CHECK(rating IN ('forgot','hard','remembered')),
 reviewed_at INTEGER NOT NULL,before_state TEXT NOT NULL,after_state TEXT NOT NULL,undone_at INTEGER
);
INSERT INTO vocabulary_reviews_next SELECT * FROM vocabulary_reviews;
DROP TABLE vocabulary_reviews;
DROP TABLE vocabulary_cards;
ALTER TABLE vocabulary_cards_next RENAME TO vocabulary_cards;
ALTER TABLE vocabulary_reviews_next RENAME TO vocabulary_reviews;
CREATE INDEX vocabulary_cards_due ON vocabulary_cards(suspended,due_at);
CREATE INDEX vocabulary_reviews_time ON vocabulary_reviews(reviewed_at);
PRAGMA user_version=9;
