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

use std::sync::Mutex;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AnyThread};
use objc2_foundation::{NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

/// What to do when somebody clicks one of these.
///
/// A notification that only brings the app forward is a dead end: you read it,
/// and then go and find the conversation yourself, which is the errand you were
/// trying not to run. The identifier on the notification is the conversation
/// id, so the click has everything it needs; it only ever lacked somewhere to
/// send it.
type WhatToDo = Box<dyn Fn(String) + Send + 'static>;

static WHEN_CLICKED: Mutex<Option<WhatToDo>> = Mutex::new(None);

/// Say what should happen when one of these is clicked.
pub fn on_click(go: impl Fn(String) + Send + 'static) {
    *WHEN_CLICKED.lock().unwrap() = Some(Box::new(go));
}

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
    // The badge was declined once, on the grounds that this app has nothing to
    // put a number on. It has: the number of agents stopped waiting on
    // somebody. That is the one count here that is a claim on their attention
    // rather than a tally of things that happened.
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert
            | UNAuthorizationOptions::Sound
            | UNAuthorizationOptions::Badge,
        &heard,
    );
}

/// How many agents are stopped waiting on somebody, on the dock icon.
///
/// Nought clears it, which is the case that matters: a badge that is right when
/// it appears and wrong for the rest of the day is worse than none.
pub fn waiting(how_many: i64) {
    let Some(center) = center() else { return };
    let done = RcBlock::new(|_error: *mut NSError| {});
    unsafe {
        let _: () = msg_send![&*center, setBadgeCount: how_many as isize,
                              withCompletionHandler: &*done];
    }
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

define_class!(
    /// The object macOS talks to about these.
    ///
    /// Two things only. A notification that arrives while the app is in front
    /// is still shown, because the whole reason one is posted at all is that
    /// the conversation it belongs to is not the one being looked at. And a
    /// click hands the conversation id back to whoever asked for it.
    #[unsafe(super(NSObject))]
    #[name = "ErrandNotificationDelegate"]
    #[ivars = ()]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            handler: &block2::Block<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            handler.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::Sound,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            handler: &block2::Block<dyn Fn()>,
        ) {
            let which = response.notification().request().identifier().to_string();
            if let Some(go) = WHEN_CLICKED.lock().unwrap().as_ref() {
                go(which);
            }
            handler.call(());
        }
    }
);

/// Start listening for clicks.
///
/// Held for the life of the process on purpose. The centre keeps only a weak
/// reference to its delegate, so one that is dropped leaves clicks going
/// nowhere, silently, which is exactly the failure this is here to end.
pub fn listen() {
    let Some(center) = center() else { return };
    let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(), init] };
    center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    std::mem::forget(delegate);
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
