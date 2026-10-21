//! Settings › Calendars: a group for each Account, with its switch and Alias field, then its
//! Calendars with a switch each.

use super::super::NativeView;
use super::super::account_dot::dot;
use super::form::{Group, Setting, description, field, groups};
use crumble_eventkit::{Access, Account};
use crumble_store::account_label;
use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, StyledExt, h_flex};
use gpui_kit::{
    Context, Div, ElementId, ParentElement, Styled, div, prelude::FluentBuilder, px, rgb,
};

/// Diameter of a Calendar's color dot.
const DOT_SIZE: f32 = 10.0;

impl NativeView {
    /// A group for each Account. A Disabled Account greys its Calendars out but keeps their
    /// switches as they were.
    pub(super) fn render_calendar_settings(&self, cx: &Context<Self>) -> Div {
        let sync = self.calendar_sync.read(cx);
        let accounts = sync.accounts();
        // Accounts read before access was revoked stay in memory; hide them with the popover's.
        if sync.access() != Access::Granted {
            return description(
                "Connect calendars from the Calendars button in the rail to choose them here.",
                cx,
            );
        }
        if accounts.is_empty() {
            return description(
                "Add accounts in System Settings › Internet Accounts to choose their Calendars here.",
                cx,
            );
        }
        groups().children(
            self.store
                .ordered_accounts(accounts)
                .into_iter()
                .map(|account| self.account_settings(account, cx)),
        )
    }

    fn account_settings(&self, account: &Account, cx: &Context<Self>) -> Group {
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
                Setting::new(
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
                .disabled(disabled)
                .control(
                    Switch::new(ElementId::Name(format!("calendar-{}", calendar.id).into()))
                        .checked(!self.store.is_calendar_disabled(&calendar.id))
                        .disabled(disabled)
                        .small()
                        .accessibility_label(calendar.name.clone())
                        .on_click(
                            cx.listener(move |view, _: &bool, _, cx| view.toggle_calendar(&id, cx)),
                        ),
                )
            });
        // The Account's own name follows its Alias in brackets.
        let heading = h_flex()
            .gap_2()
            .min_w_0()
            .child(dot(self.store.account_color(&account.id), cx))
            .child(div().min_w_0().truncate().child(name.clone()))
            .when(aliased, |names| {
                names.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_normal()
                        .text_color(muted)
                        .child(format!("({label})")),
                )
            });
        Group::new(ElementId::Name(
            format!("account-group-{}", account.id).into(),
        ))
        .heading(heading)
        .child(
            Setting::new("Enabled")
                .description("When off, Crumble leaves this Account and its Calendars out.")
                .control(
                    Switch::new(ElementId::Name(format!("account-{}", account.id).into()))
                        .checked(!disabled)
                        .small()
                        .accessibility_label(name)
                        .on_click(cx.listener(move |view, _: &bool, _, cx| {
                            view.toggle_account(&account_id, cx)
                        })),
                ),
        )
        .children(self.alias_fields.get(&account.id).map(|alias| {
            Setting::new("Alias")
                .description("The name Crumble shows for this Account.")
                .control(field(Input::new(alias).small()))
        }))
        .children(calendars)
    }
}
