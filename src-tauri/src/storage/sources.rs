//! Public source provenance, resolved from immutable conversation bindings.
use super::{
    error,
    learning_rows::OccurrenceRow,
    schema::{
        backend_profile_versions as v, conversation_backends as b, conversations as c,
        vocabulary_entries as e, vocabulary_occurrences as o,
    },
};
use crate::{
    backends::types::{BackendKind, ProfileConfig, Provider},
    vocabulary::{Occurrence, Result, Source, SourceBackend},
};
use diesel::prelude::*;

pub(super) fn resolve(db: &mut SqliteConnection, source: &Source) -> Result<Source> {
    let mut source = source.clone();
    if let Some(conversation) = &source.conversation_id {
        let binding = c::table
            .inner_join(b::table.on(b::conversation_id.eq(c::id)))
            .inner_join(
                v::table.on(v::profile_id
                    .eq(b::profile_id)
                    .and(v::revision.eq(b::profile_revision))),
            )
            .filter(c::id.eq(conversation))
            .select((v::config, c::model))
            .first::<(String, String)>(db)
            .optional()
            .map_err(error)?;
        if let Some((config, model)) = binding {
            let config: ProfileConfig = serde_json::from_str(&config).map_err(error)?;
            let actual = SourceBackend {
                kind: config.kind,
                provider: config.provider,
                model: (!model.is_empty()).then_some(model),
            };
            if source
                .backend
                .as_ref()
                .is_some_and(|provided| provided != &actual)
            {
                return Err("来源后端与实际会话不匹配，请重新选择原句。".into());
            }
            source.backend = Some(actual);
        }
    }
    if source.source_kind == "terminal" {
        let claude = source
            .thread_id
            .as_ref()
            .is_some_and(|s| s.starts_with("claude-code:"));
        if claude
            && source
                .backend
                .as_ref()
                .is_some_and(|b| b.kind != BackendKind::ClaudeCode)
        {
            return Err("终端来源命名空间与后端不匹配。".into());
        }
        source.backend.get_or_insert(SourceBackend {
            kind: if claude {
                BackendKind::ClaudeCode
            } else {
                BackendKind::Codex
            },
            provider: if claude {
                Provider::Anthropic
            } else {
                Provider::Openai
            },
            model: None,
        });
        if source
            .backend
            .as_ref()
            .is_some_and(|b| b.kind == BackendKind::ClaudeCode)
            && !claude
        {
            source.thread_id = source.thread_id.map(|s| format!("claude-code:{s}"));
        }
    }
    source.validate()?;
    Ok(source)
}

pub(super) fn matching(
    db: &mut SqliteConnection,
    source: &Source,
    entry: Option<&str>,
    language: Option<&str>,
) -> Result<Option<(String, String)>> {
    let mut rows = o::table
        .inner_join(e::table.on(e::id.eq(o::entry_id)))
        .filter(
            o::fingerprint
                .eq(source.fingerprint())
                .or(o::fingerprint.eq(source.legacy_fingerprint())),
        )
        .into_boxed();
    if let Some(entry) = entry {
        rows = rows.filter(o::entry_id.eq(entry));
    }
    if let Some(language) = language {
        rows = rows
            .filter(e::language.eq(language))
            .filter(e::deleted_at.is_null());
    }
    let candidates = rows
        .order((e::created_at, o::rowid))
        .select((o::entry_id, o::fingerprint, o::backend))
        .load::<(String, String, Option<String>)>(db)
        .map_err(error)?;
    for (entry, hash, backend) in candidates {
        let backend = backend
            .map(|v| serde_json::from_str::<SourceBackend>(&v).map_err(error))
            .transpose()?;
        if backend == source.backend {
            return Ok(Some((entry, hash)));
        }
    }
    Ok(None)
}

pub(super) fn migrate(db: &mut SqliteConnection) -> Result<()> {
    let rows = o::table
        .select(OccurrenceRow::as_select())
        .load::<OccurrenceRow>(db)
        .map_err(error)?;
    for row in rows {
        let occurrence = Occurrence::try_from(row)?;
        let source = resolve(db, &occurrence.source)?;
        // Keep old uniqueness keys, including keys whose local message was deleted.
        diesel::update(o::table.find(&occurrence.id))
            .set(
                o::backend.eq(source
                    .backend
                    .map(|b| serde_json::to_string(&b).expect("public source serializes"))),
            )
            .execute(db)
            .map_err(error)?;
    }
    Ok(())
}
