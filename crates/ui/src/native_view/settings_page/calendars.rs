//! Settings › Calendars: each Account with its Alias field and switch, and its Calendars with a
//! checkbox each.

use super::super::NativeView;
use super::super::account_dot::dot;
use super::note;
use crumble_eventkit::{Access, Account};
use crumble_store::account_label;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::{
    Context, Div, ElementId, ParentElement, Styled, div, prelude::FluentBuilder, px, rgb,
};

const ALIAS_WIDTH: f32 = 160.0;
/// Diameter of a Calendar's color dot.
const DOT_SIZE: f32 = 10.0;

impl NativeView {
    /// Each Account with a switch, and its Calendars with a checkbox each. A Disabled Account
    /// greys its Calendars out but keeps their checkboxes as they were.
    pub(super) fn render_calendar_settings(&self, cx: &Context<Self>) -> Div {
        let sync = self.calendar_sync.read(cx);
        let accounts = sync.accounts();
        // Accounts read before access was revoked stay in memory; hide them with the popover's.
        if sync.access() != Access::Granted {
            return note(
                "Connect calendars from the Calendars button in the rail to choose them here.",
                cx,
            );
        }
        if accounts.is_empty() {
            return note(
                "Add accounts in System Settings › Internet Accounts to choose their Calendars here.",
                cx,
            );
        }
        v_flex().gap_4().children(
            self.store
                .ordered_accounts(accounts)
                .into_iter()
                .map(|account| self.account_settings(account, cx)),
        )
    }

    fn account_settings(&self, account: &Account, cx: &Context<Self>) -> Div {
        let label = account_label(account).to_owned();
        let aliased = self.store.account_alias(&account.id).is_some();
        let name = self.store.account_name(account).to_owned();
        let muted = cx.theme().muted_foreground;
        let account_id = account.id.clone();
        let disabled = self.store.is_account_disabled(&account.id);
        let calendars = self
            .calendar_sync
            .read(cx)
            .calendars()
            .iter()
            .filter(|calendar| calendar.account_id == account.id)
            .map(|calendar| {
                let id = calendar.id.clone();
                Checkbox::new(ElementId::Name(format!("calendar-{}", calendar.id).into()))
                    .checked(!self.store.is_calendar_disabled(&calendar.id))
                    .disabled(disabled)
                    .accessibility_label(calendar.name.clone())
                    .on_click(
                        cx.listener(move |view, _: &bool, _, cx| view.toggle_calendar(&id, cx)),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .size(px(DOT_SIZE))
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .bg(rgb(calendar.color)),
                            )
                            .child(calendar.name.clone()),
                    )
            });
        v_flex()
            .gap_2()
            .text_sm()
            .child(
                h_flex()
                    .gap_2()
                    .child(dot(self.store.account_color(&account.id), cx))
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().min_w_0().truncate().font_medium().child(name.clone()))
                            // The Account's own name follows its Alias in brackets.
                            .when(aliased, |names| {
                                names.child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_color(muted)
                                        .child(format!("({label})")),
                                )
                            }),
                    )
                    .children(
                        self.alias_fields
                            .get(&account.id)
                            .map(|field| Input::new(field).small().w(px(ALIAS_WIDTH))),
                    )
                    .child(
                        Switch::new(ElementId::Name(format!("account-{}", account.id).into()))
                            .checked(!disabled)
                            .small()
                            .accessibility_label(name)
                            .on_click(cx.listener(move |view, _: &bool, _, cx| {
                                view.toggle_account(&account_id, cx)
                            })),
                    ),
            )
            .child(v_flex().gap_2().pl_6().children(calendars))
    }
}
