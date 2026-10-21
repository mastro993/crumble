//! Settings page: choices kept across restarts through `Store`, in sections picked from a
//! sidebar at the panel's leading edge. Every section is built from the parts in `form`. Its
//! Calendars section is in `calendars`, its Transcription, Summary and Providers sections in
//! `settings_models`, and its Local Models section in `local_models`.

mod calendars;
pub(super) mod form;

use super::controls::{app_icon, button};
use super::select::{Picked, Select};
use super::sidebar::column;
use super::sidebar_row::{activate, row_with};
use super::{NativeView, Route};
use chrono::Weekday;
use crumble_notify::Status;
use crumble_store::Store;
use crumble_theme::sidebar::{ICON_SIZE, INSET, ROW_GAP, ROW_PADDING};
use form::{Group, PAGE_PADDING, Setting, TITLE_GAP, groups};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext, Context, Div, ElementId, Entity, FontWeight, IntoElement,
    ParentElement, Styled, Window, div, prelude::FluentBuilder, px,
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

/// Widest a section's settings run; narrower space centers them.
const CONTENT_WIDTH: f32 = 720.0;
/// Height of the page's title row, in the sidebar and above the section's settings.
const TITLE_HEIGHT: f32 = 34.0;
const TITLE_SIZE: f32 = 18.0;
/// Height of a sidebar row, and of a group's label.
const ROW_HEIGHT: f32 = 30.0;
/// Corner radius of a sidebar row's fill.
const ROW_RADIUS: f32 = 10.0;
/// Space above a group's label, after the title or the group before it.
const GROUP_GAP: f32 = 16.0;
/// Size of the sidebar's title.
const SIDEBAR_TITLE_SIZE: f32 = 20.0;
/// Size of a sidebar row's name, and of a group's label.
const SIDEBAR_TEXT_SIZE: f32 = 15.0;
/// Space between a group's label and its first row.
const GROUP_LABEL_GAP: f32 = 4.0;

/// A part of the Settings page, listed in its sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SettingsSection {
    #[default]
    General,
    Calendars,
    Recording,
    Transcription,
    LocalModels,
    Summary,
    Providers,
}

impl SettingsSection {
    /// The sidebar's groups, each a label over its sections.
    const GROUPS: [(&'static str, &'static [Self]); 2] = [
        ("Calendar", &[Self::General, Self::Calendars]),
        (
            "Notes",
            &[
                Self::Recording,
                Self::Transcription,
                Self::LocalModels,
                Self::Summary,
                Self::Providers,
            ],
        ),
    ];

    pub(super) fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Calendars => "Calendars",
            Self::Recording => "Recording",
            Self::Transcription => "Transcription",
            Self::LocalModels => "Local Models",
            Self::Summary => "Summary",
            Self::Providers => "Providers",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::General => "settings",
            Self::Calendars => "calendars",
            Self::Recording => "mic",
            Self::Transcription => "voice",
            Self::LocalModels => "chip",
            Self::Summary => "ai-brain",
            Self::Providers => "key",
        }
    }
}

pub(super) type WeekdaySelect = Entity<Select<&'static str>>;

/// Select for the First day of week; choosing a day saves it.
pub(super) fn first_day_select(store: &Store, cx: &mut Context<NativeView>) -> WeekdaySelect {
    let selected = store.first_day_of_week().num_days_from_monday() as usize;
    let select =
        cx.new(|cx| Select::new("First day of week", WEEKDAYS.to_vec(), Some(selected), cx));
    cx.subscribe(
        &select,
        |view, _, Picked(day): &Picked<&'static str>, cx| {
            if let Ok(day) = day.parse::<Weekday>() {
                view.store.set_first_day_of_week(day);
                // The week now starts elsewhere, so other dates are on screen.
                view.refresh_events(false, cx);
                cx.notify();
            }
        },
    )
    .detach();
    select
}

impl NativeView {
    /// Opens Settings on `section`.
    pub(super) fn open_settings(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        self.settings_section = section;
        self.local_models.reread = true;
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
        // Downloads, or Finder, may have changed the models folder.
        self.reread_local_models(cx);
        let section = self.settings_section;
        let content = match section {
            SettingsSection::General => scrolling(section, self.render_general_settings()),
            SettingsSection::Calendars => scrolling(section, self.render_calendar_settings(cx)),
            SettingsSection::Recording => scrolling(section, self.render_suggestion_settings(cx)),
            SettingsSection::Transcription => {
                scrolling(section, self.render_transcription_settings(cx))
            }
            // Its list scrolls the whole section, title and all, drawing only the rows in view.
            SettingsSection::LocalModels => self.render_local_model_settings(cx),
            SettingsSection::Summary => scrolling(section, self.render_summary_settings()),
            SettingsSection::Providers => scrolling(section, self.render_provider_settings(cx)),
        };
        // Every row keeps the selected row's ink; only the fill marks the section on screen.
        let theme = cx.theme();
        // Group labels take the hint ink, a step lighter than secondary text.
        let (ink, label_ink) = (
            theme.sidebar_accent_foreground,
            crumble_theme::faint_foreground(theme),
        );
        let mut sidebar = column(window, cx)
            .flex()
            .flex_col()
            .gap(px(ROW_GAP))
            .p(px(INSET))
            .child(
                title("Settings")
                    .px(px(ROW_PADDING))
                    .text_size(px(SIDEBAR_TITLE_SIZE)),
            );
        for (label, sections) in SettingsSection::GROUPS {
            sidebar = sidebar.child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .h(px(ROW_HEIGHT))
                    .mt(px(GROUP_GAP - ROW_GAP))
                    .mb(px(GROUP_LABEL_GAP - ROW_GAP))
                    .px(px(ROW_PADDING))
                    .text_size(px(SIDEBAR_TEXT_SIZE))
                    .text_color(label_ink)
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
                    .text_size(px(SIDEBAR_TEXT_SIZE))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(ink),
                    each.title(),
                    cx,
                    move |view, _, cx| {
                        view.settings_section = each;
                        view.local_models.reread = true;
                        cx.notify();
                    },
                ));
            }
        }
        let sidebar = self.render_open_sidebar(sidebar, cx);
        h_flex()
            .size_full()
            .child(sidebar)
            .child(content)
            .into_any_element()
    }

    fn render_general_settings(&self) -> Div {
        groups().child(
            Group::new("settings-calendar").heading("Calendar").child(
                Setting::new("First day of week")
                    .description("The day every week starts on in the Calendar.")
                    .control(self.first_day_select.clone()),
            ),
        )
    }

    /// The switch for Suggestions, and a line under it while macOS keeps them from showing.
    fn render_suggestion_settings(&self, cx: &Context<Self>) -> Div {
        let denied = self.store.suggests_recording() && self.suggestions.status == Status::Denied;
        groups().child(
            Group::new("settings-suggestions")
                .heading("Suggestions")
                .child(
                    Setting::new("Suggest recording when a meeting starts")
                        .description(
                            "A notification offers to record the meeting. Only its Record button \
                         starts a Recording.",
                        )
                        .when(denied, |setting| {
                            setting.description(
                                h_flex()
                                    .gap_2()
                                    .flex_wrap()
                                    .child("Notifications are off for Crumble in System Settings.")
                                    .child(
                                        button("suggest-recording-settings")
                                            .link()
                                            .small()
                                            .label("Open System Settings")
                                            .on_click(|_, _, cx| {
                                                cx.open_url(NOTIFICATION_SETTINGS)
                                            }),
                                    ),
                            )
                        })
                        .control(
                            Switch::new("suggest-recording")
                                .checked(self.store.suggests_recording())
                                .small()
                                .accessibility_label("Suggest recording when a meeting starts")
                                .on_click(cx.listener(|view, on: &bool, _, cx| {
                                    view.store.set_suggests_recording(*on);
                                    cx.notify();
                                })),
                        ),
                ),
        )
    }
}

/// `child` at most `CONTENT_WIDTH` wide, centered in the space beside the sidebar.
pub(super) fn centered(child: impl IntoElement) -> Div {
    div().w_full().px(px(PAGE_PADDING)).child(
        div()
            .w_full()
            .max_w(px(CONTENT_WIDTH))
            .mx_auto()
            .child(child),
    )
}

/// A section's title and settings, scrolling together.
fn scrolling(section: SettingsSection, content: Div) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(
            centered(
                v_flex()
                    .gap(px(TITLE_GAP))
                    .child(title(section.title()))
                    .child(content),
            )
            .py(px(PAGE_PADDING)),
        )
        .overflow_y_scrollbar()
        .id("settings-content")
        .into_any_element()
}

/// The page's title, or the section's above its settings.
pub(super) fn title(text: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .h(px(TITLE_HEIGHT))
        .text_size(px(TITLE_SIZE))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text)
}
