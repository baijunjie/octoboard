//! The macOS privacy grants: reading them, prompting for them and opening their panes.

use std::sync::OnceLock;

use super::{PermissionKind, PermissionsStatus};
use crate::system_settings::{
    open_system_settings, ACCESSIBILITY_PANE_URL, SCREEN_RECORDING_PANE_URL,
};

/// The four system functions, declared by hand rather than taken from a crate: the rest of this
/// shell reaches macOS through the `objc2` family, but these are plain C entry points in two
/// frameworks nothing here otherwise links, and `core-graphics`/`core-foundation` would be two new
/// dependencies for four symbols.
mod ffi {
    use std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        // C `bool` (macOS 10.15+), so Rust's `bool` matches.
        pub fn CGPreflightScreenCaptureAccess() -> bool;
        pub fn CGRequestScreenCaptureAccess() -> bool;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        // These return `Boolean`, an unsigned char, not C `bool`; reading one as a Rust `bool`
        // would be undefined for any value other than 0 or 1.
        pub fn AXIsProcessTrusted() -> u8;
        pub fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
        /// A `CFStringRef` global.
        pub static kAXTrustedCheckOptionPrompt: *const c_void;
    }
}

static SCREEN_RECORDING_AT_LAUNCH: OnceLock<bool> = OnceLock::new();

fn screen_recording_granted() -> bool {
    // SAFETY: a plain read of the current grant; unlike the request call it never prompts.
    unsafe { ffi::CGPreflightScreenCaptureAccess() }
}

/// Only ever observes the snapshot, never seeds it: seeding here would put the invariant below at
/// the mercy of whoever reads first, and a snapshot taken after the user had already granted would
/// silently stop the restart notice from ever appearing. A miss means `capture_launch_state` did
/// not run, which `true` renders harmless — no restart is asked for — rather than inventing one.
fn screen_recording_at_launch() -> bool {
    match SCREEN_RECORDING_AT_LAUNCH.get() {
        Some(&at_launch) => at_launch,
        None => {
            debug_assert!(
                false,
                "capture_launch_state must run before a status is read"
            );
            true
        }
    }
}

/// Takes the one reading of the Screen Recording grant that says what this process can actually
/// do, before the user can have changed anything. `run` calls it first thing: a grant the user
/// made in System Settings before Octoboard launched is already in place, while one made after is
/// inert until it restarts, and only a snapshot from this point tells the two apart.
pub fn capture_launch_state() {
    let _ = SCREEN_RECORDING_AT_LAUNCH.set(screen_recording_granted());
}

fn accessibility_trusted() -> bool {
    // SAFETY: takes no arguments and only reads the process's current trust.
    unsafe { ffi::AXIsProcessTrusted() != 0 }
}

pub fn permissions_status() -> Option<PermissionsStatus> {
    Some(PermissionsStatus {
        screen_recording: screen_recording_granted(),
        accessibility: accessibility_trusted(),
        screen_recording_at_launch: screen_recording_at_launch(),
    })
}

pub fn request_permission(kind: PermissionKind) -> bool {
    match kind {
        // SAFETY: shows the system prompt the first time and returns the grant; no arguments.
        PermissionKind::ScreenRecording => unsafe { ffi::CGRequestScreenCaptureAccess() },
        PermissionKind::Accessibility => {
            use objc2_foundation::{NSDictionary, NSNumber, NSString};

            // Accessibility has no in-place grant: this shows the system's "allow Octoboard to
            // control this computer" dialog and registers Octoboard in System Settings' list, so
            // the user only flips a switch rather than finding the app with the "+" button. The
            // return value is the trust as of the call, normally still false.
            //
            // SAFETY: `kAXTrustedCheckOptionPrompt` is a constant `CFStringRef`, and `CFString`
            // is toll-free bridged with `NSString`, so reading it as one is sound.
            let key: &NSString = unsafe { &*(ffi::kAXTrustedCheckOptionPrompt as *const NSString) };
            let options = NSDictionary::from_retained_objects(&[key], &[NSNumber::new_bool(true)]);
            // SAFETY: `NSDictionary` is toll-free bridged with `CFDictionaryRef`, and `options`
            // outlives the call.
            unsafe {
                ffi::AXIsProcessTrustedWithOptions(
                    &*options as *const NSDictionary<NSString, NSNumber> as *const _,
                ) != 0
            }
        }
    }
}

pub fn open_privacy_settings(kind: PermissionKind) -> Result<(), String> {
    open_system_settings(match kind {
        PermissionKind::ScreenRecording => SCREEN_RECORDING_PANE_URL,
        PermissionKind::Accessibility => ACCESSIBILITY_PANE_URL,
    })
}
