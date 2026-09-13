CREATE TABLE vocabulary_entries (
 id TEXT PRIMARY KEY,
 language TEXT NOT NULL, language_label TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('word','phrase','sentence')),
 text TEXT NOT NULL, lookup_key TEXT NOT NULL,
 meaning TEXT NOT NULL, meaning_language TEXT NOT NULL, note TEXT NOT NULL,
 search_text TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0),
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER
);
CREATE INDEX vocabulary_entries_updated ON vocabulary_entries(deleted_at,updated_at DESC,id);
CREATE INDEX vocabulary_entries_language ON vocabulary_entries(language,lookup_key);
CREATE TABLE vocabulary_occurrences (
 id TEXT PRIMARY KEY, entry_id TEXT NOT NULL REFERENCES vocabulary_entries(id) ON DELETE CASCADE,
 source_kind TEXT NOT NULL CHECK(source_kind IN ('main','tutor','terminal','manual','import')),
 conversation_id TEXT, message_id TEXT, thread_id TEXT, turn_id TEXT, item_id TEXT,
 role TEXT NOT NULL, selected_text TEXT NOT NULL, snapshot TEXT NOT NULL,
 snapshot_hash TEXT NOT NULL, start INTEGER NOT NULL, end INTEGER NOT NULL,
 locator_version INTEGER NOT NULL, truncated INTEGER NOT NULL,
 fingerprint TEXT NOT NULL,
 FOREIGN KEY(conversation_id,message_id) REFERENCES messages(conversation_id,id) ON DELETE SET NULL,
 UNIQUE(entry_id,fingerprint)
);
CREATE INDEX vocabulary_occurrences_fingerprint ON vocabulary_occurrences(fingerprint);
CREATE TABLE vocabulary_drafts (
 id TEXT PRIMARY KEY, entry_id TEXT REFERENCES vocabulary_entries(id) ON DELETE CASCADE,
 payload TEXT NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE vocabulary_mutations (
 request_id TEXT PRIMARY KEY, operation TEXT NOT NULL, fingerprint TEXT NOT NULL,
 result TEXT NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE vocabulary_metadata (key TEXT PRIMARY KEY,value TEXT NOT NULL);
CREATE TABLE vocabulary_import_records (
 dataset_id TEXT NOT NULL, record_id TEXT NOT NULL,
 content_hash TEXT NOT NULL, local_id TEXT NOT NULL REFERENCES vocabulary_entries(id) ON DELETE CASCADE,
 PRIMARY KEY(dataset_id,record_id,content_hash)
);
PRAGMA user_version=2;
