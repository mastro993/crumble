//! The EventKit bridge. Every objc2-event-kit call is `unsafe`; each block says why it holds.
#![allow(unsafe_code)]

use std::panic::AssertUnwindSafe;
use std::ptr::NonNull;
use std::sync::{Mutex, OnceLock};

use crate::classify::{self, SourceKind};
use crate::{Access, Account, Calendar, Event, When};
use block2::RcBlock;
use chrono::{DateTime, Local, TimeDelta};
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2_core_graphics::{CGColor, CGColorRenderingIntent, CGColorSpace, kCGColorSpaceSRGB};
use objc2_event_kit::{
    EKAuthorizationStatus, EKCalendar, EKEntityType, EKEvent, EKEventStatus, EKEventStore,
    EKEventStoreChangedNotification, EKParticipantStatus, EKSource, EKSourceType,
};
use objc2_foundation::{
    NSArray, NSDate, NSError, NSNotification, NSNotificationCenter, NSObjectProtocol,
};

/// Shown for a Calendar whose color EventKit cannot express in sRGB.
const FALLBACK_COLOR: u32 = 0x8E_8E_93;

struct SharedStore(Retained<EKEventStore>);

// SAFETY: EKEventStore may be used from any thread, and Apple recommends fetching Events off the
// main thread. Crumble only ever reads through this one instance.
unsafe impl Send for SharedStore {}
// SAFETY: as above; reads do not need exclusive access.
unsafe impl Sync for SharedStore {}

/// Apple advises one long-lived store; it also posts the change notifications.
fn store() -> &'static EKEventStore {
    static STORE: OnceLock<SharedStore> = OnceLock::new();
    // SAFETY: the plain initializer takes no arguments.
    &STORE
        .get_or_init(|| SharedStore(unsafe { EKEventStore::new() }))
        .0
}

/// Runs `read`, turning an Objective-C exception (EventKit raises them when its XPC connection
/// drops) or a nil from a getter declared non-null (objc2 panics on those) into an error.
fn catch<R>(read: impl FnOnce() -> R) -> anyhow::Result<R> {
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        objc2::exception::catch(AssertUnwindSafe(read))
    }))
    .map_err(|_| anyhow::anyhow!("EventKit returned nil where it promised a value"))?
    .map_err(|exception| anyhow::anyhow!("EventKit raised an exception: {exception:?}"))
}

/// Whether Crumble may read the user's Calendars.
pub fn access() -> Access {
    // SAFETY: a class method given a valid entity type.
    let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };
    match status {
        EKAuthorizationStatus::NotDetermined => Access::NotDetermined,
        EKAuthorizationStatus::FullAccess => Access::Granted,
        _ => Access::Denied,
    }
}

/// Shows the macOS permission prompt, or answers at once if the user already chose.
///
/// `done` receives whether access was granted, on an arbitrary thread.
pub fn request_access(done: impl FnOnce(bool) + Send + 'static) {
    let done = Mutex::new(Some(done));
    let block = RcBlock::new(move |granted: Bool, _: *mut NSError| {
        if let Some(done) = done.lock().ok().and_then(|mut done| done.take()) {
            done(granted.as_bool());
        }
    });
    // SAFETY: the block is valid for this call, and EventKit copies it before returning.
    unsafe { store().requestFullAccessToEventsWithCompletion(RcBlock::as_ptr(&block)) };
}

/// Every Account with at least one event Calendar, sorted by name.
pub fn accounts() -> anyhow::Result<Vec<Account>> {
    catch(|| {
        // SAFETY: getters on live EventKit objects.
        let sources = unsafe { store().sources() };
        let mut accounts: Vec<Account> = sources
            .iter()
            .filter(|source| {
                unsafe { source.calendarsForEntityType(EKEntityType::Event) }.count() > 0
            })
            .map(|source| {
                let name = unsafe { source.title() }.to_string();
                Account {
                    id: unsafe { source.sourceIdentifier() }.to_string(),
                    provider: classify::provider(source_kind(&source), &name),
                    name,
                }
            })
            .collect();
        accounts.sort_by_key(|account| account.name.to_lowercase());
        accounts
    })
}

fn source_kind(source: &EKSource) -> SourceKind {
    // SAFETY: getter on a live EventKit object.
    match unsafe { source.sourceType() } {
        EKSourceType::Exchange => SourceKind::Exchange,
        EKSourceType::CalDAV => SourceKind::CalDav,
        _ => SourceKind::Other,
    }
}

/// Every event Calendar, sorted by name.
pub fn calendars() -> anyhow::Result<Vec<Calendar>> {
    catch(|| {
        // SAFETY: getters on live EventKit objects.
        let calendars = unsafe { store().calendarsForEntityType(EKEntityType::Event) };
        let mut calendars: Vec<Calendar> = calendars
            .iter()
            .filter_map(|calendar| {
                let account = unsafe { calendar.source() }?;
                Some(Calendar {
                    id: unsafe { calendar.calendarIdentifier() }.to_string(),
                    account_id: unsafe { account.sourceIdentifier() }.to_string(),
                    name: unsafe { calendar.title() }.to_string(),
                    color: color(&calendar),
                })
            })
            .collect();
        calendars.sort_by_key(|calendar| calendar.name.to_lowercase());
        calendars
    })
}

/// The Calendar's color converted to sRGB, since EventKit may report it in Display P3.
fn color(calendar: &EKCalendar) -> u32 {
    // SAFETY: getter on a live EventKit object; `kCGColorSpaceSRGB` is a constant CoreGraphics
    // string; the conversion takes no options dictionary.
    let srgb = unsafe {
        calendar.CGColor().and_then(|color| {
            let space = CGColorSpace::with_name(Some(kCGColorSpaceSRGB));
            CGColor::new_copy_by_matching_to_color_space(
                space.as_deref(),
                CGColorRenderingIntent::RenderingIntentDefault,
                Some(&color),
                None,
            )
        })
    };
    let Some(srgb) = srgb else {
        return FALLBACK_COLOR;
    };
    let count = CGColor::number_of_components(Some(&srgb));
    let components = CGColor::components(Some(&srgb));
    if count < 3 || components.is_null() {
        return FALLBACK_COLOR;
    }
    // SAFETY: CoreGraphics returned `count` components at a non-null pointer that lives as long
    // as `srgb`.
    let components = unsafe { std::slice::from_raw_parts(components, count) };
    let byte = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    byte(components[0]) << 16 | byte(components[1]) << 8 | byte(components[2])
}

/// Each occurrence in `calendar_ids` that overlaps `start..end`, except declined and cancelled
/// ones. EventKit silently cuts spans longer than four years.
pub fn events(
    start: DateTime<Local>,
    end: DateTime<Local>,
    calendar_ids: &[String],
) -> anyhow::Result<Vec<Event>> {
    catch(|| {
        // SAFETY: getters and a predicate built by the same store, as EventKit requires.
        let store = store();
        let chosen: Retained<NSArray<EKCalendar>> =
            unsafe { store.calendarsForEntityType(EKEntityType::Event) }
                .into_iter()
                .filter(|calendar| {
                    let id = unsafe { calendar.calendarIdentifier() }.to_string();
                    calendar_ids.contains(&id)
                })
                .collect();
        // An empty list would make the predicate search every Calendar.
        if chosen.is_empty() {
            return Vec::new();
        }
        let predicate = unsafe {
            store.predicateForEventsWithStartDate_endDate_calendars(
                &ns_date(start),
                &ns_date(end),
                Some(&chosen),
            )
        };
        unsafe { store.eventsMatchingPredicate(&predicate) }
            .iter()
            .filter(|event| !hidden(event))
            .filter_map(|event| convert(&event))
            .collect()
    })
}

/// Cancelled, or declined by the user.
fn hidden(event: &EKEvent) -> bool {
    // SAFETY: getters on live EventKit objects.
    unsafe {
        event.status() == EKEventStatus::Canceled
            || event.attendees().is_some_and(|attendees| {
                attendees.iter().any(|attendee| {
                    attendee.isCurrentUser()
                        && attendee.participantStatus() == EKParticipantStatus::Declined
                })
            })
    }
}

fn convert(event: &EKEvent) -> Option<Event> {
    // SAFETY: getters on a live EventKit object.
    unsafe {
        let start = local(&event.startDate())?;
        let end = local(&event.endDate())?;
        let when = if event.isAllDay() {
            let first = start.date_naive();
            // The end is the last day's 23:59:59 or the next midnight; one second back
            // lands on the last day either way.
            let last = (end - TimeDelta::seconds(1)).date_naive().max(first);
            When::AllDay { first, last }
        } else {
            When::Timed { start, end }
        };
        let item_id = event
            .eventIdentifier()
            .unwrap_or_else(|| event.calendarItemIdentifier())
            .to_string();
        let url = event
            .URL()
            .and_then(|url| url.absoluteString())
            .map(|url| url.to_string());
        let location = event
            .location()
            .map(|location| location.to_string())
            .filter(|location| !location.trim().is_empty());
        let notes = event.notes().map(|notes| notes.to_string());
        let join_url = classify::join_link([url.as_deref(), location.as_deref(), notes.as_deref()]);
        Some(Event {
            // Occurrences of a repeating Event share an identifier, so the start makes it unique.
            id: format!("{item_id}@{}", start.timestamp()),
            calendar_id: event.calendar()?.calendarIdentifier().to_string(),
            title: event.title().to_string(),
            when,
            location,
            join_url,
        })
    }
}

fn ns_date(time: DateTime<Local>) -> Retained<NSDate> {
    NSDate::dateWithTimeIntervalSince1970(time.timestamp() as f64)
}

fn local(date: &NSDate) -> Option<DateTime<Local>> {
    let millis = (date.timeIntervalSince1970() * 1000.0).round() as i64;
    DateTime::from_timestamp_millis(millis).map(|time| time.with_timezone(&Local))
}

/// Keeps a change subscription alive; dropping it stops the callbacks.
pub struct ChangeObserver(Retained<ProtocolObject<dyn NSObjectProtocol>>);

impl Drop for ChangeObserver {
    fn drop(&mut self) {
        // SAFETY: the token came from `addObserverForName` on the same center.
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(self.0.as_ref()) };
    }
}

/// Calls `on_change` whenever the EventKit database changes, including access changes, on
/// whichever thread posts the notification. Everything read earlier should be read again.
pub fn observe_changes(on_change: impl Fn() + Send + Sync + 'static) -> ChangeObserver {
    let block = RcBlock::new(move |_: NonNull<NSNotification>| on_change());
    // SAFETY: the notification name is EventKit's own constant, the object is the shared store,
    // and the block is sendable because `on_change` is `Send + Sync`. The center copies the block.
    let token = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(EKEventStoreChangedNotification),
            Some(store()),
            None,
            &block,
        )
    };
    ChangeObserver(token)
}
