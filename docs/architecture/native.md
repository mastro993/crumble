# Native architecture

Crumble is a GPUI desktop application built with GPUI Kit components. The app shell layout is copied from [SkillBinder](https://github.com/mastro993/SkillBinder).

## Ownership

| Package | Responsibility |
| --- | --- |
| `apps/desktop` | Native lifecycle: window options, menus, key bindings, and quitting after the last window closes |
| `crates/ui` | GPUI root view: shell, sidebar, header, navigation history, pages, user settings, and embedded icon assets |
| `crates/theme` | Colors, typography, and dimensions |

## Shell

- The macOS window uses a transparent titlebar. The traffic lights sit centred in the 50 pt header row. Other platforms keep native decorations.
- The sidebar toggle stays pinned beside the traffic lights whether the sidebar is open or closed. `Cmd-B` (`Ctrl-B` elsewhere) also toggles it. Opening and closing slide over 220 ms, and the slide is skipped when the system asks for reduced motion.
- You can resize the sidebar between 192 and 400 pt by dragging its trailing edge, or with the arrow, Home, and End keys on the focused handle. It starts at 232 pt. Width and collapsed state are not persisted.
- The back and forward arrows sit in the sidebar's header row. `Cmd-[` and `Cmd-]` (`Ctrl-[` and `Ctrl-]` elsewhere) do the same. They move through an in-memory route history, and each arrow is disabled when there is nothing to go back or forward to.
- The sidebar lists Home, Calendar, and Chat at the top and Settings pinned to the bottom. Clicking a row, or pressing Enter or Space while it has focus, opens its route and records the previous one for Back. The app starts on Home. Home and Chat show only their title for now.
- The theme follows the window's light or dark appearance.
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

## Settings

- Settings are saved as `settings.json` in the platform config directory (`~/Library/Application Support/Crumble` on macOS). A missing, unreadable, or partial file falls back to the defaults. If a save fails, the error is logged and the app keeps the new value in memory.
- **First day of week** can be any weekday and defaults to Monday. It sets the column order of every Calendar view and where each week Period begins.
