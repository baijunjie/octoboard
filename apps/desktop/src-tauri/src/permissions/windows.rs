//! No privacy grants to give on Windows, so the commands answer as if there were none, and the UI
//! shows no section for them (`permissionsPane` in `host_facts.rs`). This module serves every
//! target but macOS, Linux included.

use super::{PermissionKind, PermissionsStatus};

/// Nothing to take.
pub fn capture_launch_state() {}

pub fn permissions_status() -> Option<PermissionsStatus> {
    None
}

pub fn request_permission(_kind: PermissionKind) -> bool {
    false
}

pub fn open_privacy_settings(_kind: PermissionKind) -> Result<(), String> {
    Ok(())
}
