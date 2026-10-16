# Suggestions through user notifications

When a meeting starts, Crumble suggests recording it with a macOS notification that has a Record button. It never records on its own: a Recording starts only when the user clicks Record. Notifications go through `UNUserNotificationCenter`, bound with `objc2-user-notifications`, the objc2 family the other bridges use, in a crate of its own, `crumble-notify`. anarlog, the reference meeting-notes app, draws its own notification windows in its Tauri app rather than using the system's, so there was nothing to copy from it here.

## Rules

- **Only while Crumble runs.** A minute tick inside the app checks today's Events. There is no launch agent, background helper or login item, and nothing is scheduled ahead with a time trigger: a schedule set in advance would go stale when an Event moves, is declined or is hidden, and would post while Crumble is quit, where Record could not start anything.
- **Posted at once.** Each Suggestion is a request with no trigger, posted when the Event is due. Its identifier is the Event's key (`crumble_calendar::suggestions::key`: series UID and occurrence, else `Event::id`), so posting the same occurrence twice replaces the first notification, and a click carries the key back. The key travels as the identifier rather than in `userInfo`, since setting `userInfo` is `unsafe` in the bindings and a string is all a click needs.
- **The delegate is set while the app finishes launching**, inside GPUI's launch callback, so a click that launches or activates Crumble is delivered. It shows the banner (`Banner | List`) while Crumble is frontmost, as macOS otherwise hides it, and hands each click to one handler. The delegate and its completion handlers run on threads macOS picks, so the handler is `Send + Sync` and sends to the main thread over a channel. The center holds its delegate weakly, so the delegate is leaked once and lives as long as the app.
- **Permission is asked for with the first Suggestion**, not at launch, so the prompt arrives when a meeting gives it a reason. `requestAuthorization` prompts only the first time and answers at once after that. A denial is read with `getNotificationSettings` and explained under the Settings switch, with a button to the Notifications pane.
- **No Info.plist key or entitlement.** Notifications need none for a signed, non-sandboxed app. They need a bundle identifier and a stable signature, which `scripts/macos-runner.sh` already gives Crumble.app. Outside a bundle, as in tests, `currentNotificationCenter` throws, so every call checks for a bundle identifier first and does nothing without one.

## Considered Options

- **`NSUserNotification`**: deprecated since macOS 11, with no action buttons on recent systems.
- **Scheduled requests with a calendar trigger**: they would fire with Crumble quit and need cancelling whenever an Event changes.
- **A notification drawn by Crumble**, as anarlog does: full control of the look, but it would sit outside Notification Center, Focus and the user's per-app settings.
- **Swift behind a C ABI**, as the transcription and summary bridges do: those wrap Swift-only APIs. `UNUserNotificationCenter` is plain Objective-C, which objc2 binds.

## Consequences

- `crumble-notify` defines an Objective-C class for the delegate, which is `unsafe`, so like `crumble-eventkit` (ADR 0001) and `crumble-audio` (ADR 0005) it sets `unsafe_code = "deny"`, with a `SAFETY` comment on each block, instead of the workspace's `forbid`.
- A click that relaunches Crumble arrives before today's Events are read, so its Event is not found and the Agenda page opens. Record does nothing then.
- An Event that starts while the Mac sleeps is still suggested on wake if it started no more than 5 minutes before.
