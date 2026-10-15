//! An Event's details: the hover card over its chip and the page clicking it opens. Both are
//! read-only, as Crumble never changes Events; a meeting link becomes a Join button.

use super::event_views::Shown;
use super::events::when_text;
use crate::native_view::controls::app_icon;
use crate::native_view::menu::{opening, panel};
use crumble_eventkit::{Join, Participant, Response};
use crumble_theme::sidebar::DETAIL_SIZE;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{
    ActiveTheme, Sizable, StyledExt, button::Button, button::ButtonVariants, h_flex, v_flex,
};
use gpui_kit::{
    App, Div, ElementId, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString,
    Stateful, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

const CARD_WIDTH: f32 = 320.0;
/// Title lines the hover card shows before an ellipsis.
const CARD_TITLE_LINES: usize = 2;
/// Description lines the hover card shows before an ellipsis; the page shows them all.
const CARD_DESCRIPTION_LINES: usize = 3;
/// Participants the hover card lists before "+N more"; the page lists them all.
const CARD_PARTICIPANTS: usize = 3;
/// Widest the Event page's column grows.
const PAGE_WIDTH: f32 = 640.0;
const STATUS_ICON_SIZE: f32 = 14.0;

fn title(shown: &Shown) -> SharedString {
    if shown.event.title.is_empty() {
        "(No title)".into()
    } else {
        shown.event.title.clone().into()
    }
}

/// A full-width button that opens the meeting, named after its service.
fn join_button(id: impl Into<ElementId>, join: &Join) -> Button {
    let url = join.url.clone();
    Button::new(id)
        .primary()
        .w_full()
        .cursor_pointer()
        .icon(app_icon("video"))
        .label(format!("Join {}", join.service))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            cx.open_url(&url);
        })
}

/// A participant's name after an icon for their answer, then whether they organize or are the
/// user.
fn participant_row(participant: &Participant, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let (icon, color, answer) = match participant.response {
        Response::Accepted => ("check", theme.success, "Accepted"),
        Response::Tentative => ("help-circle", theme.warning, "Maybe"),
        Response::Declined => ("close", theme.danger, "Declined"),
        Response::Pending => ("clock", theme.muted_foreground, "Awaiting response"),
    };
    let role = match (participant.organizer, participant.you) {
        (true, true) => Some("Organizer · You"),
        (true, false) => Some("Organizer"),
        (false, true) => Some("You"),
        (false, false) => None,
    };
    h_flex()
        .id(SharedString::from(format!(
            "participant-{}",
            participant.name
        )))
        .gap_2()
        .min_w_0()
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(answer).build(window, cx)
        })
        .child(
            app_icon(icon)
                .size(px(STATUS_ICON_SIZE))
                .flex_shrink_0()
                .text_color(color),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .when(participant.response == Response::Declined, |name| {
                    name.line_through()
                })
                .child(participant.name.clone()),
        )
        .children(role.map(|role| {
            div()
                .flex_shrink_0()
                .text_color(theme.muted_foreground)
                .child(role)
        }))
}

/// A small muted heading over a group of details, as the sidebar titles its sections.
fn heading(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_size(px(DETAIL_SIZE))
        .font_semibold()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

fn participants_heading(count: usize, cx: &App) -> Div {
    heading(
        match count {
            1 => "1 participant".to_owned(),
            count => format!("{count} participants"),
        },
        cx,
    )
}

/// `text` with its line breaks and runs of spaces turned into single spaces. GPUI clamps only
/// wrapped lines, so a description of many short lines would otherwise grow the card unclamped.
fn one_paragraph(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The hover card: title, when, the start of the description, the first participants, and a
/// Join button, on the context menu's surface and with its opening motion.
pub(super) fn card(shown: &Shown, window: &Window, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let event = &shown.event;
    let more = event.participants.len().saturating_sub(CARD_PARTICIPANTS);
    let surface = panel(div().id("event-card").test_support(), cx)
        .w(px(CARD_WIDTH))
        .gap_3()
        .child(
            v_flex()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .line_clamp(CARD_TITLE_LINES)
                        .text_ellipsis()
                        .child(title(shown)),
                )
                .child(
                    div()
                        .text_color(theme.muted_foreground)
                        .child(when_text(&event.when)),
                ),
        )
        .children(event.description.as_deref().map(|description| {
            div()
                .text_color(theme.muted_foreground)
                .line_clamp(CARD_DESCRIPTION_LINES)
                .text_ellipsis()
                .child(one_paragraph(description))
        }))
        .when(!event.participants.is_empty(), |card| {
            card.child(
                v_flex()
                    .gap_1()
                    .child(participants_heading(event.participants.len(), cx))
                    .children(
                        event
                            .participants
                            .iter()
                            .take(CARD_PARTICIPANTS)
                            .map(|participant| participant_row(participant, cx)),
                    )
                    .when(more > 0, |list| {
                        list.child(
                            div()
                                .text_color(theme.muted_foreground)
                                .child(format!("+{more} more")),
                        )
                    }),
            )
        })
        .children(
            event
                .join
                .as_ref()
                .map(|join| join_button("event-card-join", join)),
        );
    opening(surface, "event-card-open", window, cx)
}

/// The Event's page: its Calendar, title, when, and location, a Join button, then the whole
/// description and every participant.
pub(super) fn page(shown: &Shown, cx: &App) -> Div {
    let theme = cx.theme();
    let event = &shown.event;
    let muted = theme.muted_foreground;
    let section = |heading: Div| v_flex().gap_2().child(heading);
    v_flex()
        .w_full()
        .max_w(px(PAGE_WIDTH))
        .gap_6()
        .pt_2()
        .text_sm()
        .child(
            v_flex()
                .gap_1()
                .when(!shown.calendar.is_empty(), |header| {
                    header.child(calendar_label(shown.calendar.clone(), shown.color, muted))
                })
                .child(div().text_2xl().font_semibold().child(title(shown)))
                .child(div().text_color(muted).child(when_text(&event.when)))
                .children(
                    event
                        .location
                        .clone()
                        .map(|location| div().text_color(muted).child(location)),
                ),
        )
        .children(
            event
                .join
                .as_ref()
                .map(|join| join_button("event-page-join", join).large()),
        )
        .children(
            event.description.clone().map(|description| {
                section(heading("Description", cx)).child(div().child(description))
            }),
        )
        .when(!event.participants.is_empty(), |page| {
            page.child(
                section(participants_heading(event.participants.len(), cx)).children(
                    event
                        .participants
                        .iter()
                        .map(|participant| participant_row(participant, cx)),
                ),
            )
        })
}

fn calendar_label(name: SharedString, color: Hsla, muted: Hsla) -> Div {
    h_flex()
        .gap_2()
        .text_color(muted)
        .child(div().size(px(8.0)).flex_shrink_0().rounded_full().bg(color))
        .child(div().min_w_0().truncate().child(name))
}
