//! The macOS privacy permissions an agent's child process can need (Screen Recording for a
//! screenshot, Accessibility for driving the UI), and the commands the Settings section grants
//! them through.
//!
//! Agents run in Octoboard's embedded terminals, and macOS's TCC attributes a privacy request from
//! a child process to the *responsible process*, which is Octoboard's own bundle. The system prompt
//! then names Octoboard, mid-task, with nothing to say why, and the app cannot intercept a prompt
//! a child raised. So the user is given a place to grant these first, after Octoboard's own
//! explanation. A grant made to another app (the Claude desktop app, say) does not carry over: TCC
//! keys grants on bundle id plus code signature.
//!
//! A Screen Recording grant does not reach a process that is already running — macOS hands the
//! capability out at launch — so the status also carries what this process started with, and the
//! UI tells the user to restart from the two disagreeing. Reading it per process rather than
//! remembering a grant as it happens is what makes the notice survive the Settings section being
//! unmounted and the webview being reloaded.
//!
//! `macos_permissions_status` only reads the current grant and never prompts, so the UI refreshes
//! it freely; `macos_request_permission` is the one place a system prompt can be raised, and
//! nothing else may call `CGRequestScreenCaptureAccess`. A refused Screen Recording prompt is
//! permanent until the user changes it in System Settings: macOS never prompts again and the
//! request returns false silently, which is why `macos_open_privacy_settings` exists. macOS keeps
//! no such refusal for Accessibility — there is no in-place grant to refuse — and will raise that
//! dialog again as often as it is asked.

#[cfg(target_os = "macos")]
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionsStatus {
    screen_recording: bool,
    accessibility: bool,
    /// What Screen Recording was when this process started, which is the only grant it can
    /// actually use: `screen_recording` without this one means the user has granted it but has yet
    /// to restart.
    screen_recording_at_launch: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionKind {
    ScreenRecording,
    Accessibility,
}

/// The four system functions, declared by hand rather than taken from a crate: the rest of this
/// shell reaches macOS through the `objc2` family, but these are plain C entry points in two
/// frameworks nothing here otherwise links, and `core-graphics`/`core-foundation` would be two new
/// dependencies for four symbols.
#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
static SCREEN_RECORDING_AT_LAUNCH: OnceLock<bool> = OnceLock::new();

#[cfg(target_os = "macos")]
fn screen_recording_granted() -> bool {
    // SAFETY: a plain read of the current grant; unlike the request call it never prompts.
    unsafe { ffi::CGPreflightScreenCaptureAccess() }
}

/// Only ever observes the snapshot, never seeds it: seeding here would put the invariant below at
/// the mercy of whoever reads first, and a snapshot taken after the user had already granted would
/// silently stop the restart notice from ever appearing. A miss means `capture_launch_state` did
/// not run, which `true` renders harmless — no restart is asked for — rather than inventing one.
#[cfg(target_os = "macos")]
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
    #[cfg(target_os = "macos")]
    let _ = SCREEN_RECORDING_AT_LAUNCH.set(screen_recording_granted());
}

#[cfg(target_os = "macos")]
fn accessibility_trusted() -> bool {
    // SAFETY: takes no arguments and only reads the process's current trust.
    unsafe { ffi::AXIsProcessTrusted() != 0 }
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub fn macos_permissions_status() -> Option<PermissionsStatus> {
    Some(PermissionsStatus {
        screen_recording: screen_recording_granted(),
        accessibility: accessibility_trusted(),
        screen_recording_at_launch: screen_recording_at_launch(),
    })
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn macos_permissions_status() -> Option<PermissionsStatus> {
    None
}

/// Returns whether the permission is granted once the prompt is out of the way. The Screen
/// Recording arm blocks until the user has answered, which is why this is a plain `fn`: Tauri runs
/// those on a worker thread, where blocking costs the window nothing. The Accessibility arm returns
/// at once — its dialog is raised asynchronously (`AXUIElement.h`: "Prompting occurs asynchronously
/// and does not affect the return value").
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn macos_request_permission(kind: PermissionKind) -> bool {
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

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn macos_request_permission(_kind: PermissionKind) -> bool {
    false
}

/// Opens the System Settings pane for `kind`. For Screen Recording that is the way back from a
/// refusal, which macOS never prompts a second time for; for Accessibility it is the only way the
/// permission is ever granted at all.
///
/// Goes through `NSWorkspace`, not `tauri-plugin-shell`: that plugin's `open` only accepts
/// `http:`, `https:`, `mailto:` and `tel:` addresses and would reject this scheme.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn macos_open_privacy_settings(kind: PermissionKind) {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};

    let url = match kind {
        PermissionKind::ScreenRecording => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        PermissionKind::Accessibility => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
    };
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn macos_open_privacy_settings(_kind: PermissionKind) {}
