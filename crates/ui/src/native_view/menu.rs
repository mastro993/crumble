//! The app's context menu: rows with an icon, a label and an optional description, and a
//! trailing check or submenu chevron, opened at the pointer or under a select above a backdrop
//! that swallows clicks outside it. A select's menu can have a search field that hides the rows
//! whose label lacks its text, and a last row that offers the text itself.
//!
//! GPUI Kit's `PopupMenu` fixes its row height, separators, and corners and cannot animate in,
//! so Crumble draws its own. One `Menu` entity holds a menu, its open submenu, and the panel one
//! of the submenu's rows can open beside it, with a single focus handle for the keyboard.

mod input;
mod render;
mod search;

pub(super) use render::{opening, panel, scrolling};
pub(super) use search::Place;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::POPUP_PRIORITY;
use gpui_kit::component::input::InputState;
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, DismissEvent, Entity, EventEmitter, FocusHandle,
    IntoElement, Pixels, ScrollHandle, SharedString, Subscription, Task, Window, deferred,
};

/// How long a submenu stays open once the pointer moves onto another row, so a diagonal move
/// toward one of its lower rows can cross the rows below its parent.
pub(super) const SUBMENU_CLOSE_DELAY: Duration = Duration::from_millis(150);

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;
type TypedHandler = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
type Content = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// One row of a menu, or a separator.
pub(super) struct MenuItem {
    id: SharedString,
    label: SharedString,
    /// A muted line under the label.
    description: Option<SharedString>,
    leading: Option<Leading>,
    checked: Option<bool>,
    disabled: bool,
    /// Drawn in the danger color, as an action that can't be undone.
    destructive: bool,
    /// Left out by the search field's text.
    hidden: bool,
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
    /// Offers `text`, the search field's, trimmed, as `Use “text”` while no row's label is it.
    Typed {
        text: SharedString,
        handler: TypedHandler,
    },
}

impl MenuItem {
    /// A row labelled `label`. Its element is `menu-item-{id}`, so `id` must be unique in the
    /// window.
    pub(super) fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            leading: None,
            checked: None,
            disabled: false,
            destructive: false,
            hidden: false,
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

    /// Adds a muted line under the label.
    pub(super) fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Makes the row a checkbox, showing a check at its trailing edge while `checked`.
    pub(super) fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Draws the row's label and icon in the danger color, for an action that can't be undone.
    pub(super) fn destructive(mut self) -> Self {
        self.destructive = true;
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

    /// The last row of a searchable menu, which offers the search field's text itself, trimmed,
    /// labelled `Use “text”`, while no other row's label is that text, ignoring case. Choosing
    /// it runs `handler` with the text after the menu has closed. Its element is
    /// `menu-item-{id}`.
    pub(super) fn typed(
        id: impl Into<SharedString>,
        handler: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            hidden: true,
            kind: Kind::Typed {
                text: SharedString::default(),
                handler: Rc::new(handler),
            },
            ..Self::new(id, "")
        }
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
        !self.disabled && !self.hidden && !matches!(self.kind, Kind::Separator)
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
    place: Place,
    focus: FocusHandle,
    /// Where focus returns when the menu closes.
    restore: FocusHandle,
    /// The focus of the view that opened the menu, where focus returns when `restore` is gone.
    fallback: FocusHandle,
    /// The highlighted row.
    highlight: Option<usize>,
    /// Scrolls the rows once a select's menu reaches its maximum height.
    scroll: ScrollHandle,
    /// The field that filters a select's rows.
    search: Option<Entity<InputState>>,
    /// The width a searchable menu was last drawn at, which it keeps as its rows are filtered.
    width: Rc<Cell<Option<Pixels>>>,
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
    _subscriptions: Vec<Subscription>,
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
        place: Place,
        fallback: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        // Tab moves focus elsewhere, a shortcut acts on the window under the menu, and a menu
        // does not outlive its app going to the background.
        let subscriptions = vec![
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
                if event.action.is_some() && !menu.inner_focused(window, cx) {
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
            place,
            focus,
            restore: window.focused(cx).unwrap_or_else(|| fallback.clone()),
            fallback,
            highlight: None,
            scroll: ScrollHandle::new(),
            search: None,
            width: Rc::default(),
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
}

/// An open context menu, held by the view that draws it, which drops it when the menu closes.
pub(super) struct OpenMenu {
    /// The element the menu was opened from.
    owner: SharedString,
    menu: Entity<Menu>,
    _dismiss: Subscription,
}

impl OpenMenu {
    /// Opens `items` at `place` for the element `owner` of a view `V`, whose own focus is
    /// `fallback`. `V` must store the result in the field `slot` returns and draw it with
    /// [`OpenMenu::render`]. Call it from a mouse-down or click listener.
    pub(super) fn new<V: 'static>(
        owner: impl Into<SharedString>,
        items: Vec<MenuItem>,
        place: impl Into<Place>,
        fallback: FocusHandle,
        window: &mut Window,
        cx: &mut Context<V>,
        slot: fn(&mut V) -> &mut Option<OpenMenu>,
    ) -> Self {
        let place = place.into();
        let menu = cx.new(|cx| Menu::new(items, place, fallback, window, cx));
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
