---
name: add-provider
description: Add a remote Provider to Crumble, a company's or server's Models reached over HTTP for Transcripts, Summaries or Intelligence. Use when adding or changing a Provider, its API key format, its key check (ping), its Models or logo, or when asked about a subscription (OAuth) sign-in.
---

# Add a remote Provider

`docs/architecture/adding-a-provider.md` is the source of truth: its steps are the checklist, `docs/architecture/providers.md` describes how Providers behave, and `docs/adr/0011-remote-providers-as-a-registry.md` records why the registry looks the way it does. Read all three, and `CONTEXT.md` for the terms (Provider, Model, Intelligence Model), before you write anything.

## Steps

1. **Research.** Do step 1 of the checklist from the Provider's own docs and its live endpoints, called with an obviously fake key. Write down the source URL beside each fact: key page, header, key shape, refused key kinds, free key check, Models list, chat and speech APIs, upload limits, streaming, subscription terms. Done when every field of the descriptor has a cited value or a cited "none".
2. **Build.** Do steps 2 to 7 in order. Copy the closest existing Provider's file and change only what your research supports. The files you touch:
   - `crates/remote/src/provider.rs`: the variant, its `id()`, `name()` and `remote()` arms, and its place in `Provider::ALL` and `Provider::REMOTE`.
   - `crates/remote/src/remote/<id>.rs`, the descriptor, and its `pub(crate) mod` line in `crates/remote/src/remote.rs`.
   - `assets/lobehub/<id>.svg` and its row in `assets/lobehub/README.md`.
   - `LOGOS` in `crates/ui/src/assets.rs`, and the arm in `logo()` in `crates/ui/src/native_view/providers/logo.rs`.
   - A wire adapter in `crates/summarize/src/remote/` or `crates/transcribe/src/remote/`, only when the API is new.
   - `crates/remote/src/remote/tests.rs`, and a list parser test in the descriptor's own file.
3. **Document.** Step 8: ADR 0011's count, `Provider` list, tables, ping list and Chat and Transcription APIs lists; the Provider list in `providers.md`; the per-Provider requests in `transcripts-and-summaries.md`; the examples in `CONTEXT.md`; and, for a new chat adapter, the `crates/summarize` row in `native.md`.
4. **Verify.** Step 9: the four checks in `AGENTS.md` pass, and with a real key the dialog reads "Saved in Keychain · Checked".

## Rules

- **Cited fields only.** Every path, header, field name, Model id and status code comes from the Provider's docs or a live answer you saw. When the docs say nothing, set the field to `None` or leave its list empty, and say so in your report.
- **Key pattern.** Match the shape gitleaks, TruffleHog and GitHub secret scanning use, in its permissive form with the stable prefix. Add a real-shaped key to `keys` in `real_key_shapes_pass_and_other_providers_keys_do_not`, which fails without it, and run it: the new key passes its own format and fails every other Provider's, and theirs fail its. Where another Provider's pattern takes it, add a `refused` entry with the stable prefix that tells them apart to that Provider's descriptor; where no prefix can, the ping tells them apart, so skip only that pair in the cross-check, with a comment citing both formats.
- **Read-only ping.** The key check GETs a key's details or the Models list: it generates nothing and spends no tokens.
- **API keys.** `sign_in.subscription` stays `None` with a comment citing the Provider's terms. A subscription sign-in comes only after those terms allow apps like Crumble to sign users in, recorded in a new ADR, following "Adding a subscription sign-in" in `adding-a-provider.md`.
- **Key safety.** A key goes only into its request's header, through `crumble_store::secrets`: keep it out of errors, logs, SQLite and UI text.
- **Small files.** Keep every file under 20,000 bytes; split a large one as `foo.rs` plus a `foo/` folder.
