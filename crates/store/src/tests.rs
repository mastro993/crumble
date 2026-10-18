use super::{AccountColor, CalendarView, Store, Weekday, account_label, db};
use crumble_eventkit::{Account, Calendar, Provider};
use diesel::{RunQueryDsl, sql_query};

impl Store {
    /// What a reopened database reads.
    fn reloaded(&mut self) -> db::State {
        let connection = self.connection.as_mut().map(db::load);
        connection
            .and_then(Result::ok)
            .unwrap_or_else(|| panic!("no database"))
    }
}

#[test]
fn a_new_database_holds_the_defaults() {
    let mut store = Store::in_memory();
    assert!(store.connection.is_some());
    assert_eq!(store.state, db::State::default());
    assert_eq!(store.first_day_of_week(), Weekday::Mon);
    assert_eq!(store.calendar_view(), CalendarView::Month);
    assert!(store.suggests_recording());
    assert_eq!(store.reloaded(), db::State::default());
}

#[test]
fn every_change_is_written_and_read_back() {
    let accounts = ["source-1", "source-2", "source-3"].map(account);
    let mut store = Store::in_memory();
    store.set_first_day_of_week(Weekday::Sun);
    store.set_calendar_view(CalendarView::Year);
    store.toggle_account_disabled("source-1");
    store.toggle_account_hidden("source-2");
    store.toggle_calendar_disabled("calendar-1");
    store.set_account_color("source-1", AccountColor::Pink);
    store.set_account_color("source-2", AccountColor::Custom(0x3366ff));
    store.set_account_alias("source-3", "Work", "me@example.com");
    store.set_listed_order(&accounts, vec!["source-3".into(), "source-2".into()]);
    store.set_summary_provider("openai");
    store.set_claude_model(" claude-opus-5-5 ");
    store.set_openai_base_url("http://localhost:11434/v1");
    store.set_openai_model("llama3.2");
    store.set_transcription_provider("openai");
    store.set_transcription_model(" whisper-large-v3 ");
    store.set_transcription_locale(Some("it_IT"));
    store.add_audio_consent("openai", "whisper-1", "API.openai.com");
    store.set_suggests_recording(false);
    assert_eq!(store.claude_model(), "claude-opus-5-5");
    assert_eq!(store.transcription_model(), "whisper-large-v3");
    assert_eq!(
        store.state.account_order,
        ["source-1", "source-3", "source-2"]
    );
    assert_eq!(store.reloaded(), store.state);

    // Turning things back off is written too.
    store.toggle_account_disabled("source-1");
    store.toggle_account_hidden("source-2");
    store.toggle_calendar_disabled("calendar-1");
    store.set_account_alias("source-3", "", "me@example.com");
    store.set_listed_order(
        &accounts,
        vec!["source-2".into(), "source-1".into(), "source-3".into()],
    );
    assert_eq!(store.reloaded(), store.state);
    assert!(store.state.disabled_accounts.is_empty());
}

#[test]
fn a_database_file_keeps_changes_across_opens() {
    let path = std::env::temp_dir().join(format!(
        "crumble-store-test-{}-{:?}.sqlite3",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    let url = path.to_string_lossy().into_owned();
    let open = || {
        crumble_db::open(&url)
            .and_then(Store::read)
            .unwrap_or_else(|error| panic!("{error}"))
    };
    let mut store = open();
    store.set_first_day_of_week(Weekday::Wed);
    store.set_account_alias("source-1", "Work", "me@example.com");
    let written = store.state.clone();
    drop(store);
    let reopened = open().state;
    let _ = std::fs::remove_file(&path);
    assert_eq!(reopened, written);
}

#[test]
fn the_summary_provider_is_the_picked_one_else_apples_when_available() {
    let mut store = Store::in_memory();
    assert_eq!(store.summary_provider(false), None);
    assert_eq!(store.summary_provider(true), Some("apple"));
    assert_eq!(store.claude_model(), "claude-sonnet-5-5");
    store.set_summary_provider("claude");
    assert_eq!(store.summary_provider(true), Some("claude"));
    assert_eq!(store.reloaded().summary_provider.as_deref(), Some("claude"));
}

#[test]
fn transcripts_start_on_apple_in_the_mac_s_locale() {
    let mut store = Store::in_memory();
    assert_eq!(store.transcription_provider(), "apple");
    assert_eq!(store.transcription_model(), "whisper-1");
    assert_eq!(store.transcription_locale(), None);
    store.set_transcription_locale(Some("it_IT"));
    store.set_transcription_locale(None);
    assert_eq!(store.reloaded().transcription_locale, None);
}

#[test]
fn audio_consent_is_per_provider_model_and_host() {
    let mut store = Store::in_memory();
    assert!(!store.has_audio_consent("openai", "whisper-1", "api.openai.com"));
    store.add_audio_consent("openai", "whisper-1", "API.OpenAI.com");
    store.add_audio_consent("openai", "whisper-1", "api.openai.com");
    assert!(store.has_audio_consent("openai", "whisper-1", "api.openai.com"));
    assert!(!store.has_audio_consent("openai", "gpt-4o-transcribe", "api.openai.com"));
    assert!(!store.has_audio_consent("openai", "whisper-1", "whisper.example.com"));
    assert_eq!(store.reloaded().audio_consents.len(), 1);
}

#[test]
fn a_bad_stored_color_falls_back_to_the_default() {
    let mut store = Store::in_memory();
    store.set_account_color("source-1", AccountColor::Red);
    if let Some(connection) = store.connection.as_mut() {
        let updated = sql_query("UPDATE accounts SET color = 'chartreuse'").execute(connection);
        assert_eq!(updated, Ok(1));
    }
    let reloaded = store
        .connection
        .take()
        .map(Store::read)
        .and_then(Result::ok)
        .unwrap_or_else(|| panic!("no database"));
    assert_eq!(
        reloaded.account_color("source-1"),
        AccountColor::default_for("source-1")
    );
}

#[test]
fn a_failed_read_is_an_error_rather_than_defaults() {
    let mut store = Store::in_memory();
    store.set_first_day_of_week(Weekday::Sun);
    let mut connection = store
        .connection
        .take()
        .unwrap_or_else(|| panic!("no database"));
    assert!(
        sql_query("DROP TABLE calendars")
            .execute(&mut connection)
            .is_ok()
    );
    assert!(Store::read(connection).is_err());
}

fn account(id: &str) -> Account {
    Account {
        id: id.into(),
        name: id.into(),
        email: None,
        provider: Provider::Other,
    }
}

fn calendar(id: &str, account_id: &str) -> Calendar {
    Calendar {
        id: id.into(),
        account_id: account_id.into(),
        name: id.into(),
        color: 0,
    }
}

#[test]
fn a_disabled_account_hides_its_calendars_but_keeps_their_own_setting() {
    let calendars = [
        calendar("work", "google"),
        calendar("holidays", "google"),
        calendar("family", "icloud"),
    ];
    let mut store = Store::in_memory();
    store.toggle_calendar_disabled("holidays");
    assert_eq!(store.shown_calendar_ids(&calendars), ["work", "family"]);

    store.toggle_account_disabled("google");
    assert_eq!(store.shown_calendar_ids(&calendars), ["family"]);

    store.toggle_account_disabled("google");
    assert_eq!(store.shown_calendar_ids(&calendars), ["work", "family"]);
}

#[test]
fn a_hidden_account_hides_its_calendars_whether_or_not_it_is_disabled() {
    let calendars = [calendar("work", "google"), calendar("family", "icloud")];
    let mut store = Store::in_memory();
    store.toggle_account_hidden("google");
    assert_eq!(store.shown_calendar_ids(&calendars), ["family"]);

    // Disabling and Enabling it again leaves it Hidden.
    store.toggle_account_disabled("google");
    store.toggle_account_disabled("google");
    assert_eq!(store.shown_calendar_ids(&calendars), ["family"]);
}

#[test]
fn reordering_the_listed_accounts_keeps_disabled_ones_in_their_slot() {
    let accounts = ["a", "b", "c", "d"].map(account);
    let mut store = Store::in_memory();
    store.toggle_account_disabled("b");
    store.set_listed_order(&accounts, vec!["d".into(), "a".into(), "c".into()]);
    assert_eq!(store.state.account_order, ["d", "b", "a", "c"]);

    // A list that no longer matches the Accounts leaves the order alone.
    store.set_listed_order(&accounts, vec!["a".into()]);
    assert_eq!(store.state.account_order, ["d", "b", "a", "c"]);
    assert_eq!(store.reloaded().account_order, ["d", "b", "a", "c"]);
}

#[test]
fn an_account_keeps_its_default_color_until_the_user_picks_one() {
    let mut store = Store::in_memory();
    let default = store.account_color("source-1");
    assert_eq!(default, AccountColor::default_for("source-1"));
    assert_ne!(default, AccountColor::Gray);
    // Fixed across launches, and spread across the palette.
    assert_eq!(AccountColor::default_for("source-1"), default);
    let spread: std::collections::BTreeSet<_> = (0..40)
        .map(|n| format!("{:?}", AccountColor::default_for(&format!("source-{n}"))))
        .collect();
    assert!(spread.len() > 3);

    store.set_account_color("source-1", AccountColor::Gray);
    assert_eq!(store.account_color("source-1"), AccountColor::Gray);
}

#[test]
fn an_alias_is_trimmed_and_a_blank_one_or_the_own_name_removes_it() {
    let own = "me@example.com";
    let mut store = Store::in_memory();
    assert_eq!(store.account_alias("source-1"), None);
    store.set_account_alias("source-1", "  Work ", own);
    assert_eq!(store.account_alias("source-1"), Some("Work"));
    store.set_account_alias("source-1", "   ", own);
    assert_eq!(store.account_alias("source-1"), None);

    store.set_account_alias("source-1", "Work", own);
    store.set_account_alias("source-1", " me@example.com", own);
    assert_eq!(store.account_alias("source-1"), None);
}

#[test]
fn accounts_follow_the_saved_order_and_new_ones_come_last() {
    let accounts = ["a", "b", "c", "d"].map(account);
    let mut store = Store::in_memory();
    store.state.account_order = vec!["c".into(), "gone".into(), "a".into()];
    let ids: Vec<&str> = store
        .ordered_accounts(&accounts)
        .iter()
        .map(|account| account.id.as_str())
        .collect();
    assert_eq!(ids, ["c", "a", "b", "d"]);
}

#[test]
fn an_account_is_labelled_with_its_email_else_its_name() {
    let mut account = account("source-1");
    assert_eq!(account_label(&account), "source-1");
    account.email = Some("me@example.com".into());
    assert_eq!(account_label(&account), "me@example.com");
}

#[test]
fn an_account_goes_by_its_alias_else_its_label() {
    let account = account("source-1");
    let mut store = Store::in_memory();
    assert_eq!(store.account_name(&account), "source-1");
    store.set_account_alias("source-1", "Work", "source-1");
    assert_eq!(store.account_name(&account), "Work");
}

#[test]
fn listed_accounts_skip_disabled_ones_and_keep_the_order() {
    let accounts = ["a", "b", "c"].map(account);
    let mut store = Store::in_memory();
    store.state.account_order = vec!["c".into(), "b".into(), "a".into()];
    store.toggle_account_disabled("b");
    let ids: Vec<&str> = store
        .listed_accounts(&accounts)
        .iter()
        .map(|account| account.id.as_str())
        .collect();
    assert_eq!(ids, ["c", "a"]);
}
