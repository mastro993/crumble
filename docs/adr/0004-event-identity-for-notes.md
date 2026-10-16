# Event identity for Notes

A Note belongs to one occurrence of an Event and must find it again after the Event moves, after EventKit re-syncs, and on another Mac once Notes sync. Crumble links a Note by two fields of `crumble_eventkit::Event`: `series_uid`, the iCalendar UID every occurrence shares on every device (EventKit's `calendarItemExternalIdentifier`), and `occurrence`, the original start of a repeating Event's occurrence (EventKit's `occurrenceDate`), which stays the same when that one occurrence is moved. The Note also saves a snapshot of the Event's title and start, used to re-match it when the key finds nothing, as for an Event that had not synced yet when the Note was made.

## Rules

- `series_uid` is the external identifier with a trailing `/RID=<n>` removed. Exchange and some CalDAV servers append it to a detached occurrence, which would otherwise split that occurrence from its series. An empty or missing identifier, as on an Event not yet synced to its server, is `None`.
- `occurrence` is set only when the Event repeats or is a detached occurrence. Otherwise the UID alone names the Event. When EventKit gives no `occurrenceDate`, the start stands in.
- An all-day occurrence is its local date, the same date `When::AllDay` shows, so it survives the Mac changing time zones. A timed one is an instant in UTC.

## Considered Options

- **`eventIdentifier`**: local to this Mac's store, and EventKit may hand out new ones after a full re-sync.
- **`calendarItemIdentifier`**: also local to this Mac's store, and shared by every occurrence of a repeating Event, so it cannot tell occurrences apart.
- **The existing `Event::id`** (`eventIdentifier` plus the start): built on `eventIdentifier`, and it changes whenever the Event moves. It stays as the per-run id that views and the Agenda dedup use.
- **`(series_uid, occurrence)`** (chosen): the UID is what CalDAV, Google, and Exchange share across devices, and the original date names one occurrence however it is moved.

anarlog, an open-source meeting-notes app for macOS, keys Apple Calendar Events the same way: the external identifier with the `/RID=` suffix stripped, plus the original occurrence date for repeating or detached Events, a local date for all-day ones and an instant otherwise.

## Consequences

- A Note whose Event vanished, deleted or out of reach, is kept with its snapshot and never deleted. How the UI shows it is decided with the Notes UI.
- One Event can show in two Calendars, as an invitation seen through two Accounts, with the same UID. A key that matches more than once is ambiguous, and whoever re-matches must pick or ask rather than guess silently.
- Editing "this and following events" in Google Calendar splits the series: the later occurrences get a new UID, the old one plus `_R<date>`. A Note on one of them no longer matches by key and falls back to its snapshot. Crumble does not strip `_R<date>`, since it names a real, separate series; anarlog does not either.
- An Event made on this Mac has no UID until it syncs. A Note made then has only its snapshot until the UID appears.
- `Account::id` and `Calendar::id` are not stable either: they change when an Account is added again and can change after a full re-sync. Stored preferences (each Account's order, color, Alias, Disabled and Hidden choices, and Disabled Calendars) are keyed by them and can be lost then. Rekeying them is a separate follow-up.
