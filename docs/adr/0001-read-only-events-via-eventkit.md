# Read-only Events through macOS EventKit

Crumble shows Events from the user's Google, Outlook, and iCloud calendars by reading them through macOS EventKit, not by calling each provider's API. macOS already syncs every account the user added in System Settings, so Crumble needs no OAuth flows, client registrations, stored credentials, or network code. Crumble only reads Events and never creates, edits, or responds to them. That is a permanent scope decision, not a v1 shortcut.

## Considered Options

- **Provider APIs** (Google Calendar API, Microsoft Graph, iCloud CalDAV): cross-platform with an in-app "Add account" flow, but it needs three adapters, an OAuth loopback flow, Keychain storage, and registered OAuth apps. A Google app in "Testing" status expires refresh tokens after 7 days.
- **ICS subscription URLs**: the simplest option, but the user pastes one link per Calendar and providers refresh them slowly.

## Consequences

- macOS only. Other platforms are deferred to V2, which will need a different source of Events.
- Accounts are added and removed in System Settings, not in Crumble. Crumble can only disable them.
- Every EventKit call is `unsafe`, and the workspace forbids `unsafe_code`. The Objective-C bridge lives in its own crate with its own lint table (`unsafe_code = "deny"` plus local allows). The UI crate keeps `forbid`.
- macOS grants calendar access only to an app bundle whose `Info.plist` has `NSCalendarsFullAccessUsageDescription`. A bare `cargo run` binary is treated as the terminal and is denied silently. So, on macOS, `cargo run` goes through a runner that wraps the binary in a minimal `.app`. The permission is tied to the code signature, so an ad-hoc signed build loses it after every change; signing with a certificate identity keeps it.
- EventKit has no provider or email field. Row labels use an email inferred from the user's own attendee address or a Calendar named after an address, else the account name from System Settings. The provider is guessed from the source type and title, and for Google Workspace from the identifiers Google gives its Events.
- A notarized release will need the Hardened Runtime calendar entitlement.
