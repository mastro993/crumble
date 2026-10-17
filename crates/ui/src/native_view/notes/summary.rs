//! A Note's Summary in the detail view: the background job that makes one from each new
//! Transcript, one Note at a time, and the Summary tab that shows it. The Model runs off the main
//! thread; its Summary is stored through `Notes` on the main thread when it lands.

use super::super::NativeView;
use super::super::controls::button;
use super::super::route::Route;
use chrono::Utc;
use crumble_notes::{NoteId, Track};
use crumble_store::secrets;
use crumble_summarize::{
    Apple, Availability, Claude, OpenAiCompatible, Provider, Speaker, Summarizer,
    apple_availability, runs_locally, transcript_text,
};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::{AnyElement, Context, ElementId, IntoElement, ParentElement, Styled, Task, div};
use std::collections::{HashMap, VecDeque};

/// The Summaries being made, for whichever Note shows.
#[derive(Default)]
pub(super) struct Summarization {
    /// Notes waiting their turn, first come first, each with the Provider asked for, if any.
    queue: VecDeque<(NoteId, Option<Provider>)>,
    running: Option<Job>,
    /// Why each Note's last attempt failed, until it is tried again.
    failed: HashMap<NoteId, Failure>,
}

impl Summarization {
    /// Whether the Note `id` is waiting, being summarized, or failed: its Summary tab shows.
    pub(super) fn involves(&self, id: &NoteId) -> bool {
        self.busy(id) || self.failed.contains_key(id)
    }

    /// Whether the Note `id` is waiting or being summarized.
    pub(super) fn busy(&self, id: &NoteId) -> bool {
        self.job(id).is_some() || self.queue.iter().any(|(queued, _)| queued == id)
    }

    /// The job running for the Note `id`, unless it was cancelled.
    fn job(&self, id: &NoteId) -> Option<&Job> {
        self.running
            .as_ref()
            .filter(|job| &job.note == id && !job.cancelled)
    }
}

struct Job {
    note: NoteId,
    provider: Provider,
    model: String,
    /// How far it is, once the Model said.
    fraction: Option<f32>,
    /// Whether its Note no longer wants it. A Model's answer cannot be stopped, so a cancelled
    /// job still holds the slot until it ends, and its result is dropped.
    cancelled: bool,
    /// Receives the job's updates; dropping it drops the job's result.
    _updates: Task<()>,
}

/// Why the Model failed, for the user, and the Provider to try again with.
struct Failure {
    message: String,
    provider: Option<Provider>,
}

/// What the job sends back to the main thread.
enum Update {
    Progress(f32),
    Done(anyhow::Result<String>),
}

impl NativeView {
    /// Queues the older Notes whose Transcript has no Summary, as when the app quit while
    /// summarizing or no Summary Model was picked yet, when the Summary Model keeps the text on
    /// the Mac. A remote one gets an older Transcript only when the user asks, from its Note's
    /// Summary tab. Run at launch and when a Summary Model is picked.
    pub(in super::super) fn summarize_unsummarized(&mut self, cx: &mut Context<Self>) {
        let Some(provider) = self.summary_provider().filter(|&provider| {
            runs_locally(provider, self.store.openai_base_url()) && self.is_configured(provider)
        }) else {
            return;
        };
        // Each keeps this Provider, so a remote one picked while they wait never gets them.
        match self.notes.unsummarized() {
            Ok(ids) => ids
                .into_iter()
                .for_each(|id| self.summarize(id, Some(provider), cx)),
            Err(error) => eprintln!("Could not find the Transcripts to summarize: {error:#}"),
        }
    }

    /// The Summary Model's Provider picked in Settings, else Apple's when it can run.
    pub(in super::super) fn summary_provider(&self) -> Option<Provider> {
        self.store
            .summary_provider(apple_availability() == Availability::Available)
            .and_then(Provider::from_id)
    }

    /// Whether `provider` has what it needs to summarize: Apple's model can run, or the remote
    /// Provider has its key, address and Model.
    pub(in super::super) fn is_configured(&self, provider: Provider) -> bool {
        match provider {
            Provider::Apple => apple_availability() == Availability::Available,
            Provider::Claude => {
                self.models.has_key(provider) && !self.store.claude_model().is_empty()
            }
            Provider::OpenAiCompatible => {
                !self.store.openai_base_url().is_empty() && !self.store.openai_model().is_empty()
            }
        }
    }

    /// Queues the Note `id` to be summarized by `provider`, else by the Summary Model picked in
    /// Settings, unless it already waits or runs.
    pub(super) fn summarize(
        &mut self,
        id: NoteId,
        provider: Option<Provider>,
        cx: &mut Context<Self>,
    ) {
        let summarization = &mut self.note_detail.summarization;
        if summarization.busy(&id) {
            return;
        }
        summarization.failed.remove(&id);
        summarization.queue.push_back((id, provider));
        self.next_summary(cx);
        cx.notify();
    }

    /// Forgets the Note `id`'s Summary work, as when its Transcript changes or goes: a job
    /// running for it finishes unseen and its result is dropped.
    pub(super) fn cancel_summary(&mut self, id: &NoteId) {
        let summarization = &mut self.note_detail.summarization;
        summarization.queue.retain(|(queued, _)| queued != id);
        summarization.failed.remove(id);
        if let Some(job) = summarization.running.as_mut().filter(|job| &job.note == id) {
            job.cancelled = true;
        }
    }

    /// Starts the first queued job that can run, unless one runs.
    fn next_summary(&mut self, cx: &mut Context<Self>) {
        if self.note_detail.summarization.running.is_some() {
            return;
        }
        while let Some((id, asked)) = self.note_detail.summarization.queue.pop_front() {
            let text = match self.transcript_text(&id) {
                Ok(text) if text.is_empty() => continue,
                Ok(text) => text,
                Err(error) => {
                    eprintln!("Could not read the Transcript to summarize: {error:#}");
                    continue;
                }
            };
            let Some(provider) = asked.or_else(|| self.summary_provider()) else {
                // The tab says to pick one; picking it summarizes the Note when that keeps the
                // text on the Mac, and offers to otherwise.
                continue;
            };
            let summarizer = match self.summarizer(provider) {
                Ok(summarizer) => summarizer,
                Err(message) => {
                    self.summary_failed(
                        id,
                        Failure {
                            message,
                            provider: asked,
                        },
                    );
                    continue;
                }
            };
            let (provider, model) = (summarizer.provider(), summarizer.model().to_owned());
            let (send, updates) = async_channel::unbounded();
            let work = cx.background_executor().spawn(async move {
                let result = summarizer.summarize(&text, &mut |fraction| {
                    let _ = send.try_send(Update::Progress(fraction));
                });
                let _ = send.try_send(Update::Done(result));
            });
            let note = id.clone();
            let updates = cx.spawn(async move |view, cx| {
                let _work = work;
                while let Ok(update) = updates.recv().await {
                    if view
                        .update(cx, |view, cx| view.summary_update(&note, asked, update, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            });
            self.note_detail.summarization.running = Some(Job {
                note: id,
                provider,
                model,
                fraction: None,
                cancelled: false,
                _updates: updates,
            });
            return;
        }
    }

    fn summary_failed(&mut self, id: NoteId, failure: Failure) {
        self.note_detail.summarization.failed.insert(id, failure);
    }

    /// The Note `id`'s Transcript as the Model reads it.
    fn transcript_text(&mut self, id: &NoteId) -> anyhow::Result<String> {
        let segments = self.notes.transcript(id)?;
        Ok(transcript_text(segments.iter().map(|segment| {
            let speaker = match segment.track {
                Track::Mic => Speaker::You,
                Track::System => Speaker::Others,
            };
            (speaker, segment.text.as_str())
        })))
    }

    /// The Summarizer for `provider`, with the Settings picks and its key from the Keychain; or
    /// why there is none, for the user.
    fn summarizer(&self, provider: Provider) -> Result<Box<dyn Summarizer + Send>, String> {
        let key = || {
            secrets::get(provider.id()).map_err(|error| {
                eprintln!("Could not read the API key: {error:#}");
                "Couldn't read the API key from the Keychain.".to_owned()
            })
        };
        Ok(match provider {
            Provider::Apple => match apple_availability().reason() {
                Some(reason) => return Err(format!("Apple isn't available: {reason}")),
                None => Box::new(Apple),
            },
            Provider::Claude => Box::new(Claude {
                api_key: key()?.ok_or("Add a Claude API key in Settings.")?,
                model: self.store.claude_model().to_owned(),
            }),
            Provider::OpenAiCompatible => Box::new(OpenAiCompatible {
                base_url: self.store.openai_base_url().to_owned(),
                api_key: key()?,
                model: self.store.openai_model().to_owned(),
            }),
        })
    }

    fn summary_update(
        &mut self,
        id: &NoteId,
        asked: Option<Provider>,
        update: Update,
        cx: &mut Context<Self>,
    ) {
        let summarization = &mut self.note_detail.summarization;
        let Some(job) = summarization.running.as_mut().filter(|job| &job.note == id) else {
            return;
        };
        match update {
            Update::Progress(fraction) => job.fraction = Some(fraction),
            Update::Done(result) => {
                let Some(job) = summarization.running.take() else {
                    return;
                };
                match result {
                    _ if job.cancelled => {}
                    Ok(text) => self.store_summary(id, job.provider, &job.model, &text),
                    Err(error) => {
                        eprintln!("Could not summarize the Transcript: {error:#}");
                        self.summary_failed(
                            id.clone(),
                            Failure {
                                message: error.to_string(),
                                provider: asked,
                            },
                        );
                    }
                }
                self.next_summary(cx);
            }
        }
        cx.notify();
    }

    /// Keeps `text` as the Note `id`'s Summary, and shows it if the Note shows.
    fn store_summary(&mut self, id: &NoteId, provider: Provider, model: &str, text: &str) {
        match self
            .notes
            .set_summary(id, provider.id(), model, text, Utc::now())
        {
            Ok(info) => self.note_changed(id, |note| note.summary = Some(info.clone())),
            // As when the Transcript or its Note went while the job ran.
            Err(error) => eprintln!("Could not keep the Summary: {error:#}"),
        }
    }

    /// The Summary tab: the job's progress, why it failed, an offer to summarize a Transcript no
    /// job took, or the Summary and who made it.
    pub(super) fn render_summary(&self, cx: &mut Context<Self>) -> AnyElement {
        let detail = &self.note_detail;
        let Some(note) = detail.note.as_ref() else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let summarization = &detail.summarization;
        if let Some(job) = summarization.job(&note.id) {
            let text = match job.fraction {
                Some(fraction) => format!("Summarizing… {}", super::transcript::percent(fraction)),
                None => "Summarizing…".to_owned(),
            };
            return v_flex()
                .gap_2()
                .text_sm()
                .text_color(muted)
                .child(text)
                .child(
                    Progress::new("summary-progress")
                        .small()
                        .loading(job.fraction.is_none())
                        .value(job.fraction.unwrap_or(0.0) * 100.0),
                )
                .into_any_element();
        }
        if summarization.busy(&note.id) {
            return div()
                .text_sm()
                .text_color(muted)
                .child("Waiting to summarize…")
                .into_any_element();
        }
        let no_model = || {
            (
                "No Summary Model picked. Choose one in Settings.".to_owned(),
                button("summary-settings")
                    .outline()
                    .small()
                    .label("Open Settings")
                    .on_click(cx.listener(|view, _, _, cx| view.navigate(Route::Settings, cx))),
            )
        };
        let id = note.id.clone();
        // A failed attempt shows above the Summary the Note still has, if any. A Transcript no
        // job took, as one older than a remote Summary Model, waits for the user to ask.
        let line = match summarization.failed.get(&note.id) {
            Some(Failure { message, provider }) => {
                let provider = *provider;
                Some((
                    message.clone(),
                    button("summary-retry")
                        .outline()
                        .small()
                        .label("Try again")
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.summarize(id.clone(), provider, cx)
                        })),
                ))
            }
            None if note.summary.is_none() && detail.spoken() => {
                Some(match self.summary_provider() {
                    Some(provider) => (
                        "Not summarized yet".to_owned(),
                        button("summary-start")
                            .outline()
                            .small()
                            .label(format!("Summarize with {}", provider.name()))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.summarize(id.clone(), Some(provider), cx)
                            })),
                    ),
                    None => no_model(),
                })
            }
            None => None,
        };
        let line = line.map(|(message, action)| {
            h_flex()
                .gap_2()
                .text_color(muted)
                .child(div().flex_1().min_w_0().child(message))
                .child(action)
        });
        let summary = note.summary.as_ref().map(|summary| {
            let provider = Provider::from_id(&summary.provider)
                .map_or(summary.provider.clone(), |provider| {
                    provider.name().to_owned()
                });
            v_flex()
                .gap_3()
                .child(
                    TextView::markdown(
                        ElementId::Name(format!("summary-{}", note.id).into()),
                        summary.text.clone(),
                    )
                    .selectable(true),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(format!("Summarized by {provider} · {}", summary.model)),
                )
        });
        v_flex()
            .gap_3()
            .text_sm()
            .children(line)
            .children(summary)
            .into_any_element()
    }
}
