//! The macOS entry: the app's own login item, `SMAppService.mainApp` (macOS 13 and later, where the
//! class exists; before it the switch is not offered). It registers and unregisters the bundle
//! itself and appears in System Settings' Login Items, where the user can also turn it off or leave
//! it waiting for approval. The calls are made by hand through `objc2`'s message sends, as the
//! framework has no binding among the crates this shell already depends on.
//!
//! A launch for the entry is recognised by the Apple event that opened the application: its
//! `keyAEPropData` parameter names `keyAELaunchedAsLogInItem` (`lgit`), the way AppKit apps have
//! told a login launch from any other since login items existed.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::NSError;

use super::Status;
use crate::apple_event::{current_enum_code, Slot};
use crate::system_settings::{open_system_settings, LOGIN_ITEMS_PANE_URL};

#[link(name = "ServiceManagement", kind = "framework")]
extern "C" {}

/// `SMAppServiceStatusEnabled`, and the two that are not it: `NotRegistered` (0),
/// `RequiresApproval` (2) and `NotFound` (3).
const STATUS_ENABLED: isize = 1;
const STATUS_REQUIRES_APPROVAL: isize = 2;

/// `keyAEPropData` ('prdt') and `keyAELaunchedAsLogInItem` ('lgit').
const KEY_PROP_DATA: u32 = u32::from_be_bytes(*b"prdt");
const LAUNCHED_AS_LOGIN_ITEM: u32 = u32::from_be_bytes(*b"lgit");

/// The `SMAppService` class, which only macOS 13 and later has.
fn service_class() -> Option<&'static AnyClass> {
    AnyClass::get(c"SMAppService")
}

pub fn supported() -> bool {
    service_class().is_some()
}

fn main_app_service() -> Result<Retained<AnyObject>, String> {
    let class = service_class()
        .ok_or_else(|| "this macOS has no login items API (it needs macOS 13)".to_string())?;
    // SAFETY: `mainAppService` is a class method that takes nothing and returns the service of
    // the main bundle.
    let service: Option<Retained<AnyObject>> = unsafe { msg_send![class, mainAppService] };
    service.ok_or_else(|| "the system gave no login item for this app".to_string())
}

fn status_of(status: isize) -> Status {
    match status {
        STATUS_ENABLED => Status::Enabled,
        STATUS_REQUIRES_APPROVAL => Status::RequiresApproval,
        _ => Status::Disabled,
    }
}

pub fn status() -> Result<Status, String> {
    let service = main_app_service()?;
    // SAFETY: `status` takes nothing and returns an `NSInteger`.
    let status: isize = unsafe { msg_send![&*service, status] };
    Ok(status_of(status))
}

pub fn set(enabled: bool) -> Result<Status, String> {
    let service = main_app_service()?;
    // SAFETY: both methods take only the error out-parameter, which `objc2` turns into a `Result`.
    let result: Result<(), Retained<NSError>> = unsafe {
        if enabled {
            msg_send![&*service, registerAndReturnError: _]
        } else {
            msg_send![&*service, unregisterAndReturnError: _]
        }
    };
    match result {
        Ok(()) => status(),
        // Unregistering an entry that is not there fails; it is off all the same.
        Err(_) if !enabled && matches!(status(), Ok(Status::Disabled)) => Ok(Status::Disabled),
        // Registering an entry that the user has left unapproved in System Settings fails, but the
        // entry is there and waits for the approval, which is a state of its own and not an error.
        Err(_) if enabled && matches!(status(), Ok(Status::RequiresApproval)) => {
            Ok(Status::RequiresApproval)
        }
        Err(err) => Err(err.localizedDescription().to_string()),
    }
}

/// Opens System Settings at its list of login items.
pub fn open_settings() -> Result<(), String> {
    open_system_settings(LOGIN_ITEMS_PANE_URL)
}

/// Whether the Apple event being handled now is the launch the login item made. Meaningful only
/// where that event is current: during AppKit's launch, which `setup` runs inside.
pub fn launched_at_login() -> bool {
    current_enum_code(KEY_PROP_DATA, Slot::Param).is_some_and(is_login_item_code)
}

/// Whether `code`, the enum value of the launch event's `keyAEPropData`, says a login item launched
/// the app.
fn is_login_item_code(code: u32) -> bool {
    code == LAUNCHED_AS_LOGIN_ITEM
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_login_item_launch_code_counts() {
        assert!(is_login_item_code(u32::from_be_bytes(*b"lgit")));
        // `keyAELaunchedAsServiceItem`, and no property at all.
        assert!(!is_login_item_code(u32::from_be_bytes(*b"svit")));
        assert!(!is_login_item_code(0));
    }

    #[test]
    fn the_status_codes_map_to_the_switch_states() {
        assert_eq!(status_of(1), Status::Enabled);
        assert_eq!(status_of(2), Status::RequiresApproval);
        assert_eq!(status_of(0), Status::Disabled);
        assert_eq!(status_of(3), Status::Disabled);
    }
}
