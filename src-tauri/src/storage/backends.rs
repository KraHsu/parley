use super::*;
use crate::backends::types::{BackendProfile, DEFAULT_CODEX_PROFILE, SaveProfile};
use rusqlite::Transaction;

fn profile_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BackendProfile> {
    let config: String = row.get(2)?;
    Ok(BackendProfile {
        id: row.get(0)?,
        revision: row.get(1)?,
        config: serde_json::from_str(&config).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
        })?,
    })
}

fn write_version(tx: &Transaction<'_>, profile: &BackendProfile) -> Result<()> {
    let config = serde_json::to_string(&profile.config).map_err(error)?;
    tx.execute(
        "UPDATE backend_profiles SET revision=?1,config=?2,updated_at=?3 WHERE id=?4",
        params![profile.revision, config, now(), profile.id],
    )
    .map_err(error)?;
    tx.execute(
        "INSERT INTO backend_profile_versions VALUES(?1,?2,?3)",
        params![profile.id, profile.revision, config],
    )
    .map_err(error)?;
    Ok(())
}

pub(super) fn sync_codex_path(tx: &Transaction<'_>, path: &str) -> Result<()> {
    let path = path.trim();
    let mut profile = tx
        .query_row(
            "SELECT id,revision,config FROM backend_profiles WHERE id=?1",
            [DEFAULT_CODEX_PROFILE],
            profile_row,
        )
        .map_err(error)?;
    if profile.config.binary_path != path {
        ensure_inactive(tx, &profile.id)?;
        profile.config.binary_path = path.to_owned();
        profile.config.validate()?;
        profile.revision += 1;
        write_version(tx, &profile)?;
    }
    Ok(())
}

fn ensure_inactive(tx: &Transaction<'_>, id: &str) -> Result<()> {
    let running: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM conversation_backends b JOIN conversations c ON c.id=b.conversation_id WHERE b.profile_id=?1 AND c.status='running')",
        [id], |r| r.get(0),
    ).map_err(error)?;
    if running {
        return Err("该服务仍在回复，请先停止生成再修改配置。".into());
    }
    Ok(())
}

impl Storage {
    pub fn backend_profiles(&self) -> Result<Vec<BackendProfile>> {
        let db = self.db.lock().unwrap();
        let mut query = db
            .prepare("SELECT id,revision,config FROM backend_profiles ORDER BY created_at,id")
            .map_err(error)?;
        query
            .query_map([], profile_row)
            .map_err(error)?
            .collect::<rusqlite::Result<_>>()
            .map_err(error)
    }

    pub fn backend_profile(&self, id: &str) -> Result<BackendProfile> {
        self.db
            .lock()
            .unwrap()
            .query_row(
                "SELECT id,revision,config FROM backend_profiles WHERE id=?1",
                [id],
                profile_row,
            )
            .map_err(error)
    }

    pub fn save_backend_profile(&self, mut request: SaveProfile) -> Result<BackendProfile> {
        request.config.validate()?;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        let profile = if let Some(id) = request.id {
            let old = tx
                .query_row(
                    "SELECT id,revision,config FROM backend_profiles WHERE id=?1",
                    [&id],
                    profile_row,
                )
                .map_err(error)?;
            if request.expected_revision != Some(old.revision) {
                return Err("服务配置已被更新，请重新读取后再保存。".into());
            }
            // Identity is immutable. Create another profile to change protocol/vendor.
            if old.config.kind != request.config.kind
                || old.config.provider != request.config.provider
            {
                return Err("更换厂商或协议请创建新的服务配置。".into());
            }
            if old.config == request.config {
                return Ok(old);
            }
            ensure_inactive(&tx, &id)?;
            let profile = BackendProfile {
                id,
                revision: old.revision + 1,
                config: request.config,
            };
            write_version(&tx, &profile)?;
            profile
        } else {
            if request.expected_revision.is_some() {
                return Err("新服务不能指定已有配置版本。".into());
            }
            let profile = BackendProfile {
                id: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                config: request.config,
            };
            let config = serde_json::to_string(&profile.config).map_err(error)?;
            tx.execute(
                "INSERT INTO backend_profiles VALUES(?1,1,?2,?3,?3)",
                params![profile.id, config, now()],
            )
            .map_err(error)?;
            tx.execute(
                "INSERT INTO backend_profile_versions VALUES(?1,1,?2)",
                params![profile.id, config],
            )
            .map_err(error)?;
            profile
        };
        if profile.id == DEFAULT_CODEX_PROFILE {
            let saved: Option<String> = tx
                .query_row("SELECT value FROM preferences WHERE id=1", [], |r| r.get(0))
                .optional()
                .map_err(error)?;
            let mut preferences: Value = saved
                .map(|s| serde_json::from_str(&s).map_err(error))
                .transpose()?
                .unwrap_or_else(|| serde_json::json!({}));
            preferences["codexPath"] = Value::String(profile.config.binary_path.clone());
            tx.execute("INSERT INTO preferences VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET value=excluded.value", [preferences.to_string()]).map_err(error)?;
        }
        tx.commit().map_err(error)?;
        Ok(profile)
    }

    pub fn interrupt_backend(&self, profile: &str) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        tx.execute("UPDATE conversations SET status='interrupted' WHERE status='running' AND id IN (SELECT conversation_id FROM conversation_backends WHERE profile_id=?1)", [profile]).map_err(error)?;
        tx.execute("UPDATE messages SET status='interrupted' WHERE status IN ('streaming','pending') AND conversation_id IN (SELECT conversation_id FROM conversation_backends WHERE profile_id=?1)", [profile]).map_err(error)?;
        tx.commit().map_err(error)
    }

    pub fn create_for_backend(
        &self,
        id: &str,
        pane: &str,
        profile_id: &str,
    ) -> Result<Conversation> {
        if id.is_empty() || id.len() > 100 || !["main", "tutor"].contains(&pane) {
            return Err("会话标识无效。".into());
        }
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(error)?;
        let profile = tx
            .query_row(
                "SELECT id,revision,config FROM backend_profiles WHERE id=?1",
                [profile_id],
                profile_row,
            )
            .map_err(error)?;
        if !profile.config.enabled {
            return Err("该模型服务已停用。".into());
        }
        tx.execute(
            "INSERT INTO conversations(id,pane,mode,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)",
            params![
                id,
                pane,
                if pane == "main" {
                    "conversation"
                } else {
                    "express"
                },
                now()
            ],
        )
        .map_err(error)?;
        tx.execute(
            "INSERT INTO conversation_backends VALUES(?1,?2,?3)",
            params![id, profile_id, profile.revision],
        )
        .map_err(error)?;
        tx.commit().map_err(error)?;
        drop(db);
        self.read(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::types::{BackendKind, ProfileConfig, Provider};

    #[test]
    fn upgrade_creates_a_readable_pre_migration_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("parley.sqlite3");
        {
            let db = Connection::open(&path).unwrap();
            for migration in &super::super::MIGRATIONS[..3] {
                db.execute_batch(migration).unwrap();
            }
            db.execute_batch("PRAGMA journal_mode=WAL; INSERT INTO preferences VALUES(1,'{\"targetLanguage\":\"ja\"}');").unwrap();
        }
        let storage = Storage::open(&path).unwrap();
        assert_eq!(storage.preferences().unwrap().target_language, "ja");
        let backups: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("parley-before-v4-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        let backup =
            Connection::open_with_flags(&backups[0], rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        assert_eq!(
            backup
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            backup
                .query_row(
                    "SELECT json_extract(value,'$.targetLanguage') FROM preferences",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "ja"
        );
    }

    fn config() -> ProfileConfig {
        ProfileConfig {
            name: "My API".into(),
            kind: BackendKind::OpenaiResponses,
            provider: Provider::Openai,
            endpoint: "https://api.openai.com/v1".into(),
            binary_path: String::new(),
            enabled: true,
        }
    }

    #[test]
    fn revisions_preserve_old_bindings_and_reject_stale_edits() {
        let s = Storage::memory();
        let p = s
            .save_backend_profile(SaveProfile {
                id: None,
                expected_revision: None,
                config: config(),
            })
            .unwrap();
        s.create_for_backend("c", "main", &p.id).unwrap();
        let mut changed = p.config.clone();
        changed.name = "Renamed".into();
        let updated = s
            .save_backend_profile(SaveProfile {
                id: Some(p.id.clone()),
                expected_revision: Some(1),
                config: changed.clone(),
            })
            .unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(s.read("c").unwrap().backend.unwrap().profile_revision, 1);
        assert!(
            s.save_backend_profile(SaveProfile {
                id: Some(p.id),
                expected_revision: Some(1),
                config: changed
            })
            .is_err()
        );
    }

    #[test]
    fn codex_disconnect_does_not_interrupt_other_backends() {
        let s = Storage::memory();
        let p = s
            .save_backend_profile(SaveProfile {
                id: None,
                expected_revision: None,
                config: config(),
            })
            .unwrap();
        s.create("codex", "main").unwrap();
        s.create_for_backend("api", "tutor", &p.id).unwrap();
        for id in ["codex", "api"] {
            s.begin(
                id,
                "same-user-id",
                "hello",
                ConversationConfig {
                    model: "test",
                    target: "en",
                    native: "zh-CN",
                    mode: "conversation",
                },
            )
            .unwrap();
            s.event(
                id,
                "item/agentMessage/delta",
                &serde_json::json!({"itemId":"same-item-id","delta":"你好"}),
            )
            .unwrap();
        }
        s.interrupt_backend(DEFAULT_CODEX_PROFILE).unwrap();
        assert_eq!(s.read("codex").unwrap().status, "interrupted");
        let api = s.read("api").unwrap();
        assert_eq!(api.status, "running");
        assert_eq!(api.messages[1].status, "streaming");
        let mut changed = p.config;
        changed.enabled = false;
        assert!(
            s.save_backend_profile(SaveProfile {
                id: Some(p.id),
                expected_revision: Some(1),
                config: changed
            })
            .is_err()
        );
    }

    #[test]
    fn legacy_preferences_and_default_profile_share_one_path() {
        let s = Storage::memory();
        let mut preferences = Preferences {
            codex_path: "/tools/codex".into(),
            ..Default::default()
        };
        s.save(&preferences, &[]).unwrap();
        let mut p = s.backend_profile(DEFAULT_CODEX_PROFILE).unwrap();
        assert_eq!(p.config.binary_path, preferences.codex_path);
        p.config.binary_path = "/new/codex".into();
        s.save_backend_profile(SaveProfile {
            id: Some(p.id),
            expected_revision: Some(p.revision),
            config: p.config,
        })
        .unwrap();
        preferences = s.preferences().unwrap();
        assert_eq!(preferences.codex_path, "/new/codex");
        preferences.codex_path = " /new/codex ".into();
        let revision = s.backend_profile(DEFAULT_CODEX_PROFILE).unwrap().revision;
        s.save(&preferences, &[]).unwrap();
        s.save(&preferences, &[]).unwrap();
        assert_eq!(
            s.backend_profile(DEFAULT_CODEX_PROFILE).unwrap().revision,
            revision
        );
    }

    #[test]
    fn version_three_migration_keeps_original_threads_and_message_ids() {
        let mut db = Connection::open_in_memory().unwrap();
        for migration in &super::super::MIGRATIONS[..3] {
            db.execute_batch(migration).unwrap();
        }
        db.execute(
            "INSERT INTO preferences VALUES(1,?1)",
            [r#"{"codexPath":"/old tools/codex","mainModel":"original-model"}"#],
        )
        .unwrap();
        db.execute_batch("INSERT INTO conversations(id,pane,thread_id,account,signature,created_at,updated_at) VALUES('old','main','original-thread','original-account','original-signature',1,1); INSERT INTO messages(id,conversation_id,role,text,status) VALUES('original-item','old','assistant','原句','complete');").unwrap();
        super::super::migrate(&mut db).unwrap();
        let profile = db
            .query_row(
                "SELECT id,revision,config FROM backend_profiles",
                [],
                profile_row,
            )
            .unwrap();
        assert_eq!(profile.config.binary_path, "/old tools/codex");
        let original: (String,String,String,String) = db.query_row("SELECT thread_id,account,signature,m.id FROM conversations c JOIN messages m ON c.id=m.conversation_id", [], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
        assert_eq!(
            original,
            (
                "original-thread".into(),
                "original-account".into(),
                "original-signature".into(),
                "original-item".into()
            )
        );
        assert_eq!(
            db.query_row(
                "SELECT profile_id FROM conversation_backends WHERE conversation_id='old'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            DEFAULT_CODEX_PROFILE
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
    }
}
