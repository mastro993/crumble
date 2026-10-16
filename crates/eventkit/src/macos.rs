//! The EventKit bridge. Every objc2-event-kit call is `unsafe`; each block says why it holds.
#![allow(unsafe_code)]

use std::cell::OnceCell;
use std::panic::AssertUnwindSafe;
use std::ptr::NonNull;
use std::sync::{Mutex, OnceLock};

use crate::classify::{self, SourceKind};
use crate::{Access, Account, Calendar, Event, Participant, Response, When};
use block2::RcBlock;
use chrono::{DateTime, Local, TimeDelta};
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2_core_graphics::{CGColor, CGColorRenderingIntent, CGColorSpace, kCGColorSpaceSRGB};
use objc2_event_kit::{
    EKAuthorizationStatus, EKCalendar, EKEntityType, EKEvent, EKEventStatus, EKEventStore,
    EKEventStoreChangedNotification, EKParticipant, EKParticipantStatus, EKSource, EKSourceType,
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
                let kind = source_kind(&source);
                let calendar_names: Vec<String> =
                    unsafe { source.calendarsForEntityType(EKEntityType::Event) }
                        .iter()
                        .map(|calendar| unsafe { calendar.title() }.to_string())
                        .collect();
                // Read at most once, and only for the services that sync Events with people.
                let events = OnceCell::new();
                let recent = || {
                    events.get_or_init(|| match kind {
                        SourceKind::CalDav | SourceKind::Exchange => recent_events(&source),
                        SourceKind::Other => Vec::new(),
                    })
                };
                Account {
                    id: unsafe { source.sourceIdentifier() }.to_string(),
                    email: classify::account_email(
                        own_address(recent()),
                        calendar_names.iter().map(String::as_str),
                    ),
                    provider: classify::provider(kind, &name, || {
                        recent().iter().any(|event| {
                            unsafe { event.calendarItemExternalIdentifier() }
                                .is_some_and(|id| classify::is_google_event_id(&id.to_string()))
                        })
                    }),
                    name,
                }
            })
            .collect();
        accounts.sort_by_key(|account| account.name.to_lowercase());
        accounts
    })
}

/// How far around today Events are read to learn about an Account.
const PROBE_SPAN: TimeDelta = TimeDelta::days(90);

/// The Events within [`PROBE_SPAN`] of today in `source`.
// ponytail: an Account with no Events in that span keeps its title and stays Other; widen the
// span if that bites.
fn recent_events(source: &EKSource) -> Vec<Retained<EKEvent>> {
    let now = Local::now();
    // SAFETY: getters on live EventKit objects, and a predicate built by the same store.
    unsafe {
        let calendars = source
            .calendarsForEntityType(EKEntityType::Event)
            .allObjects();
        // An empty list would make the predicate search every Calendar.
        if calendars.is_empty() {
            return Vec::new();
        }
        let predicate = store().predicateForEventsWithStartDate_endDate_calendars(
            &ns_date(now - PROBE_SPAN),
            &ns_date(now + PROBE_SPAN),
            Some(&calendars),
        );
        store().eventsMatchingPredicate(&predicate).to_vec()
    }
}

/// The user's own address among the organizers and attendees of `events`.
fn own_address(events: &[Retained<EKEvent>]) -> Option<String> {
    // SAFETY: getters on live EventKit objects.
    unsafe {
        events.iter().find_map(|event| {
            let attendees = event
                .attendees()
                .map(|list| list.to_vec())
                .unwrap_or_default();
            event
                .organizer()
                .into_iter()
                .chain(attendees)
                .filter(|participant| participant.isCurrentUser())
                .find_map(|participant| {
                    let url = participant.URL().absoluteString()?.to_string();
                    classify::mailto_address(&url)
                })
        })
    }
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
        let join = classify::join_link([url.as_deref(), location.as_deref(), notes.as_deref()]);
        Some(Event {
            // Occurrences of a repeating Event share an identifier, so the start makes it unique;
            // `Event::item_id` reads the identifier back.
            id: Event::occurrence_id(&item_id, start),
            calendar_id: event.calendar()?.calendarIdentifier().to_string(),
            title: event.title().to_string(),
            when,
            location,
            description: notes.as_deref().and_then(classify::description),
            join,
            participants: participants(event),
        })
    }
}

/// The organizer first, then the other attendees. EventKit often lists the organizer among the
/// attendees too, with their answer, so that entry is moved first rather than repeated.
fn participants(event: &EKEvent) -> Vec<Participant> {
    // SAFETY: getters on live EventKit objects.
    unsafe {
        let Some(attendees) = event.attendees().filter(|list| !list.is_empty()) else {
            return Vec::new();
        };
        let url = |participant: &EKParticipant| {
            participant
                .URL()
                .absoluteString()
                .map(|url| url.to_string())
        };
        let organizer = event.organizer();
        let organizer_url = organizer.as_deref().and_then(url);
        let entry = |participant: &EKParticipant, organizer: bool| Participant {
            name: participant
                .name()
                .map(|name| name.to_string())
                .filter(|name| !name.trim().is_empty())
                .or_else(|| url(participant).and_then(|url| classify::mailto_address(&url)))
                .or_else(|| url(participant))
                .unwrap_or_default(),
            response: match participant.participantStatus() {
                EKParticipantStatus::Accepted => Response::Accepted,
                EKParticipantStatus::Tentative => Response::Tentative,
                EKParticipantStatus::Declined => Response::Declined,
                EKParticipantStatus::Delegated => Response::Delegated,
                _ => Response::Pending,
            },
            organizer,
            you: participant.isCurrentUser(),
        };
        let mut list: Vec<Participant> = attendees
            .iter()
            .map(|attendee| {
                let organizer = organizer_url.is_some() && url(&attendee) == organizer_url;
                entry(&attendee, organizer)
            })
            .collect();
        match list.iter().position(|participant| participant.organizer) {
            Some(ix) => list[..=ix].rotate_right(1),
            None => {
                if let Some(organizer) = organizer {
                    // The organizer goes, whatever EventKit reports for them.
                    list.insert(
                        0,
                        Participant {
                            response: Response::Accepted,
                            ..entry(&organizer, true)
                        },
                    );
                }
            }
        }
        list
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
