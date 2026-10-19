//! One row of the Agenda list: a day header, the Now line, an Event, which shows its progress
//! when it marks now, a Quick note, or the end.

use super::super::keyboard::focusable;
use super::super::sidebar_row::hover_fill;
use chrono::{DateTime, Local};
use crumble_calendar::agenda::{Row, day_label, progress, time_label, time_left};
use crumble_eventkit::Event;
use crumble_theme::CONTENT_PADDING;
use crumble_theme::sidebar::{RADIUS, ROW_PADDING};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ElementId, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px, relative,
};
use std::collections::HashMap;
use std::rc::Rc;

/// Thickness of the Now line, and the size of the dot that starts it.
const NOW_LINE: f32 = 1.5;
const NOW_DOT: f32 = 7.0;
/// Width of the bar in an Event's color at a row's leading edge.
const BAR: f32 = 3.0;
/// Strength of the accent tinting the card of the Event that marks now, and filling the part
/// of it that has passed.
const CURRENT_TINT: f32 = 0.04;
const CURRENT_FILL: f32 = 0.08;
/// Space between an Event's card and the column's edges.
const CARD_INSET: f32 = CONTENT_PADDING - ROW_PADDING;

/// Where a row sits in the Agenda list: `alone` says it holds nothing else; `now` is the time
/// the Event marking now shows its progress at.
pub(super) struct RowItem {
    pub(super) alone: bool,
    pub(super) selected: bool,
    pub(super) now: DateTime<Local>,
}

type SelectRow = Rc<dyn Fn(&Row, &mut Window, &mut App)>;

/// What the card of the Event marking now adds: how much of it has passed, and what is left.
struct Current {
    progress: f32,
    left: Option<String>,
}

/// One item of the Agenda list; Events and Quick notes select on click, and on Enter or Space
/// once Tab reaches them.
pub(super) fn render_row(
    row: &Row,
    item: RowItem,
    colors: &HashMap<String, Hsla>,
    select: SelectRow,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let RowItem {
        alone,
        selected,
        now,
    } = item;
    let selectable =
        |id: String, color: Hsla, title: String, time: String, current: Option<Current>| {
            let (row, select) = (row.clone(), select.clone());
            let hover = hover_fill(cx);
            let accent = theme.primary;
            div().px(px(CARD_INSET)).child(
                focusable(
                    h_flex().id(ElementId::Name(id.into())).test_support(),
                    title.clone(),
                )
                .relative()
                .cursor_pointer()
                .px(px(ROW_PADDING))
                .py_1p5()
                .gap_3()
                .items_stretch()
                .rounded(px(RADIUS))
                .when(current.is_some() && !selected, |row| {
                    row.bg(accent.opacity(CURRENT_TINT))
                })
                .when(selected, |row| row.bg(theme.sidebar_accent))
                .when(!selected, |row| row.hover(move |style| style.bg(hover)))
                .on_click(move |_, window, cx| select(&row, window, cx))
                // The part of the Event that has passed fills the card from its leading edge.
                .when_some(current.as_ref(), |row, current| {
                    row.child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .w(relative(current.progress))
                            .rounded(px(RADIUS))
                            .bg(accent.opacity(CURRENT_FILL)),
                    )
                })
                .child(div().flex_shrink_0().w(px(BAR)).rounded_full().bg(color))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().truncate().child(title))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(time),
                        ),
                )
                .children(current.and_then(|current| current.left).map(|left| {
                    div()
                        .flex_shrink_0()
                        .self_center()
                        .text_xs()
                        .font_medium()
                        .text_color(accent)
                        .child(left)
                })),
            )
        };
    let event_row = |event: &Event, current: Option<Current>| {
        let color = colors
            .get(&event.calendar_id)
            .copied()
            .unwrap_or(theme.muted_foreground);
        let title = if event.title.is_empty() {
            "(No title)".to_owned()
        } else {
            event.title.clone()
        };
        selectable(
            format!("agenda-event-{}", event.id),
            color,
            title,
            time_label(&event.when),
            current,
        )
    };
    let item = div().px(px(CONTENT_PADDING));
    match row {
        Row::Day(day) => item
            .pt_4()
            .pb_1()
            .text_sm()
            .font_semibold()
            .child(day_label(*day, now.date_naive()))
            .into_any_element(),
        Row::Now => now_line(cx).into_any_element(),
        Row::Event(event) => event_row(event, None).into_any_element(),
        Row::Current(event) => event_row(
            event,
            Some(Current {
                progress: progress(event, now),
                left: time_left(event, now),
            }),
        )
        .into_any_element(),
        // No Account color: a Quick note belongs to none.
        Row::QuickNote(note) => selectable(
            format!("agenda-note-{}", note.id),
            theme.muted_foreground.opacity(0.5),
            note.label.clone(),
            note.at.format("%H:%M").to_string(),
            None,
        )
        .into_any_element(),
        Row::End => item
            .py_4()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(if alone {
                "No past Events"
            } else {
                "End of Agenda"
            })
            .into_any_element(),
    }
}

/// The Now line: a thin line in the accent color across the whole column, starting from a dot
/// level with the rows' bars.
fn now_line(cx: &App) -> impl IntoElement {
    let accent = cx.theme().primary;
    div()
        .relative()
        .w_full()
        .h(px(NOW_DOT))
        .my_1()
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px((NOW_DOT - NOW_LINE) / 2.0))
                .h(px(NOW_LINE))
                .bg(accent),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left(px(CARD_INSET + ROW_PADDING + (BAR - NOW_DOT) / 2.0))
                .size(px(NOW_DOT))
                .rounded_full()
                .bg(accent),
        )
}

#[cfg(test)]
mod tests {
    use super::{RowItem, render_row};
    use chrono::Local;
    use crumble_calendar::agenda::{QuickNote, Row};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, Context, IntoElement, ParentElement, Render, Styled, TestAppContext,
        Window, WindowBounds, WindowOptions, div, px, size,
    };
    use std::collections::HashMap;
    use std::rc::Rc;

    /// Stands in for the Agenda list, counting the rows it is asked to select.
    #[derive(Default)]
    struct List {
        selected: Vec<Row>,
    }

    impl Render for List {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let row = Row::QuickNote(Rc::new(QuickNote {
                id: "kickoff".into(),
                at: Local::now(),
                label: "Kickoff ideas".into(),
            }));
            let item = RowItem {
                alone: true,
                selected: false,
                now: Local::now(),
            };
            let list = cx.entity().downgrade();
            let select = Rc::new(move |row: &Row, _: &mut Window, cx: &mut gpui_kit::App| {
                let _ = list.update(cx, |list, _| list.selected.push(row.clone()));
            });
            div()
                .size_full()
                .child(render_row(&row, item, &HashMap::new(), select, cx))
        }
    }

    #[gpui_kit::test]
    fn a_quick_note_row_selects_on_click_enter_and_space(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (handle, list) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            Default::default(),
                            size(px(320.), px(80.)),
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |_, cx| cx.new(|_| List::default()),
                )
            })
            .unwrap_or_else(|error| panic!("open test window: {error}"));

        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            // The click also focuses the row, which the keyboard then selects.
            window.click("agenda-note-kickoff", cx);
            window.press("enter", cx);
            window.press("space", cx);
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));

        cx.update(|cx| {
            let selected = &list.read(cx).selected;
            assert_eq!(selected.len(), 3);
            assert!(matches!(&selected[0], Row::QuickNote(note) if note.id == "kickoff"));
        });
    }
}
