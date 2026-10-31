//! Moving a file or directory to the system Trash.

use std::fmt;
use std::path::Path;

/// Why a path could not be moved to the Trash: the path as shown and the trash crate's detail. A
/// plain I/O-level failure: the caller decides which coded error, if any, it becomes.
#[derive(Debug)]
pub struct TrashFailure {
    pub path: String,
    pub detail: String,
}

impl TrashFailure {
    pub fn new(path: &Path, detail: impl fmt::Display) -> Self {
        Self {
            path: path.to_string_lossy().into_owned(),
            detail: detail.to_string(),
        }
    }
}

impl fmt::Display for TrashFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "could not move {} to the Trash: {}",
            self.path, self.detail
        )
    }
}

impl std::error::Error for TrashFailure {}

/// Moves `path` to the system Trash, where it can be recovered. On macOS this goes through
/// `NSFileManager`: the default, Finder through AppleScript, needs an Automation permission the
/// hardened app does not hold. It costs the Finder's "Put Back" option; the folder is still
/// recoverable from the Trash.
pub fn move_to_trash(path: &Path) -> Result<(), TrashFailure> {
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut context = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        context.set_delete_method(DeleteMethod::NsFileManager);
    }
    context
        .delete(path)
        .map_err(|err| TrashFailure::new(path, err))
}
