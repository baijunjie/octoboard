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
//! `permissions_status` only reads the current grant and never prompts, so the UI refreshes
//! it freely; `request_permission` is the one place a system prompt can be raised, and
//! nothing else may call `CGRequestScreenCaptureAccess`. A refused Screen Recording prompt is
//! permanent until the user changes it in System Settings: macOS never prompts again and the
//! request returns false silently, which is why `open_privacy_settings` exists. macOS keeps
//! no such refusal for Accessibility — there is no in-place grant to refuse — and will raise that
//! dialog again as often as it is asked.

use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

pub use platform::capture_launch_state;

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

#[tauri::command]
pub fn permissions_status() -> Option<PermissionsStatus> {
    platform::permissions_status()
}

/// Returns whether the permission is granted once the prompt is out of the way. The Screen
/// Recording arm blocks until the user has answered, which is why this is a plain `fn`: Tauri runs
/// those on a worker thread, where blocking costs the window nothing. The Accessibility arm returns
/// at once — its dialog is raised asynchronously (`AXUIElement.h`: "Prompting occurs asynchronously
/// and does not affect the return value").
#[tauri::command]
pub fn request_permission(kind: PermissionKind) -> bool {
    platform::request_permission(kind)
}

/// Opens the System Settings pane for `kind`. For Screen Recording that is the way back from a
/// refusal, which macOS never prompts a second time for; for Accessibility it is the only way the
/// permission is ever granted at all.
///
/// Fails with a message when the system declines to open the pane.
#[tauri::command]
pub fn open_privacy_settings(kind: PermissionKind) -> Result<(), String> {
    platform::open_privacy_settings(kind)
}
