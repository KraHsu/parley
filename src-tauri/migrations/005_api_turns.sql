CREATE TABLE backend_credentials (
 profile_id TEXT PRIMARY KEY REFERENCES backend_profiles(id),
 slot_id TEXT NOT NULL UNIQUE,
 scope TEXT NOT NULL,
 endpoint TEXT NOT NULL
);
CREATE TABLE model_turns (
 id TEXT PRIMARY KEY,
 conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
 profile_id TEXT NOT NULL,
 profile_revision INTEGER NOT NULL,
 auth_scope TEXT NOT NULL,
 model TEXT NOT NULL,
 user_message_id TEXT NOT NULL,
 assistant_message_id TEXT NOT NULL,
 provider_input TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('streaming','complete','failed','interrupted')),
 sequence INTEGER NOT NULL DEFAULT 0,
 usage TEXT CHECK(usage IS NULL OR json_valid(usage)),
 provider_output TEXT CHECK(provider_output IS NULL OR json_valid(provider_output)),
 created_at INTEGER NOT NULL,
 FOREIGN KEY(profile_id,profile_revision) REFERENCES backend_profile_versions(profile_id,revision),
 UNIQUE(conversation_id,user_message_id)
);
CREATE INDEX model_turns_conversation ON model_turns(conversation_id,created_at,id);
PRAGMA user_version=5;
