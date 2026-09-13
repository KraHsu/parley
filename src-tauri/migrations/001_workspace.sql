CREATE TABLE preferences (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);
CREATE TABLE conversations (
 id TEXT PRIMARY KEY, pane TEXT NOT NULL CHECK(pane IN ('main','tutor')),
 title TEXT NOT NULL DEFAULT '新的对话', model TEXT NOT NULL DEFAULT '',
 target_language TEXT NOT NULL DEFAULT 'en', native_language TEXT NOT NULL DEFAULT 'zh-CN',
 mode TEXT NOT NULL DEFAULT 'conversation', draft TEXT NOT NULL DEFAULT '',
 thread_id TEXT, account TEXT, signature TEXT NOT NULL DEFAULT '',
 status TEXT NOT NULL DEFAULT 'idle', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE messages (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL,
 conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
 role TEXT NOT NULL CHECK(role IN ('user','assistant','note')),
 text TEXT NOT NULL, status TEXT NOT NULL, turn_id TEXT,
 UNIQUE(conversation_id,id)
);
CREATE INDEX messages_conversation ON messages(conversation_id,sequence);
CREATE INDEX conversations_updated ON conversations(updated_at DESC);
PRAGMA user_version=1;
