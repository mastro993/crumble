//! `UNUserNotificationCenter` through objc2. Its completion handlers and the delegate run on
//! threads of the system's choosing, so the caller's callbacks are `Send`.

#![allow(unsafe_code)]

use std::sync::{Mutex, OnceLock};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, define_class, msg_send};
use objc2_foundation::{NSArray, NSBundle, NSError, NSSet, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification,
    UNNotificationAction, UNNotificationActionOptions, UNNotificationCategory,
    UNNotificationCategoryOptions, UNNotificationDefaultActionIdentifier,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSettings, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

use crate::{Action, BODY, RECORD, Response, Status, Suggestion};

type Handler = Box<dyn Fn(Response) + Send + Sync>;

/// Where the delegate sends clicks; set once, at launch.
static HANDLER: OnceLock<Handler> = OnceLock::new();

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `Delegate` does not implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "CrumbleNotificationDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        /// Shows the banner while Crumble is frontmost too; macOS hides it by default.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion: &block2::DynBlock<dyn Fn()>,
        ) {
            let clicked = response.actionIdentifier();
            // SAFETY: a static NSString UserNotifications exports.
            let action = if &*clicked == unsafe { UNNotificationDefaultActionIdentifier } {
                Some(Action::Open)
            } else if clicked.to_string() == RECORD {
                Some(Action::Record)
            } else {
                None
            };
            if let (Some(action), Some(handler)) = (action, HANDLER.get()) {
                let key = response.notification().request().identifier().to_string();
                handler(Response { key, action });
            }
            completion.call(());
        }
    }
);

/// The notification center, or `None` outside an app bundle, where asking for it throws.
fn center() -> Option<Retained<UNUserNotificationCenter>> {
    NSBundle::mainBundle().bundleIdentifier()?;
    Some(UNUserNotificationCenter::currentNotificationCenter())
}

/// Sends clicks on Suggestions to `handler`, and registers the "Record" button. Call it once,
/// on the main thread, before the app finishes launching, so a click that launches Crumble is
/// heard; later calls change nothing.
pub fn set_handler(handler: impl Fn(Response) + Send + Sync + 'static) {
    let Some(center) = center() else {
        return;
    };
    if HANDLER.set(Box::new(handler)).is_err() {
        return;
    }
    let record = UNNotificationAction::actionWithIdentifier_title_options(
        &NSString::from_str(RECORD),
        &NSString::from_str("Record"),
        // Brings Crumble forward, as recording shows in its window.
        UNNotificationActionOptions::Foreground,
    );
    let category = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        &NSString::from_str(RECORD),
        &NSArray::from_retained_slice(&[record]),
        &NSArray::new(),
        UNNotificationCategoryOptions::empty(),
    );
    center.setNotificationCategories(&NSSet::from_retained_slice(&[category]));
    // SAFETY: NSObject's `init` takes a fresh allocation and returns it initialised.
    let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(), init] };
    center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // The center holds its delegate weakly; this one lives as long as the app.
    std::mem::forget(delegate);
}

/// Reads whether Crumble may show notifications, without asking, and hands it to `done`.
pub fn authorization(done: impl FnOnce(Status) + Send + 'static) {
    let Some(center) = center() else {
        return done(Status::Denied);
    };
    let done = once(done);
    let block = RcBlock::new(move |settings: std::ptr::NonNull<UNNotificationSettings>| {
        // SAFETY: the center hands a valid settings object for the call.
        let status = unsafe { settings.as_ref() }.authorizationStatus();
        done(match status {
            UNAuthorizationStatus::NotDetermined => Status::NotDetermined,
            UNAuthorizationStatus::Denied => Status::Denied,
            _ => Status::Allowed,
        });
    });
    center.getNotificationSettingsWithCompletionHandler(&block);
}

/// Asks to show banners with sound, which prompts only the first time, and hands `done`
/// whether they may show.
pub fn authorize(done: impl FnOnce(bool) + Send + 'static) {
    let Some(center) = center() else {
        return done(false);
    };
    let done = once(done);
    let block = RcBlock::new(move |granted: Bool, _error: *mut NSError| done(granted.as_bool()));
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &block,
    );
}

/// Posts `suggestion` at once, with the "Record" button. A failure is logged.
pub fn post(suggestion: &Suggestion) {
    let Some(center) = center() else {
        return;
    };
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(&suggestion.title));
    content.setBody(&NSString::from_str(BODY));
    content.setCategoryIdentifier(&NSString::from_str(RECORD));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(&suggestion.key),
        &content,
        // No trigger posts it now.
        None,
    );
    let block = RcBlock::new(|error: *mut NSError| {
        // SAFETY: the center hands a valid error or null.
        if let Some(error) = unsafe { error.as_ref() } {
            eprintln!(
                "Could not post a Suggestion: {}",
                error.localizedDescription()
            );
        }
    });
    center.addNotificationRequest_withCompletionHandler(&request, Some(&block));
}

/// `done` as an `Fn` that runs it the first time only, as completion blocks are `Fn`.
fn once<T>(done: impl FnOnce(T) + Send + 'static) -> impl Fn(T) + 'static {
    let done = Mutex::new(Some(done));
    move |value| {
        if let Some(done) = done.lock().ok().and_then(|mut done| done.take()) {
            done(value);
        }
    }
}
