# Native architecture

Crumble is a GPUI desktop application built with GPUI Kit components. The app shell layout is copied from [SkillBinder](https://github.com/mastro993/SkillBinder).

## Ownership

| Package | Responsibility |
| --- | --- |
| `apps/desktop` | Native lifecycle: window options, menus, key bindings, and quitting after the last window closes |
| `crates/ui` | GPUI root view: shell, sidebar, header, navigation history, and embedded icon assets |
| `crates/theme` | Colors, typography, and dimensions |

## Shell

- The macOS window uses a transparent titlebar. The traffic lights sit centred in the 50 pt header row. Other platforms keep native decorations.
- The sidebar toggle stays pinned beside the traffic lights whether the sidebar is open or closed. `Cmd-B` (`Ctrl-B` elsewhere) also toggles it. Opening and closing slide over 220 ms, and the slide is skipped when the system asks for reduced motion.
- You can resize the sidebar between 192 and 400 pt by dragging its trailing edge, or with the arrow, Home, and End keys on the focused handle. It starts at 232 pt. Width and collapsed state are not persisted.
- The back and forward arrows sit in the sidebar's header row. `Cmd-[` and `Cmd-]` (`Ctrl-[` and `Ctrl-]` elsewhere) do the same. They move through an in-memory route history, and each arrow is disabled when there is nothing to go back or forward to.
- The sidebar lists Home, Calendar, and Chat at the top and Settings pinned to the bottom. Clicking a row, or pressing Enter or Space while it has focus, opens its route and records the previous one for Back. The app starts on Home. Each route shows only its title for now.
- The theme follows the window's light or dark appearance.
