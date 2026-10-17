//! Suggestions: a notification as a meeting starts, offering to record it. Each minute, today's
//! Events in the shown Calendars go through `crumble_calendar::suggestions::due`; clicking a
//! notification opens the Event's page, and its "Record" button also starts a Recording there,
//! as the empty state's Record does. Crumble never records without that click.

use super::{NativeView, Route};
use chrono::{Days, Local};
use crumble_calendar::{suggestions, today};
use crumble_eventkit::Access;
use crumble_notify::{Action, Response, Status, Suggestion};
use crumble_sync::CalendarEvents;
use gpui_kit::{AppContext, Context, Entity, Subscription, Task, Window};
use std::collections::HashSet;
use std::rc::Rc;

#[cfg(debug_assertions)]
gpui_kit::actions!(
    crumble,
    [
        /// Posts a Suggestion for an invented Event starting now; debug builds only.
        SuggestSample
    ]
);

/// What the Suggestions need while the app runs.
pub(super) struct Suggestions {
    /// Today's Events in the shown Calendars.
    today: Entity<CalendarEvents>,
    /// The keys of the Events suggested in this run.
    suggested: HashSet<String>,
    /// Whether notifications show, as last read; Settings explains a denial.
    pub(super) status: Status,
    /// A click that came before today's Events were first read, as when it launched Crumble;
    /// handled once they are.
    pending: Option<Response>,
    /// The invented Event [`SuggestSample`] posted, so its clicks find it.
    #[cfg(debug_assertions)]
    sample: Option<crumble_eventkit::Event>,
    /// Handles the clicks on notifications.
    _clicks: Task<()>,
    _subscriptions: [Subscription; 2],
}

impl Suggestions {
    /// Starts hearing clicks on notifications. Called while the app launches, so a click that
    /// launches Crumble is heard too.
    pub(super) fn new(window: &mut Window, cx: &mut Context<NativeView>) -> Self {
        let today = cx.new(|_| CalendarEvents::default());
        let (sender, clicks) = async_channel::unbounded();
        crumble_notify::set_handler(move |response| {
            let _ = sender.try_send(response);
        });
        let clicks = cx.spawn_in(window, async move |view, cx| {
            while let Ok(response) = clicks.recv().await {
                if view
                    .update_in(cx, |view, window, cx| {
                        view.open_suggestion(response, window, cx)
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let subscriptions = [
            cx.observe_in(&today, window, |view, _, window, cx| {
                if let Some(response) = view.suggestions.pending.take() {
                    view.open_suggestion(response, window, cx);
                }
                view.suggest(cx);
            }),
            // Coming back from System Settings shows what changed there.
            cx.observe_window_activation(window, |view, window, cx| {
                if window.is_window_active() {
                    view.read_notification_status(cx);
                }
            }),
        ];
        Self {
            today,
            suggested: HashSet::new(),
            pending: None,
            status: Status::NotDetermined,
            #[cfg(debug_assertions)]
            sample: None,
            _clicks: clicks,
            _subscriptions: subscriptions,
        }
    }
}

impl NativeView {
    /// Reads today's Events in the shown Calendars, unless they are already read; `force` reads
    /// again anyway, as after EventKit reports a change.
    pub(super) fn refresh_today(&mut self, force: bool, cx: &mut Context<Self>) {
        let sync = self.calendar_sync.read(cx);
        let today = today();
        let range = (sync.access() == Access::Granted)
            .then(|| Some((today, today.checked_add_days(Days::new(1))?)))
            .flatten();
        let calendar_ids = self.store.shown_calendar_ids(sync.calendars());
        self.suggestions.today.update(cx, |events, cx| {
            events.refresh(range, calendar_ids, force, cx);
        });
    }

    /// Posts a Suggestion for each Event due now, when the setting is on. Runs each minute and
    /// after each read of today's Events.
    pub(super) fn suggest(&mut self, cx: &mut Context<Self>) {
        if !self.store.suggests_recording() {
            return;
        }
        let shown = self
            .store
            .shown_calendar_ids(self.calendar_sync.read(cx).calendars());
        let recording = self.recording_event().map(suggestions::key);
        let suggestions = &mut self.suggestions;
        let today = suggestions.today.read(cx);
        let due: Vec<Suggestion> = suggestions::due(
            today.events(),
            &shown,
            recording.as_deref(),
            &suggestions.suggested,
            Local::now(),
        )
        .into_iter()
        .map(|event| Suggestion {
            key: suggestions::key(event),
            title: super::notes::event_title(event),
        })
        .collect();
        suggestions
            .suggested
            .extend(due.iter().map(|suggestion| suggestion.key.clone()));
        self.post_suggestions(due, cx);
    }

    /// Posts `due`, asking for notifications first; macOS prompts only the first time, so the
    /// prompt comes with the first meeting rather than at launch.
    fn post_suggestions(&mut self, due: Vec<Suggestion>, cx: &mut Context<Self>) {
        if due.is_empty() {
            return;
        }
        let (answered, answer) = async_channel::bounded(1);
        crumble_notify::authorize(move |allowed| {
            let _ = answered.try_send(allowed);
        });
        cx.spawn(async move |view, cx| {
            if answer.recv().await == Ok(true) {
                for suggestion in &due {
                    crumble_notify::post(suggestion);
                }
            }
            let _ = view.update(cx, |view, cx| view.read_notification_status(cx));
        })
        .detach();
    }

    /// Reads whether notifications show, for the hint under the setting.
    pub(super) fn read_notification_status(&mut self, cx: &mut Context<Self>) {
        let (answered, answer) = async_channel::bounded(1);
        crumble_notify::authorization(move |status| {
            let _ = answered.try_send(status);
        });
        cx.spawn(async move |view, cx| {
            if let Ok(status) = answer.recv().await {
                let _ = view.update(cx, |view, cx| {
                    view.suggestions.status = status;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Opens the page of the Event `response` names, and for "Record" records its Note unless a
    /// Recording runs or the Note has one. Before today's Events are first read, the click waits
    /// for them; an Event not among them then opens the Agenda page.
    fn open_suggestion(&mut self, response: Response, window: &mut Window, cx: &mut Context<Self>) {
        let today = self.suggestions.today.read(cx);
        let read = today.reads() > 0;
        let event = suggestions::find(today.events(), &response.key).cloned();
        #[cfg(debug_assertions)]
        let event = event.or_else(|| {
            self.suggestions
                .sample
                .clone()
                .filter(|sample| suggestions::key(sample) == response.key)
        });
        let Some(event) = event else {
            if read {
                self.navigate(Route::Agenda, cx);
            } else {
                self.suggestions.pending = Some(response);
            }
            return;
        };
        self.navigate(Route::Event(Rc::new(event)), cx);
        if response.action == Action::Record {
            // The click is the user starting it.
            self.maintain_note_detail(window, cx);
            self.record_new_note(window, cx);
        }
    }

    /// Posts a Suggestion for an invented Event starting now, to try the notification and its
    /// clicks without a real meeting.
    #[cfg(debug_assertions)]
    pub(super) fn suggest_sample(&mut self, cx: &mut Context<Self>) {
        let now = Local::now();
        let sample = crumble_eventkit::Event {
            id: format!("sample-{}", now.timestamp()),
            series_uid: None,
            occurrence: None,
            calendar_id: "sample".to_owned(),
            title: "Sample meeting".to_owned(),
            when: crumble_eventkit::When::Timed {
                start: now,
                end: now + chrono::TimeDelta::minutes(30),
            },
            location: None,
            description: None,
            join: Some(crumble_eventkit::Join {
                url: "https://meet.example.com/sample".to_owned(),
                service: "Google Meet",
            }),
            participants: Vec::new(),
        };
        let suggestion = Suggestion {
            key: suggestions::key(&sample),
            title: sample.title.clone(),
        };
        self.suggestions.sample = Some(sample);
        self.post_suggestions(vec![suggestion], cx);
    }
}
