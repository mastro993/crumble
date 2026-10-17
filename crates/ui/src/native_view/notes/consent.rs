//! Which Transcription Model makes a Transcript, and the one check a Recording's audio passes
//! before it leaves the Mac: a remote Model gets it only once the user agreed to send audio to
//! that Provider, Model and host, which `Store` remembers (ADR 0008).

use super::super::NativeView;
use crumble_notes::NoteId;
use crumble_remote::{Provider, base_url, host, runs_locally};
use crumble_store::secrets;
use crumble_transcribe::{Apple, OpenAiCompatible, Transcriber};
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::{Context, Window};

/// The Providers that transcribe, in the order Settings and the Note actions menu list them.
pub(in super::super) const TRANSCRIBERS: [Provider; 2] =
    [Provider::Apple, Provider::OpenAiCompatible];

/// What the user agrees to before audio goes to a remote Model.
pub(super) struct Consent {
    provider: Provider,
    model: String,
    /// The server's host in lowercase, as the consent is kept.
    host: String,
}

impl NativeView {
    /// The Transcription Model's Provider picked in Settings: Apple's until another is.
    pub(in super::super) fn transcription_provider(&self) -> Provider {
        Provider::from_id(self.store.transcription_provider()).unwrap_or(Provider::Apple)
    }

    /// The locale Transcripts are made in: the one picked in Settings, else the Mac's.
    pub(super) fn transcription_locale(&self) -> String {
        self.store
            .transcription_locale()
            .map_or_else(crumble_transcribe::current_locale, str::to_owned)
    }

    /// Whether `provider` can transcribe with what Settings holds: Apple always offers to, and
    /// the OpenAI-compatible server needs an address requests can go to and a Transcription
    /// Model, as its Transcriber checks before it uploads.
    pub(in super::super) fn can_transcribe(&self, provider: Provider) -> bool {
        match provider {
            Provider::Apple => true,
            Provider::Claude => false,
            Provider::OpenAiCompatible => {
                base_url(self.store.openai_base_url()).is_ok()
                    && !self.store.transcription_model().is_empty()
            }
        }
    }

    /// What the user must agree to before `provider` gets a Recording's audio; `None` when it
    /// runs on the Mac, has no address requests can go to, or the user already agreed for its
    /// Model and host.
    pub(super) fn consent_needed(&self, provider: Provider) -> Option<Consent> {
        let url = self.store.openai_base_url();
        if runs_locally(provider, url) {
            return None;
        }
        let consent = Consent {
            provider,
            model: self.store.transcription_model().to_owned(),
            host: host(url)?,
        };
        let agreed = self
            .store
            .has_audio_consent(provider.id(), &consent.model, &consent.host);
        (!agreed).then_some(consent)
    }

    /// Asks whether to send the Note `id`'s Recording to `consent`'s Model. Send remembers the
    /// answer and transcribes; Don't send leaves the Recording untranscribed, with its
    /// Transcript tab offering to transcribe it.
    pub(super) fn ask_consent(
        &mut self,
        id: NoteId,
        consent: Consent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let Consent {
            provider,
            model,
            host,
        } = consent;
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (view, id, model, host) = (view.clone(), id.clone(), model.clone(), host.clone());
            alert
                .title(format!("Send this Recording to {}?", provider.name()))
                .description(format!(
                    "Crumble will upload the audio of this Recording to {host} to make its \
                     Transcript. You'll only be asked once for {model} on {host}."
                ))
                .show_cancel(true)
                .cancel_text("Don't send")
                .ok_text("Send")
                .ok_variant(ButtonVariant::Primary)
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.store.add_audio_consent(provider.id(), &model, &host);
                        view.queue_transcription(id.clone(), provider, cx);
                    });
                    true
                })
        });
    }

    /// The Transcriber for `provider`, with the Settings picks and the server's key from the
    /// Keychain; or why there is none, for the user.
    pub(super) fn transcriber(
        &self,
        provider: Provider,
    ) -> Result<Box<dyn Transcriber + Send>, String> {
        Ok(match provider {
            Provider::Apple => Box::new(Apple),
            Provider::OpenAiCompatible => Box::new(OpenAiCompatible {
                base_url: self.store.openai_base_url().to_owned(),
                api_key: secrets::get(provider.id()).map_err(|error| {
                    eprintln!("Could not read the API key: {error:#}");
                    "Couldn't read the API key from the Keychain.".to_owned()
                })?,
                model: self.store.transcription_model().to_owned(),
            }),
            Provider::Claude => return Err("Claude can't transcribe Recordings.".to_owned()),
        })
    }
}
