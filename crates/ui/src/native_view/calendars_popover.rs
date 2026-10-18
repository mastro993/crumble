//! The Calendars popover: the Calendars section beside the rail's Calendars button. It opens
//! while the pointer rests on the button or the popover, or with a click, and stays open while a
//! row's menu, Rename field, or drag is active.

use std::cell::Cell;
use std::rc::Rc;

use super::NativeView;
use super::menu::{SUBMENU_CLOSE_DELAY, opening, panel};
use super::rail::rail_button;
use crumble_theme::menu::{PADDING, WINDOW_MARGIN};
use crumble_theme::sidebar::{DETAIL_SIZE, ROW_PADDING, SECTION_HEIGHT};
use gpui_kit::base::{Align, POPUP_PRIORITY, Placement, Positioner};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    App, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Pixels,
    StatefulInteractiveElement, Styled, Task, Window, canvas, deferred, div, px,
};

/// Width of the popover; its rows are this less the menu padding on each side.
pub(super) const WIDTH: f32 = 240.0;
/// Space between the rail button and the popover.
const GAP: f32 = 8.0;

/// Whether the popover is open, and why.
#[derive(Default)]
pub(super) struct CalendarsPopover {
    open: bool,
    /// Opened by a click or a key, so it stays open after the pointer leaves.
    pinned: bool,
    /// Closed with Escape or a click while the pointer is over it; it stays closed until the
    /// pointer leaves.
    dismissed: bool,
    /// Where the button and the popover were last drawn.
    button: Rc<Cell<Bounds<Pixels>>>,
    popover: Rc<Cell<Bounds<Pixels>>>,
    /// Opens or closes the popover once the pointer has stayed for the delay.
    pending: Option<Task<()>>,
}

impl CalendarsPopover {
    fn pointer_over(&self, window: &Window) -> bool {
        let pointer = window.mouse_position();
        self.button.get().contains(&pointer) || (self.open && self.popover.get().contains(&pointer))
    }
}

impl NativeView {
    /// Whether the popover should be open now.
    fn calendars_wanted(&mut self, window: &Window, cx: &App) -> bool {
        let over = self.calendars_popover.pointer_over(window);
        // Only an Account row's drag holds it; resizing the page's sidebar is a drag too.
        let dragging = self.account_reorder.is_dragging() && cx.has_active_drag();
        let held = self.menu.is_some() || self.renaming.is_some() || dragging;
        let popover = &mut self.calendars_popover;
        if !over {
            popover.dismissed = false;
        }
        // A hold only keeps an open popover open: a menu opened elsewhere, such as a Note's
        // actions, must not open it.
        !popover.dismissed && (popover.pinned || over || (held && popover.open))
    }

    /// Opens or closes the popover after the delay when the pointer or a hold changed what it
    /// should be. Runs on each hover change and each frame, which notices a hold ending.
    pub(super) fn settle_calendars(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self.calendars_wanted(window, cx);
        let popover = &mut self.calendars_popover;
        if wanted == popover.open {
            popover.pending = None;
            return;
        }
        if popover.pending.is_some() {
            return;
        }
        popover.pending = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor().timer(SUBMENU_CLOSE_DELAY).await;
            let _ = view.update_in(cx, |view, window, cx| {
                let wanted = view.calendars_wanted(window, cx);
                view.set_calendars_open(wanted, cx);
            });
        }));
    }

    fn set_calendars_open(&mut self, open: bool, cx: &mut Context<Self>) {
        let popover = &mut self.calendars_popover;
        popover.pending = None;
        popover.open = open;
        if !open {
            popover.pinned = false;
        }
        cx.notify();
    }

    /// A click or a key on the button opens the popover, or closes it when open.
    fn click_calendars(&mut self, cx: &mut Context<Self>) {
        let open = !self.calendars_popover.open;
        self.calendars_popover.pinned = open;
        self.calendars_popover.dismissed = !open;
        self.set_calendars_open(open, cx);
    }

    fn dismiss_calendars(&mut self, cx: &mut Context<Self>) {
        if self.calendars_popover.open {
            self.calendars_popover.dismissed = true;
            self.set_calendars_open(false, cx);
        }
    }

    /// The rail's Calendars button, with the popover beside it while open.
    pub(super) fn render_calendars_button(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let popover = &self.calendars_popover;
        let open = popover.open;
        let (bounds, card) = (popover.button.clone(), popover.popover.clone());
        let surface = open.then(|| {
            let theme = cx.theme();
            let title = div()
                .flex()
                .items_center()
                .h(px(SECTION_HEIGHT))
                .px(px(ROW_PADDING))
                .text_size(px(DETAIL_SIZE))
                .text_color(theme.muted_foreground.opacity(0.8))
                .child("Calendars");
            let surface = panel(div().id("calendars-popover"), cx)
                .w(px(WIDTH))
                // A long list scrolls instead of running past the window.
                .max_h(window.viewport_size().height - px(2.0 * WINDOW_MARGIN))
                .on_hover(cx.listener(|view, _, window, cx| view.settle_calendars(window, cx)))
                .on_mouse_down_out(cx.listener(|view, _, window, cx| {
                    // The button's own click toggles the popover, and a row's menu is outside
                    // it.
                    if !view
                        .calendars_popover
                        .button
                        .get()
                        .contains(&window.mouse_position())
                        && view.menu.is_none()
                    {
                        view.set_calendars_open(false, cx);
                    }
                }))
                .child(
                    canvas(move |drawn, _, _| card.set(drawn), |_, _, _, _| {})
                        .absolute()
                        .inset_0(),
                )
                .child(title)
                .child(
                    div()
                        .id("calendars-popover-rows")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .children(self.render_calendars_section(window, cx)),
                );
            deferred(
                Positioner::side(bounds.get())
                    .placement(Placement::Right)
                    .align(Align::Start)
                    .offset(px(GAP))
                    .margin(px(WINDOW_MARGIN))
                    .occlude()
                    .child(opening(surface, "calendars-popover-open", window, cx)),
            )
            .with_priority(POPUP_PRIORITY)
        });
        div()
            .id("calendars-button")
            .relative()
            .on_hover(cx.listener(|view, _, window, cx| view.settle_calendars(window, cx)))
            .on_key_down(cx.listener(|view, event: &gpui_kit::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    view.dismiss_calendars(cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                rail_button("nav-calendars", "calendars", open, cx)
                    .accessibility_label("Calendars")
                    .on_click(cx.listener(|view, _, _, cx| view.click_calendars(cx))),
            )
            .child(
                canvas(move |drawn, _, _| bounds.set(drawn), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .children(surface)
    }
}

/// Width of an Account row inside the popover, for its drag preview.
pub(super) const ROW_WIDTH: f32 = WIDTH - 2.0 * PADDING;
