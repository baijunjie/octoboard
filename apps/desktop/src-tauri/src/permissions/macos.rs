//! The macOS privacy grants: reading them, prompting for them and opening their panes.

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

/// The argument that has Octoboard's own executable print its Screen Recording grant and exit. A
/// contract between versions: the updater replaces the bundle in place before the restart, so a
/// running version may start the next one's executable with it.
const SCREEN_RECORDING_PROBE_ARG: &str = "--screen-recording-probe";

/// How long a probe may take before its answer is given up on; the first run of a freshly built
/// executable takes over a second.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Answers `SCREEN_RECORDING_PROBE_ARG` when this process was started with it, which `main` asks
/// before anything else: it prints `1` or `0` and the process exits without starting the app.
pub fn answer_probe_if_asked() -> bool {
    if std::env::args().nth(1).as_deref() != Some(SCREEN_RECORDING_PROBE_ARG) {
        return false;
    }
    print!("{}", u8::from(preflight_in_this_process()));
    true
}

fn preflight_in_this_process() -> bool {
    // SAFETY: a plain read of the grant; unlike the request call it never prompts.
    unsafe { ffi::CGPreflightScreenCaptureAccess() }
}

/// Read from a fresh process rather than this one: in an Octoboard launched without the grant,
/// `CGPreflightScreenCaptureAccess` keeps answering false after the user grants it, while a process
/// started since — every capture command an agent runs, and this probe — sees the grant at once.
/// The probe is Octoboard's own executable, so it needs no helper of its own. Should it fail to
/// start, answer or finish in time, the reading falls back to this process's own.
fn screen_recording_granted() -> bool {
    probe_screen_recording().unwrap_or_else(preflight_in_this_process)
}

fn probe_screen_recording() -> Option<bool> {
    let mut probe = Command::new(std::env::current_exe().ok()?)
        .arg(SCREEN_RECORDING_PROBE_ARG)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match probe.try_wait() {
            Ok(Some(status)) if status.success() => {
                // One byte, so the pipe cannot fill and stall the probe before it exits.
                let mut answer = Vec::new();
                probe.stdout.take()?.read_to_end(&mut answer).ok()?;
                return match answer.as_slice() {
                    b"1" => Some(true),
                    b"0" => Some(false),
                    _ => None,
                };
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(Some(_)) => return None,
            Ok(None) | Err(_) => {
                let _ = probe.kill();
                let _ = probe.wait();
                return None;
            }
        }
    }
}

fn accessibility_trusted() -> bool {
    // SAFETY: takes no arguments and only reads the process's current trust.
    unsafe { ffi::AXIsProcessTrusted() != 0 }
}

pub fn permissions_status() -> Option<PermissionsStatus> {
    Some(PermissionsStatus {
        screen_recording: screen_recording_granted(),
        accessibility: accessibility_trusted(),
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
