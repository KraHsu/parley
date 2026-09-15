CREATE TABLE conversation_context (
 conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
 messages TEXT NOT NULL
);
PRAGMA user_version=7;
