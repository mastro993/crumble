//! The Agenda page on the window: keeping it current, its header, and the list beside the Note.

use super::super::NativeView;
use super::super::account_dot::tint;
use super::super::controls::{app_icon, button};
use super::super::route::Route;
use super::row::{RowItem, render_row};
use chrono::{Local, NaiveDate, Timelike, Utc};
use crumble_calendar::agenda::{self, QuickNote, Row, horizon};
use crumble_calendar::{local_midnight, today};
use crumble_eventkit::Access;
use crumble_notes::{Note, Notes};
use crumble_theme::{CONTENT_PADDING, SURFACE_RADIUS};
use gpui_kit::component::{ActiveTheme, Sizable, button::ButtonVariants, h_flex};
use gpui_kit::{
    AnyElement, App, Context, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, linear_color_stop, linear_gradient, list,
    prelude::FluentBuilder, px,
};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The list reads further back until it holds this many rows, enough to fill a tall window.
const MIN_ROWS: usize = 40;
/// Height of the fade that rows scroll away under at the list's top.
const TOP_FADE: f32 = 32.0;

impl NativeView {
    /// Keeps the Agenda current while it shows: reads it the first time, again after the day,
    /// the shown Calendars, or EventKit change, and further back near the end of the list;
    /// rebuilds the rows each minute so Events join as they start.
    pub(in super::super) fn maintain_agenda(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.agenda.step_scroll_to_top(window, cx);
        let sync = self.calendar_sync.read(cx);
        if sync.access() != Access::Granted {
            // The rows are gone, so is their selection.
            self.agenda.selected = None;
            return;
        }
        let ids = self.store.shown_calendar_ids(sync.calendars());
        let today = today();
        let horizon = horizon(today);
        let notes = &mut self.notes;
        let agenda = &mut self.agenda;
        let wants_more = agenda.rows.len() < MIN_ROWS || agenda.near_end.get();
        agenda.events.update(cx, |events, cx| {
            events.read_next(today, ids, wants_more, cx);
        });

        let loaded = agenda.events.read(cx);
        let now = Local::now();
        let minute = now
            .with_second(0)
            .and_then(|now| now.with_nanosecond(0))
            .unwrap_or(now);
        if agenda.built != Some((minute, loaded.reads())) {
            agenda.built = Some((minute, loaded.reads()));
            let from = loaded.span().map_or(today, |span| span.from);
            let end = !loaded.reading() && from <= horizon;
            if let Some(quick_notes) = read_quick_notes(notes, from, today) {
                agenda.quick_notes = quick_notes;
            }
            let quick_notes: Vec<Rc<QuickNote>> = agenda
                .quick_notes
                .iter()
                .map(|note| {
                    Rc::new(QuickNote {
                        id: note.id.to_string(),
                        at: note.listed_at().with_timezone(&Local),
                        label: quick_note_label(note),
                    })
                })
                .collect();
            let rows = agenda::rows(loaded.events(), &quick_notes, now, from, end);
            agenda.set_rows(rows);
            agenda.reselect();
        }

        if !agenda.ticking {
            agenda.ticking = true;
            cx.spawn(async move |this, cx| {
                loop {
                    // Wakes on the minute, when Events start and end.
                    let wait = 60 - u64::from(Local::now().second());
                    cx.background_executor()
                        .timer(Duration::from_secs(wait))
                        .await;
                    let shown = this.update(cx, |view, cx| {
                        let shown = view.route == Route::Agenda;
                        if shown {
                            cx.notify();
                        } else {
                            view.agenda.ticking = false;
                        }
                        shown
                    });
                    if !matches!(shown, Ok(true)) {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    /// Selects the row of `row`, showing its Note beside the list.
    fn select_agenda_row(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) {
        self.agenda.select(row);
        self.maintain_note_detail(window, cx);
        cx.notify();
    }

    /// The Agenda page's header: New note, then the selected row's Note actions. Without
    /// calendar access the list shows no rows, so New note is left out.
    pub(in super::super) fn render_agenda_toolbar(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let granted = self.calendar_sync.read(cx).access() == Access::Granted;
        h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .when(granted, |toolbar| {
                toolbar.child(
                    button("agenda-new-note")
                        .ghost()
                        .small()
                        .icon(app_icon("note-add").size_4())
                        .tooltip("New note")
                        .on_click(
                            cx.listener(|view, _, window, cx| view.new_note(true, window, cx)),
                        ),
                )
            })
            // Empty header space still drags and zooms the window.
            .child(div().flex_1())
            .children(
                self.agenda
                    .selected
                    .is_some()
                    .then(|| self.render_note_actions(cx))
                    .flatten(),
            )
    }

    /// The Agenda list, the page's second-level sidebar, beside an empty content area, with a
    /// button back to the top once the list is scrolled.
    pub(in super::super) fn render_agenda(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let sync = self.calendar_sync.read(cx);
        // One device pixel, matching the panel's hairline.
        let hairline = px(1.0 / window.scale_factor());
        let column = div()
            .relative()
            .size_full()
            // Over the panel's rounded corner, which does not clip its children.
            .rounded_tl(px(SURFACE_RADIUS))
            .bg(surface(cx))
            .border_r(hairline)
            .border_color(theme.border);
        let column = if sync.access() == Access::Granted {
            let colors: Rc<HashMap<String, Hsla>> = Rc::new(
                sync.calendars()
                    .iter()
                    .map(|calendar| {
                        let color = self.store.account_color(&calendar.account_id);
                        (calendar.id.clone(), tint(color, cx))
                    })
                    .collect(),
            );
            let rows = self.agenda.rows.clone();
            let selected = rows.iter().position(|row| self.agenda.is_selected(row));
            let view = cx.entity().downgrade();
            let today = today();
            column
                .child(
                    list(self.agenda.list.clone(), move |index, _, cx| {
                        let Some(row) = rows.get(index) else {
                            return div().into_any_element();
                        };
                        let item = RowItem {
                            first: index == 0,
                            alone: rows.len() == 1,
                            selected: selected == Some(index),
                            today,
                        };
                        let view = view.clone();
                        let select =
                            Rc::new(move |row: &Row, window: &mut Window, cx: &mut App| {
                                let _ = view
                                    .update(cx, |view, cx| view.select_agenda_row(row, window, cx));
                            });
                        render_row(row, item, &colors, select, cx)
                    })
                    .size_full(),
                )
                .child({
                    // Rows fade away under the list's top; the fade grows in over its first
                    // scrolled points so the top row isn't dimmed at rest.
                    let surface = surface(cx);
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        // Clear of the column's hairline.
                        .right(hairline)
                        .h(self.agenda.scroll_offset().min(px(TOP_FADE)))
                        .bg(linear_gradient(
                            180.0,
                            linear_color_stop(surface, 0.0),
                            linear_color_stop(surface.opacity(0.0), 1.0),
                        ))
                })
                .when(self.agenda.scroll_offset() > px(0.0), |column| {
                    column.child(
                        h_flex()
                            .absolute()
                            .top_3()
                            .left_0()
                            .right_0()
                            .justify_center()
                            .child(
                                button("agenda-top")
                                    .outline()
                                    .small()
                                    .rounded(px(999.0))
                                    .shadow_md()
                                    .icon(app_icon("scroll-to-top"))
                                    .accessibility_label("Scroll to top")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        let agenda = &view.agenda;
                                        agenda
                                            .scrolling_up
                                            .set(Some((Instant::now(), agenda.scroll_offset())));
                                        cx.notify();
                                    })),
                            ),
                    )
                })
        } else {
            column.child(
                div()
                    .p(px(CONTENT_PADDING))
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("Connect calendars from the Calendars button in the sidebar to see past Events."),
            )
        };
        let content = self.agenda.selected.is_some().then(|| {
            div()
                .id("agenda-content")
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_y_scroll()
                .px(px(CONTENT_PADDING))
                .pb_6()
                .child(self.render_note_detail(cx))
        });
        h_flex()
            .size_full()
            .child(self.render_sidebar(column, cx))
            .children(content)
            .into_any_element()
    }
}

/// The Quick notes listed from `from` through `today`, newest first; `None` when they cannot be
/// read, which is logged, so the listed ones stay.
fn read_quick_notes(notes: &mut Notes, from: NaiveDate, today: NaiveDate) -> Option<Vec<Note>> {
    let start = |day: NaiveDate| local_midnight(day).map(|start| start.with_timezone(&Utc));
    let (Some(from), Some(to)) = (start(from), today.succ_opt().and_then(start)) else {
        return Some(Vec::new());
    };
    notes
        .quick_notes(from..to)
        .inspect_err(|error| eprintln!("Could not read the Quick notes: {error}"))
        .ok()
}

/// A Quick note's title, or `Quick note` when it has none.
fn quick_note_label(note: &Note) -> String {
    note.title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "Quick note".to_owned())
}

/// The Agenda column's opaque surface: half the sidebar over the page, so this second-level
/// column sits between them.
fn surface(cx: &App) -> Hsla {
    let theme = cx.theme();
    theme.background.blend(theme.sidebar.opacity(0.5))
}
