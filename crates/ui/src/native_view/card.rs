//! Cards: content floating over the window, opened by resting the pointer on an element or by
//! clicking a button. Every card is drawn as the context menu is, with the same surface, edge,
//! shadow, and opening motion, and roomier padding.
//!
//! GPUI Kit's `HoverCard` cannot take the menu's look, wraps its trigger in an element that cannot
//! be sized, and slides over its trigger when there is no room below it. A hover card opens beside
//! its trigger and flips to the other side when the window has no room, as a submenu does.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use super::menu::{SUBMENU_CLOSE_DELAY, opening, panel, scrolling};
use crumble_theme::menu::WINDOW_MARGIN;
use gpui_kit::base::{Align, POPUP_PRIORITY, Placement, Positioner, TestSupportExt};
use gpui_kit::component::button::Button;
use gpui_kit::component::popover::Popover;
use gpui_kit::{
    Anchor, AnyElement, App, Bounds, Context, ElementId, InteractiveElement, IntoElement,
    ParentElement, Pixels, ScrollHandle, StatefulInteractiveElement, Styled, Task, Window, canvas,
    deferred, div, phi, px,
};

/// How long the pointer rests on the trigger before the card opens.
pub(super) const OPEN_DELAY: Duration = Duration::from_millis(500);
/// Space between a hover card's trigger and the card.
const HOVER_GAP: f32 = 4.0;
/// Space between a click card's button and the card, before the 4 pt GPUI Kit's popover adds.
const CLICK_GAP: f32 = 12.0;
/// Tallest a click card grows before it scrolls.
const CLICK_MAX_HEIGHT: f32 = 420.0;
/// Space between a card's edge and its content, roomier than a menu's, which lines up rows.
const PADDING: f32 = 16.0;

/// The surface every card shares, `id` in tests.
fn surface(
    id: &'static str,
    cx: &App,
) -> impl StatefulInteractiveElement + ParentElement + Styled + IntoElement + use<> {
    // A card is drawn inside its trigger, so it would inherit a tight line height such as a Week
    // view block's; the window's own is restored.
    panel(div().id(id).test_support(), cx)
        .line_height(phi())
        .p(px(PADDING))
}

/// Whether the card is open, and where the pointer is.
#[derive(Default)]
struct HoverState {
    open: bool,
    over_trigger: bool,
    over_card: bool,
    /// Where the trigger was last drawn; the card opens beside it.
    trigger: Rc<Cell<Bounds<Pixels>>>,
    /// Opens or closes the card once the pointer has stayed for the delay.
    pending: Option<Task<()>>,
}

impl HoverState {
    /// The pointer entered or left the trigger, or the card when `card`. The card opens after
    /// [`OPEN_DELAY`] and closes after the menu's submenu delay, so the pointer can cross the gap
    /// between the trigger and the card.
    fn hover(&mut self, card: bool, hovered: bool, cx: &mut Context<Self>) {
        if card {
            self.over_card = hovered;
        } else {
            self.over_trigger = hovered;
        }
        let open = self.over_trigger || self.over_card;
        if open == self.open {
            self.pending = None;
            return;
        }
        let delay = if open {
            OPEN_DELAY
        } else {
            SUBMENU_CLOSE_DELAY
        };
        self.pending = Some(cx.spawn(async move |state, cx| {
            cx.background_executor().timer(delay).await;
            let _ = state.update(cx, |state, cx| {
                state.open = open;
                state.pending = None;
                cx.notify();
            });
        }));
    }
}

/// `trigger`, whose element id is `id`, showing `content` in a card beside it while the pointer
/// rests on it. The card is `hover-card` in tests.
pub(super) fn hover_card<E: StatefulInteractiveElement + ParentElement>(
    id: impl Into<ElementId>,
    trigger: E,
    content: impl FnOnce(&mut Window, &mut App) -> AnyElement,
    window: &mut Window,
    cx: &mut App,
) -> E {
    let state = window.with_element_namespace("hover-card", |window| {
        window.use_keyed_state(id, cx, |_, _| HoverState::default())
    });
    let (open, bounds) = {
        let state = state.read(cx);
        (state.open, state.trigger.clone())
    };
    let card = open.then(|| {
        let hover = window.listener_for(&state, |state, hovered: &bool, _, cx| {
            state.hover(true, *hovered, cx)
        });
        let surface = surface("hover-card", cx)
            .on_hover(hover)
            .child(content(window, cx));
        deferred(
            Positioner::side(bounds.get())
                .placement(Placement::Right)
                .align(Align::Start)
                .offset(px(HOVER_GAP))
                .margin(px(WINDOW_MARGIN))
                .occlude()
                .child(opening(surface, "hover-card-open", window, cx)),
        )
        .with_priority(POPUP_PRIORITY)
    });
    trigger
        .on_hover(window.listener_for(&state, |state, hovered: &bool, _, cx| {
            state.hover(false, *hovered, cx)
        }))
        // Pinned to every edge: an unpinned absolute child starts inside the trigger's padding.
        .child(
            canvas(move |trigger, _, _| bounds.set(trigger), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
        .children(card)
}

/// `trigger` opening `content` in a card below it, right-aligned with it, which closes when
/// clicked away from or with Escape, and scrolls when it is taller than [`CLICK_MAX_HEIGHT`]. The
/// card is `click-card` in tests.
pub(super) fn click_card(
    id: impl Into<ElementId>,
    trigger: Button,
    content: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> Popover {
    Popover::new(id)
        .anchor(Anchor::TopRight)
        .offset(px(CLICK_GAP))
        .appearance(false)
        .trigger(trigger)
        .content(move |_, window, cx| {
            let scroll = window
                .use_keyed_state("click-card-scroll", cx, |_, _| ScrollHandle::new())
                .read(cx)
                .clone();
            let surface = surface("click-card", cx)
                .max_h(px(CLICK_MAX_HEIGHT))
                .child(scrolling("click-card-rows", &scroll, [content(window, cx)]));
            opening(surface, "click-card-open", window, cx)
        })
}
