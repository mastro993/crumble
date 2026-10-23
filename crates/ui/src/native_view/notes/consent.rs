//! Which Transcription Model makes a Transcript, and the one check a Recording's audio passes
//! before it leaves the Mac: a remote Model gets it only once the user agreed to send audio to
//! that Provider, Model and host, which `Store` remembers (ADR 0008).

use super::super::NativeView;
use super::super::providers::saved_key;
use crumble_notes::NoteId;
use crumble_remote::{Job, Provider, host, runs_locally};
use crumble_transcribe::{Apple, Local, Transcriber};
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::{Context, Window};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// What the user agrees to before audio goes to a remote Model.
pub(super) struct Consent {
    provider: Provider,
    model: String,
    /// The host of the Provider's address in lowercase, as the consent is kept.
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

    /// What the user must agree to before `provider` gets a Recording's audio; `None` when its
    /// address is on the Mac, it has no address requests can go to or no Transcription Model,
    /// or the user already agreed for that Model and host.
    pub(super) fn consent_needed(&self, provider: Provider) -> Option<Consent> {
        let address = self.provider_address(provider);
        if runs_locally(provider, &address) {
            return None;
        }
        let consent = Consent {
            provider,
            model: self.provider_model(provider, Job::Transcription)?,
            host: host(&address)?,
        };
        let agreed = self
            .store
            .has_audio_consent(provider.id(), &consent.model, &consent.host);
        (!agreed).then_some(consent)
    }

    /// Asks whether to send the Recordings of the Notes `ids` to `consent`'s Model. Send
    /// remembers the answer and transcribes them; Don't send leaves them untranscribed, with
    /// their Transcript tabs offering to transcribe them.
    pub(super) fn ask_consent(
        &mut self,
        ids: Vec<NoteId>,
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
        let (title, these) = match ids.len() {
            1 => (
                format!("Send this Recording to {}?", provider.name()),
                "this Recording".to_owned(),
            ),
            count => (
                format!("Send {count} Recordings to {}?", provider.name()),
                format!("these {count} Recordings"),
            ),
        };
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (view, ids, model, host) = (view.clone(), ids.clone(), model.clone(), host.clone());
            alert
                .entrance(DialogEntrance::Fade)
                .title(title.clone())
                .description(format!(
                    "Crumble will upload the audio of {these} to {host} to make their \
                     Transcripts. You'll only be asked once for {model} on {host}."
                ))
                .show_cancel(true)
                .cancel_text("Don't send")
                .ok_text("Send")
                .ok_variant(ButtonVariant::Primary)
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.store.add_audio_consent(provider.id(), &model, &host);
                        for id in &ids {
                            view.queue_transcription(id.clone(), provider, cx);
                        }
                    });
                    true
                })
        });
    }

    /// The Transcriber for `provider`, with the Settings picks and a remote Provider's key from
    /// the Keychain; or why there is none, for the user.
    /// `cancel` stops a Local Model's worker once set.
    pub(super) fn transcriber(
        &self,
        provider: Provider,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Box<dyn Transcriber + Send>, String> {
        Ok(match provider {
            Provider::Apple => Box::new(Apple),
            Provider::Local => {
                let (library, downloaded) = self
                    .local_model_on_disk()
                    .ok_or_else(|| format!("{} isn't downloaded.", self.local_model_name()))?;
                Box::new(Local {
                    model: library.path(&downloaded.model),
                    vad: library.vad(),
                    name: downloaded.model.name,
                    cancel: cancel.clone(),
                })
            }
            _ => Box::new(crumble_transcribe::Remote {
                provider,
                base_url: self.provider_address(provider),
                api_key: saved_key(provider)?,
                model: (self.provider_model(provider, Job::Transcription)).unwrap_or_default(),
            }),
        })
    }

    /// What `provider` is called where the user picks it to transcribe: the picked Local
    /// Model's name for a Local Model.
    pub(in super::super) fn transcriber_name(&self, provider: Provider) -> String {
        match provider {
            Provider::Local => self.local_model_name(),
            _ => provider.name().to_owned(),
        }
    }
}
