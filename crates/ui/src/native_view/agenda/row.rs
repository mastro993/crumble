//! One row of the Agenda list: a day header, the Now line, an Event, which shows its progress
//! when it marks now, a Quick note, or the end.

use super::super::keyboard::focusable;
use super::super::sidebar::surface;
use super::super::sidebar_row::hover_fill;
use super::now_hover::NowHover;
use chrono::{DateTime, Local};
use crumble_calendar::agenda::{Row, day_label, progress, time_label, time_left};
use crumble_eventkit::Event;
use crumble_theme::CONTENT_PADDING;
use crumble_theme::sidebar::{RADIUS, ROW_PADDING};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Bounds, ElementId, FontFeatures, Hsla, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Window, canvas, div, fill, hsla, point,
    prelude::FluentBuilder, px, relative, size,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

/// Thickness of the Now line, and the height of the strip around it the pointer hovers to show
/// the time.
const NOW_LINE: f32 = 2.0;
const NOW_ROW: f32 = 16.0;
/// Space between the Now line and the rows above and below it, and between the time and the line.
const NOW_SPACE: f32 = 3.0;
/// How far below its place the time starts as it enters and ends as it leaves.
const NOW_TIME_SLIDE: f32 = 4.0;
/// Length of each of the Now line's rounded dashes, and the space between them.
const NOW_DASH: f32 = 6.0;
const NOW_DASH_GAP: f32 = 4.0;
/// Strength of the accent in the time above the Now line, and in the line, which is fainter.
const NOW_OPACITY: f32 = 0.5;
const NOW_LINE_OPACITY: f32 = 0.3;
/// Width of the bar in an Event's color at a row's leading edge.
const BAR: f32 = 3.0;
/// The soft tan filling the part of the Event marking now that has passed: the accent's hue,
/// saturated enough to read brown rather than grey, a shade below the card in lightness, and a
/// step deeper over a selected row so it still shows. Dark mode mirrors the lightness.
const PASSED_SATURATION: f32 = 0.45;
const PASSED_LIGHTNESS: f32 = 0.885;
const PASSED_SELECTED_LIGHTNESS: f32 = 0.845;
/// Strength of the accent in the time left on that card, quieter than the title.
const TIME_LEFT_OPACITY: f32 = 0.5;
/// Space between an Event's card and the column's edges.
const CARD_INSET: f32 = CONTENT_PADDING - ROW_PADDING;

/// Where a row sits in the Agenda list: `alone` says it holds nothing else; `now` is the time
/// the Event marking now shows its progress at and the Now line shows; `now_hover` says whether
/// the pointer is on the Now line.
pub(super) struct RowItem {
    pub(super) alone: bool,
    pub(super) selected: bool,
    pub(super) now: DateTime<Local>,
    pub(super) now_hover: Rc<Cell<NowHover>>,
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
        now_hover,
    } = item;
    let selectable =
        |id: String, color: Hsla, title: String, time: String, current: Option<Current>| {
            let (row, select) = (row.clone(), select.clone());
            let hover = hover_fill(cx);
            let accent = theme.primary;
            let lightness = if selected {
                PASSED_SELECTED_LIGHTNESS
            } else {
                PASSED_LIGHTNESS
            };
            let lightness = if theme.mode.is_dark() {
                1.0 - lightness
            } else {
                lightness
            };
            let passed = hsla(accent.h, PASSED_SATURATION, lightness, 1.0);
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
                // The Event marking now sits on the sidebar color, a shade lighter than a
                // selected row's fill.
                .when(current.is_some() && !selected, |row| row.bg(theme.sidebar))
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
                            .bg(passed),
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
                        .text_color(accent.opacity(TIME_LEFT_OPACITY))
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
        Row::Now => now_line(now, now_hover, cx).into_any_element(),
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

/// The Now line: a thin dashed line in the accent color across the column's cards, between two
/// rows with a little space on either side. Under the pointer the current time, to the second,
/// slides up and fades in above the line's trailing end, and slides down and fades out after.
fn now_line(now: DateTime<Local>, hover: Rc<Cell<NowHover>>, cx: &App) -> impl IntoElement {
    // Mixed into the column's surface, so the time and the line are solid colors.
    let (surface, primary) = (surface(cx), cx.theme().primary);
    let time = surface.blend(primary.opacity(NOW_OPACITY));
    let line = surface.blend(primary.opacity(NOW_LINE_OPACITY));
    let height = NOW_LINE + 2.0 * NOW_SPACE;
    let shown = if cx.reduce_motion() {
        f32::from(u8::from(hover.get().over()))
    } else {
        hover.get().shown(Instant::now())
    };
    // Over the bottom of the row above, at the trailing end its time leaves empty.
    let label = (shown > 0.0).then(|| {
        div()
            .absolute()
            .bottom(px(height - NOW_SPACE - NOW_TIME_SLIDE * (1.0 - shown)))
            .right(px(CARD_INSET))
            .opacity(shown)
            .text_xs()
            .font_medium()
            // Tabular figures: every digit is as wide, so the time holds still as it ticks.
            .font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)])))
            .text_color(time)
            .child(now.format("%H:%M:%S").to_string())
    });
    // The row holds just the line and its space; the strip the pointer hovers spills over the
    // rows around it.
    div()
        .relative()
        .w_full()
        .h(px(height))
        .child(
            div()
                .id("agenda-now")
                .absolute()
                .left_0()
                .right_0()
                .top(px((height - NOW_ROW) / 2.0))
                .h(px(NOW_ROW))
                .on_hover(move |over, window, _| {
                    let mut now_hover = hover.get();
                    now_hover.set(*over, Instant::now());
                    hover.set(now_hover);
                    window.refresh();
                }),
        )
        .child(
            // Inset as the Event cards are, so its ends line up with theirs.
            div()
                .absolute()
                .left(px(CARD_INSET))
                .right(px(CARD_INSET))
                .top(px(NOW_SPACE))
                .h(px(NOW_LINE))
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| dashes(bounds, line, window),
                    )
                    .size_full(),
                ),
        )
        .children(label)
}

/// Paints rounded dashes of `color` along `bounds`, the last one cut short at its end.
fn dashes(bounds: Bounds<gpui_kit::Pixels>, color: Hsla, window: &mut Window) {
    let radius = bounds.size.height / 2.0;
    let mut x = bounds.origin.x;
    while x < bounds.right() {
        let width = px(NOW_DASH).min(bounds.right() - x);
        let dash = Bounds::new(point(x, bounds.origin.y), size(width, bounds.size.height));
        window.paint_quad(fill(dash, color).corner_radii(radius));
        x += px(NOW_DASH + NOW_DASH_GAP);
    }
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
                now_hover: Rc::default(),
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
