//! An Event's details: the hover card over its Calendar chip, and the buttons the Agenda page's
//! header shows for the selected Event. Both are read-only, as Crumble never changes Events; a
//! meeting link becomes a Join button while the Event is today and not over.

use super::card::click_card;
use super::controls::{app_icon, button};
use chrono::{DateTime, Local};
use crumble_calendar::events::{joinable, when_text};
use crumble_calendar::links::{one_paragraph, parse_links};
use crumble_eventkit::{Event, Join, Participant, Response};
use crumble_theme::sidebar::DETAIL_SIZE;
use gpui_kit::component::{
    ActiveTheme, Sizable, StyledExt, button::Button, button::ButtonVariants, h_flex, v_flex,
};
use gpui_kit::{
    App, Div, ElementId, HighlightStyle, InteractiveElement, InteractiveText, IntoElement,
    ParentElement, SharedString, Stateful, StatefulInteractiveElement, Styled, StyledText,
    UnderlineStyle, div, prelude::FluentBuilder, px,
};
use std::ops::Range;

const CARD_WIDTH: f32 = 320.0;
/// Title lines the hover card shows before an ellipsis.
const CARD_TITLE_LINES: usize = 2;
/// Description lines the hover card shows before an ellipsis; the header's box shows them all.
const CARD_DESCRIPTION_LINES: usize = 3;
/// Participants the hover card lists before "+N more"; the header's box lists them all.
const CARD_PARTICIPANTS: usize = 3;
/// Width of the description and participants boxes below the header.
const BOX_WIDTH: f32 = 360.0;
const STATUS_ICON_SIZE: f32 = 14.0;

fn title(event: &Event) -> SharedString {
    if event.title.is_empty() {
        "(No title)".into()
    } else {
        event.title.clone().into()
    }
}

/// The Event's meeting link, while it can be joined at `now`.
fn join_at(event: &Event, now: DateTime<Local>) -> Option<&Join> {
    event.join.as_ref().filter(|_| joinable(event, now))
}

/// A button that opens the meeting, named after its service.
fn join_button(id: impl Into<ElementId>, join: &Join) -> Button {
    let url = join.url.clone();
    button(id)
        .primary()
        .icon(app_icon("video"))
        .label(format!("Join {}", join.service))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            cx.open_url(&url);
        })
}

/// A participant's name after an icon for their answer, then whether they organize or are the
/// user.
fn participant_row(ix: usize, participant: &Participant, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let (icon, color, answer) = match participant.response {
        Response::Accepted => ("check", theme.success, "Accepted"),
        Response::Tentative => ("help-circle", theme.warning, "Maybe"),
        Response::Declined => ("close", theme.danger, "Declined"),
        Response::Delegated => ("forward", theme.muted_foreground, "Delegated"),
        Response::Pending => ("clock", theme.muted_foreground, "Awaiting response"),
    };
    let role = match (participant.organizer, participant.you) {
        (true, true) => Some("Organizer · You"),
        (true, false) => Some("Organizer"),
        (false, true) => Some("You"),
        (false, false) => None,
    };
    h_flex()
        .id(("participant", ix))
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

/// `text` with its links underlined in the link color, each opening in the browser when clicked.
fn linked_text(id: &'static str, text: &str, cx: &App) -> InteractiveText {
    let color = cx.theme().link;
    let (text, links) = parse_links(text);
    let (ranges, urls): (Vec<Range<usize>>, Vec<String>) = links.into_iter().unzip();
    let style = HighlightStyle {
        color: Some(color),
        underline: Some(UnderlineStyle {
            thickness: px(1.),
            color: Some(color),
            wavy: false,
        }),
        ..HighlightStyle::default()
    };
    let styled =
        StyledText::new(text).with_highlights(ranges.iter().map(|range| (range.clone(), style)));
    InteractiveText::new(id, styled).on_click(ranges, move |ix, _, cx| {
        if let Some(url) = urls.get(ix) {
            cx.open_url(url);
        }
    })
}

/// The hover card's content: title, when, the start of the description, the first
/// participants, and a Join button while it can be joined at `now`.
pub(super) fn card(event: &Event, now: DateTime<Local>, cx: &App) -> Div {
    let theme = cx.theme();
    let more = event.participants.len().saturating_sub(CARD_PARTICIPANTS);
    v_flex()
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
                        .child(title(event)),
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
                .child(one_paragraph(&parse_links(description).0))
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
                            .enumerate()
                            .map(|(ix, participant)| participant_row(ix, participant, cx)),
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
        .children(join_at(event, now).map(|join| join_button("event-card-join", join).w_full()))
}

/// A button for each of the description, the participants, and joining while it can be joined
/// at `now`. The description and the participants open in boxes that close when clicked away
/// from.
pub(super) fn buttons(event: &Event, now: DateTime<Local>) -> Div {
    let description = event.description.clone().map(|description| {
        click_card(
            "event-description",
            Button::new("event-description")
                .ghost()
                .small()
                .cursor_pointer()
                .icon(app_icon("notes").size_4())
                .tooltip("Description"),
            move |_, cx| {
                v_flex()
                    .w(px(BOX_WIDTH))
                    .gap_2()
                    .child(heading("Description", cx))
                    .child(linked_text("event-description-text", &description, cx))
                    .into_any_element()
            },
        )
    });
    let participants = (!event.participants.is_empty()).then(|| {
        let list = event.participants.clone();
        click_card(
            "event-participants",
            Button::new("event-participants")
                .ghost()
                .small()
                .cursor_pointer()
                .icon(app_icon("user-group").size_4())
                .label(list.len().to_string())
                .tooltip("Participants"),
            move |_, cx| {
                v_flex()
                    .w(px(BOX_WIDTH))
                    .gap_2()
                    .child(participants_heading(list.len(), cx))
                    .children(
                        list.iter()
                            .enumerate()
                            .map(|(ix, participant)| participant_row(ix, participant, cx)),
                    )
                    .into_any_element()
            },
        )
    });
    h_flex()
        .flex_shrink_0()
        .gap_1()
        .children(description)
        .children(participants)
        .children(join_at(event, now).map(|join| join_button("event-header-join", join).small()))
}

#[cfg(test)]
mod tests {
    use super::buttons;

    use chrono::NaiveDate;
    use crumble_eventkit::{Event, Participant, Response, When};
    use gpui_kit::base::TestSupportExt;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Render,
        Styled, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
    };

    /// Stands in for the window header holding an Event's buttons.
    struct Header(Event);

    impl Render for Header {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("header")
                .test_support()
                .size_full()
                .flex()
                .items_start()
                .child(buttons(&self.0, chrono::Local::now()))
        }
    }

    #[gpui_kit::test]
    fn the_participants_box_opens_on_click_and_closes_when_clicked_away_from(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let day = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap_or_default();
        let event = Event {
            id: "standup".into(),
            series_uid: None,
            occurrence: None,
            calendar_id: "work".into(),
            title: "Standup".into(),
            when: When::AllDay {
                first: day,
                last: day,
            },
            location: None,
            description: Some("Agenda".into()),
            join: None,
            participants: vec![Participant {
                name: "Ada".into(),
                response: Response::Accepted,
                organizer: true,
                you: false,
            }],
        };
        let (handle, _) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            Default::default(),
                            size(px(800.), px(500.)),
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |_, cx| cx.new(|_| Header(event)),
                )
            })
            .unwrap_or_else(|error| panic!("open test window: {error}"));

        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("click-card").is_none());
            window.click("event-participants", cx);
            window.render_frame(cx);
            window.render_frame(cx);
            assert!(window.try_find("click-card").is_some(), "box did not open");
            // Away from the box, which hangs below the trigger at the right edge.
            window.click_at("header", gpui_kit::point(px(10.), px(400.)), cx);
            window.render_frame(cx);
            assert!(window.try_find("click-card").is_none(), "box stayed open");
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));
    }
}
