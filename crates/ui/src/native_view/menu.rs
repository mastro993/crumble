//! The app's context menu: rows with an icon, a label, and a trailing check or submenu chevron,
//! opened at the pointer above a backdrop that swallows clicks outside it.
//!
//! GPUI Kit's `PopupMenu` fixes its row height, separators, and corners and cannot animate in,
//! so Crumble draws its own. One `Menu` entity holds a menu, its open submenu, and the panel one
//! of the submenu's rows can open beside it, with a single focus handle for the keyboard.

mod render;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::POPUP_PRIORITY;
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, DismissEvent, Entity, EventEmitter, FocusHandle,
    IntoElement, KeyDownEvent, Pixels, Point, SharedString, Subscription, Task, Window, deferred,
};

/// How long a submenu stays open once the pointer moves onto another row, so a diagonal move
/// toward one of its lower rows can cross the rows below its parent.
pub(super) const SUBMENU_CLOSE_DELAY: Duration = Duration::from_millis(150);

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;
type Content = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// One row of a menu, or a separator.
pub(super) struct MenuItem {
    id: SharedString,
    label: SharedString,
    leading: Option<Leading>,
    checked: Option<bool>,
    disabled: bool,
    kind: Kind,
}

/// What sits in a row's icon slot.
enum Leading {
    /// An app icon, drawn in the muted color.
    Icon(&'static str),
    Element(Rc<dyn Fn(&App) -> AnyElement>),
}

enum Kind {
    Separator,
    Action(Option<Handler>),
    /// ponytail: one level deep; rows of a submenu that are submenus themselves do nothing.
    Submenu(Vec<MenuItem>),
    /// Any content, such as a color picker, in a panel beside the row: the menu's third level.
    /// Only a submenu's rows open one.
    Panel {
        content: Content,
        focus: FocusHandle,
    },
}

impl MenuItem {
    /// A row labelled `label`. Its element is `menu-item-{id}`, so `id` must be unique in the
    /// window.
    pub(super) fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            leading: None,
            checked: None,
            disabled: false,
            kind: Kind::Action(None),
        }
    }

    pub(super) fn separator() -> Self {
        Self {
            kind: Kind::Separator,
            ..Self::new("", "")
        }
    }

    /// Leads the row with the app icon `name`.
    pub(super) fn icon(mut self, name: &'static str) -> Self {
        self.leading = Some(Leading::Icon(name));
        self
    }

    /// Leads the row with any element that fits the icon slot, such as a color dot.
    pub(super) fn leading(mut self, element: impl Fn(&App) -> AnyElement + 'static) -> Self {
        self.leading = Some(Leading::Element(Rc::new(element)));
        self
    }

    /// Makes the row a checkbox, showing a check at its trailing edge while `checked`.
    pub(super) fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub(super) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Runs `handler` when the row is chosen, after the menu has closed.
    pub(super) fn on_select(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.kind = Kind::Action(Some(Rc::new(handler)));
        self
    }

    /// Opens `items` beside the row.
    pub(super) fn submenu(mut self, items: Vec<MenuItem>) -> Self {
        self.kind = Kind::Submenu(items);
        self
    }

    /// Opens `content` in a panel beside the row, as a third level; only a submenu's row can.
    /// Opening it from the keyboard focuses `focus`, which should be inside `content`.
    pub(super) fn panel(
        mut self,
        focus: FocusHandle,
        content: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.kind = Kind::Panel {
            content: Rc::new(content),
            focus,
        };
        self
    }

    /// Whether the pointer and the keyboard can highlight the row.
    fn selectable(&self) -> bool {
        !self.disabled && !matches!(self.kind, Kind::Separator)
    }

    fn submenu_items(&self) -> Option<&[MenuItem]> {
        match &self.kind {
            Kind::Submenu(items) => Some(items),
            _ => None,
        }
    }

    fn opens_panel(&self) -> bool {
        matches!(self.kind, Kind::Panel { .. })
    }
}

/// The next selectable row after `from`, wrapping around; with nothing highlighted, the first
/// row going down and the last going up.
fn step(items: &[MenuItem], from: Option<usize>, down: bool) -> Option<usize> {
    let len = items.len();
    let start = from.unwrap_or(if down { len.checked_sub(1)? } else { 0 });
    (1..=len)
        .map(|offset| {
            if down {
                (start + offset) % len
            } else {
                (start + len - offset) % len
            }
        })
        .find(|&ix| items[ix].selectable())
}

/// A menu and its open submenu.
pub(super) struct Menu {
    items: Vec<MenuItem>,
    /// Where the menu's top-left corner was asked to be, in window coordinates.
    position: Point<Pixels>,
    focus: FocusHandle,
    /// Where focus returns when the menu closes.
    restore: FocusHandle,
    /// The focus of the view that opened the menu, where focus returns when `restore` is gone.
    fallback: FocusHandle,
    /// The highlighted row.
    highlight: Option<usize>,
    submenu: Option<Submenu>,
    /// Where each row was last drawn; a submenu opens beside its row.
    row_bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    /// Closes the submenu once the pointer has stayed off its row for `SUBMENU_CLOSE_DELAY`.
    closing_submenu: Option<Task<()>>,
    /// Where the row whose panel is open was last drawn; the panel opens beside it.
    panel_row: Rc<Cell<Bounds<Pixels>>>,
    /// Closes the panel once the pointer has stayed off its row for `SUBMENU_CLOSE_DELAY`.
    closing_panel: Option<Task<()>>,
    closed: bool,
    _subscriptions: [Subscription; 3],
}

struct Submenu {
    /// The row it opened from.
    parent: usize,
    /// The highlighted submenu row; while one is, the arrow keys move through the submenu.
    highlight: Option<usize>,
    /// The submenu row whose panel is open.
    panel: Option<usize>,
}

impl EventEmitter<DismissEvent> for Menu {}

impl Menu {
    fn new(
        items: Vec<MenuItem>,
        position: Point<Pixels>,
        fallback: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        // Tab moves focus elsewhere, a shortcut acts on the window under the menu, and a menu
        // does not outlive its app going to the background.
        let subscriptions = [
            cx.on_focus_out(&focus, window, |menu, _, window, cx| {
                // Focus left with a panel that closed while it held focus; the menu takes it back.
                if menu.fallback.contains_focused(window, cx) {
                    menu.close(window, cx);
                } else {
                    window.focus(&menu.focus, cx);
                }
            }),
            // Keys typed into a panel, such as a hex field's, run actions of their own.
            cx.observe_keystrokes(|menu, event, window, cx| {
                if event.action.is_some() && !menu.panel_focused(window, cx) {
                    menu.close(window, cx);
                }
            }),
            cx.observe_window_activation(window, |menu, window, cx| {
                if !window.is_window_active() {
                    menu.close(window, cx);
                }
            }),
        ];
        Self {
            row_bounds: Rc::new(RefCell::new(vec![Bounds::default(); items.len()])),
            items,
            position,
            focus,
            restore: window.focused(cx).unwrap_or_else(|| fallback.clone()),
            fallback,
            highlight: None,
            submenu: None,
            closing_submenu: None,
            panel_row: Rc::new(Cell::new(Bounds::default())),
            closing_panel: None,
            closed: false,
            _subscriptions: subscriptions,
        }
    }

    /// Closes the menu, handing focus back if it still has it.
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.closing_submenu = None;
        self.closing_panel = None;
        if self.focus.contains_focused(window, cx) {
            // What had focus may be gone, such as a Rename field that ended when the menu took
            // focus from it.
            let restore = Some(&self.restore)
                .filter(|restore| self.fallback.contains(restore, window))
                .unwrap_or(&self.fallback);
            window.focus(restore, cx);
        }
        cx.emit(DismissEvent);
    }

    /// Chooses row `ix`, or row `sub` of its submenu: an action closes the menu and then runs,
    /// so it can move focus, and a submenu opens.
    fn choose(
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
    fn panel_focused(&self, window: &Window, cx: &App) -> bool {
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
    fn hover_row(&mut self, ix: usize, hovered: bool, cx: &mut Context<Self>) {
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
    fn hover_submenu(&mut self, sub: Option<usize>, hovered: bool, cx: &mut Context<Self>) {
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
    fn hover_panel(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.closing_submenu = None;
            self.closing_panel = None;
            cx.notify();
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
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

/// An open context menu, held by the view that draws it, which drops it when the menu closes.
pub(super) struct OpenMenu {
    /// The element the menu was opened from.
    owner: SharedString,
    menu: Entity<Menu>,
    _dismiss: Subscription,
}

impl OpenMenu {
    /// Opens `items` at `position` for the element `owner` of a view `V`, whose own focus is
    /// `fallback`. `V` must store the result in the field `slot` returns and draw it with
    /// [`OpenMenu::render`]. Call it from a mouse-down listener.
    pub(super) fn new<V: 'static>(
        owner: impl Into<SharedString>,
        items: Vec<MenuItem>,
        position: Point<Pixels>,
        fallback: FocusHandle,
        window: &mut Window,
        cx: &mut Context<V>,
        slot: fn(&mut V) -> &mut Option<OpenMenu>,
    ) -> Self {
        let menu = cx.new(|cx| Menu::new(items, position, fallback, window, cx));
        let dismiss = cx.subscribe(&menu, move |view, menu, _: &DismissEvent, cx| {
            let open = slot(view);
            if open.as_ref().is_some_and(|open| open.menu == menu) {
                *open = None;
                cx.notify();
            }
        });
        // Keeps the focusable elements under the pointer from taking focus back as the press
        // bubbles up.
        window.prevent_default();
        let focus = menu.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.notify();
        Self {
            owner: owner.into(),
            menu,
            _dismiss: dismiss,
        }
    }

    /// Closes the menu and hands focus back, as choosing a row does.
    pub(super) fn close(&self, window: &mut Window, cx: &mut App) {
        self.menu.update(cx, |menu, cx| menu.close(window, cx));
    }

    /// Whether the menu was opened from the element `owner`.
    pub(super) fn is_for(&self, owner: &str) -> bool {
        self.owner == owner
    }

    /// The menu in a layer above the window's content, wherever in the tree it is drawn.
    pub(super) fn render(&self) -> impl IntoElement + use<> {
        deferred(self.menu.clone()).with_priority(POPUP_PRIORITY)
    }
}

#[cfg(test)]
mod tests {
    use super::{MenuItem, step};

    #[test]
    fn the_arrow_keys_skip_separators_and_disabled_rows_and_wrap() {
        let items = [
            MenuItem::new("hide", "Hide"),
            MenuItem::separator(),
            MenuItem::new("up", "Move Up").disabled(true),
            MenuItem::new("down", "Move Down"),
        ];
        assert_eq!(step(&items, None, true), Some(0));
        assert_eq!(step(&items, None, false), Some(3));
        assert_eq!(step(&items, Some(0), true), Some(3));
        assert_eq!(step(&items, Some(3), true), Some(0));
        assert_eq!(step(&items, Some(0), false), Some(3));
        assert_eq!(step(&[], None, true), None);
        assert_eq!(step(&[MenuItem::separator()], None, true), None);
    }
}
