//! Keyboard reachability for custom clickable elements that are not a GPUI Kit `Button`.

use gpui_kit::{InteractiveElement, SharedString, StatefulInteractiveElement};

/// Tab reaches `element`; Enter and Space then run its click handler, as they do on a Button.
pub(crate) fn focusable<E: InteractiveElement + StatefulInteractiveElement>(
    element: E,
    label: impl Into<SharedString>,
) -> E {
    element.tab_index(0).aria_label(label)
}
