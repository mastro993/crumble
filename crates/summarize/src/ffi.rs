//! The C ABI of `swift/Summarize.swift`, and safe wrappers over it. Every bridge function
//! blocks until its async work in Swift is done.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char};

use anyhow::{Context, anyhow};

use crate::Availability;

unsafe extern "C" {
    fn crumble_summarize_availability() -> i32;
    fn crumble_summarize_context_size() -> i32;
    fn crumble_summarize_token_counts(texts: *const c_char, out: *mut *mut c_char) -> i32;
    fn crumble_summarize_respond(
        instructions: *const c_char,
        prompt: *const c_char,
        max_tokens: i32,
        out: *mut *mut c_char,
    ) -> i32;
    fn crumble_summarize_free(string: *mut c_char);
}

/// Copies a string the bridge returned, then frees it.
fn take(string: *mut c_char) -> String {
    if string.is_null() {
        return String::new();
    }
    // SAFETY: the bridge returns NUL-terminated `strdup` copies, freed once here.
    unsafe {
        let copy = CStr::from_ptr(string).to_string_lossy().into_owned();
        crumble_summarize_free(string);
        copy
    }
}

/// `text` as a C string; speech recognition never writes a NUL, so one is dropped.
fn c_string(text: &str) -> CString {
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

/// Runs a bridge function that answers in `out`.
fn run(call: impl FnOnce(*mut *mut c_char) -> i32) -> anyhow::Result<String> {
    let mut out = std::ptr::null_mut();
    let code = call(&raw mut out);
    let answer = take(out);
    if code == 0 {
        Ok(answer)
    } else {
        Err(anyhow!(answer))
    }
}

/// Whether Apple's on-device model can run on this Mac now.
pub fn apple_availability() -> Availability {
    // SAFETY: takes nothing.
    match unsafe { crumble_summarize_availability() } {
        0 => Availability::Available,
        1 => Availability::Unsupported,
        2 => Availability::NotEnabled,
        3 => Availability::NotReady,
        _ => Availability::Unavailable,
    }
}

/// How many tokens the model reads at once, response included.
pub(crate) fn context_size() -> usize {
    // SAFETY: takes nothing.
    usize::try_from(unsafe { crumble_summarize_context_size() }).unwrap_or(0)
}

/// The token count of each of `texts`, in order.
pub(crate) fn token_counts(texts: &[&str]) -> anyhow::Result<Vec<usize>> {
    // U+001E, the record separator, never comes out of speech recognition or the model.
    let joined = c_string(&texts.join("\u{1e}"));
    let counts = run(|out| {
        // SAFETY: a valid C string and an out pointer, alive until the call returns.
        unsafe { crumble_summarize_token_counts(joined.as_ptr(), out) }
    })?;
    counts
        .lines()
        .map(|count| count.parse().context("a bad token count"))
        .collect()
}

/// The model's answer to `prompt`, following `instructions`, in at most `max_tokens` tokens.
pub(crate) fn respond(
    instructions: &str,
    prompt: &str,
    max_tokens: usize,
) -> anyhow::Result<String> {
    let (instructions, prompt) = (c_string(instructions), c_string(prompt));
    let max_tokens = i32::try_from(max_tokens).unwrap_or(i32::MAX);
    run(|out| {
        // SAFETY: valid C strings and an out pointer, alive until the call returns.
        unsafe {
            crumble_summarize_respond(instructions.as_ptr(), prompt.as_ptr(), max_tokens, out)
        }
    })
}
