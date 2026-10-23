//! The Transcript tab: the job's progress, why it failed, an offer to transcribe a Recording
//! no job took, or the Transcript and who made it.

use super::super::NativeView;
use super::super::controls::button;
use super::super::settings_page::SettingsSection;
use super::recording::clock;
use super::transcript::{Failure, Stage};
use crumble_notes::Track;
use crumble_remote::{Job, Provider};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};

/// The width of the time column, so the columns after it line up whatever the time's digits.
const TIME_WIDTH: f32 = 44.0;
/// The width of the "You" and "Others" column.
const SPEAKER_WIDTH: f32 = 52.0;

impl NativeView {
    /// The Transcript tab: the job's progress, why it failed, an offer to transcribe a
    /// Recording no job took, or the Transcript and who made it, each line with its time, which
    /// seeks the player, and who spoke.
    pub(super) fn render_transcript(&self, cx: &mut Context<Self>) -> AnyElement {
        let detail = &self.note_detail;
        let Some(note) = detail.note.as_ref() else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let transcription = &detail.transcription;
        if let Some(job) = transcription.job(&note.id) {
            let (text, fraction) = match &job.stage {
                Stage::Starting => ("Transcribing…".to_owned(), None),
                Stage::Downloading { language, fraction } => (
                    format!(
                        "Downloading the speech model for {language}… {}",
                        percent(*fraction)
                    ),
                    Some(*fraction),
                ),
                Stage::Transcribing(fraction) => (
                    format!("Transcribing… {}", percent(*fraction)),
                    Some(*fraction),
                ),
            };
            return v_flex()
                .gap_2()
                .text_sm()
                .text_color(muted)
                .child(text)
                .child(
                    Progress::new("transcript-progress")
                        .small()
                        .loading(fraction.is_none())
                        .value(fraction.unwrap_or(0.0) * 100.0),
                )
                .into_any_element();
        }
        if transcription.busy(&note.id) {
            return div()
                .text_sm()
                .text_color(muted)
                .child("Waiting to transcribe…")
                .into_any_element();
        }
        if let Some(failure) = transcription.failed.get(&note.id) {
            if matches!(failure, Failure::Unsupported { .. }) {
                return div()
                    .text_sm()
                    .text_color(muted)
                    .child(failure.message())
                    .into_any_element();
            }
            let (id, provider) = (note.id.clone(), failure.provider());
            return line(
                failure.message(),
                button("transcript-retry")
                    .outline()
                    .small()
                    .label("Try again")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.transcribe(id.clone(), Some(provider), Some(window), cx)
                    })),
                muted,
            );
        }
        let Some(made) = &note.transcript else {
            // A finished Recording no job took: one stopped by quitting, one the user did not
            // agree to send, or one whose Transcript was never asked for.
            let provider = self.transcription_provider();
            if provider == Provider::Local && !self.ready_for(provider, Job::Transcription) {
                return line(
                    format!(
                        "Not transcribed yet. {} isn't downloaded.",
                        self.local_model_name()
                    ),
                    button("transcript-local-models")
                        .outline()
                        .small()
                        .label("Open Settings")
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.open_settings(SettingsSection::LocalModels, cx)
                        })),
                    muted,
                );
            }
            if !self.ready_for(provider, Job::Transcription) {
                return line(
                    format!(
                        "Not transcribed yet. Finish setting up {} in Settings.",
                        provider.name()
                    ),
                    button("transcript-settings")
                        .outline()
                        .small()
                        .label("Open Settings")
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.open_settings(SettingsSection::Providers, cx)
                        })),
                    muted,
                );
            }
            let id = note.id.clone();
            return line(
                "Not transcribed yet".to_owned(),
                button("transcript-start")
                    .outline()
                    .small()
                    .label(format!(
                        "Transcribe with {}",
                        self.transcriber_name(provider)
                    ))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.transcribe(id.clone(), Some(provider), Some(window), cx)
                    })),
                muted,
            );
        };
        let lines = detail
            .transcript
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                let start = segment.start;
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(
                        div().w(px(TIME_WIDTH)).flex_shrink_0().child(
                            button(("transcript-time", index))
                                .link()
                                .xsmall()
                                .label(clock(start))
                                .accessibility_label(format!("Play from {}", clock(start)))
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.seek(start, window, cx)
                                })),
                        ),
                    )
                    .child(
                        div()
                            .w(px(SPEAKER_WIDTH))
                            .flex_shrink_0()
                            .text_color(muted)
                            .child(match segment.track {
                                Track::Mic => "You",
                                Track::System => "Others",
                            }),
                    )
                    .child(div().flex_1().min_w_0().child(segment.text.clone()))
                    .into_any_element()
            });
        // ponytail: every line renders at once; a list that renders only what shows, once
        // hour-long Transcripts feel slow.
        v_flex()
            .gap_2()
            .text_sm()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(transcribed_by(&made.provider, &made.model)),
            )
            .children(
                detail
                    .transcript
                    .is_empty()
                    .then(|| div().text_color(muted).child("No speech was heard.")),
            )
            .children(lines)
            .into_any_element()
    }
}

/// A line of muted text with an action at its end.
fn line(message: String, action: impl IntoElement, muted: gpui_kit::Hsla) -> AnyElement {
    h_flex()
        .gap_2()
        .text_sm()
        .text_color(muted)
        .child(div().flex_1().min_w_0().child(message))
        .child(action)
        .into_any_element()
}

/// "Transcribed by Apple, on this Mac", or by a remote Provider and its Model.
fn transcribed_by(provider: &str, model: &str) -> String {
    match Provider::from_id(provider) {
        Some(Provider::Apple) => "Transcribed by Apple, on this Mac".to_owned(),
        Some(provider) => format!("Transcribed by {} · {model}", provider.name()),
        None => format!("Transcribed by {provider} · {model}"),
    }
}

/// `42%`.
pub(super) fn percent(fraction: f32) -> String {
    format!("{:.0}%", (fraction * 100.0).clamp(0.0, 100.0))
}

#[cfg(test)]
mod tests {
    use super::{percent, transcribed_by};

    #[test]
    fn transcribed_by_names_the_provider_and_a_remote_model() {
        assert_eq!(
            transcribed_by("apple", "Apple SpeechAnalyzer"),
            "Transcribed by Apple, on this Mac"
        );
        assert_eq!(
            transcribed_by("openai-compatible", "whisper-1"),
            "Transcribed by OpenAI-compatible server · whisper-1"
        );
        assert_eq!(
            transcribed_by("openai", "gpt-transcribe"),
            "Transcribed by OpenAI · gpt-transcribe"
        );
    }

    #[test]
    fn percent_rounds_and_stays_in_range() {
        assert_eq!(percent(0.0), "0%");
        assert_eq!(percent(0.424), "42%");
        assert_eq!(percent(1.2), "100%");
    }
}
