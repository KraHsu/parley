use super::types::{BackendKind, BackendProfile};
use crate::storage::Storage;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Mutex};
use zeroize::Zeroizing;

const SERVICE: &str = "dev.parley.model-api";

pub struct Credential {
    pub key: Zeroizing<String>,
    pub scope: String,
    pub endpoint: String,
}

#[derive(Default)]
pub struct Credentials {
    // Also serializes system-keystore operations, whose implementations may not be concurrent.
    sessions: Mutex<HashMap<String, Credential>>,
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
        let scope = uuid::Uuid::new_v4().to_string();
        let mut sessions = self.sessions.lock().unwrap();
        if persist {
            let reference = CredentialReference {
                slot_id: uuid::Uuid::new_v4().to_string(),
                scope,
                endpoint: profile.config.endpoint.clone(),
            };
            let new = entry(&reference.slot_id)?;
            new.set_password(key.as_str()).map_err(|_| {
                "无法保存到系统凭据库，请解锁凭据库，或选择仅本次会话使用。".to_owned()
            })?;
            let old = match storage.replace_credential(profile, Some(&reference)) {
                Ok(old) => old,
                Err(error) => {
                    let _ = new.delete_credential();
                    return Err(error);
                }
            };
            sessions.remove(&profile.id);
            if let Some(old) = old {
                let _ = entry(&old.slot_id)
                    .and_then(|e| e.delete_credential().map_err(|_| "删除旧凭据失败".into()));
            }
        } else {
            storage.check_profile_revision(&profile.id, profile.revision)?;
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
        let key = entry(&reference.slot_id)?
            .get_password()
            .map_err(|_| "无法读取 API Key，请解锁系统凭据库或重新输入密钥。".to_owned())?;
        Ok(Credential {
            key: Zeroizing::new(key),
            scope: reference.scope,
            endpoint: reference.endpoint,
        })
    }

    pub fn remove(&self, storage: &Storage, profile: &BackendProfile) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        // Keep the reference if the system store refuses deletion, so the user can retry.
        storage.remove_credential_with(profile, |reference| {
            if let Some(reference) = reference {
                match entry(&reference.slot_id)?.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(_) => return Err("系统凭据库未能删除密钥，请解锁后重试。".into()),
                }
            }
            Ok(())
        })?;
        sessions.remove(&profile.id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::api_tests::turn;
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
