//! The Recording's controls: Record, the running Recording with Stop, the player, the hints
//! under the card when a track stays silent, and the header pill.

use super::super::super::NativeView;
use super::super::super::controls::{Inactive, app_icon, button};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex};
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};
use std::time::Duration;

/// How long a track may stay silent before the hint shows.
const SILENCE_HINT_AFTER: Duration = Duration::from_secs(10);
/// The scrubber's width.
const SCRUBBER_WIDTH: f32 = 140.0;
/// System Settings › Privacy & Security › Screen & System Audio Recording.
const SYSTEM_AUDIO_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
/// System Settings › Privacy & Security › Microphone.
const MICROPHONE_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";

impl NativeView {
    /// The card header's trailing side: Record, the running Recording with Stop, or the player.
    pub(in super::super) fn render_recording_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let detail = &self.note_detail;
        let note = detail.note.as_ref()?;
        let theme = cx.theme();
        if let Some(active) = detail
            .recording
            .as_ref()
            .filter(|active| active.target.note == note.id)
        {
            let elapsed = active.recorder.status().elapsed;
            return Some(
                h_flex()
                    .gap_2()
                    .text_sm()
                    .child(red_dot(cx))
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(clock(elapsed)),
                    )
                    .child(
                        button("recording-stop")
                            .ghost()
                            .small()
                            .icon(app_icon("stop").size_4())
                            .label("Stop")
                            .on_click(cx.listener(|view, _, window, cx| {
                                if let Some(id) = view.stop_recording(cx) {
                                    view.transcribe(id, None, Some(window), cx);
                                }
                            })),
                    )
                    .into_any_element(),
            );
        }
        if let Some(playback) = detail
            .playback
            .as_ref()
            .filter(|playback| playback.note == note.id)
        {
            let (playing, position, length) = (
                playback.player.is_playing(),
                playback.player.position(),
                playback.player.duration(),
            );
            return Some(
                h_flex()
                    .gap_2()
                    .text_sm()
                    .child(
                        button("recording-play")
                            .ghost()
                            .small()
                            .icon(app_icon(if playing { "pause" } else { "play" }).size_4())
                            .accessibility_label(if playing { "Pause" } else { "Play" })
                            .on_click(
                                cx.listener(|view, _, window, cx| view.toggle_playback(window, cx)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(SCRUBBER_WIDTH))
                            .child(Slider::new(&playback.scrubber)),
                    )
                    .child(div().text_color(theme.muted_foreground).child(format!(
                        "{} / {}",
                        clock(position),
                        clock(length)
                    )))
                    .into_any_element(),
            );
        }
        if note.recording.is_some() {
            return None;
        }
        Some(record_button("recording-record", self.recording_elsewhere(), cx).into_any_element())
    }

    /// The name of the Note being recorded, when one is.
    fn recording_elsewhere(&self) -> Option<String> {
        self.note_detail
            .recording
            .as_ref()
            .map(|active| active.target.name.clone())
    }

    /// The empty state's Record button: makes a Note for the Event and records it at once.
    pub(in super::super) fn render_empty_record_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        record_button("note-empty-record", self.recording_elsewhere(), cx)
            .outline()
            .on_click(cx.listener(|view, _, window, cx| view.record_new_note(window, cx)))
            .into_any_element()
    }

    /// Under the card, while the shown Note records: a line for each track that has stayed
    /// silent since the start, with a button to the System Settings pane that allows it.
    pub(in super::super) fn render_recording_hints(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let detail = &self.note_detail;
        let Some(active) = detail.recording.as_ref().filter(|active| {
            detail
                .note
                .as_ref()
                .is_some_and(|note| note.id == active.target.note)
        }) else {
            return Vec::new();
        };
        let status = active.recorder.status();
        if status.elapsed < SILENCE_HINT_AFTER {
            return Vec::new();
        }
        let hint = |id: &'static str, text: &'static str, url: &'static str| {
            h_flex()
                .w_full()
                .gap_2()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(div().flex_1().min_w_0().child(text))
                .child(
                    button(id)
                        .flex_shrink_0()
                        .link()
                        .small()
                        .label("Open System Settings")
                        .on_click(move |_, _, cx| cx.open_url(url)),
                )
                .into_any_element()
        };
        let mut hints = Vec::new();
        if status.system_silent {
            hints.push(hint(
                "recording-hint-system",
                "No sound from the Mac yet. If a call is playing, allow Crumble in System Settings › \
                 Privacy & Security › Screen & System Audio Recording.",
                SYSTEM_AUDIO_SETTINGS,
            ));
        }
        if status.mic_silent {
            hints.push(hint(
                "recording-hint-mic",
                "No sound from the microphone yet. Allow Crumble in System Settings › Privacy & \
                 Security › Microphone.",
                MICROPHONE_SETTINGS,
            ));
        }
        hints
    }

    /// The header's "● Recording · 12:03" pill while a Recording runs; it opens that Note.
    pub(in crate::native_view) fn render_recording_pill(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let active = self.note_detail.recording.as_ref()?;
        let elapsed = active.recorder.status().elapsed;
        Some(
            button("recording-pill")
                .ghost()
                .small()
                .tooltip(format!("Open “{}”", active.target.name))
                .child(
                    h_flex()
                        .gap_1p5()
                        .child(red_dot(cx))
                        .child(format!("Recording · {}", clock(elapsed))),
                )
                .on_click(cx.listener(|view, _, _, cx| view.open_recording(cx)))
                .into_any_element(),
        )
    }
}

/// Record, or Record disabled with the recording Note's name when `elsewhere` is one.
fn record_button(
    id: &'static str,
    elsewhere: Option<String>,
    cx: &mut Context<NativeView>,
) -> gpui_kit::component::button::Button {
    let button = button(id)
        .ghost()
        .small()
        .icon(app_icon("mic").size_4())
        .label("Record")
        .inactive(elsewhere.is_some());
    match elsewhere {
        Some(name) => button.tooltip(format!("Recording “{name}”")),
        None => button.on_click(cx.listener(|view, _, _, cx| view.record(cx))),
    }
}

fn red_dot(cx: &Context<NativeView>) -> impl IntoElement {
    div()
        .size(px(8.0))
        .flex_shrink_0()
        .rounded_full()
        .bg(cx.theme().danger)
}

/// `0:07`, `12:03`, or `1:02:03`.
pub(in super::super) fn clock(time: Duration) -> String {
    let seconds = time.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::clock;
    use std::time::Duration;

    #[test]
    fn clock_shows_minutes_and_seconds_then_hours() {
        assert_eq!(clock(Duration::ZERO), "0:00");
        assert_eq!(clock(Duration::from_millis(7_900)), "0:07");
        assert_eq!(clock(Duration::from_secs(12 * 60 + 3)), "12:03");
        assert_eq!(clock(Duration::from_secs(3600 + 2 * 60 + 3)), "1:02:03");
    }
}
