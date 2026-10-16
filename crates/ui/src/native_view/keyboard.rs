//! Keyboard reachability for custom clickable elements that are not a GPUI Kit `Button`.

use gpui_kit::{Div, SharedString, Stateful, StatefulInteractiveElement};

/// Tab reaches `element`; Enter and Space then run its click handler, as they do on a Button.
pub(crate) fn focusable(element: Stateful<Div>, label: impl Into<SharedString>) -> Stateful<Div> {
    element.tab_index(0).aria_label(label)
}
