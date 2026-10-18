//! The Calendars section the Calendars popover shows: one row per Account, or the step that
//! grants access.

mod account_row;
#[cfg(test)]
mod tests;

use std::time::Instant;

use super::NativeView;
use super::controls::app_icon;
use super::settings_page::SettingsSection;
use super::sidebar_reorder::{RowDrag, grab_cursor};
use super::sidebar_row::{activate, row};
use crumble_store::{AccountColor, CalendarsSection, calendars_section};
use crumble_theme::sidebar::{DETAIL_SIZE, ROW_GAP, ROW_PADDING};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    AnyElement, App, Context, Div, DragMoveEvent, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Stateful, Styled, Window, div, px,
};

const PRIVACY_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Calendars";
const INTERNET_ACCOUNTS: &str =
    "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension";

/// Muted copy sits a step below its label, as in SkillBinder.
const MUTED_OPACITY: f32 = 0.65;

/// What an Account row's context menu asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountAction {
    /// Edit the Account's Alias in place.
    Rename,
    /// Give the dot this color.
    Recolor(AccountColor),
    /// Move the row to this index.
    MoveTo(usize),
}

impl NativeView {
    /// The section's rows; `None` without EventKit.
    pub(super) fn render_calendars_section(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.account_reorder.moving(Instant::now()) {
            window.request_animation_frame();
        }
        let sync = self.calendar_sync.read(cx);
        let accounts = self.store.listed_accounts(sync.accounts());
        let rows: Vec<AnyElement> = match calendars_section(
            sync.access(),
            sync.loaded(),
            sync.accounts().len(),
            accounts.len(),
        ) {
            CalendarsSection::Hidden => return None,
            CalendarsSection::Connect => vec![
                activate(
                    row(
                        "connect-calendars",
                        app_icon("calendar"),
                        "Connect calendars",
                        false,
                        cx,
                    ),
                    "Connect calendars",
                    cx,
                    |view, _, cx| view.calendar_sync.read(cx).request_access(),
                )
                .into_any_element(),
            ],
            CalendarsSection::AllowAccess => vec![
                activate(
                    row(
                        "allow-calendar-access",
                        app_icon("calendar"),
                        "Allow calendar access",
                        false,
                        cx,
                    ),
                    "Allow calendar access",
                    cx,
                    |_, _, cx| cx.open_url(PRIVACY_SETTINGS),
                )
                .into_any_element(),
            ],
            CalendarsSection::AddAccounts => vec![
                hint(
                    "add-accounts",
                    "Add accounts in System Settings › Internet Accounts",
                    cx,
                    |_, _, cx| cx.open_url(INTERNET_ACCOUNTS),
                )
                .into_any_element(),
            ],
            CalendarsSection::AllDisabled => vec![
                hint(
                    "all-accounts-disabled",
                    "All accounts disabled · Open Settings",
                    cx,
                    |view, _, cx| view.open_settings(SettingsSection::Calendars, cx),
                )
                .into_any_element(),
            ],
            CalendarsSection::Accounts => {
                let stored: Vec<String> =
                    accounts.iter().map(|account| account.id.clone()).collect();
                let (order, floating) = self
                    .account_reorder
                    .displayed(&stored, cx.has_active_drag());
                let now = Instant::now();
                order
                    .iter()
                    .filter_map(|id| accounts.iter().find(|account| &account.id == id))
                    .enumerate()
                    .map(|(index, account)| {
                        let placement = Placement {
                            index,
                            len: order.len(),
                            offset: self.account_reorder.offset(&account.id, now),
                            floating: floating.as_ref() == Some(&account.id),
                        };
                        self.account_row(account, placement, cx)
                    })
                    .collect()
            }
        };
        Some(
            div()
                .id("sidebar-calendars")
                .flex()
                .flex_col()
                .gap(px(ROW_GAP))
                // Drag moves are measured from the first row's slot.
                .on_drag_move(cx.listener(
                    |view, event: &DragMoveEvent<RowDrag<String>>, window, cx| {
                        let drag = event.drag(cx);
                        let top = drag.top(event.event.position, event.bounds);
                        view.drag_account(drag.id.clone(), top, window, cx)
                    },
                ))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|view, _, _, cx| view.finish_account_drag(cx)),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|view, _, _, cx| view.finish_account_drag(cx)),
                )
                .children(rows)
                .into_any_element(),
        )
    }

    /// The listed Account identifiers in their saved order.
    fn account_ids(&self, cx: &App) -> Vec<String> {
        self.store
            .listed_accounts(self.calendar_sync.read(cx).accounts())
            .into_iter()
            .map(|account| account.id.clone())
            .collect()
    }

    fn drag_account(&mut self, id: String, top: f32, window: &mut Window, cx: &mut Context<Self>) {
        grab_cursor(window, cx);
        let stored = self.account_ids(cx);
        if self
            .account_reorder
            .drag(&stored, &id, top, !cx.reduce_motion())
        {
            cx.notify();
        }
    }

    /// Saves the order shown when the pointer is released.
    fn finish_account_drag(&mut self, cx: &mut Context<Self>) {
        let stored = self.account_ids(cx);
        let (released, animate) = (cx.has_active_drag(), !cx.reduce_motion());
        if let Some(order) = self.account_reorder.finish(&stored, released, animate) {
            self.save_account_order(order, cx);
        }
        cx.notify();
    }

    /// Moves `id` to `target`, sliding the rows it passes.
    fn move_account(&mut self, id: &str, target: usize, cx: &mut Context<Self>) {
        let stored = self.account_ids(cx);
        let animate = !cx.reduce_motion();
        if let Some(order) = self
            .account_reorder
            .move_to(&stored, &id.to_owned(), target, animate)
        {
            self.save_account_order(order, cx);
        }
        cx.notify();
    }

    fn save_account_order(&mut self, order: Vec<String>, cx: &App) {
        let accounts = self.calendar_sync.read(cx).accounts();
        self.store.set_listed_order(accounts, order);
    }

    fn set_account_color(&mut self, id: &str, color: AccountColor, cx: &mut Context<Self>) {
        self.store.set_account_color(id, color);
        cx.notify();
    }

    pub(super) fn toggle_account(&mut self, id: &str, cx: &mut Context<Self>) {
        self.store.toggle_account_disabled(id);
        self.refresh_events(false, cx);
        cx.notify();
    }

    fn toggle_hidden(&mut self, id: &str, cx: &mut Context<Self>) {
        self.store.toggle_account_hidden(id);
        self.refresh_events(false, cx);
        cx.notify();
    }

    pub(super) fn toggle_calendar(&mut self, id: &str, cx: &mut Context<Self>) {
        self.store.toggle_calendar_disabled(id);
        self.refresh_events(false, cx);
        cx.notify();
    }
}

/// Where an Account row sits in the list, and how it is drawn while rows move.
struct Placement {
    index: usize,
    len: usize,
    /// How far the row is drawn from its slot while it slides.
    offset: f32,
    /// The row is being dragged, so its floating preview stands in for it.
    floating: bool,
}

/// A muted line of copy that runs `action` when clicked.
fn hint(
    id: &'static str,
    text: &'static str,
    cx: &Context<NativeView>,
    action: impl Fn(&mut NativeView, &mut Window, &mut Context<NativeView>) + 'static,
) -> Stateful<Div> {
    let theme = cx.theme();
    let (muted, hover) = (
        theme.muted_foreground.opacity(MUTED_OPACITY),
        theme.foreground,
    );
    activate(
        div()
            .id(id)
            .px(px(ROW_PADDING))
            .py_1()
            .cursor_pointer()
            .text_size(px(DETAIL_SIZE))
            .text_color(muted)
            .hover(move |style| style.text_color(hover))
            .child(text),
        text,
        cx,
        action,
    )
}
