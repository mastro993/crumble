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
Said of an Account or a Calendar whose Events Crumble hides. It is Crumble's own choice, kept across restarts, and changes nothing in macOS.
_Avoid_: hidden, muted, removed

**Calendars section**:
The sidebar section that lists every Account. Its title says "Calendars" even though each row is an Account.

## Relationships

- A **Calendar view** shows exactly one **Period**, the one that contains the **Focused date**
- **First day of week** decides where a week **Period** begins and ends
- **Weekend** stays Saturday and Sunday whatever the **First day of week** is
- An **Account** has one or more **Calendars**; a **Calendar** has many **Events**
- The **Calendar page** shows **Events** from every **Calendar** that is not **Disabled** and whose **Account** is not **Disabled**, inside the visible **Period**
- Disabling an **Account** hides all its **Calendars**' **Events** but keeps each **Calendar**'s own setting

## Example dialogue

> **Dev:** "If the user is in Month view on March 2027 and switches to Week, which **Period** do they see?"
> **Domain expert:** "The week that contains the **Focused date**, March 1. **Today** only matters if they press Today."

## Flagged ambiguities

- "view" meant both a GPUI view and Week/Month/Year. Resolved: the latter is always a **Calendar view**.
- "calendar" meant both the page and a synced list of Events. Resolved: the page is the **Calendar page** and a synced list is a **Calendar**.
- "meeting" meant only Events with attendees. Resolved: every synced entry is an **Event**.
- The **Calendars section** lists **Accounts**, not **Calendars**. Resolved: keep the "Calendars" title for users; in code and conversation its rows are **Accounts**.
