//! One row of the Agenda list: a day header, the Now line, an Event, which shows its progress
//! when it marks now, a Quick note, or the end.

use super::super::keyboard::focusable;
use super::now_hover::NowHover;
use chrono::{DateTime, Local};
use crumble_calendar::agenda::{Row, day_label, progress, time_label, time_left};
use crumble_eventkit::Event;
use crumble_theme::CONTENT_PADDING;
use crumble_theme::sidebar::{RADIUS, ROW_PADDING};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Bounds, ElementId, FontFeatures, Hsla, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Point, StatefulInteractiveElement, Styled,
    Window, canvas, deferred, div, fill, hsla, point, prelude::FluentBuilder, px, relative, size,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

/// Thickness of the Now line, and the height of the strip around it the pointer hovers to show
/// the time.
const NOW_LINE: f32 = 2.0;
/// Diameter of the dot that starts the Now line, as on the Calendar's current time line.
const NOW_DOT: f32 = 8.0;
const NOW_ROW: f32 = 16.0;
/// Length of each rounded dash of the Now line, and the gap between two.
const NOW_DASH: f32 = 6.0;
const NOW_GAP: f32 = 4.0;
/// Strength of the red in the Now line, quieter than the time's chip.
const NOW_LINE_OPACITY: f32 = 0.5;
/// Space between the Now line and the rows above and below it.
const NOW_SPACE: f32 = 3.0;
/// Height of the Now line's row: the line and its space.
const NOW_HEIGHT: f32 = NOW_LINE + 2.0 * NOW_SPACE;
/// Space between the column's edge and the time beside the Now line.
const NOW_TIME_GAP: f32 = 6.0;
/// How far left of its place the time starts as it enters, and right of it it ends as it leaves.
const NOW_TIME_SLIDE: f32 = 4.0;
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

/// Where a row sits in the Agenda list: `alone` says it lists no Event or Quick note; `menu_open` says
/// its context menu is open; `now` is the time the Event marking now shows its progress at and
/// the Now line shows; `now_hover` says whether the pointer is on the Now line.
pub(super) struct RowItem {
    pub(super) alone: bool,
    pub(super) selected: bool,
    pub(super) menu_open: bool,
    pub(super) now: DateTime<Local>,
    pub(super) now_hover: Rc<Cell<NowHover>>,
}

type SelectRow = Rc<dyn Fn(&Row, &mut Window, &mut App)>;
type OpenRowMenu = Rc<dyn Fn(&Row, Point<Pixels>, &mut Window, &mut App)>;

/// The element id of an Event's or a Quick note's row, which also owns its context menu.
pub(super) fn row_id(row: &Row) -> Option<String> {
    match row {
        Row::Event(event) | Row::Current(event) => Some(format!("agenda-event-{}", event.id)),
        Row::QuickNote(note) => Some(format!("agenda-note-{}", note.id)),
        _ => None,
    }
}

/// What the card of the Event marking now adds: how much of it has passed, and what is left.
struct Current {
    progress: f32,
    left: Option<String>,
}

/// One item of the Agenda list; Events and Quick notes select on click, and on Enter or Space
/// once Tab reaches them, and open their context menu on a right click.
pub(super) fn render_row(
    row: &Row,
    item: RowItem,
    colors: &HashMap<String, Hsla>,
    select: SelectRow,
    menu: OpenRowMenu,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let RowItem {
        alone,
        selected,
        menu_open,
        now,
        now_hover,
    } = item;
    let id = row_id(row).unwrap_or_default();
    let selectable = |color: Hsla, title: String, time: String, current: Option<Current>| {
        let (row, select, menu) = (row.clone(), select.clone(), menu.clone());
        let menu_row = row.clone();
        // The secondary fill, in light mode a step stronger than a sidebar row's: half of a
        // sidebar row's would match the Event marking now, which sits on the sidebar color.
        let hover = theme.secondary.opacity(0.5);
        // Over the Event marking now it replaces the card's sidebar color, so mix it in.
        let hover = if current.is_some() {
            theme.sidebar.blend(hover)
        } else {
            hover
        };
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
                h_flex()
                    .id(ElementId::Name(id.clone().into()))
                    .test_support(),
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
            .when(selected, |row| row.bg(theme.secondary))
            // The hover fill stays while the row's context menu is open.
            .when(menu_open && !selected, |row| row.bg(hover))
            .when(!selected, |row| row.hover(move |style| style.bg(hover)))
            .on_click(move |_, window, cx| select(&row, window, cx))
            .on_mouse_down(
                MouseButton::Right,
                move |event: &MouseDownEvent, window, cx| {
                    menu(&menu_row, event.position, window, cx)
                },
            )
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
        selectable(color, title, time_label(&event.when), current)
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
        Row::NothingLeft => item
            .pb_1()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child("No more events today")
            .into_any_element(),
        Row::Current(event) => event_row(
            event,
            Some(Current {
                progress: progress(event, now),
                left: time_left(event, now),
            }),
        )
        .into_any_element(),
        Row::Event(event) => event_row(event, None).into_any_element(),
        // No Account color: a Quick note belongs to none.
        Row::QuickNote(note) => selectable(
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
                "No notes yet"
            } else {
                "End of Agenda"
            })
            .into_any_element(),
    }
}

/// The Now line: a thin, faint red line of rounded dashes across the whole column, between two
/// rows with a little space on either side. Under the pointer the current time, to the second,
/// in a red chip, slides in from the left and fades in beside the line's trailing end, outside
/// the column, and slides on to the right and fades out after.
fn now_line(now: DateTime<Local>, hover: Rc<Cell<NowHover>>, cx: &App) -> impl IntoElement {
    let red = cx.theme().danger;
    let line = red.opacity(NOW_LINE_OPACITY);
    let over = hover.get().over();
    let shown = if cx.reduce_motion() {
        f32::from(u8::from(over))
    } else {
        hover.get().shown(Instant::now())
    };
    // Enters from the left and leaves to the right.
    let slide = NOW_TIME_SLIDE * (1.0 - shown) * if over { -1.0 } else { 1.0 };
    // Beside the line's trailing end, past the column's edge.
    let label = (shown > 0.0).then(|| {
        div()
            .absolute()
            .left(relative(1.0))
            .ml(px(NOW_TIME_GAP + slide))
            .top(px((NOW_HEIGHT - NOW_ROW) / 2.0))
            // Drawn last and unclipped, as the time sits outside the list.
            .child(deferred(
                div().h(px(NOW_ROW)).flex().items_center().child(
                    Tag::danger()
                        .small()
                        .rounded_full()
                        .opacity(shown)
                        .font_medium()
                        // Tabular figures: every digit is as wide, so the time holds still as
                        // it ticks.
                        .font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)])))
                        .child(now.format("%H:%M:%S").to_string()),
                ),
            ))
    });
    // The row holds just the line and its space; the strip the pointer hovers spills over the
    // rows around it.
    div()
        .relative()
        .w_full()
        .h(px(NOW_HEIGHT))
        .child(
            div()
                .id("agenda-now")
                .absolute()
                .left_0()
                .right_0()
                .top(px((NOW_HEIGHT - NOW_ROW) / 2.0))
                .h(px(NOW_ROW))
                .on_hover(track_now_hover(hover)),
        )
        .child(
            // Starts where the day headers and the Event color bars do and runs on to the column's
            // trailing edge.
            div()
                .absolute()
                .left(px(CONTENT_PADDING))
                .right_0()
                .top(px(NOW_SPACE))
                .h(px(NOW_LINE))
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            let mut x = bounds.origin.x;
                            while x < bounds.right() {
                                let width = px(NOW_DASH).min(bounds.right() - x);
                                window.paint_quad(
                                    fill(
                                        Bounds::new(
                                            point(x, bounds.origin.y),
                                            size(width, bounds.size.height),
                                        ),
                                        line,
                                    )
                                    .corner_radii(px(NOW_LINE / 2.0)),
                                );
                                x += px(NOW_DASH + NOW_GAP);
                            }
                        },
                    )
                    .size_full(),
                ),
        )
        .child(
            div()
                .absolute()
                .left(px(CONTENT_PADDING - NOW_DOT / 2.0))
                .top(px(NOW_SPACE + (NOW_LINE - NOW_DOT) / 2.0))
                .size(px(NOW_DOT))
                .rounded_full()
                .bg(red),
        )
        .children(label)
}

/// Records the pointer coming onto or leaving what shows the Now line's time.
fn track_now_hover(hover: Rc<Cell<NowHover>>) -> impl Fn(&bool, &mut Window, &mut App) {
    move |over, window, _| {
        let mut now_hover = hover.get();
        now_hover.set(*over, Instant::now());
        hover.set(now_hover);
        window.refresh();
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

    /// Stands in for the Agenda list, counting the rows it is asked to select, and those it is
    /// asked to open the context menu of.
    #[derive(Default)]
    struct List {
        selected: Vec<Row>,
        menus: Vec<Row>,
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
                menu_open: false,
                now: Local::now(),
                now_hover: Rc::default(),
            };
            let list = cx.entity().downgrade();
            let menus = list.clone();
            let select = Rc::new(move |row: &Row, _: &mut Window, cx: &mut gpui_kit::App| {
                let _ = list.update(cx, |list, _| list.selected.push(row.clone()));
            });
            let menu = Rc::new(
                move |row: &Row, _, _: &mut Window, cx: &mut gpui_kit::App| {
                    let _ = menus.update(cx, |list, _| list.menus.push(row.clone()));
                },
            );
            div()
                .size_full()
                .child(render_row(&row, item, &HashMap::new(), select, menu, cx))
        }
    }

    #[gpui_kit::test]
    fn a_quick_note_row_selects_on_click_enter_and_space_and_opens_its_menu_on_right_click(
        cx: &mut TestAppContext,
    ) {
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
            window.right_click("agenda-note-kickoff", cx);
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));

        cx.update(|cx| {
            let selected = &list.read(cx).selected;
            assert_eq!(selected.len(), 3);
            assert!(matches!(&selected[0], Row::QuickNote(note) if note.id == "kickoff"));
            // A right click opens its context menu and does not select it.
            let menus = &list.read(cx).menus;
            assert!(matches!(menus.as_slice(), [Row::QuickNote(note)] if note.id == "kickoff"));
        });
    }
}
