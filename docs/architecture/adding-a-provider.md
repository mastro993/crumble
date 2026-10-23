# Adding a Provider

Part of the [native architecture](native.md): how a remote Provider is added. [Providers](providers.md) describes how they behave.

## Steps

Follow these steps in order. Record the source URL of every fact; never write a field, path or Model id from memory.

1. **Research**, from the Provider's own docs, and try its endpoints with an obviously fake key:
   - Its API key: the page where users make one, the header it goes in, and its shape. Cross-check the shape with the secret scanners, gitleaks (`config/gitleaks.toml`), TruffleHog (`pkg/detectors/<name>`) and GitHub secret scanning's supported patterns, and keep a permissive form of the stable prefix. List the other kinds of key it issues that its API refuses for this use, such as admin keys or subscription tokens.
   - A free key check: a GET that refuses a bad key with 400 or 401 and generates nothing, such as a key-details endpoint or the Models list. Note its status and body for a bad key.
   - Its Models list: the path, whether it needs the key, the answer's shape, and how chat and speech Models tell apart.
   - Its chat API and speech API, if any: which of `Chat` and `Speech` they speak, the usual address, extra headers, the upload limit, whether M4A is accepted, and whether answers time segments, words or nothing.
   - Whether its chat API streams.
   - Its subscription terms: whether its consumer plans may be used from other apps. Unless they allow it in writing, it takes an API key only.
   - Its logo in [LobeHub Icons](https://github.com/lobehub/lobe-icons).
2. **The Provider**, in `crates/remote/src/provider.rs`: a variant with a doc comment, its `id()`, lowercase and never changed once released (it is stored, the Keychain account and the logo's file name), its `name()` as users see it, and its `remote()` arm. Add it to `Provider::ALL` and `Provider::REMOTE`, before `OpenAiCompatible`, and grow their lengths.
3. **The descriptor**, `crates/remote/src/remote/<id>.rs` (`-` becomes `_`), declared as `pub(crate) mod <id>;` in `crates/remote/src/remote.rs`, holding `pub(crate) static REMOTE: Remote`. Start from the closest Provider's file:
   - `key_url`: the page where users make a key, `None` for a server the user runs.
   - `base_url`: the usual address, `None` when the user must give one. OpenAI's APIs append `/chat/completions` and `/audio/transcriptions`, so an OpenAI-style address is the one the Provider's docs give OpenAI's SDK, usually ending in its version (`/v1`); Anthropic's, Gemini's, ElevenLabs' and xAI's speech APIs append their own paths.
   - `sign_in.api_key`: `KeyFormat { pattern, starts, refused, required, header }`: the whole-key regex, `None` for any text, the ways keys start, for the hint and to tell an incomplete key from another kind, each refused kind's regex from its start and the sentence the user reads, whether a key is required, and `KeyHeader::Bearer` or `KeyHeader::Named`.
   - `sign_in.subscription`: `None`, with a comment saying why.
   - `headers`: what every request carries besides the key, such as an API version.
   - `chat` and `speech`: the API for each, or `None`.
   - `streams`: whether the chat API streams.
   - `check.key`: a `KeyCheckRequest { path, unusable }` when the Models list does not prove the key, as a public list does not; `unusable` reads a disabled or blocked key from the answer.
   - `check.models`: a `ModelsRequest { path, keyed, forbidden_refuses_key, read }`, or `None` without a list; `forbidden_refuses_key` is `true` only when the Provider's keys have no scopes, so a 403 refuses the key rather than only the list; `read` returns `Listed { chat, speech }`, through `data_ids` for an OpenAI-style list. Both paths follow `base_url`, so they leave out what it already ends in.
   - `models`: `Models { chat, speech, intelligence }`: suggestions, the default first, and the Intelligence default, one of `chat`, or `""` when it does not chat.
4. **The logo**: download the file from `@lobehub/icons-static-svg` at the version in `assets/lobehub/README.md`, check its hash and that it holds only shapes, then normalize it as that README says (64 by 64, no `style`, no `<title>`). Take the mono file, which uses `fill="currentColor"`, unless the brand is only recognizable in color; a color file must read on light and dark surfaces. Save it as `assets/lobehub/<id>.svg` and add its row, with sha256 and kind, to the README. `THIRD_PARTY_NOTICES.md` covers LobeHub's files; a logo from elsewhere needs its own notice.
5. **The UI**: map `logos/<id>.svg` to its file in `LOGOS`, in `crates/ui/src/assets.rs`, and add the Provider's arm, `Logo::Mono` or `Logo::Color`, to `logo()` in `crates/ui/src/native_view/providers/logo.rs`; that `match` is exhaustive, so the compiler points at it. It is the only `match` on `Provider` in `crates/ui` without a catch-all arm. The list, the dialog, the selects and the menus read `Provider::REMOTE`, `Provider::ALL` and the descriptor, so nothing else changes.
6. **A wire adapter, only for an API Crumble does not speak yet**: a `Chat` or `Speech` variant in `crates/remote/src/remote.rs`, then either:
   - for chat, a module in `crates/summarize/src/remote/` with `request`, the URL and body, and `text`, the answer with its refusals and cut answers, declared in `crates/summarize/src/remote.rs` with its arm in both of `Remote::ask`'s `match chat`;
   - for speech, a module in `crates/transcribe/src/remote/` with its piece limits, `MAX_PIECE` for size and `MAX_LENGTH` when the API caps how long a piece plays, and `transcribe`, which returns `Timed` pieces, timed words going through `words::segments_or_text`, declared in `crates/transcribe/src/remote.rs` with its arm in `Remote::transcribe`'s `match speech`.

   Test the request and the documented example answers in the module.
7. **Tests**, in `crates/remote/src/remote/tests.rs`: the Provider's row in `each_provider_declares_what_it_does`, which fails until every remote Provider has one; and, in `real_key_shapes_pass_and_other_providers_keys_do_not`, a real-shaped key declared beside the others and its row in `keys`, which fails until every Provider with a key pattern has one, so it passes its own format and fails every other Provider's, then a check that each of its refused kinds is refused. When another Provider's pattern takes the new key, add a `refused` entry to that Provider's descriptor where a stable prefix tells the two apart, as OpenAI's `^sk-ant-` and `^sk-or-` do; when none can, such as `sk-` and 32 hex digits at two Providers, the ping tells them apart, so skip only that pair in the cross-check, with a comment citing both formats. In the descriptor's own file, test its list parser on the documented answer. Patterns, suggestions, ids and logos are covered by tests that already loop over every Provider.
8. **Docs**: in ADR 0011, its count of companies, its list of `Provider` variants, its tables, its ping list, and the Providers named under Chat APIs and Transcription APIs, with any piece limit; the list of Providers in [Providers](providers.md); the per-Provider transcription requests and piece limits in [Transcripts and Summaries](transcripts-and-summaries.md); and the Provider examples in `CONTEXT.md`. A new chat adapter from step 6 also goes into the `crates/summarize` row of `native.md`'s Ownership table. A new term goes into `CONTEXT.md` too.
9. **Verify** with the four checks in `AGENTS.md`, every file under 20,000 bytes, then with a real key: "Saved in Keychain · Checked", its Models listed, and a short Recording transcribed or summarized.

## Adding a subscription sign-in

None is offered; ADR 0011 says which Providers forbid one and which might allow it.

- First confirm, in the Provider's own terms, that apps like Crumble may sign users in, and record it in a new ADR. Never reuse another app's OAuth client or imitate one.
- The flow becomes a `Subscription` variant in `crates/remote/src/remote.rs`, set in the descriptor's `sign_in.subscription`. A Provider with no `api_key` is then subscription only. The jobs it covers are declared with it; OpenAI's covers chat, never Transcription.
- Its tokens go in the Keychain through `crumble_store::secrets`, beside the keys, never in SQLite; a refresh replaces them, and Sign out deletes and revokes them.
- The dialog gets a sign-in button above the API key, in the Provider's required branding, with Sign out and whatever usage screens its terms ask for. A signed-in Provider is ready for the jobs the sign-in covers.
