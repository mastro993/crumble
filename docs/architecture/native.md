# Native architecture

Crumble is a GPUI desktop application built with GPUI Kit components. The app shell layout is copied from [SkillBinder](https://github.com/mastro993/SkillBinder).

## Ownership

| Package | Responsibility |
| --- | --- |
| `apps/desktop` | Native lifecycle: window options, menus, key bindings, and quitting after the last window closes |
| `crates/ui` | GPUI root view: shell, rail, Calendars popover, header, navigation history, pages, the Note detail view with its Recording controls, player, and Summary and Transcript tabs, the queues of Transcripts and Summaries being made, the alert asking to send a Recording's audio to a remote Model, Settings › Models and Providers, the Suggestions posted each minute and what a click on one opens, and embedded icon assets. Changes stored state only through `Store` and `Notes` methods and never persists itself. Draws the Events `crates/sync` reads where `crates/calendar` places them, and holds only presentation state: focus, scroll, hover, motion, the minute redraws, the running `Recorder` and open `Player`, the running transcription and summary jobs, which Providers have a key in the Keychain, and which Events were suggested in this run |
| `crates/calendar` | Calendar logic without GPUI or IO: Calendar views, Periods and today; where Events fall on each day and how overlapping ones share columns; the Agenda list's rows with Quick notes among the Events, which days it reads next, how a read is split and filtered; which Events to suggest recording and the key a Suggestion finds its Event by; links in Event descriptions |
| `crates/sync` | GPUI entities that read EventKit off the main thread and keep the result current: `CalendarSync` (access, Accounts, Calendars), `CalendarEvents` (the Calendar page's Events, and today's for Suggestions), and `AgendaEvents` (the Agenda's Events and the days they cover). Depends on GPUI core only, not on GPUI Kit |
| `crates/db` | The SQLite file `crumble.sqlite3`, the data folder it sits in, every migration, and the Diesel schema. Opens connections and runs pending migrations; holds no queries |
| `crates/store` | State kept across restarts, in the `crates/db` file with each change written as it happens, and Account rules without GPUI: Account order, colors and their tones, Aliases and labels, which Calendars show given Disabled and Hidden Accounts, which state the Calendars section shows for the calendar access status, the Summary and Transcription Model choices, whether Suggestions are on, the remote Models the user agreed to send audio to ([ADR 0008](../adr/0008-remote-transcription-and-audio-consent.md)), and API keys in the Keychain through `secrets` ([ADR 0007](../adr/0007-summaries-and-model-providers.md)) |
| `crates/notes` | Notes and Quick notes in the `crates/db` file: creating, editing and soft-deleting them, and finding a Note's Event occurrence again; each Recording's start, length and folder, deleting that folder, and finishing Recordings a crash left running; each Note's Transcript, replaced whole, with the Provider and Model that made it, and which finished Recordings have none; each Note's Summary, cleared with its Transcript, and which Transcripts have none |
| `crates/audio` | Recordings without GPUI: `Recorder` captures the microphone and a process tap of the Mac's sound into two AAC files, removing the Mac's sound from the microphone, `Player` plays them together, `recorded_length` reads what the files hold, and `mic_access` asks for the microphone ([ADR 0005](../adr/0005-recording-with-a-process-tap.md)) |
| `crates/transcribe` | Transcripts without GPUI: the `Transcriber` trait and its two implementations, `Apple` (SpeechAnalyzer on the Mac) and `OpenAiCompatible` (each track uploaded in pieces under 20 MB, repackaged into M4A), each returning segments in time order, labelled by track; `model_status` and `install_model` manage the locale's speech model, and `supported_locales` lists SpeechAnalyzer's. Swift behind a C ABI ([ADR 0006](../adr/0006-on-device-transcription-via-swift-bridge.md), [ADR 0008](../adr/0008-remote-transcription-and-audio-consent.md)) |
| `crates/summarize` | Summaries without GPUI: the `Summarizer` trait and its three implementations, `Apple` (Foundation Models on the Mac, in parts that fit its context), `Claude` and `OpenAiCompatible` over HTTP; the prompt; the text a Model reads. Swift behind a C ABI ([ADR 0007](../adr/0007-summaries-and-model-providers.md)) |
| `crates/remote` | What Summaries and Transcripts share about Models: the `Provider` enum, `runs_locally` (Apple, or a loopback server), `base_url` (an address requests can go to, or why not), a URL's `host` read with the HTTP client's own `Uri` parser, the URL without credentials (`without_userinfo`), POSTs with `ureq`, and HTTP failures as sentences a user can act on ([ADR 0008](../adr/0008-remote-transcription-and-audio-consent.md)) |
| `crates/notify` | Suggestions without GPUI: `UNUserNotificationCenter` through objc2, posting a notification with a "Record" button, asking for and reading notification permission, and handing clicks to one handler ([ADR 0009](../adr/0009-suggestions-through-user-notifications.md)) |
| `crates/theme` | Colors, typography, and dimensions |
| `crates/eventkit` | Read-only bridge to macOS EventKit: calendar access, Accounts, Calendars, Events, and change notifications as plain Rust data |

Code that does not need GPUI lives in domain crates rather than in `crates/ui`, as decided in [ADR 0002](../adr/0002-crates-split-on-gpui-boundary.md).

## Shell

- The macOS window uses a transparent titlebar. The traffic lights sit centred in the 42 pt header row. Other platforms keep native decorations.
- A 52 pt rail runs down the window's leading edge, below the header, on the sidebar color: a 36 pt icon button each for Home, Agenda, Calendar, and Chat, a hairline, the Calendars button (see Events), and Settings pinned to the bottom. Each button names its page in a tooltip to its right, and the current page's button keeps the fill a button shows under the pointer and shows its icon solid; an Event's page keeps Calendar's. Clicking a button opens its page and records the previous one for Back. The app starts on Home. Home and Chat show only their title for now. Without EventKit the hairline and the Calendars button are left out.
- The page sits in a panel beside the rail, on the page color, with 12 pt rounded corners (`SURFACE_RADIUS`), a hairline edge, and 4 pt of space to the window's trailing and bottom edges.
- The back and forward arrows sit pinned beside the traffic lights. `Cmd-[` and `Cmd-]` (`Ctrl-[` and `Ctrl-]` elsewhere) do the same. They move through an in-memory route history, and each arrow is disabled when there is nothing to go back or forward to.
- A page can have a sidebar of its own at the panel's leading edge: the Agenda page, with the Agenda list, and the Settings page, with its sections, which always stays open (see Settings). On the Agenda page a sidebar toggle follows the arrows, and `Cmd-B` (`Ctrl-B` elsewhere) also toggles it; on other pages neither does anything. Opening and closing slide over 220 ms, and the slide is skipped when the system asks for reduced motion. The sidebar's visible leading edge stays rounded to the panel's corners while it slides, since the panel does not clip its children to its corners.
- You can resize either sidebar between 240 and 480 pt by dragging its trailing edge, or with the arrow, Home, and End keys on the focused handle; while the edge is held or dragged, a 3 pt line in the focus ring color at half strength highlights it. It starts at 320 pt, and both sidebars share one width, so resizing one resizes the other. Width and collapsed state are not persisted.
- The theme follows the window's light or dark appearance. Both modes use one warm palette, in `crumble_theme`: cream surfaces (`#faf5ef`, `#f5ece0`, `#f0e2d0`) and brown ink (`#1f150a`, `#3e2b13`). Light mode draws the page on `#faf5ef`, the sidebar on `#f5ece0`, and fills and borders in `#f0e2d0`, with `#1f150a` text and `#3e2b13` buttons. Dark mode uses browns a step darker: the page on `#0f0a05`, the sidebar on `#1f150a`, fills and borders in `#2e200e`, with `#faf5ef` text and `#f0e2d0` buttons. Secondary text and icons are the second ink, `#3e2b13` or `#f0e2d0`, at 75%; focus rings at 50%, text selection at 20%. gpui-kit derives the other colors from these.
- The header and the page share 24 pt horizontal padding (`CONTENT_PADDING`), so header controls line up with page content. A page's toolbar starts clear of the arrows and, on the Agenda page, the sidebar toggle.
- While a Recording runs, the header's trailing end shows a pill, a red dot and "Recording · 12:03", on every page; clicking it opens the Note being recorded (see Recording).
- Double-clicking empty header space zooms the window, and dragging it moves the window. Presses on header buttons and tabs stay with the control, so they never zoom or drag the window.

## Behavior

Each area has its own page:

- [Calendar and Agenda](calendar.md): the Calendar views and the Agenda.
- [Events](events.md): Events, their context menus, and Cards.
- [Settings](settings.md): the Settings page.
- [Notes](notes.md): Notes and Quick notes.
- [Recording and Suggestions](recording.md): Recordings and the Suggestions that offer them.
- [Transcripts and Summaries](transcripts-and-summaries.md): Transcripts and Summaries.

## Agent automation

- `cargo run -p crumble --features mcp` installs a [gpui-mcp](https://github.com/themixednuts/gpui-mcp) bridge on the main window under the app ID `crumble`. The `gpui-mcp` server in `.mcp.json` finds it, and agents can then read the semantic tree, click, type, and take window screenshots through GPUI's own event pipeline instead of the system mouse. Builds without the feature carry no bridge.
- The bridge needs GPUI automation hooks, so the workspace patches `gpui-pre` with gpui-mcp's copy for every build. The bridge and the patch are pinned to the same gpui-mcp revision as the installed server; update all three together.
- Only elements with an `.id(...)` appear in the tree. In GPUI Kit 0.7.0 a Select's options are drawn but not published, so pick them with the keyboard.
