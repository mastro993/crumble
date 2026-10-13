//! Settings page: the user settings saved in `settings.json`.

use super::NativeView;
use crate::settings::Settings;
use chrono::Weekday;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme, IndexPath, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::{AppContext, Context, Entity, IntoElement, ParentElement, Styled, Window, div, px};

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
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Calendar"),
            )
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
    }
}
