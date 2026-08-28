//! Putting a finished errand on screen, the way macOS actually does it now.
//!
//! This exists because the obvious route is a trap, and the trap is silent.
//! Tauri's notification plugin posts through `NSUserNotificationCenter`, which
//! Apple deprecated years ago and has since removed. On this machine it reports
//! success, raises no permission prompt, and delivers nothing at all. An errand
//! finishing quietly is indistinguishable from an errand still running, which
//! is the one thing a notification exists to prevent.
//!
//! So the current framework is used directly. `UNUserNotificationCenter` is
//! also what produces the system's own permission question, which is why asking
//! is worth doing at launch rather than at the moment something lands: it is a
//! question about notifications in general, and it reads much better before
//! there is one waiting behind it.
//!
//! It needs a real bundle with a real identity. Run out of a build directory
//! with no signature there is no app for macOS to attribute a notification to,
//! and it refuses. That is a property of the operating system rather than
//! something to work around, so the failure says so in as many words.

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
    UNUserNotificationCenter,
};

/// Ask, once, whether this app may put things on screen.
///
/// The system remembers the answer, so this is a question the person is asked
/// exactly once however many times the app is opened. A no is a decision, not a
/// fault, and nothing here tries to ask again.
pub fn ask() {
    let Some(center) = center() else {
        eprintln!("notifications: no bundle to attribute them to, so none will be shown");
        return;
    };
    let heard = RcBlock::new(|allowed: Bool, error: *mut NSError| {
        if let Some(why) = unsafe { error.as_ref() } {
            // Worth saying in full. The one that happens for real is
            // "notifications are not allowed for this application", and it
            // reads like somebody's decision when it is usually the system
            // holding a grudge against this bundle identifier from an earlier
            // attempt that asked the wrong way.
            eprintln!("notifications: the system refused: {why:?}");
        } else if !allowed.as_bool() {
            eprintln!("notifications: not allowed, so errands will finish quietly");
        }
    });
    // Alert and sound only. A badge would count things nobody asked to have
    // counted, and this app has nothing to put a number on.
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &heard,
    );
}

/// Put one line on screen.
///
/// `about` names which errand it is, so two of them finishing minutes apart do
/// not replace each other in the corner of the screen.
pub fn show(about: &str, title: &str, body: &str) {
    let Some(center) = center() else { return };
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));

    // No trigger, which means now.
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(about),
        &content,
        None,
    );
    let landed = RcBlock::new(|error: *mut NSError| {
        if !error.is_null() {
            eprintln!("notifications: that one could not be shown");
        }
    });
    center.addNotificationRequest_withCompletionHandler(&request, Some(&landed));
}

/// The system's notification centre, if there is one to be had.
///
/// There is not, when the app has no bundle: asking for it in that case is a
/// hard crash rather than an error, which is why this is behind a check at all.
fn center() -> Option<objc2::rc::Retained<UNUserNotificationCenter>> {
    let bundled = objc2_foundation::NSBundle::mainBundle()
        .bundleIdentifier()
        .is_some();
    bundled.then(UNUserNotificationCenter::currentNotificationCenter)
}
