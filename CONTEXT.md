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

## Relationships

- A **Calendar view** shows exactly one **Period**, the one that contains the **Focused date**
- **First day of week** decides where a week **Period** begins and ends
- **Weekend** stays Saturday and Sunday whatever the **First day of week** is

## Example dialogue

> **Dev:** "If the user is in Month view on March 2027 and switches to Week, which **Period** do they see?"
> **Domain expert:** "The week that contains the **Focused date**, March 1. **Today** only matters if they press Today."

## Flagged ambiguities

- "view" meant both a GPUI view and Week/Month/Year. Resolved: the latter is always a **Calendar view**.
