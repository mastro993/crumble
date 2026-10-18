//! How the pointer and the keyboard move through a menu: hovering rows, the arrow keys, and
//! choosing a row.

use super::{Kind, Menu, MenuItem, SUBMENU_CLOSE_DELAY, Submenu, step};
use gpui_kit::{App, Context, KeyDownEvent, Window};

impl Menu {
    /// Chooses row `ix`, or row `sub` of its submenu: an action closes the menu and then runs,
    /// so it can move focus, and a submenu opens.
    pub(super) fn choose(
        &mut self,
        ix: usize,
        sub: Option<usize>,
        from_keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let item = match sub {
            None => self.items.get(ix),
            Some(sub) => self.items[ix]
                .submenu_items()
                .and_then(|items| items.get(sub)),
        };
        let Some(item) = item.filter(|item| item.selectable()) else {
            return;
        };
        match &item.kind {
            Kind::Action(handler) => {
                let handler = handler.clone();
                self.close(window, cx);
                if let Some(handler) = handler {
                    handler(window, cx);
                }
            }
            Kind::Submenu(_) if sub.is_none() => self.open_submenu(ix, from_keyboard, cx),
            Kind::Panel { focus, .. } => {
                let focus = focus.clone();
                if let Some(sub) = sub {
                    self.open_panel(sub, cx);
                    if from_keyboard {
                        window.focus(&focus, cx);
                    }
                }
            }
            _ => {}
        }
    }

    /// Opens row `ix`'s submenu and highlights its first row when the keyboard opened it.
    fn open_submenu(&mut self, ix: usize, from_keyboard: bool, cx: &mut Context<Self>) {
        self.closing_submenu = None;
        self.highlight = Some(ix);
        if self
            .submenu
            .as_ref()
            .is_none_or(|submenu| submenu.parent != ix)
        {
            self.submenu = Some(Submenu {
                parent: ix,
                highlight: None,
                panel: None,
            });
        }
        if from_keyboard
            && let (Some(submenu), Some(items)) =
                (self.submenu.as_mut(), self.items[ix].submenu_items())
        {
            submenu.highlight = step(items, None, true);
        }
        cx.notify();
    }

    /// Opens the panel of submenu row `sub`.
    fn open_panel(&mut self, sub: usize, cx: &mut Context<Self>) {
        self.closing_submenu = None;
        self.closing_panel = None;
        if let Some(submenu) = self.submenu.as_mut() {
            submenu.highlight = Some(sub);
            submenu.panel = Some(sub);
        }
        cx.notify();
    }

    fn close_panel_later(&mut self, cx: &mut Context<Self>) {
        let open = self
            .submenu
            .as_ref()
            .is_some_and(|submenu| submenu.panel.is_some());
        if !open || self.closing_panel.is_some() {
            return;
        }
        self.closing_panel = Some(cx.spawn(async move |menu, cx| {
            cx.background_executor().timer(SUBMENU_CLOSE_DELAY).await;
            let _ = menu.update(cx, |menu, cx| {
                if let Some(submenu) = menu.submenu.as_mut() {
                    submenu.panel = None;
                }
                menu.closing_panel = None;
                cx.notify();
            });
        }));
    }

    /// Whether focus is in the open panel rather than on the menu itself.
    pub(super) fn panel_focused(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx) && !self.focus.is_focused(window)
    }

    fn close_submenu_later(&mut self, cx: &mut Context<Self>) {
        if self.submenu.is_none() || self.closing_submenu.is_some() {
            return;
        }
        self.closing_submenu = Some(cx.spawn(async move |menu, cx| {
            cx.background_executor().timer(SUBMENU_CLOSE_DELAY).await;
            let _ = menu.update(cx, |menu, cx| {
                menu.submenu = None;
                menu.closing_submenu = None;
                cx.notify();
            });
        }));
    }

    /// The pointer entered or left row `ix`; a disabled row only lets the submenu close.
    pub(super) fn hover_row(&mut self, ix: usize, hovered: bool, cx: &mut Context<Self>) {
        if hovered && !self.items[ix].selectable() {
            self.highlight = None;
            self.close_submenu_later(cx);
        } else if hovered {
            self.highlight = Some(ix);
            if self.items[ix].submenu_items().is_some() {
                self.open_submenu(ix, false, cx);
            } else {
                self.close_submenu_later(cx);
            }
        } else if self.highlight == Some(ix)
            && self
                .submenu
                .as_ref()
                .is_none_or(|submenu| submenu.parent != ix)
        {
            self.highlight = None;
        }
        cx.notify();
    }

    /// The pointer entered the open submenu, or entered or left its row `sub`. A row with a
    /// panel opens it, and the others let an open panel close.
    pub(super) fn hover_submenu(
        &mut self,
        sub: Option<usize>,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(parent) = self.submenu.as_ref().map(|submenu| submenu.parent) else {
            return;
        };
        if hovered {
            self.closing_submenu = None;
            self.highlight = Some(parent);
            if let Some(sub) = sub {
                if self.opens_panel(parent, sub) {
                    self.open_panel(sub, cx);
                } else {
                    self.close_panel_later(cx);
                }
                if let Some(submenu) = self.submenu.as_mut() {
                    submenu.highlight = Some(sub);
                }
            }
        } else if let Some(submenu) = self.submenu.as_mut()
            && sub.is_some()
            && submenu.highlight == sub
            && submenu.panel != sub
        {
            submenu.highlight = None;
        }
        cx.notify();
    }

    /// Whether row `sub` of row `parent`'s submenu opens a panel.
    fn opens_panel(&self, parent: usize, sub: usize) -> bool {
        self.items[parent]
            .submenu_items()
            .and_then(|items| items.get(sub))
            .is_some_and(MenuItem::opens_panel)
    }

    /// The pointer entered the open panel, which keeps it and the submenu open.
    pub(super) fn hover_panel(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.closing_submenu = None;
            self.closing_panel = None;
            cx.notify();
        }
    }

    pub(super) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.modified() {
            return;
        }
        let in_submenu = self
            .submenu
            .as_ref()
            .and_then(|submenu| submenu.highlight.map(|sub| (submenu.parent, sub)));
        let panel_open = self
            .submenu
            .as_ref()
            .is_some_and(|submenu| submenu.panel.is_some());
        match (event.keystroke.key.as_str(), in_submenu) {
            // From the menu or from inside the panel, which lets only Escape through.
            ("left" | "escape", _) if panel_open => {
                if let Some(submenu) = self.submenu.as_mut() {
                    submenu.panel = None;
                }
                self.closing_panel = None;
                window.focus(&self.focus, cx);
            }
            ("right", Some((parent, sub))) => {
                if self.opens_panel(parent, sub) {
                    self.choose(parent, Some(sub), true, window, cx);
                }
            }
            (key @ ("up" | "down"), Some((parent, sub))) => {
                if let (Some(submenu), Some(items)) =
                    (self.submenu.as_mut(), self.items[parent].submenu_items())
                {
                    submenu.highlight = step(items, Some(sub), key == "down");
                    submenu.panel = None;
                }
            }
            (key @ ("up" | "down"), None) => {
                self.highlight = step(&self.items, self.highlight, key == "down");
                self.submenu = None;
                self.closing_submenu = None;
            }
            ("right", None) => {
                if let Some(ix) = self.highlight
                    && self.items[ix].submenu_items().is_some()
                {
                    self.open_submenu(ix, true, cx);
                }
            }
            ("left", Some(_)) => self.submenu = None,
            ("enter" | "space", Some((parent, sub))) => {
                self.choose(parent, Some(sub), true, window, cx)
            }
            ("enter" | "space", None) => {
                if let Some(ix) = self.highlight {
                    self.choose(ix, None, true, window, cx);
                }
            }
            ("escape", _) if self.submenu.is_some() => self.submenu = None,
            ("escape", _) => self.close(window, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}
