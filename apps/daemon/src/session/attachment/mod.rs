//! Files a client attaches to a session, kept as files its agent can read by path.
//!
//! A client that shares no file system with the daemon (a phone) cannot hand the agent a path of
//! its own, so it uploads the file in parts (`create_attachment_upload`, `write_attachment_part`)
//! and saves it (`save_attachment`): the daemon writes it under the session's scratch directory,
//! under a safe form of its original name ([`name`]), and answers with the path, which the client
//! puts into the message it sends. The file goes away with the scratch directory, when the
//! session's process ends.
//!
//! The parts go to a spool file in the same directory as they arrive, so a large file is never
//! held in memory, and the save renames it to its name. An upload that is never saved removes its
//! spool file when it is dropped. An image the user pastes keeps its own request
//! (`crate::session::pasted_image`), whose path the agents take as an attached image.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Context, Result};

use super::scratch::{files_home, not_running, subdirectory};
use super::upload::{Body, Kind, Taken, UploadBudget, Uploads, MAX_PART_BYTES};
use crate::platform::fs::create_private;
use crate::platform::names::with_suffix;
use crate::protocol::AttachmentLimits;
use crate::state::AppState;

pub mod name;

/// The largest file taken, in bytes: documents, logs and archives a phone sends, not bulk
/// transfer.
pub const MAX_BYTES: usize = 100 * 1024 * 1024;

/// The declared sizes of every connection's attachment uploads together, spooled on disk until
/// each is saved or dropped.
pub const MAX_UPLOAD_BYTES: usize = 512 * 1024 * 1024;

/// The directory under a session's scratch directory the files go in.
pub(crate) const DIRECTORY: &str = "attachments";

/// How many numbered names a save tries before it gives up.
const MAX_NAME_TRIES: u32 = 1000;

/// What sets attachment uploads apart.
static ATTACHMENT: Kind = Kind {
    noun: "file",
    max_bytes: MAX_BYTES,
    handle_prefix: "attachment",
    uploads_limit: "attachment_uploads",
    bytes_limit: "attachment_upload_bytes",
};

/// Distinguishes two spool files made within one millisecond.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// What the `snapshot` tells a client of attachments, whose presence says the daemon takes them.
pub fn limits() -> AttachmentLimits {
    AttachmentLimits {
        max_bytes: MAX_BYTES as u64,
        max_part_bytes: MAX_PART_BYTES as u64,
    }
}

impl UploadBudget {
    /// The daemon's budget of attachment uploads, [`MAX_UPLOAD_BYTES`].
    pub fn attachments() -> Self {
        Self::new(MAX_UPLOAD_BYTES)
    }
}

/// One connection's attachment uploads, their parts spooled on disk.
pub type AttachmentUploads = Uploads<Spool>;

impl Uploads<Spool> {
    /// A connection's attachment uploads, under the daemon's attachment budget `budget`.
    pub fn attachments(budget: UploadBudget) -> Self {
        Self::of_kind(&ATTACHMENT, budget)
    }

    /// Starts uploading a file of `size` bytes, called `name` where it came from, for the session
    /// `session_id` (not archived), and returns the upload's handle; its spool file is created at once.
    /// See [`Uploads::create_with`] for the refusals. **Blocks.**
    pub fn create(
        &self,
        state: &AppState,
        session_id: &str,
        name: &str,
        size: u64,
    ) -> Result<String> {
        self.create_with(state, session_id, size, |scratch| {
            Spool::open(session_id, scratch, name::safe_name(name))
        })
    }
}

/// The file an attachment upload's parts are written to as they arrive, removed when dropped
/// unless it was saved.
#[derive(Debug)]
pub struct Spool {
    path: PathBuf,
    /// Open while parts arrive; closed before the save renames it, which Windows refuses for a
    /// file still open.
    file: Option<File>,
    /// The name the saved file is to have, before any number a collision adds.
    name: String,
    saved: bool,
}

impl Spool {
    /// A new, empty spool file in the attachments directory of `scratch`, readable by its owner
    /// alone, for a file to be named `name`. **Blocks.**
    fn open(session_id: &str, scratch: &Path, name: String) -> Result<Self> {
        let directory = subdirectory(session_id, scratch, DIRECTORY)?;
        // Hidden, and never a name `name::safe_name` gives, which never begins with a dot.
        let path = with_suffix(
            &directory.join(format!(
                ".upload-{}-{}",
                crate::time::now_millis(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )),
            ".partial",
        );
        let file = create_private(&path, OpenOptions::new().write(true).create_new(true))
            .with_context(|| format!("creating {}", path.display()))?;
        Ok(Self {
            path,
            file: Some(file),
            name,
            saved: false,
        })
    }
}

impl Body for Spool {
    fn append(&mut self, part: &[u8]) -> Result<()> {
        let Some(file) = self.file.as_mut() else {
            bail!("the file's parts are closed");
        };
        file.write_all(part)
            .with_context(|| format!("writing {}", self.path.display()))
    }
}

impl Drop for Spool {
    /// Removes an upload that was not saved. Done in place, also on an async thread: it is one
    /// unlink in the daemon's own scratch directory, and done later on the blocking pool it could
    /// still be there when a client, or a test, looks right after the upload ended.
    fn drop(&mut self) {
        drop(self.file.take());
        if !self.saved {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Saves the file whose parts the upload `taken` holds for the session `session_id`, under its name,
/// numbered (`report-2.pdf`) when a file of that name is there already, and returns its absolute
/// path. Refused with `unknown_session` or `session_not_running` (an archived session); a failure to write is an
/// error shown as it is. The upload's bytes stay counted against the budget until it returns.
/// **Blocks.**
pub fn save(state: &AppState, session_id: &str, taken: Taken<Spool>) -> Result<PathBuf> {
    let mut spool = taken.body;
    files_home(state, session_id)?;
    drop(spool.file.take());
    let Some(directory) = spool.path.parent().map(Path::to_path_buf) else {
        bail!("the spool file {} has no directory", spool.path.display());
    };
    for n in 1..=MAX_NAME_TRIES {
        let target = directory.join(name::numbered(&spool.name, n));
        // The name is claimed by creating it, so two saves never take one name, and the spool is
        // then renamed over the claim.
        match create_private(&target, OpenOptions::new().write(true).create_new(true)) {
            Ok(claim) => drop(claim),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(not_running(session_id))
            }
            Err(err) => {
                return Err(err).with_context(|| format!("creating {}", target.display()));
            }
        }
        return match std::fs::rename(&spool.path, &target) {
            Ok(()) => {
                spool.saved = true;
                Ok(target)
            }
            Err(err) => {
                let _ = std::fs::remove_file(&target);
                Err(err).with_context(|| format!("writing {}", target.display()))
            }
        };
    }
    bail!(
        "{MAX_NAME_TRIES} files named like {} are there already",
        spool.name
    )
}

#[cfg(test)]
mod tests;
