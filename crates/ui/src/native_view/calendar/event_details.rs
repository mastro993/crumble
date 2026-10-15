//! An Event's details: the hover card over its chip and the header of the page clicking it
//! opens. Both are read-only, as Crumble never changes Events; a meeting link becomes a Join
//! button.

use super::event_views::Shown;
use super::events::when_text;
use crate::native_view::card::click_card;
use crate::native_view::controls::{app_icon, button};
use crumble_eventkit::{Event, Join, Participant, Response};
use crumble_theme::sidebar::DETAIL_SIZE;
use gpui_kit::component::breadcrumb::{Breadcrumb, BreadcrumbItem};
use gpui_kit::component::{
    ActiveTheme, Sizable, StyledExt, button::Button, button::ButtonVariants, h_flex, v_flex,
};
use gpui_kit::{
    App, ClickEvent, Div, ElementId, HighlightStyle, InteractiveElement, InteractiveText,
    IntoElement, MouseButton, ParentElement, SharedString, Stateful, StatefulInteractiveElement,
    Styled, StyledText, UnderlineStyle, Window, div, prelude::FluentBuilder, px,
};
use std::ops::Range;

const CARD_WIDTH: f32 = 320.0;
/// Title lines the hover card shows before an ellipsis.
const CARD_TITLE_LINES: usize = 2;
/// Description lines the hover card shows before an ellipsis; the page's box shows them all.
const CARD_DESCRIPTION_LINES: usize = 3;
/// Participants the hover card lists before "+N more"; the page's box lists them all.
const CARD_PARTICIPANTS: usize = 3;
/// Width of the description and participants boxes on the Event page.
const BOX_WIDTH: f32 = 360.0;
const STATUS_ICON_SIZE: f32 = 14.0;

fn title(event: &Event) -> SharedString {
    if event.title.is_empty() {
        "(No title)".into()
    } else {
        event.title.clone().into()
    }
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

/// `text` as shown, each `[label](address)` written as its label, with the byte ranges of its
/// links in what is shown and where each goes: those labels and the bare web links.
fn parse_links(text: &str) -> (String, Vec<(Range<usize>, String)>) {
    let mut shown = String::with_capacity(text.len());
    let mut links = Vec::new();
    let mut rest = text;
    while let Some(next) = rest.chars().next() {
        let (label, url, len) = if let Some((label, url, len)) = labelled_link(rest) {
            (label, url, len)
        } else if let Some(len) = bare_link(rest) {
            (&rest[..len], &rest[..len], len)
        } else {
            shown.push(next);
            rest = &rest[next.len_utf8()..];
            continue;
        };
        let start = shown.len();
        shown.push_str(label);
        links.push((start..shown.len(), url.to_owned()));
        rest = &rest[len..];
    }
    (shown, links)
}

/// The label, the address, and the length of the `[label](address)` link `text` starts with.
fn labelled_link(text: &str) -> Option<(&str, &str, usize)> {
    let inner = text.strip_prefix('[')?;
    let close = inner.find("](")?;
    let label = &inner[..close];
    let after = &inner[close + 2..];
    let url = &after[..after.find(')')?];
    let fits = !label.is_empty() && !label.contains(['[', ']', '\n']);
    (fits && is_web_link(url) && !url.contains(char::is_whitespace)).then_some((
        label,
        url,
        1 + close + 2 + url.len() + 1,
    ))
}

/// The length of the web link `text` starts with, without the punctuation after it.
fn bare_link(text: &str) -> Option<usize> {
    let len = text
        .find(|c: char| c.is_whitespace() || "<>\"'()[]".contains(c))
        .unwrap_or(text.len());
    let link = text[..len].trim_end_matches(['.', ',', ';', ':', '!', '?']);
    is_web_link(link).then_some(link.len())
}

/// An `https://` or `http://` address with something after the scheme.
fn is_web_link(text: &str) -> bool {
    ["https://", "http://"].into_iter().any(|scheme| {
        text.strip_prefix(scheme)
            .is_some_and(|rest| !rest.is_empty())
    })
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

/// `text` with its line breaks and runs of spaces turned into single spaces. GPUI clamps only
/// wrapped lines, so a description of many short lines would otherwise grow the card unclamped.
fn one_paragraph(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The hover card's content: title, when, the start of the description, the first
/// participants, and a Join button.
pub(super) fn card(shown: &Shown, cx: &App) -> Div {
    let theme = cx.theme();
    let event = &shown.event;
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
                .map(|join| join_button("event-card-join", join).w_full()),
        )
}

/// The Event page's header: a breadcrumb back to the Calendar page, then a button for each of
/// the description, the participants, and joining. The description and the participants open in
/// boxes that close when clicked away from.
pub(super) fn toolbar(
    event: &Event,
    open_calendar: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
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
                            .map(|participant| participant_row(participant, cx)),
                    )
                    .into_any_element()
            },
        )
    });
    h_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .child(
            // Presses stay with the breadcrumb, so they never drag or zoom the window.
            div()
                .min_w_0()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    Breadcrumb::new()
                        .text_base()
                        .child(
                            BreadcrumbItem::new("Calendar")
                                .flex_shrink_0()
                                .on_click(open_calendar),
                        )
                        .child(
                            BreadcrumbItem::new(title(event))
                                .min_w_0()
                                .truncate()
                                .font_semibold(),
                        ),
                ),
        )
        // Empty header space still drags and zooms the window.
        .child(div().flex_1())
        .child(
            h_flex()
                .flex_shrink_0()
                .gap_1()
                .children(description)
                .children(participants)
                .children(
                    event
                        .join
                        .as_ref()
                        .map(|join| join_button("event-page-join", join).small()),
                ),
        )
        .text_color(cx.theme().foreground)
}

#[cfg(test)]
mod tests {
    use super::{parse_links, toolbar};

    /// What `text` shows, and each link as its shown text and where it goes.
    fn links(text: &str) -> (String, Vec<(String, String)>) {
        let (shown, links) = parse_links(text);
        let links = links
            .into_iter()
            .map(|(range, url)| (shown[range].to_owned(), url))
            .collect();
        (shown, links)
    }

    #[test]
    fn bare_links_are_found_without_the_punctuation_around_them() {
        let text = "Notes: https://docs.example.com/a?b=1. Deck (http://x.io/d), <https://y.it>";
        let (shown, found) = links(text);
        assert_eq!(shown, text);
        let found: Vec<&str> = found.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(
            found,
            [
                "https://docs.example.com/a?b=1",
                "http://x.io/d",
                "https://y.it"
            ]
        );
        // A bare scheme or a word that only starts like one is no link.
        assert!(links("see https:// or httpbin and ftp://x").1.is_empty());
    }

    #[test]
    fn labelled_links_show_their_label_and_go_to_their_address() {
        let (shown, found) =
            links("📆 [Notes on Fellow](https://fellow.link/Q8), è https://è.it/π");
        assert_eq!(shown, "📆 Notes on Fellow, è https://è.it/π");
        assert_eq!(
            found,
            [
                ("Notes on Fellow".into(), "https://fellow.link/Q8".into()),
                ("https://è.it/π".into(), "https://è.it/π".into()),
            ]
        );
        // Brackets that are not a link stay as they are.
        let plain = "[draft](see notes) and [x](ftp://y)";
        assert_eq!(links(plain), (plain.to_owned(), Vec::new()));
    }
    use chrono::NaiveDate;
    use crumble_eventkit::{Event, Participant, Response, When};
    use gpui_kit::base::TestSupportExt;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Render,
        Styled, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
    };

    /// Stands in for the window header holding an Event page's toolbar.
    struct Header(Event);

    impl Render for Header {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("header")
                .test_support()
                .size_full()
                .flex()
                .items_start()
                .child(toolbar(&self.0, |_, _, _| {}, cx))
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
