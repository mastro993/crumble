//! What Crumble knows about each remote Provider, one file each in `remote/`: where its API is,
//! the key it takes, which jobs its Models do and through which API, the Models it suggests,
//! and how to check a key without spending tokens. Adding a Provider starts with a file here
//! (see `docs/architecture/adding-a-provider.md`).

pub(crate) mod anthropic;
pub(crate) mod deepseek;
pub(crate) mod elevenlabs;
pub(crate) mod gemini;
pub(crate) mod openai;
pub(crate) mod openai_compatible;
pub(crate) mod openrouter;
pub(crate) mod xai;

use serde_json::Value;

use crate::{Job, KeyFormat};

/// A remote Provider: how to reach it and what its Models do. Read it through
/// [`crate::Provider::remote`].
#[derive(Debug)]
pub struct Remote {
    /// The page where the user makes an API key; `None` for a server the user runs.
    pub(crate) key_url: Option<&'static str>,
    /// Its API's address, which the user may replace; `None` when the user must give one.
    pub(crate) base_url: Option<&'static str>,
    /// How the user signs in.
    pub(crate) sign_in: SignIn,
    /// Headers every request carries besides the key, such as Anthropic's API version.
    pub(crate) headers: &'static [(&'static str, &'static str)],
    /// The API its chat Models answer through, for Summaries and Intelligence; `None` when it
    /// has none.
    pub(crate) chat: Option<Chat>,
    /// The API that transcribes; `None` when it doesn't.
    pub(crate) speech: Option<Speech>,
    /// Whether its chat API can stream an answer as it is written. Crumble doesn't stream yet
    /// (ADR 0007); this is what Intelligence will need.
    pub(crate) streams: bool,
    /// How a key is checked and the Models are listed.
    pub(crate) check: Check,
    /// The Models Crumble suggests before the Provider lists its own.
    pub(crate) models: Models,
}

/// How the user signs in to a remote Provider.
#[derive(Debug)]
pub(crate) struct SignIn {
    /// The API key it takes; `None` when it takes none.
    pub(crate) api_key: Option<KeyFormat>,
    /// A subscription it can be used through, signed in with OAuth; `None` while Crumble offers
    /// none for it.
    pub(crate) subscription: Option<Subscription>,
}

/// A subscription sign-in through OAuth. Crumble offers none yet: every Provider takes an API
/// key. One becomes a variant here naming its flow (see `docs/architecture/adding-a-provider.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Subscription {}

/// The API a remote Provider's chat Models answer through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chat {
    /// Anthropic's Messages API: `POST {base}/v1/messages`.
    AnthropicMessages,
    /// OpenAI's chat completions: `POST {base}/chat/completions`, which most APIs also speak.
    OpenAiChat,
    /// Google's `POST {base}/v1beta/models/{model}:generateContent`.
    GeminiGenerateContent,
}

/// The API a remote Provider transcribes through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speech {
    /// OpenAI's `POST {base}/audio/transcriptions`, which Whisper servers also speak.
    OpenAiTranscriptions,
    /// ElevenLabs' `POST {base}/v1/speech-to-text`: timed words.
    ElevenLabsSpeechToText,
    /// xAI's `POST {base}/stt`: timed words.
    XaiSpeechToText,
    /// Google's Files API upload, then `POST {base}/v1beta/interactions` with a transcribe
    /// Model: timed words.
    GeminiInteractions,
}

/// How a remote Provider checks a key and lists its Models, without spending tokens.
#[derive(Debug)]
pub(crate) struct Check {
    /// A `GET` that proves the key, when listing Models doesn't, as their list needs none.
    pub(crate) key: Option<KeyCheckRequest>,
    /// A `GET` that lists the Models; `None` when the Provider has no list.
    pub(crate) models: Option<ModelsRequest>,
}

/// A `GET` that proves a key.
#[derive(Debug)]
pub(crate) struct KeyCheckRequest {
    /// Its path after the base URL.
    pub(crate) path: &'static str,
    /// Why a key the request accepted can't be used, read from its answer, such as a disabled
    /// key.
    pub(crate) unusable: fn(&Value) -> Option<&'static str>,
}

/// A `GET` that lists a remote Provider's Models.
#[derive(Debug)]
pub(crate) struct ModelsRequest {
    /// Its path after the base URL.
    pub(crate) path: &'static str,
    /// Whether it sends the key; OpenRouter's list is public.
    pub(crate) keyed: bool,
    /// Whether a 403 refuses the key itself. A Gemini key has no scopes, so a 403 means its
    /// project can't use the API or Google blocked the key; elsewhere a 403 is a key that may
    /// use Models but not list them.
    pub(crate) forbidden_refuses_key: bool,
    /// The Models in its answer, by job.
    pub(crate) read: fn(&Value) -> Listed,
}

/// The Models a remote Provider lists, by job.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    /// Its chat Models, for Summaries and Intelligence.
    pub chat: Vec<String>,
    /// Its speech Models, for Transcripts.
    pub speech: Vec<String>,
}

/// The Models Crumble suggests for a remote Provider.
#[derive(Debug)]
pub(crate) struct Models {
    /// Chat Models, the default for Summaries first.
    pub(crate) chat: &'static [&'static str],
    /// Speech Models, the default first. Few APIs list theirs, so these are what Settings
    /// offers.
    pub(crate) speech: &'static [&'static str],
    /// The default for Intelligence, one of `chat`.
    pub(crate) intelligence: &'static str,
}

impl Remote {
    /// The page where the user makes an API key; `None` for a server the user runs.
    pub fn key_url(&self) -> Option<&'static str> {
        self.key_url
    }

    /// Its API's usual address; `None` when the user must give one.
    pub fn default_base_url(&self) -> Option<&'static str> {
        self.base_url
    }

    /// The address requests go to: `saved`, the one the user gave, else the usual one, else "".
    pub fn base_url<'a>(&self, saved: &'a str) -> &'a str {
        match saved.trim() {
            "" => self.base_url.unwrap_or_default(),
            saved => saved,
        }
    }

    /// The API key it takes; `None` when it takes none.
    pub fn api_key(&self) -> Option<&KeyFormat> {
        self.sign_in.api_key.as_ref()
    }

    /// Whether it can be used only through a subscription, with no API key.
    pub fn subscription_only(&self) -> bool {
        self.sign_in.api_key.is_none() && self.sign_in.subscription.is_some()
    }

    /// The headers a request with `key` carries: the key in its header, then those every
    /// request carries.
    pub fn headers(&self, key: Option<&str>) -> Vec<(&'static str, String)> {
        let key = key
            .filter(|key| !key.trim().is_empty())
            .zip(self.api_key())
            .map(|(key, format)| format.header(key));
        key.into_iter()
            .chain((self.headers.iter()).map(|(name, value)| (*name, (*value).to_owned())))
            .collect()
    }

    /// The API its chat Models answer through; `None` when it has none.
    pub fn chat(&self) -> Option<Chat> {
        self.chat
    }

    /// The API it transcribes through; `None` when it doesn't.
    pub fn speech(&self) -> Option<Speech> {
        self.speech
    }

    /// Whether its Models do `job`.
    pub fn does(&self, job: Job) -> bool {
        match job {
            Job::Transcription => self.speech.is_some(),
            Job::Summary | Job::Intelligence => self.chat.is_some(),
        }
    }

    /// Whether its chat API can stream an answer as it is written.
    pub fn streams(&self) -> bool {
        self.streams
    }

    /// The Model `job` uses until the user picks one; `None` when it doesn't do `job` or has
    /// no default, as for a server the user runs.
    pub(crate) fn default_model(&self, job: Job) -> Option<&'static str> {
        let model = match job {
            _ if !self.does(job) => None,
            Job::Transcription => self.models.speech.first().copied(),
            Job::Summary => self.models.chat.first().copied(),
            Job::Intelligence => Some(self.models.intelligence),
        };
        model.filter(|model| !model.is_empty())
    }

    /// The Model `job` uses: `saved`, the one the user picked, else the default; `None` when
    /// it doesn't do `job` or has neither.
    pub fn model<'a>(&self, job: Job, saved: Option<&'a str>) -> Option<&'a str> {
        let saved = saved.map(str::trim).filter(|saved| !saved.is_empty());
        self.does(job)
            .then(|| saved.or(self.default_model(job)))
            .flatten()
    }

    /// Whether it is set up: it has what every request needs, a key when it requires one and an
    /// address requests can go to. `saved_base_url` is the address the user gave, "" for the
    /// usual one.
    pub fn is_set_up(&self, has_key: bool, saved_base_url: &str) -> bool {
        let key = has_key || !self.api_key().is_some_and(KeyFormat::required);
        key && crate::base_url(self.base_url(saved_base_url)).is_ok()
    }

    /// The Models offered for `job`: Crumble's suggestions, then those `listed` by the
    /// Provider, each once. Empty when it doesn't do `job`.
    pub fn models(&self, job: Job, listed: Option<&Listed>) -> Vec<String> {
        if !self.does(job) {
            return Vec::new();
        }
        let (suggested, listed) = match job {
            Job::Transcription => (self.models.speech, listed.map(|listed| &listed.speech)),
            Job::Summary | Job::Intelligence => {
                (self.models.chat, listed.map(|listed| &listed.chat))
            }
        };
        let mut models: Vec<String> = suggested.iter().map(|&model| model.to_owned()).collect();
        for model in listed.into_iter().flatten() {
            if !models.contains(model) {
                models.push(model.clone());
            }
        }
        models
    }
}

/// Each `data[].id` in an OpenAI-style list, sorted, whose id `keep` accepts.
pub(crate) fn data_ids(answer: &Value, keep: fn(&str) -> bool) -> Vec<String> {
    let mut ids: Vec<String> = (answer["data"].as_array().into_iter().flatten())
        .filter_map(|model| model["id"].as_str())
        .filter(|id| keep(id))
        .map(str::to_owned)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests;
