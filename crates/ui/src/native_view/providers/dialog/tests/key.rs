//! The API key row: what a key typed says, when it is pinged, where the focus goes once it is
//! saved or removed, and that one being checked outlives the dialog.

use super::super::check::Check;
use super::super::key::PAUSE;
use super::{Harness, KEY, saved};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ClipboardItem, TestAppContext};
use std::io::{Read, Write};
use std::net::TcpListener;

/// A server on this Mac that answers one request, OpenAI's Models list, with none: its address,
/// as OpenAI's API has it.
fn serve_models() -> String {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => panic!("could not listen: {error}"),
    };
    let address = match listener.local_addr() {
        Ok(address) => address,
        Err(error) => panic!("no address: {error}"),
    };
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let (mut head, mut buffer) = (Vec::<u8>::new(), [0; 1024]);
        while !head.windows(4).any(|end| end == b"\r\n\r\n") {
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(read) => head.extend(&buffer[..read]),
            }
        }
        let body = r#"{"data":[]}"#;
        let answer = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(answer.as_bytes()).ok();
    });
    format!("http://{address}/v1")
}

#[gpui_kit::test]
fn a_key_that_cannot_be_the_provider_s_says_why_and_is_not_pinged(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, false);
    // Empty, the row offers the page where keys are made.
    assert!(harness.present(cx, "provider-key-get"));

    harness.type_key(cx, &format!("sk-ant-api03-{}", "a".repeat(93)));
    assert_eq!(
        harness.key_status(cx).as_deref(),
        Some("This is an Anthropic key.")
    );
    assert!(!harness.present(cx, "provider-key-get"));
    // Neither Enter nor a pause sends it.
    harness.step(cx, |window, cx| window.press("enter", cx));
    cx.executor().advance_clock(PAUSE * 2);
    harness.settle(cx);
    assert!(!harness.checking(cx));
    assert_eq!(
        harness.key_status(cx).as_deref(),
        Some("This is an Anthropic key.")
    );
    assert_eq!(cx.update(|cx| harness.owner.read(cx).saves), 0);
}

#[gpui_kit::test]
fn a_key_that_looks_right_is_pinged_once_typing_pauses(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, false);
    harness.type_key(cx, KEY);
    // Nothing is sent before the pause, and the row says nothing yet.
    assert!(!harness.checking(cx));
    assert_eq!(harness.key_status(cx), None);
    assert!(!harness.present(cx, "provider-key-get"));

    // The ping goes to port 9, where nothing answers.
    cx.executor().advance_clock(PAUSE);
    harness.settle(cx);
    assert_eq!(
        harness.key_status(cx).as_deref(),
        Some("Couldn't reach OpenAI. Check the address and the internet connection.")
    );
    assert!(harness.present(cx, "provider-key-try-again"));
    assert_eq!(cx.update(|cx| harness.owner.read(cx).saves), 0);
}

#[gpui_kit::test]
fn enter_pings_a_key_that_looks_right_at_once(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, false);
    harness.type_key(cx, KEY);
    harness.step(cx, |window, cx| window.press("enter", cx));
    assert!(harness.checking(cx));
    assert_eq!(harness.key_status(cx).as_deref(), Some("Checking…"));
    // Enter doesn't close the dialog as it confirms others.
    assert!(harness.present(cx, "provider-done"));
    // A newer edit drops the running ping.
    harness.step(cx, |window, cx| window.press("backspace", cx));
    assert!(!harness.checking(cx));
}

#[gpui_kit::test]
fn a_pasted_key_is_pinged_at_once(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, false);
    cx.write_to_clipboard(ClipboardItem::new_string(KEY.to_owned()));
    harness.step(cx, |window, cx| {
        window.click("provider-key", cx);
        window.press("cmd-v", cx);
    });
    assert!(harness.checking(cx));
    assert_eq!(harness.key_status(cx).as_deref(), Some("Checking…"));
}

#[gpui_kit::test]
fn replace_shows_the_field_and_keeps_the_saved_key(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    assert_eq!(harness.key_status(cx).as_deref(), Some("Saved in Keychain"));
    assert!(!harness.present(cx, "provider-key"));

    harness.step(cx, |window, cx| window.click("provider-key-replace", cx));
    assert!(harness.present(cx, "provider-key"));
    assert!(!harness.present(cx, "provider-key-remove"));
    harness.check(cx, |window| {
        assert_eq!(window.find("provider-key").focused(), Some(true));
    });
    assert_eq!(cx.update(|cx| harness.owner.read(cx).removals), 0);
}

#[gpui_kit::test]
fn a_new_address_drops_what_the_saved_key_s_check_found(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    harness.step(cx, |window, cx| {
        (harness.dialog(cx)).update(cx, |dialog, cx| dialog.key_saved(Ok(()), window, cx));
    });
    assert_eq!(
        harness.key_status(cx).as_deref(),
        Some("Saved in Keychain · Checked")
    );

    harness.step(cx, |window, cx| {
        window.click("provider-address", cx);
        window.input("0", cx);
    });
    assert_eq!(harness.key_status(cx).as_deref(), Some("Saved in Keychain"));
}

#[gpui_kit::test]
fn remove_asks_to_delete_the_key_and_shows_the_field(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    harness.step(cx, |window, cx| window.click("provider-key-remove", cx));
    assert_eq!(cx.update(|cx| harness.owner.read(cx).removals), 1);
    assert!(harness.present(cx, "provider-key"));
    assert!(!harness.present(cx, "provider-key-replace"));
    assert!(harness.present(cx, "provider-key-get"));
}

#[gpui_kit::test]
fn a_key_being_typed_waits_for_the_address_to_be_finished(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, false);
    harness.type_key(cx, KEY);
    cx.executor().advance_clock(PAUSE);
    harness.settle(cx);
    assert!(harness.present(cx, "provider-key-try-again"));

    harness.step(cx, |window, cx| {
        window.click("provider-address", cx);
        window.input("0", cx);
    });
    // However long typing pauses, the key goes to no address half typed.
    cx.executor().advance_clock(PAUSE * 2);
    harness.settle(cx);
    assert!(!harness.checking(cx));
    assert_eq!(harness.key_status(cx), None);

    // Finished, the address gets it.
    harness.step(cx, |window, cx| window.press("enter", cx));
    assert!(harness.checking(cx));
    assert_eq!(harness.key_status(cx).as_deref(), Some("Checking…"));
}

#[gpui_kit::test]
fn escape_closes_the_dialog_once_the_key_typed_is_saved(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, false);
    harness.type_key(cx, KEY);
    // The owner keeps the key, as once its ping passes.
    harness.step(cx, |window, cx| {
        let saved = harness.owner.update(cx, |owner, _| {
            owner.saved.has_key = true;
            owner.saved.clone()
        });
        harness.dialog(cx).update(cx, |dialog, cx| {
            dialog.set_saved(saved, cx);
            dialog.key_saved(Ok(()), window, cx);
        });
    });
    assert!(!harness.present(cx, "provider-key"));

    harness.step(cx, |window, cx| window.press("escape", cx));
    assert!(!harness.present(cx, "provider-done"));
}

#[gpui_kit::test]
fn remove_from_the_keyboard_focuses_the_field_and_escape_still_closes(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    let mut on_remove = false;
    for _ in 0..10 {
        harness.step(cx, |window, cx| window.press("tab", cx));
        harness.check(cx, |window| {
            on_remove = window.find("provider-key-remove").focused() == Some(true);
        });
        if on_remove {
            break;
        }
    }
    assert!(on_remove, "Tab never reached Remove");

    harness.step(cx, |window, cx| window.press("space", cx));
    assert_eq!(cx.update(|cx| harness.owner.read(cx).removals), 1);
    harness.check(cx, |window| {
        assert_eq!(window.find("provider-key").focused(), Some(true));
    });
    harness.step(cx, |window, cx| window.press("escape", cx));
    assert!(!harness.present(cx, "provider-done"));
}

#[gpui_kit::test]
fn a_saved_key_drops_what_check_again_found_about_the_old_one(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    harness.step(cx, |_, cx| {
        harness.dialog(cx).update(cx, |dialog, cx| {
            dialog.check = Check::Failed("OpenAI didn't accept this key.".to_owned());
            cx.notify();
        });
    });
    assert!(harness.present(cx, "provider-check-status"));

    harness.step(cx, |window, cx| {
        (harness.dialog(cx)).update(cx, |dialog, cx| dialog.key_saved(Ok(()), window, cx));
    });
    assert!(!harness.present(cx, "provider-check-status"));
}

#[gpui_kit::test]
fn a_removed_key_drops_what_check_again_found(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    harness.step(cx, |_, cx| {
        harness.dialog(cx).update(cx, |dialog, cx| {
            dialog.check = Check::Passed;
            cx.notify();
        });
    });
    assert!(harness.present(cx, "provider-check-status"));

    harness.step(cx, |window, cx| window.click("provider-key-remove", cx));
    assert!(!harness.present(cx, "provider-check-status"));
}

#[gpui_kit::test]
fn a_key_being_checked_as_the_dialog_closes_is_still_kept(cx: &mut TestAppContext) {
    let mut saved = saved(false);
    saved.base_url = serve_models();
    let harness = Harness::open_with(cx, saved);
    harness.type_key(cx, KEY);
    harness.step(cx, |window, cx| window.press("enter", cx));
    assert!(harness.checking(cx));

    harness.step(cx, |window, cx| window.click("provider-done", cx));
    assert!(!harness.present(cx, "provider-done"));
    harness.settle(cx);
    assert_eq!(cx.update(|cx| harness.owner.read(cx).saves), 1);
}
