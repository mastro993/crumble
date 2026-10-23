//! Saves, lists, reads and removes a made-up key in the login Keychain. Run it by hand:
//! `cargo test -p crumble-store --locked -- --ignored`. It leaves no item behind. Moving the old
//! keys is tested in `secrets` itself, between made-up services, so no real key moves.

use crumble_store::secrets;

/// No real Provider has this id.
const ACCOUNT: &str = "crumble-keychain-test";

fn ok<T>(result: anyhow::Result<T>) -> T {
    result.unwrap_or_else(|error| panic!("{error:#}"))
}

#[test]
#[ignore = "writes to the login Keychain"]
fn a_key_is_saved_listed_read_replaced_and_removed() {
    ok(secrets::delete(ACCOUNT));
    assert_eq!(ok(secrets::get(ACCOUNT)), None);
    assert!(!ok(secrets::saved()).contains(ACCOUNT));
    ok(secrets::set(ACCOUNT, "not-a-real-key-1"));
    ok(secrets::set(ACCOUNT, "not-a-real-key-2"));
    assert!(ok(secrets::saved()).contains(ACCOUNT));
    assert_eq!(
        ok(secrets::get(ACCOUNT)).as_deref(),
        Some("not-a-real-key-2")
    );
    ok(secrets::delete(ACCOUNT));
    assert_eq!(ok(secrets::get(ACCOUNT)), None);
    assert!(!ok(secrets::saved()).contains(ACCOUNT));
    ok(secrets::delete(ACCOUNT));
}
