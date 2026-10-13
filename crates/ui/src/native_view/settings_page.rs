//! Settings page: the user settings saved in `settings.json`.

use super::NativeView;
use super::calendars_section::provider_icon;
use crate::settings::Settings;
use chrono::Weekday;
use crumble_eventkit::{Access, Account};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{
    ActiveTheme, Disableable, IndexPath, Sizable, StyledExt, h_flex, v_flex,
};
use gpui_kit::{
    AppContext, Context, Div, ElementId, Entity, IntoElement, ParentElement, Styled, Window, div,
    prelude::FluentBuilder, px, rgb,
};

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

const SELECT_WIDTH: f32 = 160.0;
/// Diameter of a Calendar's color dot.
const DOT_SIZE: f32 = 10.0;

pub(super) type WeekdaySelect = Entity<SelectState<Vec<&'static str>>>;

/// Dropdown for the First day of week; choosing a day saves it.
pub(super) fn first_day_select(
    settings: &Settings,
    window: &mut Window,
    cx: &mut Context<NativeView>,
) -> WeekdaySelect {
    let selected = settings.first_day_of_week.num_days_from_monday() as usize;
    let select = cx.new(|cx| {
        SelectState::new(
            WEEKDAYS.to_vec(),
            Some(IndexPath::new(selected)),
            window,
            cx,
        )
    });
    cx.subscribe_in(&select, window, |view, _, event: &SelectEvent<_>, _, cx| {
        let SelectEvent::Confirm(Some(day)) = event else {
            return;
        };
        if let Ok(day) = day.parse::<Weekday>() {
            view.settings.first_day_of_week = day;
            view.settings.save();
            cx.notify();
        }
    })
    .detach();
    select
}

impl NativeView {
    pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .max_w(px(560.0))
            .gap_4()
            .child(div().text_xl().child("Settings"))
            .child(group_title("Calendar", cx))
            .child(
                h_flex()
                    .justify_between()
                    .gap_4()
                    .text_sm()
                    .child("First day of week")
                    // Select fills its parent, so a fixed-width box sizes it.
                    .child(
                        div().w(px(SELECT_WIDTH)).child(
                            Select::new(&self.first_day_select)
                                .small()
                                .menu_width(px(SELECT_WIDTH))
                                .accessibility_label("First day of week"),
                        ),
                    ),
            )
            .children(self.render_calendar_settings(cx))
    }

    /// Each Account and its Calendars, with a checkbox per Calendar. A Disabled Account greys
    /// its Calendars out but keeps their checkboxes as they were.
    fn render_calendar_settings(&self, cx: &Context<Self>) -> Option<Div> {
        let sync = self.calendar_sync.read(cx);
        let accounts = sync.accounts();
        // Accounts read before access was revoked stay in memory; hide them with the sidebar's.
        if sync.access() != Access::Granted || accounts.is_empty() {
            return None;
        }
        Some(
            v_flex()
                .gap_4()
                .child(group_title("Calendars", cx))
                .children(
                    accounts
                        .iter()
                        .map(|account| self.account_settings(account, cx)),
                ),
        )
    }

    fn account_settings(&self, account: &Account, cx: &Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let disabled = self.settings.disabled_accounts.contains(&account.id);
        let calendars = self
            .calendar_sync
            .read(cx)
            .calendars()
            .iter()
            .filter(|calendar| calendar.account_id == account.id)
            .map(|calendar| {
                let id = calendar.id.clone();
                Checkbox::new(ElementId::Name(format!("calendar-{}", calendar.id).into()))
                    .checked(!self.settings.disabled_calendars.contains(&calendar.id))
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
                    .child(provider_icon(account.provider).size_4().text_color(muted))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_medium()
                            .child(account.name.clone()),
                    )
                    .when(disabled, |header| {
                        header.child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted)
                                .child("Account disabled"),
                        )
                    }),
            )
            .child(v_flex().gap_2().pl_6().children(calendars))
    }
}

fn group_title(title: &'static str, cx: &Context<NativeView>) -> Div {
    div()
        .text_sm()
        .font_semibold()
        .text_color(cx.theme().muted_foreground)
        .child(title)
}
