// Mirrors the existing SQLite migrations; column types are checked by Diesel.
// No schema changes are performed by this module. Explicit rowid declarations
// expose SQLite insertion order used by the pre-Diesel repositories.

diesel::table! {
    backend_credentials (profile_id) {
        profile_id -> Text,
        slot_id -> Text,
        scope -> Text,
        endpoint -> Text,
    }
}

diesel::table! {
    backend_profile_versions (profile_id, revision) {
        profile_id -> Text,
        revision -> BigInt,
        config -> Text,
    }
}

diesel::table! {
    backend_profiles (id) {
        id -> Text,
        revision -> BigInt,
        config -> Text,
        created_at -> BigInt,
        updated_at -> BigInt,
    }
}

diesel::table! {
    conversation_backends (conversation_id) {
        conversation_id -> Text,
        profile_id -> Text,
        profile_revision -> BigInt,
    }
}

diesel::table! {
    conversations (id) {
        rowid -> BigInt,
        id -> Text,
        pane -> Text,
        title -> Text,
        model -> Text,
        target_language -> Text,
        native_language -> Text,
        mode -> Text,
        draft -> Text,
        thread_id -> Nullable<Text>,
        account -> Nullable<Text>,
        signature -> Text,
        status -> Text,
        created_at -> BigInt,
        updated_at -> BigInt,
    }
}

diesel::table! {
    messages (sequence) {
        sequence -> BigInt,
        id -> Text,
        conversation_id -> Text,
        role -> Text,
        text -> Text,
        status -> Text,
        turn_id -> Nullable<Text>,
    }
}

diesel::table! {
    model_turns (id) {
        id -> Text,
        conversation_id -> Text,
        profile_id -> Text,
        profile_revision -> BigInt,
        auth_scope -> Text,
        model -> Text,
        user_message_id -> Text,
        assistant_message_id -> Text,
        provider_input -> Text,
        status -> Text,
        sequence -> BigInt,
        usage -> Nullable<Text>,
        provider_output -> Nullable<Text>,
        created_at -> BigInt,
    }
}

diesel::table! {
    preferences (id) {
        id -> BigInt,
        value -> Text,
    }
}

diesel::table! {
    vocabulary_cards (id) {
        id -> Text,
        entry_id -> Text,
        direction -> Text,
        stage -> BigInt,
        due_at -> BigInt,
        last_reviewed_at -> Nullable<BigInt>,
        suspended -> BigInt,
        schedule_version -> BigInt,
        revision -> BigInt,
    }
}

diesel::table! {
    vocabulary_drafts (id) {
        id -> Text,
        entry_id -> Nullable<Text>,
        payload -> Text,
        updated_at -> BigInt,
    }
}

diesel::table! {
    vocabulary_entries (id) {
        id -> Text,
        language -> Text,
        language_label -> Text,
        kind -> Text,
        text -> Text,
        lookup_key -> Text,
        meaning -> Text,
        meaning_language -> Text,
        note -> Text,
        search_text -> Text,
        revision -> BigInt,
        created_at -> BigInt,
        updated_at -> BigInt,
        deleted_at -> Nullable<BigInt>,
    }
}

diesel::table! {
    vocabulary_entry_tags (entry_id, tag_id) {
        entry_id -> Text,
        tag_id -> Text,
    }
}

diesel::table! {
    vocabulary_import_records (dataset_id, record_id, content_hash) {
        rowid -> BigInt,
        dataset_id -> Text,
        record_id -> Text,
        content_hash -> Text,
        local_id -> Text,
    }
}

diesel::table! {
    vocabulary_metadata (key) {
        key -> Text,
        value -> Text,
    }
}

diesel::table! {
    vocabulary_mutations (request_id) {
        request_id -> Text,
        operation -> Text,
        fingerprint -> Text,
        result -> Text,
        created_at -> BigInt,
    }
}

diesel::table! {
    vocabulary_occurrences (id) {
        rowid -> BigInt,
        id -> Text,
        entry_id -> Text,
        source_kind -> Text,
        conversation_id -> Nullable<Text>,
        message_id -> Nullable<Text>,
        thread_id -> Nullable<Text>,
        turn_id -> Nullable<Text>,
        item_id -> Nullable<Text>,
        role -> Text,
        selected_text -> Text,
        snapshot -> Text,
        snapshot_hash -> Text,
        start -> BigInt,
        end -> BigInt,
        locator_version -> BigInt,
        truncated -> BigInt,
        fingerprint -> Text,
        backend -> Nullable<Text>,
    }
}

diesel::table! {
    vocabulary_reviews (id) {
        rowid -> BigInt,
        id -> Text,
        card_id -> Text,
        request_id -> Text,
        rating -> Text,
        reviewed_at -> BigInt,
        before_state -> Text,
        after_state -> Text,
        undone_at -> Nullable<BigInt>,
    }
}

diesel::table! {
    vocabulary_tags (id) {
        id -> Text,
        name -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    backend_credentials,
    backend_profile_versions,
    backend_profiles,
    conversation_backends,
    conversations,
    messages,
    model_turns,
    preferences,
    vocabulary_cards,
    vocabulary_drafts,
    vocabulary_entries,
    vocabulary_entry_tags,
    vocabulary_import_records,
    vocabulary_metadata,
    vocabulary_mutations,
    vocabulary_occurrences,
    vocabulary_reviews,
    vocabulary_tags,
);
