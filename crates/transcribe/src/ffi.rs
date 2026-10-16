//! The C ABI of `swift/Transcribe.swift`, and safe wrappers over it. Every bridge function
//! blocks until its async work in Swift is done; progress arrives on a Swift thread meanwhile.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::Path;

use anyhow::{Context, anyhow};

use crate::ModelStatus;

/// What the bridge calls with the caller's context and a fraction done.
type Progress = extern "C" fn(*mut c_void, f32);

unsafe extern "C" {
    fn crumble_transcribe_current_locale() -> *mut c_char;
    fn crumble_transcribe_locale_name(locale: *const c_char) -> *mut c_char;
    fn crumble_transcribe_model_status(locale: *const c_char) -> i32;
    fn crumble_transcribe_install_model(
        locale: *const c_char,
        progress: Progress,
        context: *mut c_void,
        out: *mut *mut c_char,
    ) -> i32;
    fn crumble_transcribe_file(
        path: *const c_char,
        locale: *const c_char,
        progress: Progress,
        context: *mut c_void,
        out: *mut *mut c_char,
    ) -> i32;
    fn crumble_transcribe_free(string: *mut c_char);
}

/// The progress closure the bridge's context points at.
type Callback<'a> = &'a mut (dyn FnMut(f32) + Send);

extern "C" fn report(context: *mut c_void, fraction: f32) {
    // SAFETY: `context` is the `Callback` that `run` passed, alive until the bridge returns. The
    // bridge calls it from one task at a time, and the closure is `Send`.
    let callback = unsafe { &mut *context.cast::<Callback<'_>>() };
    callback(fraction.clamp(0.0, 1.0));
}

/// Copies a string the bridge returned, then frees it.
fn take(string: *mut c_char) -> String {
    if string.is_null() {
        return String::new();
    }
    // SAFETY: the bridge returns NUL-terminated `strdup` copies, freed once here.
    unsafe {
        let copy = CStr::from_ptr(string).to_string_lossy().into_owned();
        crumble_transcribe_free(string);
        copy
    }
}

fn c_string(text: &str) -> anyhow::Result<CString> {
    CString::new(text).with_context(|| format!("{text:?} holds a NUL"))
}

/// Runs a bridge function that reports progress and answers in `out`.
fn run(
    progress: Callback<'_>,
    call: impl FnOnce(Progress, *mut c_void, *mut *mut c_char) -> i32,
) -> anyhow::Result<String> {
    let mut callback = progress;
    let mut out = std::ptr::null_mut();
    let code = call(report, (&raw mut callback).cast(), &raw mut out);
    let answer = take(out);
    if code == 0 {
        Ok(answer)
    } else {
        Err(anyhow!(answer))
    }
}

/// The Mac's current locale identifier, such as `en_US`: the one to transcribe in until Settings
/// offers a choice.
pub fn current_locale() -> String {
    // SAFETY: takes nothing, returns a string for `take`.
    take(unsafe { crumble_transcribe_current_locale() })
}

/// `locale`'s name in the Mac's language, such as "English (United States)".
pub fn language_name(locale: &str) -> String {
    let Ok(identifier) = c_string(locale) else {
        return locale.to_owned();
    };
    // SAFETY: a valid C string in, a string for `take` out.
    take(unsafe { crumble_transcribe_locale_name(identifier.as_ptr()) })
}

/// Whether `locale`'s model is installed, downloadable, or unsupported. Blocks briefly.
pub fn model_status(locale: &str) -> ModelStatus {
    let Ok(identifier) = c_string(locale) else {
        return ModelStatus::Unsupported;
    };
    // SAFETY: a valid C string in.
    match unsafe { crumble_transcribe_model_status(identifier.as_ptr()) } {
        3 => ModelStatus::Installed,
        2 => ModelStatus::Downloading,
        1 => ModelStatus::Downloadable,
        _ => ModelStatus::Unsupported,
    }
}

/// Downloads `locale`'s model, Apple's on-device asset. Blocks until it is installed; `progress`
/// gets 0.0..=1.0, from another thread while this one waits.
pub fn install_model(locale: &str, mut progress: impl FnMut(f32) + Send) -> anyhow::Result<()> {
    let identifier = c_string(locale)?;
    run(&mut progress, |report, context, out| {
        // SAFETY: valid C strings, a callback with its context, and an out pointer, all alive
        // until the call returns.
        unsafe { crumble_transcribe_install_model(identifier.as_ptr(), report, context, out) }
    })
    .map(drop)
}

/// The bridge's lines for the audio file at `path`.
pub(crate) fn transcribe_file(
    path: &Path,
    locale: &str,
    progress: Callback<'_>,
) -> anyhow::Result<String> {
    let path = c_string(
        path.to_str()
            .ok_or_else(|| anyhow!("{} is not UTF-8", path.display()))?,
    )?;
    let identifier = c_string(locale)?;
    run(progress, |report, context, out| {
        // SAFETY: as in `install_model`.
        unsafe { crumble_transcribe_file(path.as_ptr(), identifier.as_ptr(), report, context, out) }
    })
}
