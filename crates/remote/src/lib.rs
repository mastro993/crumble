//! Who runs a Model and how Crumble reaches it: the [`Provider`]s and the [`Job`]s their Models
//! do, a [`Remote`] for each remote Provider saying where its API is, what its API keys look
//! like and which jobs it does, the [`ping`] that checks a key, and the HTTP that Summaries
//! (ADR 0007), Transcripts (ADR 0008) and Settings (ADR 0011) share. An API key goes only into
//! its request's header: never into an error, a log, or anything else this crate returns.
//! No GPUI.

mod address;
mod http;
mod key;
mod ping;
mod provider;
mod remote;

pub use address::{base_url, host, without_userinfo};
pub use http::{TIMEOUT, delete, failure, get, no_response, post, post_json, post_reading_header};
pub(crate) use key::KeyHeader;
pub use key::{KeyCheck, KeyFormat};
pub use ping::ping;
pub use provider::{Job, Provider, runs_locally};
pub use remote::{Chat, Listed, Remote, Speech};
