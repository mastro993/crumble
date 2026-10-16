//! The Note detail view, shared by the Agenda content area and the Event page: the Note's title
//! big at the top with when it happened below, then a card holding the tab bar and the tab.

use super::super::NativeView;
use super::super::controls::{app_icon, button};
use super::{Subject, placeholder};
use chrono::Local;
use crumble_calendar::events::when_text;
use crumble_theme::SURFACE_RADIUS;
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};

/// Widest the view grows, so lines of the Memo stay readable.
const MAX_WIDTH: f32 = 720.0;

impl NativeView {
    pub(in super::super) fn render_note_detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let detail = &self.note_detail;
        let theme = cx.theme();
        let Some(subject) = &detail.subject else {
            return div().into_any_element();
        };
        let when = match subject {
            Subject::Event(event) => when_text(&event.when),
            Subject::QuickNote(note) => note
                .listed_at()
                .with_timezone(&Local)
                .format("%a %-d %b · %H:%M")
                .to_string(),
        };
        let title = if detail.note.is_some() {
            Input::new(&detail.title)
                .appearance(false)
                // Level with the line below it.
                .px_0()
                .text_2xl()
                .font_semibold()
                .into_any_element()
        } else {
            div()
                .text_2xl()
                .font_semibold()
                .child(placeholder(subject))
                .into_any_element()
        };
        let body = if detail.note.is_some() {
            v_flex()
                .child(
                    // The trailing side stays free for the Recording controls.
                    h_flex()
                        .px_3()
                        .border_b_1()
                        .border_color(theme.border)
                        .child(
                            TabBar::new("note-tabs")
                                .underline()
                                .small()
                                .selected_index(0)
                                .child(Tab::new().label("Memo")),
                        ),
                )
                .child(
                    div()
                        .p_3()
                        .child(Textarea::new(&detail.memo).appearance(false)),
                )
                .into_any_element()
        } else {
            v_flex()
                .items_center()
                .gap_3()
                .py_10()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("No note yet")
                .child(
                    button("note-empty-new")
                        .outline()
                        .small()
                        .icon(app_icon("note-add").size_4())
                        .label("New note")
                        .on_click(
                            cx.listener(|view, _, window, cx| view.new_note(false, window, cx)),
                        ),
                )
                .into_any_element()
        };
        v_flex()
            .w_full()
            .max_w(px(MAX_WIDTH))
            .gap_4()
            .pt_2()
            .child(
                v_flex().gap_1().child(title).child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(when),
                ),
            )
            .child(
                div()
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(SURFACE_RADIUS))
                    .overflow_hidden()
                    .child(body),
            )
            .children(detail.save_failed.then(|| {
                div()
                    .text_xs()
                    .text_color(theme.danger)
                    .child("Couldn't save")
            }))
            .into_any_element()
    }
}
