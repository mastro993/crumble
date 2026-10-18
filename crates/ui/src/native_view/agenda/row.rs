//! One row of the Agenda list: a day header, the Now line, an Event, a Quick note, or the end.

use super::super::sidebar_row::hover_fill;
use chrono::NaiveDate;
use crumble_calendar::agenda::{Row, day_label, time_label};
use crumble_theme::CONTENT_PADDING;
use crumble_theme::sidebar::{RADIUS, ROW_PADDING};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ElementId, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use std::collections::HashMap;
use std::rc::Rc;

/// Thickness of the Now line, and the size of the dot that starts it.
const NOW_LINE: f32 = 2.0;
const NOW_DOT: f32 = 8.0;

/// Where a row sits in the Agenda list: `first` opens it, `alone` says it holds nothing else.
pub(super) struct RowItem {
    pub(super) first: bool,
    pub(super) alone: bool,
    pub(super) selected: bool,
    pub(super) today: NaiveDate,
}

type SelectRow = Rc<dyn Fn(&Row, &mut Window, &mut App)>;

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
        first,
        alone,
        selected,
        today,
    } = item;
    let selectable = |id: String, color: Hsla, title: String, time: String| {
        let row = row.clone();
        let hover = hover_fill(cx);
        div()
            .px(px(CONTENT_PADDING - ROW_PADDING))
            .child(
                h_flex()
                    .id(ElementId::Name(id.into()))
                    .test_support()
                    .tab_index(0)
                    .aria_label(title.clone())
                    .cursor_pointer()
                    .px(px(ROW_PADDING))
                    .py_1p5()
                    .gap_3()
                    .items_stretch()
                    .rounded(px(RADIUS))
                    .when(selected, |row| row.bg(theme.sidebar_accent))
                    .when(!selected, |row| row.hover(move |style| style.bg(hover)))
                    .on_click(move |_, window, cx| select(&row, window, cx))
                    .child(div().flex_shrink_0().w(px(3.0)).rounded_full().bg(color))
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
                    ),
            )
            .into_any_element()
    };
    let item = div().px(px(CONTENT_PADDING));
    match row {
        Row::Day(day) => item
            // The header strip already spaces the list from the window's top.
            .map(|item| if first { item.pt_1() } else { item.pt_4() })
            .pb_1()
            .text_sm()
            .font_semibold()
            .child(day_label(*day, today))
            .into_any_element(),
        Row::Now => h_flex()
            .w_full()
            .py_1()
            // The dot hangs into the padding so the line starts level with the Events.
            .pl(px(CONTENT_PADDING - NOW_DOT / 2.0))
            .pr(px(CONTENT_PADDING))
            .child(
                div()
                    .flex_shrink_0()
                    .size(px(NOW_DOT))
                    .rounded_full()
                    .bg(theme.danger),
            )
            .child(div().flex_1().h(px(NOW_LINE)).bg(theme.danger))
            .into_any_element(),
        Row::Event(event) => {
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
            )
        }
        // No Account color: a Quick note belongs to none.
        Row::QuickNote(note) => selectable(
            format!("agenda-note-{}", note.id),
            theme.muted_foreground.opacity(0.5),
            note.label.clone(),
            note.at.format("%H:%M").to_string(),
        ),
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
                first: true,
                alone: true,
                selected: false,
                today: Local::now().date_naive(),
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
