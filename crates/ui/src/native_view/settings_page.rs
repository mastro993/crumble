//! Settings page: choices kept across restarts through `Store`, in sections picked from a
//! sidebar at the panel's leading edge. Its Calendars section is in `calendars`, and its Models
//! and Providers sections in `settings_models`.

mod calendars;

use super::controls::{app_icon, button};
use super::sidebar::column;
use super::sidebar_row::{activate, row_with};
use super::{NativeView, Route};
use chrono::Weekday;
use crumble_notify::Status;
use crumble_store::Store;
use crumble_theme::CONTENT_PADDING;
use crumble_theme::sidebar::{ICON_SIZE, INSET, LABEL_SIZE, ROW_GAP, ROW_PADDING};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, IndexPath, Sizable, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext, Context, Div, ElementId, Entity, FontWeight, InteractiveElement,
    IntoElement, ParentElement, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
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

/// System Settings › Notifications, open on Crumble.
const NOTIFICATION_SETTINGS: &str = "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=io.github.mastro993.crumble";

const SELECT_WIDTH: f32 = 160.0;
/// Widest a section's settings run.
const CONTENT_WIDTH: f32 = 560.0;
/// Height of the page's title row, in the sidebar and above the section's settings.
const TITLE_HEIGHT: f32 = 34.0;
const TITLE_SIZE: f32 = 18.0;
/// Height of a sidebar row, and of a group's label.
const ROW_HEIGHT: f32 = 30.0;
/// Corner radius of a sidebar row's fill.
const ROW_RADIUS: f32 = 10.0;
/// Space above a group's label, after the group before it.
const GROUP_GAP: f32 = 12.0;
/// Space between a group's label and its first row.
const GROUP_LABEL_GAP: f32 = 4.0;

/// A part of the Settings page, listed in its sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SettingsSection {
    #[default]
    General,
    Calendars,
    Recording,
    Models,
    Providers,
}

impl SettingsSection {
    /// The sidebar's groups, each a label over its sections.
    const GROUPS: [(&'static str, &'static [Self]); 2] = [
        ("Calendar", &[Self::General, Self::Calendars]),
        ("Notes", &[Self::Recording, Self::Models, Self::Providers]),
    ];

    fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Calendars => "Calendars",
            Self::Recording => "Recording",
            Self::Models => "Models",
            Self::Providers => "Providers",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::General => "settings",
            Self::Calendars => "calendars",
            Self::Recording => "mic",
            Self::Models => "ai-brain",
            Self::Providers => "key",
        }
    }
}

pub(super) type WeekdaySelect = Entity<SelectState<Vec<&'static str>>>;

/// Dropdown for the First day of week; choosing a day saves it.
pub(super) fn first_day_select(
    store: &Store,
    window: &mut Window,
    cx: &mut Context<NativeView>,
) -> WeekdaySelect {
    let selected = store.first_day_of_week().num_days_from_monday() as usize;
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
            view.store.set_first_day_of_week(day);
            // The week now starts elsewhere, so other dates are on screen.
            view.refresh_events(false, cx);
            cx.notify();
        }
    })
    .detach();
    select
}

impl NativeView {
    /// Opens Settings on `section`.
    pub(super) fn open_settings(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        self.settings_section = section;
        self.navigate(Route::Settings, cx);
    }

    /// The sections sidebar beside the picked section's settings.
    pub(super) fn render_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Apple's Model can start or stop being able to run while any section shows.
        self.notice_apple_availability(window, cx);
        let section = self.settings_section;
        let content = match section {
            SettingsSection::General => self.render_general_settings(),
            SettingsSection::Calendars => self.render_calendar_settings(cx),
            SettingsSection::Recording => self.render_suggestion_settings(cx),
            SettingsSection::Models => self.render_model_settings(cx),
            SettingsSection::Providers => self.render_provider_settings(cx),
        };
        // Every row keeps the selected row's ink; only the fill marks the section on screen.
        let theme = cx.theme();
        let (ink, muted) = (theme.sidebar_accent_foreground, theme.muted_foreground);
        let mut sidebar = column(window, cx)
            .flex()
            .flex_col()
            .gap(px(ROW_GAP))
            .p(px(INSET))
            .child(title("Settings").px(px(ROW_PADDING)));
        for (index, (label, sections)) in SettingsSection::GROUPS.into_iter().enumerate() {
            sidebar = sidebar.child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .h(px(ROW_HEIGHT))
                    // The title's row already leaves room above the first group.
                    .when(index > 0, |label| label.mt(px(GROUP_GAP - ROW_GAP)))
                    .mb(px(GROUP_LABEL_GAP - ROW_GAP))
                    .px(px(ROW_PADDING))
                    .text_size(px(LABEL_SIZE))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(muted)
                    .child(label),
            );
            for &each in sections {
                sidebar = sidebar.child(activate(
                    row_with(
                        ElementId::NamedInteger("settings-section".into(), each as u64),
                        app_icon(each.icon())
                            .size(px(ICON_SIZE))
                            .flex_shrink_0()
                            .text_color(ink),
                        each.title(),
                        each == section,
                        cx,
                    )
                    .h(px(ROW_HEIGHT))
                    .rounded(px(ROW_RADIUS))
                    .text_color(ink),
                    each.title(),
                    cx,
                    move |view, _, cx| {
                        view.settings_section = each;
                        cx.notify();
                    },
                ));
            }
        }
        let sidebar = self.render_open_sidebar(sidebar, cx);
        h_flex()
            .size_full()
            .child(sidebar)
            .child(
                div()
                    .id("settings-content")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    // The section's title sits level with the sidebar's.
                    .pt(px(INSET))
                    .px(px(CONTENT_PADDING))
                    .pb(px(CONTENT_PADDING))
                    .child(
                        v_flex()
                            .max_w(px(CONTENT_WIDTH))
                            .gap_4()
                            .child(title(section.title()))
                            .child(content),
                    ),
            )
            .into_any_element()
    }

    fn render_general_settings(&self) -> Div {
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
            )
    }

    /// The switch for Suggestions, and a line under it while macOS keeps them from showing.
    fn render_suggestion_settings(&self, cx: &Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .gap_4()
                    .text_sm()
                    .child("Suggest recording when a meeting starts")
                    .child(
                        Switch::new("suggest-recording")
                            .checked(self.store.suggests_recording())
                            .small()
                            .accessibility_label("Suggest recording when a meeting starts")
                            .on_click(cx.listener(|view, on: &bool, _, cx| {
                                view.store.set_suggests_recording(*on);
                                cx.notify();
                            })),
                    ),
            )
            .when(
                self.store.suggests_recording() && self.suggestions.status == Status::Denied,
                |group| {
                    group.child(
                        h_flex()
                            .gap_2()
                            .text_xs()
                            .text_color(muted)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child("Notifications are off for Crumble in System Settings."),
                            )
                            .child(
                                button("suggest-recording-settings")
                                    .flex_shrink_0()
                                    .link()
                                    .small()
                                    .label("Open System Settings")
                                    .on_click(|_, _, cx| cx.open_url(NOTIFICATION_SETTINGS)),
                            ),
                    )
                },
            )
    }
}

/// The page's title, or the section's above its settings.
fn title(text: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .h(px(TITLE_HEIGHT))
        .text_size(px(TITLE_SIZE))
        .font_weight(FontWeight::BOLD)
        .child(text)
}

/// A line of secondary text in place of a section's settings.
fn note(text: &'static str, cx: &Context<NativeView>) -> Div {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}
