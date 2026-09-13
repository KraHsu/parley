CREATE TABLE vocabulary_tags (id TEXT PRIMARY KEY,name TEXT NOT NULL UNIQUE);
CREATE TABLE vocabulary_entry_tags (
 entry_id TEXT NOT NULL REFERENCES vocabulary_entries(id) ON DELETE CASCADE,
 tag_id TEXT NOT NULL REFERENCES vocabulary_tags(id) ON DELETE CASCADE,
 PRIMARY KEY(entry_id,tag_id)
);
CREATE TABLE vocabulary_cards (
 id TEXT PRIMARY KEY,entry_id TEXT NOT NULL REFERENCES vocabulary_entries(id) ON DELETE CASCADE,
 direction TEXT NOT NULL CHECK(direction IN ('recognition','production')),
 stage INTEGER NOT NULL DEFAULT 0 CHECK(stage BETWEEN 0 AND 5),
 due_at INTEGER NOT NULL,last_reviewed_at INTEGER,suspended INTEGER NOT NULL DEFAULT 0,
 schedule_version INTEGER NOT NULL DEFAULT 1 CHECK(schedule_version=1),revision INTEGER NOT NULL DEFAULT 1,
 UNIQUE(entry_id,direction)
);
CREATE INDEX vocabulary_cards_due ON vocabulary_cards(suspended,due_at);
CREATE TABLE vocabulary_reviews (
 id TEXT PRIMARY KEY,card_id TEXT NOT NULL REFERENCES vocabulary_cards(id) ON DELETE CASCADE,
 request_id TEXT NOT NULL UNIQUE,rating TEXT NOT NULL CHECK(rating IN ('forgot','hard','remembered')),
 reviewed_at INTEGER NOT NULL,before_state TEXT NOT NULL,after_state TEXT NOT NULL,
 undone_at INTEGER
);
CREATE INDEX vocabulary_reviews_time ON vocabulary_reviews(reviewed_at);
PRAGMA user_version=3;
