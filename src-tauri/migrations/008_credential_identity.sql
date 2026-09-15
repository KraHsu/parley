-- Salted one-way identities let session-only keys resume after a restart.
-- Neither table contains a key or permits authenticating to a provider.
CREATE TABLE backend_credential_salts (
 profile_id TEXT PRIMARY KEY REFERENCES backend_profiles(id) ON DELETE CASCADE,
 salt TEXT NOT NULL
);
CREATE TABLE backend_credential_scopes (
 profile_id TEXT NOT NULL REFERENCES backend_profiles(id) ON DELETE CASCADE,
 fingerprint TEXT NOT NULL,
 scope TEXT NOT NULL,
 PRIMARY KEY (profile_id, fingerprint)
);
PRAGMA user_version=8;
