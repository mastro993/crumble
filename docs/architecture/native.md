# Native architecture

Crumble is a GPUI desktop application built with GPUI Kit components. The app shell layout is copied from [SkillBinder](https://github.com/mastro993/SkillBinder).

## Ownership

| Package | Responsibility |
| --- | --- |
| `apps/desktop` | Native lifecycle: window options, menus, key bindings, and quitting after the last window closes |
| `crates/ui` | GPUI root view: shell, sidebar, header, navigation history, pages, user settings, and embedded icon assets |
| `crates/theme` | Colors, typography, and dimensions |
| `crates/eventkit` | Read-only bridge to macOS EventKit: calendar access, Accounts, Calendars, Events, and change notifications as plain Rust data |

## Shell

- The macOS window uses a transparent titlebar. The traffic lights sit centred in the 50 pt header row. Other platforms keep native decorations.
- The sidebar toggle stays pinned beside the traffic lights whether the sidebar is open or closed. `Cmd-B` (`Ctrl-B` elsewhere) also toggles it. Opening and closing slide over 220 ms, and the slide is skipped when the system asks for reduced motion.
- You can resize the sidebar between 192 and 400 pt by dragging its trailing edge, or with the arrow, Home, and End keys on the focused handle. It starts at 232 pt. Width and collapsed state are not persisted.
- The back and forward arrows sit in the sidebar's header row. `Cmd-[` and `Cmd-]` (`Ctrl-[` and `Ctrl-]` elsewhere) do the same. They move through an in-memory route history, and each arrow is disabled when there is nothing to go back or forward to.
- The sidebar lists Home, Calendar, and Chat at the top, the Calendars section below them (see Events), and Settings pinned to the bottom. Clicking a row, or pressing Enter or Space while it has focus, opens its route and records the previous one for Back. The app starts on Home. Home and Chat show only their title for now.
- The theme follows the window's light or dark appearance.
- The header and the page share 24 pt horizontal padding (`CONTENT_PADDING`), so header controls line up with page content. With the sidebar collapsed, the header's leading edge moves right to clear the sidebar toggle.
- Double-clicking empty header space zooms the window, and dragging it moves the window. Presses on header buttons and tabs stay with the control, so they never zoom or drag the window.

## Calendar

Terms such as Calendar view, Period, and Focused date are defined in [CONTEXT.md](../../CONTEXT.md).

- The Calendar page shows one Period in a Week, Month, or Year Calendar view. It opens on Month view showing today. The Calendar view and Focused date stay in memory while the app runs and are not persisted.
- On the Calendar route the header holds Today, Previous, and Next buttons, the Period title, and a segmented Week | Month | Year switcher. The title truncates when the header is too narrow.
- `Cmd-T` jumps to today and `Cmd-←` / `Cmd-→` step one Period (`Ctrl` elsewhere). These shortcuts are handled only while the Calendar page is open.
- Week view is an hourly grid from 00:00 to 24:00 that scrolls on its own. Month view stretches 4–6 week rows to fill the page and mutes days from neighbouring months. Year view shows twelve months as rounded cards in a 4×3 grid. Each card always has six week rows, so the cards in a grid row end level, and the grid stays at most 1200 pt wide. The current month's name is bold. Clicking a month name opens it in Month view.
- Week and Month grids sit in a bordered frame with 12 pt rounded corners (`SURFACE_RADIUS`). Shaded cells in the corners follow the curve, including while the Week hours scroll.
- Today is circled in the accent color. Weekends (Saturday and Sunday) are shaded in every Calendar view. In Year view, adjacent weekend columns share one rounded strip.
- Today is read from the local clock on every render, so a window left open past midnight updates on its next redraw.

## Events

Crumble reads Events from macOS EventKit and never changes them; see [ADR 0001](../adr/0001-read-only-events-via-eventkit.md). Account, Calendar, and Event are defined in [CONTEXT.md](../../CONTEXT.md).

- `crates/eventkit` is the only crate that calls Objective-C. Every EventKit call is `unsafe`, so it sets its own lints with `unsafe_code = "deny"` and local allows, while the other crates keep the workspace's `forbid`. On other platforms it reports `Access::Unsupported` and returns nothing.
- Calls go through one shared event store. EventKit exceptions, raised when its connection to the calendar daemon drops, and nils from getters declared non-null become errors instead of crashing the app.
- Accounts are the macOS sources that hold at least one event Calendar, sorted by name. Their provider is guessed: Exchange is Outlook, a CalDAV source titled "iCloud" is Apple, and a CalDAV title containing "gmail" or "google" is Google. Any other CalDAV source is Google when one of its Events within 90 days of today has an external identifier ending in `@google.com`, which catches Google Workspace accounts on their own domain. Anything else is Other.
- Calendar colors are converted to sRGB. Events are returned one per occurrence, with declined and cancelled ones left out. All-day Events are whole dates; timed Events are in the system time zone. A join link is the first `meet.google.com`, Teams, Zoom, or Webex link found in the Event's URL, then its location, then its notes.
- `CalendarSync`, owned by the root view, holds the access state, the Accounts, and the Calendars. At launch it reads them only if access is already granted, so Crumble never prompts on its own. It reads again whenever EventKit reports a change, off the main thread. A failed read is logged and the last data stays on screen.
- The sidebar's Calendars section sits below Chat. Its rows are Accounts, each with a provider icon (Google, Microsoft for Outlook, Apple, or a calendar for Other). Clicking a row does nothing; hovering it, or tabbing to it, shows a ⋯ button whose menu Disables or Enables the Account, and a Disabled Account's row is dimmed. Before access is asked the section shows one "Connect calendars" row that opens the macOS prompt. After a refusal it shows "Allow calendar access", which opens Privacy & Security › Calendars. With access and no Accounts it shows a hint that opens Internet Accounts. Accounts are added and removed in System Settings, so the section has no add or delete action. On platforms without EventKit the section is hidden.
- macOS grants calendar access only to an app bundle whose `Info.plist` has `NSCalendarsFullAccessUsageDescription`. `apps/desktop/Info.plist` holds it, and the macOS Cargo runner (`scripts/macos-runner.sh`) wraps `crumble` in `Crumble.app`, signs it, and launches it with `open`. Ctrl-C quits the app too. Test binaries run directly.

## Settings

- Settings are saved as `settings.json` in the platform config directory (`~/Library/Application Support/Crumble` on macOS). A missing, unreadable, or partial file falls back to the defaults. If a save fails, the error is logged and the app keeps the new value in memory.
- **First day of week** can be any weekday and defaults to Monday. It sets the column order of every Calendar view and where each week Period begins.
- **Calendars** lists every Account with its Calendars, each with a checkbox, its color, and its name. Every Calendar starts enabled; unchecking one Disables it. A Disabled Account greys its Calendars out with an "Account disabled" note and keeps their checkboxes as they were. The group is hidden while access is not granted. The Disabled Accounts and Calendars are saved as sets of EventKit identifiers (`disabled_accounts`, `disabled_calendars`) and change nothing in macOS.
