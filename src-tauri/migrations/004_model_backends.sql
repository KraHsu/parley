-- Public settings only. Immutable revisions preserve historical connection identity.
CREATE TABLE backend_profiles (
 id TEXT PRIMARY KEY,
 revision INTEGER NOT NULL CHECK(revision > 0),
 config TEXT NOT NULL CHECK(json_valid(config)),
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL
);
CREATE TABLE backend_profile_versions (
 profile_id TEXT NOT NULL REFERENCES backend_profiles(id),
 revision INTEGER NOT NULL CHECK(revision > 0),
 config TEXT NOT NULL CHECK(json_valid(config)),
 PRIMARY KEY(profile_id,revision)
);
INSERT INTO backend_profiles VALUES (
 'codex-default', 1,
 json_object('name','本机 Codex','kind','codex','provider','openai','endpoint','',
 'binaryPath',COALESCE((SELECT json_extract(value,'$.codexPath') FROM preferences WHERE id=1),''),
 'enabled',json('true')),
 CAST(strftime('%s','now') AS INTEGER)*1000, CAST(strftime('%s','now') AS INTEGER)*1000
);
INSERT INTO backend_profile_versions SELECT id,revision,config FROM backend_profiles;
CREATE TABLE conversation_backends (
 conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
 profile_id TEXT NOT NULL,
 profile_revision INTEGER NOT NULL,
 FOREIGN KEY(profile_id,profile_revision) REFERENCES backend_profile_versions(profile_id,revision)
);
INSERT INTO conversation_backends
 SELECT id,'codex-default',1 FROM conversations;
CREATE INDEX conversation_backend_profile ON conversation_backends(profile_id,conversation_id);
PRAGMA user_version=4;
