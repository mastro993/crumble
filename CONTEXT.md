# Crumble

Crumble is a personal desktop app. This glossary defines the words used for its parts.

## Language

### Calendar

**Calendar view**:
How the Calendar page lays out time: Week, Month, or Year. Always written with the "Calendar" prefix. A bare "view" means a GPUI view.
_Avoid_: scale, mode, layout

**Period**:
The range of dates a Calendar view shows: one week, one month, or one year. Previous and Next step one Period.
_Avoid_: range, page

**Focused date**:
The date that decides which Period is shown. Previous and Next move it by one Period, Today resets it, and switching Calendar view keeps it.
_Avoid_: selected date, anchor, cursor

**Today**:
The current local date.

**Weekend**:
Saturday and Sunday. Weekends are visually distinct in every Calendar view.

**First day of week**:
The weekday that starts each week row and column in the Calendar. It is a user setting that can be any weekday, defaults to Monday, and is kept across restarts.
_Avoid_: week start

### Sync

**Calendar page**:
The page that lays out time in a Calendar view. Always written "Calendar page" when it could be confused with a Calendar.

**Account**:
One group of Calendars that macOS keeps together, such as a Google, Outlook, or iCloud login, On My Mac, or Birthdays. macOS owns Accounts; Crumble only reads them.
_Avoid_: connection, integration, login

**Calendar**:
One list of Events inside an Account, such as "Work" or "Holidays". Never means the Calendar page.
_Avoid_: feed, source

**Event**:
One entry synced from a Calendar, whether or not it has other attendees. Crumble only reads Events and never changes them.
_Avoid_: meeting, appointment

**Disabled**:
Said of an Account or a Calendar that Crumble leaves out entirely. It is chosen in Settings, kept across restarts, and changes nothing in macOS.
_Avoid_: muted, removed, off

**Event page**:
The page that shows everything about one Event and its Note. Clicking an Event in the Calendar page, or selecting it in the Agenda page, opens it.
_Avoid_: event detail view, event screen

**Details card**:
The short summary of an Event shown while the pointer rests on it.
_Avoid_: tooltip, popover, preview

**Participant**:
Someone invited to an Event, including its organizer, with their answer to the invitation.
_Avoid_: attendee, guest, invitee

**Hidden**:
Said of an Account whose Events Crumble leaves out of its pages while it stays listed in the Calendars section. It is chosen by clicking the Account's row there, which unchecks its checkbox, is kept across restarts, and changes nothing in macOS.
_Avoid_: muted, unchecked, toggled off

**Alias**:
A name the user gives an Account in Crumble. The sidebar shows only the Alias; Settings shows the Alias followed by the Account's own name in brackets. It is kept across restarts and changes nothing in macOS.
_Avoid_: nickname, display name, rename

**Calendars section**:
The sidebar section that lists every Account that is not Disabled. Its title says "Calendars" even though each row is an Account.

**Settings page**:
The page where the user picks the First day of week, Disables or Enables Accounts and Calendars, gives Aliases, and picks the Model that makes Summaries with what its Provider needs. Written "Settings" when nothing else could be meant.
_Avoid_: preferences, options

### Agenda

**Agenda page**:
The page that lists every Event that has started and every Quick note, newest first, grouped by day.
_Avoid_: history, timeline, feed

**Agenda list**:
The scrolling column of the Agenda page that holds its Events, between the sidebar and the page's content. Scrolling down reaches further into the past.
_Avoid_: secondary sidebar, feed

**In progress**:
Said of a timed Event that has started and not yet ended. An all-day Event is never In progress.
_Avoid_: current, live, ongoing

### Notes

**Note**:
What the user keeps about one conversation: an optional title, at most one Recording with its Transcript and Summary, and a Memo. A Note is attached to one occurrence of an Event, or stands alone as a Quick note.
_Avoid_: session, meeting note, document

**Quick note**:
A Note not attached to any Event. The Agenda page lists it at the time its Recording started, or else when it was made.
_Avoid_: standalone note, ad-hoc note

**Memo**:
What the user types into a Note.
_Avoid_: notes, manual notes, text

**Recording**:
The audio of a Note, taken from the microphone and from the Mac's own sound together. It is kept on the Mac until the user deletes it, and is sent away only to a remote Model the user picked to make its Transcript, after Crumble warns that it will be.
_Avoid_: registration, capture, audio file

**Transcript**:
The text of what was said in a Recording. It can be made again from the same Recording.
_Avoid_: transcription, captions

**Summary**:
A short account of a Transcript. Each Note has at most one.
_Avoid_: recap, digest, minutes

**Model**:
What turns a Recording into a Transcript, or a Transcript into a Summary. The user picks one for each job.
_Avoid_: engine, backend, AI

**Provider**:
Where a Model runs: on the Mac, such as Apple's on-device Models, or remotely, such as Anthropic or any OpenAI-compatible server.
_Avoid_: vendor, service, backend

## Relationships

- A **Calendar view** shows exactly one **Period**, the one that contains the **Focused date**
- **First day of week** decides where a week **Period** begins and ends
- **Weekend** stays Saturday and Sunday whatever the **First day of week** is
- An **Account** has one or more **Calendars**; a **Calendar** has many **Events**
- The **Calendar page** shows **Events** from every **Calendar** that is not **Disabled** and whose **Account** is neither **Disabled** nor **Hidden**, inside the visible **Period**
- The **Agenda page** shows the same **Events** as the **Calendar page** would, but only those that have started, whatever the **Focused date**
- Disabling an **Account** leaves out all its **Calendars** but keeps each **Calendar**'s own setting
- **Disabled** lasts until the user changes it in Settings; **Hidden** lasts until the user changes it in the **Calendars section**
- Only an **Account** can be **Hidden**; a **Calendar** can only be **Disabled**
- An **Event** is drawn in its **Account**'s color, whatever its **Calendar**'s color is in macOS
- One occurrence of an **Event** has at most one **Note**, which every **Calendar** showing that occurrence shares; a **Note** belongs to at most one occurrence, never to a whole repeating series
- A **Note** has at most one **Recording**; its **Transcript** comes from that **Recording** and its **Summary** from that **Transcript** only, never from the **Memo**
- A **Note** follows its **Event**'s **Disabled** and **Hidden** state; a **Quick note** is always shown

## Example dialogue

> **Dev:** "If the user is in Month view on March 2027 and switches to Week, which **Period** do they see?"
> **Domain expert:** "The week that contains the **Focused date**, March 1. **Today** only matters if they press Today."

## Flagged ambiguities

- "view" meant both a GPUI view and Week/Month/Year. Resolved: the latter is always a **Calendar view**.
- "calendar" meant both the page and a synced list of Events. Resolved: the page is the **Calendar page** and a synced list is a **Calendar**.
- "meeting" meant only Events with attendees. Resolved: every synced entry is an **Event**.
- "hide" meant both the Settings choice and the sidebar's checkbox. Resolved: the Settings one, which also drops the sidebar row, is **Disabled**; the sidebar one is **Hidden**.
- "current event" meant both an **Event** that has started and one happening today. Resolved: one that has started and not ended is **In progress**.
- "settings" meant both the **Settings page** and everything Crumble keeps across restarts, such as the last **Calendar view** or the **Account** order. Resolved: "Settings" is only the page; the rest is not a domain term.
- The **Calendars section** lists **Accounts**, not **Calendars**. Resolved: keep the "Calendars" title for users; in code and conversation its rows are **Accounts**.
