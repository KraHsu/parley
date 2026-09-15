use super::types::{BackendKind, BackendProfile};
use crate::storage::Storage;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

const SERVICE: &str = "dev.parley.model-api";

pub struct Credential {
    pub key: Zeroizing<String>,
    pub scope: String,
    pub endpoint: String,
}

pub struct Credentials {
    // Also serializes system-keystore operations, whose implementations may not be concurrent.
    sessions: Mutex<HashMap<String, Credential>>,
    keystore: Arc<dyn SecretStore>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialReference {
    pub slot_id: String,
    pub scope: String,
    pub endpoint: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub configured: bool,
    pub persistence: &'static str,
}

fn entry(slot: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, slot)
        .map_err(|_| "系统凭据库不可用，可选择仅本次会话使用。".into())
}

trait SecretStore: Send + Sync {
    fn read(&self, slot: &str) -> Result<Zeroizing<String>, String>;
    fn write(&self, slot: &str, key: &str) -> Result<(), String>;
    fn remove(&self, slot: &str) -> Result<(), String>;
}
struct SystemSecrets;
impl SecretStore for SystemSecrets {
    fn read(&self, slot: &str) -> Result<Zeroizing<String>, String> {
        entry(slot)?
            .get_password()
            .map(Zeroizing::new)
            .map_err(|_| "无法读取 API Key，请解锁系统凭据库或重新输入密钥。".into())
    }
    fn write(&self, slot: &str, key: &str) -> Result<(), String> {
        entry(slot)?
            .set_password(key)
            .map_err(|_| "无法保存到系统凭据库，请解锁凭据库，或选择仅本次会话使用。".into())
    }
    fn remove(&self, slot: &str) -> Result<(), String> {
        match entry(slot)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("系统凭据库未能删除密钥，请解锁后重试。".into()),
        }
    }
}
impl Default for Credentials {
    fn default() -> Self {
        Self {
            sessions: Mutex::default(),
            keystore: Arc::new(SystemSecrets),
        }
    }
}
impl Credentials {
    pub fn status(
        &self,
        storage: &Storage,
        profile: &BackendProfile,
    ) -> Result<CredentialStatus, String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(&profile.id)
            .is_some_and(|c| c.endpoint == profile.config.endpoint);
        let saved = storage
            .credential_reference(&profile.id)?
            .is_some_and(|c| c.endpoint == profile.config.endpoint);
        Ok(CredentialStatus {
            configured: session || saved,
            persistence: if session {
                "session"
            } else if saved {
                "system"
            } else {
                "missing"
            },
        })
    }

    pub fn set(
        &self,
        storage: &Storage,
        profile: &BackendProfile,
        key: String,
        persist: bool,
    ) -> Result<CredentialStatus, String> {
        let key = Zeroizing::new(key);
        if key.trim().is_empty() || key.len() > 8192 || key.contains(['\0', '\n', '\r']) {
            return Err("API Key 不能为空，且不能包含换行。".into());
        }
        if profile.config.kind == BackendKind::Codex {
            return Err("Codex 登录由官方 CLI 管理。".into());
        }
        let mut sessions = self.sessions.lock().unwrap();
        let previous = sessions
            .get(&profile.id)
            .filter(|c| c.endpoint == profile.config.endpoint && c.key.as_str() == key.as_str())
            .map(|c| c.scope.clone());
        let previous = if previous.is_some() {
            previous
        } else {
            storage
                .credential_reference(&profile.id)?
                .filter(|r| r.endpoint == profile.config.endpoint)
                .and_then(|r| {
                    let saved = self.keystore.read(&r.slot_id).ok()?;
                    (saved.as_str() == key.as_str()).then_some(r.scope)
                })
        };
        let scope = storage.credential_scope(profile, key.as_str(), previous.as_deref())?;
        if persist {
            let reference = CredentialReference {
                slot_id: uuid::Uuid::new_v4().to_string(),
                scope,
                endpoint: profile.config.endpoint.clone(),
            };
            self.keystore.write(&reference.slot_id, key.as_str())?;
            let old = match storage.replace_credential(profile, Some(&reference)) {
                Ok(old) => old,
                Err(error) => {
                    let _ = self.keystore.remove(&reference.slot_id);
                    return Err(error);
                }
            };
            sessions.remove(&profile.id);
            if let Some(old) = old {
                let _ = self.keystore.remove(&old.slot_id);
            }
        } else {
            // Switching to session-only must not silently restore an older saved
            // account after restart. If removal fails, leave the prior state intact.
            self.remove_saved(storage, profile)?;
            sessions.insert(
                profile.id.clone(),
                Credential {
                    key,
                    scope,
                    endpoint: profile.config.endpoint.clone(),
                },
            );
        }
        drop(sessions);
        self.status(storage, profile)
    }

    pub fn get(&self, storage: &Storage, profile: &BackendProfile) -> Result<Credential, String> {
        let sessions = self.sessions.lock().unwrap();
        if let Some(c) = sessions
            .get(&profile.id)
            .filter(|c| c.endpoint == profile.config.endpoint)
        {
            return Ok(Credential {
                key: Zeroizing::new(c.key.to_string()),
                scope: c.scope.clone(),
                endpoint: c.endpoint.clone(),
            });
        }
        let reference = storage
            .credential_reference(&profile.id)?
            .filter(|r| r.endpoint == profile.config.endpoint)
            .ok_or("请为当前服务地址配置 API Key。")?;
        let key = self.keystore.read(&reference.slot_id)?;
        Ok(Credential {
            key,
            scope: reference.scope,
            endpoint: reference.endpoint,
        })
    }

    fn remove_saved(&self, storage: &Storage, profile: &BackendProfile) -> Result<(), String> {
        storage.remove_credential_with(profile, |reference| {
            if let Some(reference) = reference {
                self.keystore.remove(&reference.slot_id)?;
            }
            Ok(())
        })
    }
    pub fn remove(&self, storage: &Storage, profile: &BackendProfile) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        self.remove_saved(storage, profile)?;
        sessions.remove(&profile.id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::api_tests::turn;
    #[derive(Default)]
    struct TestSecrets {
        values: Mutex<HashMap<String, String>>,
        fail: std::sync::atomic::AtomicBool,
    }
    impl SecretStore for TestSecrets {
        fn read(&self, slot: &str) -> Result<Zeroizing<String>, String> {
            self.values
                .lock()
                .unwrap()
                .get(slot)
                .cloned()
                .map(Zeroizing::new)
                .ok_or("missing".into())
        }
        fn write(&self, slot: &str, key: &str) -> Result<(), String> {
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("locked".into());
            }
            self.values.lock().unwrap().insert(slot.into(), key.into());
            Ok(())
        }
        fn remove(&self, slot: &str) -> Result<(), String> {
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("locked".into());
            }
            self.values.lock().unwrap().remove(slot);
            Ok(())
        }
    }
    #[test]
    fn system_session_transitions_preserve_verified_legacy_scope_and_fail_atomically() {
        let storage = Storage::memory();
        let profile = turn(&storage, "system", "main").profile;
        let secrets = Arc::new(TestSecrets::default());
        secrets.write("old-slot", "same-key").unwrap();
        storage
            .replace_credential(
                &profile,
                Some(&CredentialReference {
                    slot_id: "old-slot".into(),
                    scope: "legacy-scope".into(),
                    endpoint: profile.config.endpoint.clone(),
                }),
            )
            .unwrap();
        let credentials = Credentials {
            sessions: Mutex::default(),
            keystore: secrets.clone(),
        };
        credentials
            .set(&storage, &profile, "same-key".into(), true)
            .unwrap();
        assert_eq!(
            credentials.get(&storage, &profile).unwrap().scope,
            "legacy-scope"
        );
        assert_eq!(secrets.values.lock().unwrap().len(), 1);
        credentials
            .set(&storage, &profile, "same-key".into(), false)
            .unwrap();
        assert!(secrets.values.lock().unwrap().is_empty());
        assert!(storage.credential_reference(&profile.id).unwrap().is_none());
        drop(credentials);
        let credentials = Credentials {
            sessions: Mutex::default(),
            keystore: secrets.clone(),
        };
        assert!(!credentials.status(&storage, &profile).unwrap().configured);
        credentials
            .set(&storage, &profile, "same-key".into(), false)
            .unwrap();
        assert_eq!(
            credentials.get(&storage, &profile).unwrap().scope,
            "legacy-scope"
        );
        secrets
            .fail
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(
            credentials
                .set(&storage, &profile, "different-key".into(), true)
                .is_err()
        );
        assert_eq!(
            credentials.get(&storage, &profile).unwrap().key.as_str(),
            "same-key"
        );
        secrets
            .fail
            .store(false, std::sync::atomic::Ordering::SeqCst);
        credentials
            .set(&storage, &profile, "different-key".into(), true)
            .unwrap();
        secrets
            .fail
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(
            credentials
                .set(&storage, &profile, "same-key".into(), false)
                .is_err()
        );
        assert_eq!(
            credentials.get(&storage, &profile).unwrap().key.as_str(),
            "different-key"
        );
        assert!(storage.credential_reference(&profile.id).unwrap().is_some());
    }
    #[test]
    fn reentered_session_key_resumes_after_database_reopen_but_other_keys_cannot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let storage = Storage::open(&path).unwrap();
        let mut first = turn(&storage, "conversation", "main");
        let credentials = Credentials::default();
        credentials
            .set(&storage, &first.profile, "same-secret".into(), false)
            .unwrap();
        first.auth_scope = credentials.get(&storage, &first.profile).unwrap().scope;
        storage.begin_turn(&first).unwrap();
        let output = serde_json::json!({"continuation":"private-native-context"});
        storage
            .update_turn(&first, 1, "Bonjour", "complete", None, Some(&output))
            .unwrap();
        drop(credentials);
        drop(storage);
        let storage = Storage::open(&path).unwrap();
        let credentials = Credentials::default();
        assert!(
            !credentials
                .status(&storage, &first.profile)
                .unwrap()
                .configured
        );
        credentials
            .set(&storage, &first.profile, "different-secret".into(), false)
            .unwrap();
        let mut follow = first.clone();
        follow.id = uuid::Uuid::new_v4().to_string();
        follow.user_id = uuid::Uuid::new_v4().to_string();
        follow.assistant_id = uuid::Uuid::new_v4().to_string();
        follow.auth_scope = credentials.get(&storage, &first.profile).unwrap().scope;
        assert!(storage.begin_turn(&follow).is_err());
        credentials
            .set(&storage, &first.profile, "same-secret".into(), false)
            .unwrap();
        follow.auth_scope = credentials.get(&storage, &first.profile).unwrap().scope;
        assert_eq!(follow.auth_scope, first.auth_scope);
        assert_eq!(
            storage.api_history("conversation").unwrap()[0].2,
            Some(output)
        );
        storage.begin_turn(&follow).unwrap();
        storage
            .update_turn(&follow, 1, "Salut", "complete", None, None)
            .unwrap();
        assert_eq!(storage.read("conversation").unwrap().messages.len(), 4);
        drop(storage);
        let bytes = std::fs::read(path).unwrap();
        assert!(
            !bytes
                .windows(b"same-secret".len())
                .any(|w| w == b"same-secret")
        );
        assert!(
            !bytes
                .windows(b"different-secret".len())
                .any(|w| w == b"different-secret")
        );
    }
    #[test]
    fn legacy_verified_scope_is_preserved_and_profile_endpoint_identities_are_separate() {
        let storage = Storage::memory();
        let profile = turn(&storage, "one", "main").profile;
        let other = turn(&storage, "two", "tutor").profile;
        let old = storage
            .credential_scope(&profile, "fixture-key", Some("legacy-scope"))
            .unwrap();
        assert_eq!(old, "legacy-scope");
        assert_eq!(
            storage
                .credential_scope(&profile, "fixture-key", None)
                .unwrap(),
            old
        );
        assert_ne!(
            storage
                .credential_scope(&other, "fixture-key", None)
                .unwrap(),
            old
        );
        let mut changed = profile.clone();
        changed.config.endpoint = "https://other.invalid/v1".into();
        assert_ne!(
            storage
                .credential_scope(&changed, "fixture-key", None)
                .unwrap(),
            old
        );
    }
    #[test]
    fn session_keys_are_endpoint_bound_and_never_enter_the_database() {
        let storage = Storage::memory();
        let profile = turn(&storage, "c", "main").profile;
        let credentials = Credentials::default();
        credentials
            .set(&storage, &profile, "fixture-secret".into(), false)
            .unwrap();
        assert!(storage.credential_reference(&profile.id).unwrap().is_none());
        assert!(
            !serde_json::to_string(&storage.backend_profiles().unwrap())
                .unwrap()
                .contains("fixture-secret")
        );
        let first = credentials.get(&storage, &profile).unwrap();
        assert_eq!(first.key.as_str(), "fixture-secret");
        credentials
            .set(&storage, &profile, "second-fixture".into(), false)
            .unwrap();
        assert_ne!(
            credentials.get(&storage, &profile).unwrap().scope,
            first.scope
        );
        let mut changed = profile.clone();
        changed.config.endpoint = "https://other.example.invalid/v1".into();
        assert!(!credentials.status(&storage, &changed).unwrap().configured);
        assert!(credentials.get(&storage, &changed).is_err());
        assert!(
            !Credentials::default()
                .status(&storage, &profile)
                .unwrap()
                .configured
        );
        credentials.remove(&storage, &profile).unwrap();
        assert!(!credentials.status(&storage, &profile).unwrap().configured);
    }
}
