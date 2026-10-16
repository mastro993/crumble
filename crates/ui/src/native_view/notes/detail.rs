//! The Note detail view, shared by the Agenda content area and the Event page: the Note's title
//! big at the top with when it happened below, then a card holding the tab bar, the Recording
//! controls on its trailing side, and the tab: the Summary or the Transcript once there is one,
//! or the Memo.

use super::super::NativeView;
use super::super::controls::{app_icon, button};
use super::tabs::NoteTab;
use super::{Subject, event_title};
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
        let (border, muted, danger) = {
            let theme = cx.theme();
            (theme.border, theme.muted_foreground, theme.danger)
        };
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
            let title = match subject {
                Subject::Event(event) => event_title(event),
                Subject::QuickNote(_) => "Quick note".to_owned(),
            };
            div()
                .text_2xl()
                .font_semibold()
                .child(title)
                .into_any_element()
        };
        let body = if detail.note.as_ref().is_some() {
            let (tabs, shown) = detail.tabs();
            let tab = match shown {
                NoteTab::Memo => Textarea::new(&detail.memo)
                    .appearance(false)
                    .into_any_element(),
                NoteTab::Summary => self.render_summary(cx),
                NoteTab::Transcript => self.render_transcript(cx),
            };
            v_flex()
                .child(
                    h_flex()
                        .px_3()
                        .gap_2()
                        .border_b_1()
                        .border_color(border)
                        .child(
                            TabBar::new("note-tabs")
                                .underline()
                                .small()
                                .selected_index(
                                    tabs.iter().position(|tab| *tab == shown).unwrap_or(0),
                                )
                                .on_click(cx.listener(move |view, index: &usize, _, cx| {
                                    let (tabs, _) = view.note_detail.tabs();
                                    if let Some(tab) = tabs.get(*index) {
                                        view.note_detail.tab = Some(*tab);
                                        cx.notify();
                                    }
                                }))
                                .children(
                                    tabs.iter()
                                        .map(|tab| Tab::new().label(tab.label()).cursor_pointer()),
                                ),
                        )
                        .child(div().flex_1())
                        .children(self.render_recording_controls(cx)),
                )
                .child(div().p_3().child(tab))
                .into_any_element()
        } else {
            v_flex()
                .items_center()
                .gap_3()
                .py_10()
                .text_sm()
                .text_color(muted)
                .child("No note yet")
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            button("note-empty-new")
                                .outline()
                                .small()
                                .icon(app_icon("note-add").size_4())
                                .label("New note")
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.new_note(false, window, cx)
                                })),
                        )
                        .child(self.render_empty_record_button(cx)),
                )
                .into_any_element()
        };
        v_flex()
            .w_full()
            .max_w(px(MAX_WIDTH))
            .gap_4()
            .pt_2()
            .child(
                v_flex()
                    .gap_1()
                    .child(title)
                    .child(div().text_sm().text_color(muted).child(when)),
            )
            .child(
                div()
                    .border_1()
                    .border_color(border)
                    .rounded(px(SURFACE_RADIUS))
                    .overflow_hidden()
                    .child(body),
            )
            .children(self.render_recording_hints(cx))
            .children(
                detail
                    .save_failed
                    .then(|| div().text_xs().text_color(danger).child("Couldn't save")),
            )
            .children(
                detail
                    .recording_failed
                    .then(|| div().text_xs().text_color(danger).child("Couldn't record")),
            )
            .into_any_element()
    }
}
