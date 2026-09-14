use super::*;
use crate::backends::types::{BackendProfile, DEFAULT_CODEX_PROFILE, SaveProfile};

pub(super) fn sync_codex_path(
    db: &mut diesel::SqliteConnection,
    path: &str,
) -> super::typed::DbResult<()> {
    use super::schema::{backend_profile_versions as v, backend_profiles as p};
    use diesel::prelude::*;
    let path = path.trim();
    let mut profile = typed_profile(db, DEFAULT_CODEX_PROFILE)?;
    if profile.config.binary_path != path {
        typed_inactive(db, &profile.id)?;
        profile.config.binary_path = path.to_owned();
        profile.config.validate()?;
        profile.revision += 1;
        let config = serde_json::to_string(&profile.config).map_err(error)?;
        diesel::update(p::table.find(&profile.id))
            .set((
                p::revision.eq(profile.revision),
                p::config.eq(&config),
                p::updated_at.eq(now()),
            ))
            .execute(db)?;
        diesel::insert_into(v::table)
            .values((
                v::profile_id.eq(&profile.id),
                v::revision.eq(profile.revision),
                v::config.eq(config),
            ))
            .execute(db)?;
    }
    Ok(())
}

#[derive(diesel::Queryable, diesel::Selectable)]
#[diesel(table_name = super::schema::backend_profiles)]
struct ProfileRecord {
    id: String,
    revision: i64,
    config: String,
}
impl TryFrom<ProfileRecord> for BackendProfile {
    type Error = super::typed::DbError;
    fn try_from(row: ProfileRecord) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            revision: row.revision,
            config: serde_json::from_str(&row.config).map_err(error)?,
        })
    }
}
pub(super) fn typed_profile(
    db: &mut diesel::SqliteConnection,
    profile_id: &str,
) -> super::typed::DbResult<BackendProfile> {
    use super::schema::backend_profiles as p;
    use diesel::prelude::*;
    p::table
        .find(profile_id)
        .select(ProfileRecord::as_select())
        .first::<ProfileRecord>(db)?
        .try_into()
}
fn typed_inactive(db: &mut diesel::SqliteConnection, id: &str) -> super::typed::DbResult<()> {
    use super::schema::{conversation_backends as b, conversations as c};
    use diesel::prelude::*;
    let running = diesel::select(diesel::dsl::exists(
        c::table
            .inner_join(b::table.on(b::conversation_id.eq(c::id)))
            .filter(b::profile_id.eq(id))
            .filter(c::status.eq("running")),
    ))
    .get_result::<bool>(db)?;
    if running {
        return Err("该服务仍在回复，请先停止生成再修改配置。".into());
    }
    Ok(())
}
impl Storage {
    pub fn backend_profiles(&self) -> Result<Vec<BackendProfile>> {
        use super::schema::backend_profiles as p;
        use diesel::prelude::*;
        self.typed(|db| {
            p::table
                .order((p::created_at, p::id))
                .select(ProfileRecord::as_select())
                .load::<ProfileRecord>(db)?
                .into_iter()
                .map(TryInto::try_into)
                .collect()
        })
    }
    pub fn backend_profile(&self, id: &str) -> Result<BackendProfile> {
        self.typed(|db| typed_profile(db, id))
    }
    pub fn save_backend_profile(&self, mut request: SaveProfile) -> Result<BackendProfile> {
        use super::schema::{
            backend_profile_versions as v, backend_profiles as p, preferences as pref,
        };
        use diesel::prelude::*;
        request.config.validate()?;
        self.typed_transaction(|db| {
            let is_new = request.id.is_none();
            let profile = if let Some(id) = request.id {
                let old = typed_profile(db, &id)?;
                if request.expected_revision != Some(old.revision) {
                    return Err("服务配置已被更新，请重新读取后再保存。".into());
                }
                if old.config.kind != request.config.kind
                    || old.config.provider != request.config.provider
                {
                    return Err("更换厂商或协议请创建新的服务配置。".into());
                }
                if old.config == request.config {
                    return Ok(old);
                }
                typed_inactive(db, &id)?;
                BackendProfile {
                    id,
                    revision: old.revision + 1,
                    config: request.config,
                }
            } else {
                if request.expected_revision.is_some() {
                    return Err("新服务不能指定已有配置版本。".into());
                }
                BackendProfile {
                    id: uuid::Uuid::new_v4().to_string(),
                    revision: 1,
                    config: request.config,
                }
            };
            let config = serde_json::to_string(&profile.config).map_err(error)?;
            if is_new {
                diesel::insert_into(p::table)
                    .values((
                        p::id.eq(&profile.id),
                        p::revision.eq(profile.revision),
                        p::config.eq(&config),
                        p::created_at.eq(now()),
                        p::updated_at.eq(now()),
                    ))
                    .execute(db)?;
            } else {
                diesel::update(p::table.find(&profile.id))
                    .set((
                        p::revision.eq(profile.revision),
                        p::config.eq(&config),
                        p::updated_at.eq(now()),
                    ))
                    .execute(db)?;
            }
            diesel::insert_into(v::table)
                .values((
                    v::profile_id.eq(&profile.id),
                    v::revision.eq(profile.revision),
                    v::config.eq(config),
                ))
                .execute(db)?;
            if profile.id == DEFAULT_CODEX_PROFILE {
                let saved: Option<String> = pref::table
                    .find(1_i64)
                    .select(pref::value)
                    .first(db)
                    .optional()?;
                let mut preferences: Value = saved
                    .map(|s| serde_json::from_str(&s).map_err(error))
                    .transpose()?
                    .unwrap_or_else(|| serde_json::json!({}));
                preferences["codexPath"] = Value::String(profile.config.binary_path.clone());
                let serialized = preferences.to_string();
                diesel::insert_into(pref::table)
                    .values((pref::id.eq(1_i64), pref::value.eq(&serialized)))
                    .on_conflict(pref::id)
                    .do_update()
                    .set(pref::value.eq(&serialized))
                    .execute(db)?;
            }
            Ok(profile)
        })
    }
    pub fn interrupt_backend(&self, profile: &str) -> Result<()> {
        use super::schema::{conversation_backends as b, conversations as c, messages as m};
        use diesel::prelude::*;
        self.typed_transaction(|db| {
            let ids = b::table
                .filter(b::profile_id.eq(profile))
                .select(b::conversation_id);
            diesel::update(
                c::table
                    .filter(c::id.eq_any(ids))
                    .filter(c::status.eq("running")),
            )
            .set(c::status.eq("interrupted"))
            .execute(db)?;
            diesel::update(
                m::table
                    .filter(m::conversation_id.eq_any(ids))
                    .filter(m::status.eq_any(["streaming", "pending"])),
            )
            .set(m::status.eq("interrupted"))
            .execute(db)?;
            Ok(())
        })
    }
    pub fn create_for_backend(
        &self,
        id: &str,
        pane: &str,
        profile_id: &str,
    ) -> Result<Conversation> {
        use super::schema::{conversation_backends as b, conversations as c};
        use diesel::prelude::*;
        if id.is_empty() || id.len() > 100 || !["main", "tutor"].contains(&pane) {
            return Err("会话标识无效。".into());
        }
        self.typed_transaction(|db| {
            let profile = typed_profile(db, profile_id)?;
            if !profile.config.enabled {
                return Err("该模型服务已停用。".into());
            }
            diesel::insert_into(c::table)
                .values((
                    c::id.eq(id),
                    c::pane.eq(pane),
                    c::mode.eq(if pane == "main" {
                        "conversation"
                    } else {
                        "express"
                    }),
                    c::created_at.eq(now()),
                    c::updated_at.eq(now()),
                ))
                .execute(db)?;
            diesel::insert_into(b::table)
                .values((
                    b::conversation_id.eq(id),
                    b::profile_id.eq(profile_id),
                    b::profile_revision.eq(profile.revision),
                ))
                .execute(db)?;
            Ok(())
        })?;
        self.read(id)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::integer;
    use super::*;
    use crate::backends::types::{BackendKind, ProfileConfig, Provider};
    use diesel::prelude::*;

    #[test]
    fn upgrade_creates_a_readable_pre_migration_backup() {
        for version in [3, 4, 5] {
            check_upgrade_backup(version);
        }
    }

    fn check_upgrade_backup(version: usize) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("parley.sqlite3");
        {
            let mut db = SqliteConnection::establish(path.to_str().unwrap()).unwrap();
            for migration in &super::super::MIGRATIONS[..version] {
                db.batch_execute(migration).unwrap();
            }
            db.batch_execute("PRAGMA journal_mode=WAL; INSERT INTO preferences VALUES(1,'{\"targetLanguage\":\"ja\"}');").unwrap();
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
                    .starts_with("parley-before-v6-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        let mut uri = url::Url::from_file_path(&backups[0]).unwrap();
        uri.set_query(Some("mode=ro"));
        let mut backup = SqliteConnection::establish(uri.as_str()).unwrap();
        assert_eq!(
            super::super::schema_version(&mut backup).unwrap(),
            version as i64
        );
        let saved = super::super::schema::preferences::table
            .select(super::super::schema::preferences::value)
            .first::<String>(&mut backup)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&saved).unwrap()["targetLanguage"],
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
        let mut db = super::super::typed::connect(":memory:").unwrap();
        for migration in &super::super::MIGRATIONS[..3] {
            db.batch_execute(migration).unwrap();
        }
        use super::super::schema::{
            conversation_backends as b, conversations as c, messages as m, preferences as pref,
        };
        diesel::insert_into(pref::table)
            .values((
                pref::id.eq(1_i64),
                pref::value.eq(r#"{"codexPath":"/old tools/codex","mainModel":"original-model"}"#),
            ))
            .execute(&mut db)
            .unwrap();
        db.batch_execute("INSERT INTO conversations(id,pane,thread_id,account,signature,created_at,updated_at) VALUES('old','main','original-thread','original-account','original-signature',1,1); INSERT INTO messages(id,conversation_id,role,text,status) VALUES('original-item','old','assistant','原句','complete');").unwrap();
        super::super::migrate(&mut db).unwrap();
        assert_eq!(
            typed_profile(&mut db, DEFAULT_CODEX_PROFILE)
                .unwrap()
                .config
                .binary_path,
            "/old tools/codex"
        );
        let original = c::table
            .inner_join(m::table.on(c::id.eq(m::conversation_id)))
            .select((c::thread_id, c::account, c::signature, m::id))
            .first::<(Option<String>, Option<String>, String, String)>(&mut db)
            .unwrap();
        assert_eq!(
            original,
            (
                Some("original-thread".into()),
                Some("original-account".into()),
                "original-signature".into(),
                "original-item".into()
            )
        );
        assert_eq!(
            b::table
                .find("old")
                .select(b::profile_id)
                .first::<String>(&mut db)
                .unwrap(),
            DEFAULT_CODEX_PROFILE
        );
        assert_eq!(
            integer(
                &mut db,
                "SELECT count(*) AS value FROM pragma_foreign_key_check"
            ),
            0
        );
    }
}
