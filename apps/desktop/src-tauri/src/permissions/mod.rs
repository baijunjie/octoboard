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
//! A Screen Recording grant made while Octoboard runs makes macOS offer to quit the app, and
//! `CGPreflightScreenCaptureAccess` keeps answering false in a process launched before the grant,
//! which is why the grant is read from a fresh process (see `macos.rs`). Octoboard itself captures
//! nothing, though. The agents' processes do, and macOS judges each by its own launch, so a capture
//! command an agent starts after the grant has it, even in a session opened before it; only a
//! process already running before the grant that captures by itself might still go without. So the
//! Settings section asks for no restart.
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

pub use platform::answer_probe_if_asked;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionsStatus {
    screen_recording: bool,
    accessibility: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionKind {
    ScreenRecording,
    Accessibility,
}

/// Async because the Screen Recording reading starts a process and waits for it, which is not to
/// hold up the thread commands run on otherwise.
#[tauri::command(async)]
pub fn permissions_status() -> Option<PermissionsStatus> {
    platform::permissions_status()
}

/// Returns whether the permission is granted as of the call. Neither arm waits for the user's
/// answer: the Screen Recording request raises the system prompt and returns, still false, and the
/// Accessibility dialog is raised asynchronously too (`AXUIElement.h`: "Prompting occurs
/// asynchronously and does not affect the return value"). Async all the same, since Tauri runs a
/// plain command inline on the main thread, and raising the Screen Recording prompt is not free: the
/// request took 43 ms when measured from a worker thread.
#[tauri::command(async)]
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
