//! macOS: the shell shows the notification itself and hears the click on it through a delegate of
//! the user notification center.
//!
//! The plugin shows a notification through `notify-rust`, which hears what the user does with it
//! only when it has buttons. Here each notification is delivered with the label of the window that
//! raised it in its user info, and one delegate, set for the life of the process, hears the click,
//! including on a notification the user opens later from Notification Center. Nothing waits on a
//! delivered notification: `NSUserNotificationCenter` reports no dismissal, so waiting on one means
//! polling its delivered list, a synchronous call to the notification service, for as long as the
//! notification stays in Notification Center; done from the main thread for every notification left
//! there, it leaves the main thread no time for input.

// `UNUserNotificationCenter`, which replaces `NSUserNotificationCenter`, refuses a process outside an
// app bundle, which a development build is; the notification plugin stays on the older API too.
#![allow(deprecated)]

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_foundation::{
    NSDictionary, NSOperationQueue, NSString, NSUserNotification, NSUserNotificationCenter,
    NSUserNotificationCenterDelegate,
};
use tauri::AppHandle;

/// The user info key holding the label of the window that raised the notification.
const WINDOW_KEY: &str = "window";

struct DelegateIvars {
    app: AppHandle,
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and `ClickDelegate` implements no
    // `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "OctoboardNotificationClickDelegate"]
    #[ivars = DelegateIvars]
    struct ClickDelegate;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for ClickDelegate {}

    // SAFETY: the method matches the protocol's signature.
    unsafe impl NSUserNotificationCenterDelegate for ClickDelegate {
        /// A click on the notification, on its banner or in Notification Center: the window that
        /// raised it is brought forward, and the notification, answered, leaves Notification
        /// Center.
        #[unsafe(method(userNotificationCenter:didActivateNotification:))]
        fn did_activate(
            &self,
            center: &NSUserNotificationCenter,
            notification: &NSUserNotification,
        ) {
            let label = label_of(notification);
            center.removeDeliveredNotification(notification);
            // Queued on the main queue rather than run here: the click also activates the app,
            // which brings forward whichever window was in front, and the window asked for has to
            // come forward after that. `run_on_main_thread` would not do, since on the main thread
            // it runs the task at once.
            let app = self.ivars().app.clone();
            let bring_forward = block2::RcBlock::new(move || {
                super::bring_forward(&app, label.as_deref());
            });
            // SAFETY: the main queue runs the block on the main thread, where windows are shown.
            unsafe { NSOperationQueue::mainQueue().addOperationWithBlock(&bring_forward) };
        }
    }
);

/// Sets the delegate that hears the clicks, for the life of the process. Called on the main thread
/// at setup, before any notification is shown. A notification sent through the notification
/// plugin's `notify` would replace it with `mac-notification-sys`'s own delegate for good, after
/// which a click only activates the app; the windows' capabilities grant the plugin's permission
/// query alone, so notifications on macOS go through `show`.
pub fn install(app: &AppHandle) {
    let delegate = ClickDelegate::alloc().set_ivars(DelegateIvars { app: app.clone() });
    // SAFETY: `NSObject`'s `init` takes no arguments and returns the initialised object.
    let delegate: Retained<ClickDelegate> = unsafe { msg_send![super(delegate), init] };
    // SAFETY: the delegate is kept alive below for the rest of the process, so the center's
    // unretained reference to it never dangles.
    unsafe {
        NSUserNotificationCenter::defaultUserNotificationCenter()
            .setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    }
    // The center does not retain its delegate.
    std::mem::forget(delegate);
}

/// Delivers the notification, marked with the asking window's label, and returns at once.
pub fn show(app: &AppHandle, title: String, body: String, label: &str) -> Result<(), String> {
    // Set once for the process, on the first notification: the bundle identifier hook it installs
    // stays out of everything that runs before one is shown.
    set_application(app);
    NSUserNotificationCenter::defaultUserNotificationCenter()
        .deliverNotification(&notification(&title, &body, label));
    Ok(())
}

/// A notification with `title` and `body`, carrying `label` in its user info.
fn notification(title: &str, body: &str, label: &str) -> Retained<NSUserNotification> {
    let notification = NSUserNotification::new();
    let key = NSString::from_str(WINDOW_KEY);
    let value = NSString::from_str(label);
    let user_info = NSDictionary::<NSString, AnyObject>::from_slices(&[&*key], &[&**value]);
    notification.setTitle(Some(&NSString::from_str(title)));
    notification.setInformativeText(Some(&NSString::from_str(body)));
    // A click on the notification itself is the only answer; without this it shows a Show button.
    notification.setHasActionButton(false);
    // SAFETY: the user info holds only strings, which the notification center can archive.
    unsafe { notification.setUserInfo(Some(&user_info)) };
    notification
}

/// The label of the window that raised `notification`, if it carries one: a notification left in
/// Notification Center by an earlier version of the app does not.
fn label_of(notification: &NSUserNotification) -> Option<String> {
    let user_info = notification.userInfo()?;
    let value = user_info.objectForKey(&NSString::from_str(WINDOW_KEY))?;
    let value = value.downcast::<NSString>().ok()?;
    Some(value.to_string())
}

/// Attributes the notifications to the app's own bundle, or to Terminal for a development build,
/// whose binary is in no bundle LaunchServices knows (the plugin does the same). Without it the
/// first notification attributes itself to whatever application AppleScript resolves, Finder by
/// default.
fn set_application(app: &AppHandle) {
    let _ = mac_notification_sys::set_application(application(
        tauri::is_dev(),
        &app.config().identifier,
    ));
}

/// The application the notifications are attributed to, which a click activates.
fn application(is_dev: bool, identifier: &str) -> &str {
    if is_dev {
        "com.apple.Terminal"
    } else {
        identifier
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_foundation::NSNumber;

    /// A notification whose user info holds `value` under `key`.
    fn with_user_info(key: &str, value: &AnyObject) -> Retained<NSUserNotification> {
        let notification = NSUserNotification::new();
        let key = NSString::from_str(key);
        let user_info = NSDictionary::<NSString, AnyObject>::from_slices(&[&*key], &[value]);
        // SAFETY: the user info is only read back in memory, never archived.
        unsafe { notification.setUserInfo(Some(&user_info)) };
        notification
    }

    #[test]
    fn a_notification_is_attributed_to_the_app_except_in_a_development_build() {
        assert_eq!(
            application(false, "dev.octoboard.desktop"),
            "dev.octoboard.desktop"
        );
        assert_eq!(
            application(true, "dev.octoboard.desktop"),
            "com.apple.Terminal"
        );
    }

    #[test]
    fn a_notification_carries_its_title_and_body() {
        let notification = notification("Build finished", "All 12 checks passed", "main");
        assert_eq!(
            notification
                .title()
                .map(|title| title.to_string())
                .as_deref(),
            Some("Build finished")
        );
        assert_eq!(
            notification
                .informativeText()
                .map(|body| body.to_string())
                .as_deref(),
            Some("All 12 checks passed")
        );
    }

    #[test]
    fn a_notification_has_no_action_button() {
        let notification = notification("title", "body", "main");
        assert!(!notification.hasActionButton());
    }

    #[test]
    fn a_notification_gives_back_the_label_it_was_made_with() {
        for label in ["main", "session-3", "窗口-ウィンドウ", ""] {
            let notification = notification("title", "body", label);
            assert_eq!(label_of(&notification).as_deref(), Some(label));
        }
    }

    #[test]
    fn a_notification_without_user_info_has_no_label() {
        assert_eq!(label_of(&NSUserNotification::new()), None);
    }

    #[test]
    fn user_info_without_the_window_key_has_no_label() {
        let other = NSString::from_str("main");
        let notification = with_user_info("label", &other);
        assert_eq!(label_of(&notification), None);
    }

    #[test]
    fn a_label_that_is_not_a_string_is_ignored() {
        let number = NSNumber::new_i32(7);
        let notification = with_user_info(WINDOW_KEY, &number);
        assert_eq!(label_of(&notification), None);
    }
}
