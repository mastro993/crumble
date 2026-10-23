// @generated automatically by Diesel CLI.

diesel::table! {
    accounts (id) {
        id -> Text,
        position -> Nullable<Integer>,
        color -> Nullable<Text>,
        alias -> Nullable<Text>,
        disabled -> Bool,
        hidden -> Bool,
    }
}

diesel::table! {
    app_state (id) {
        id -> Integer,
        first_day_of_week -> Integer,
        calendar_view -> Text,
        transcription_locale -> Nullable<Text>,
        suggest_recording -> Bool,
        local_model -> Nullable<Text>,
        summary_provider -> Nullable<Text>,
        transcription_provider -> Text,
        intelligence_provider -> Nullable<Text>,
    }
}

diesel::table! {
    calendars (id) {
        id -> Text,
        disabled -> Bool,
    }
}

diesel::table! {
    notes (id) {
        id -> Text,
        title -> Nullable<Text>,
        memo -> Text,
        event_series_uid -> Nullable<Text>,
        event_occurrence -> Nullable<Text>,
        event_title -> Nullable<Text>,
        event_start -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        deleted_at -> Nullable<Text>,
        recording_started_at -> Nullable<Text>,
        recording_duration_ms -> Nullable<Integer>,
        transcript_model -> Nullable<Text>,
        transcript_locale -> Nullable<Text>,
        transcribed_at -> Nullable<Text>,
        summary -> Nullable<Text>,
        summary_provider -> Nullable<Text>,
        summary_model -> Nullable<Text>,
        summarized_at -> Nullable<Text>,
        transcript_provider -> Nullable<Text>,
    }
}

diesel::table! {
    providers (id) {
        id -> Text,
        base_url -> Nullable<Text>,
        summary_model -> Nullable<Text>,
        transcription_model -> Nullable<Text>,
        intelligence_model -> Nullable<Text>,
    }
}

diesel::table! {
    remote_audio_consents (provider, model, host) {
        provider -> Text,
        model -> Text,
        host -> Text,
        acknowledged_at -> Text,
    }
}

diesel::table! {
    transcript_segments (id) {
        id -> Text,
        note_id -> Text,
        track -> Text,
        start_ms -> Integer,
        end_ms -> Integer,
        text -> Text,
    }
}

diesel::joinable!(transcript_segments -> notes (note_id));

diesel::allow_tables_to_appear_in_same_query!(
    accounts,
    app_state,
    calendars,
    notes,
    providers,
    remote_audio_consents,
    transcript_segments,
);
