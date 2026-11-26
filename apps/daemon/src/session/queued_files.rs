//! The files of a message put on a session's composer queue: the images and attached files saved
//! for the session (`save_pasted_image`, `save_attachment`) are moved out of its scratch
//! directory, which goes with its process, into its composer directory
//! (`AppState::composer_dir`), which lasts as long as the session's record, so a queued message
//! still finds them after the session's interruption or a daemon restart. See "The composer
//! queue" in `apps/daemon/PROTOCOL.md`.

use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::attachment::name::numbered;
use super::{attachment, pasted_image};
use crate::platform::fs::create_private;
use crate::protocol::{error_code, CodedError};
use crate::state::AppState;

/// The most names tried for a moved file whose name the composer directory holds already.
const MAX_NAME_TRIES: u32 = 1000;

/// The images and files of a queued message as it will carry them, and the moves that make them
/// so: checked and their names in the composer directory claimed, nothing moved yet. The caller
/// [`Self::commit`]s the moves before it changes the queue, so the message never names a file not
/// there yet, and undoes them ([`Moved::undo`]) where the change is refused; or [`Self::abandon`]s
/// the plan.
#[must_use]
pub(crate) struct Planned {
    pub images: Vec<String>,
    pub files: Vec<String>,
    /// Each file to move, the empty file claiming its name in the composer directory, and the
    /// field the path came in, for a refusal naming it.
    moves: Vec<(PathBuf, PathBuf, &'static str)>,
}

/// The files a committed plan moved, which [`Self::undo`] moves back.
#[must_use]
pub(crate) struct Moved {
    moves: Vec<(PathBuf, PathBuf)>,
}

impl Moved {
    /// The paths in the composer directory the files were moved to.
    pub fn paths(&self) -> Vec<String> {
        self.moves
            .iter()
            .map(|(_, to)| to.to_string_lossy().into_owned())
            .collect()
    }

    /// Moves every file back where it came from, for a queue change that was refused. **Blocks.**
    pub fn undo(self) {
        for (from, to) in self.moves.iter().rev() {
            if let Err(err) = move_back(to, from) {
                tracing::warn!(%err, "moving a queued message's file back failed");
            }
        }
    }
}

impl Planned {
    /// Moves every file onto its claimed name. A file gone meanwhile (another request moved it
    /// first) is refused with `unreadable_request` naming its field, and the files moved before it
    /// are moved back, the other claims given back: then nothing is moved. **Blocks.**
    pub fn commit(self) -> Result<Moved> {
        let mut done = Vec::new();
        let mut moves = self.moves.into_iter();
        while let Some((from, to, field)) = moves.next() {
            if let Err(err) = move_onto_claim(&from, &to) {
                let _ = std::fs::remove_file(&to);
                for (_, rest, _) in moves {
                    let _ = std::fs::remove_file(rest);
                }
                Moved { moves: done }.undo();
                let gone = err
                    .downcast_ref::<io::Error>()
                    .is_some_and(|err| err.kind() == io::ErrorKind::NotFound);
                return Err(if gone {
                    CodedError::raised(
                        error_code::UNREADABLE_REQUEST,
                        format!("`{}` is no longer there", from.display()),
                        &[("field", field)],
                    )
                } else {
                    err
                });
            }
            done.push((from, to));
        }
        Ok(Moved { moves: done })
    }

    /// Gives the claimed names back, moving nothing. **Blocks.**
    pub fn abandon(self) {
        for (_, to, _) in &self.moves {
            let _ = std::fs::remove_file(to);
        }
    }
}

/// Plans the images and files of a queued message of the session `session_id`
/// ([`Planned`]). Each image is one saved for the session, in its scratch directory's or its
/// composer directory's `pasted` directory: one in the scratch directory is to be moved into the
/// composer directory, one there already keeps its path. A file that is an attachment saved in
/// the session's scratch directory is to be moved likewise; any other path is kept as given.
/// Refused with `unreadable_request` (`field: images`) for an image that is none of the session's,
/// with nothing claimed. **Blocks.**
pub(crate) fn plan(
    state: &AppState,
    session_id: &str,
    images: &[String],
    files: &[String],
) -> Result<Planned> {
    let queued_images = state.composer_dir(session_id).join(pasted_image::DIRECTORY);
    let scratch_images = scratch_subdirectory(state, session_id, pasted_image::DIRECTORY);
    let queued_files = state.composer_dir(session_id).join(attachment::DIRECTORY);
    let scratch_files = scratch_subdirectory(state, session_id, attachment::DIRECTORY);
    // A path named twice would be claimed twice and moved once, its second move finding it gone.
    let mut named = std::collections::HashSet::new();
    for (path, field) in images
        .iter()
        .map(|image| (image, "images"))
        .chain(files.iter().map(|file| (file, "files")))
    {
        if !named.insert(path) {
            return Err(CodedError::raised(
                error_code::UNREADABLE_REQUEST,
                format!("`{path}` is named more than once"),
                &[("field", field)],
            ));
        }
    }
    // Every image checked before any name is claimed.
    let mut sources = Vec::new();
    for image in images {
        let path = Path::new(image);
        let named = path.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .starts_with(pasted_image::NAME_PREFIX)
        });
        let in_queued = path.parent() == Some(queued_images.as_path());
        let in_scratch = scratch_images
            .as_deref()
            .is_some_and(|scratch| path.parent() == Some(scratch));
        if !(named && (in_queued || in_scratch) && is_file(path)?) {
            return Err(CodedError::raised(
                error_code::UNREADABLE_REQUEST,
                format!("`{image}` is not an image saved for this session"),
                &[("field", "images")],
            ));
        }
        sources.push(in_scratch);
    }
    let mut planned = Planned {
        images: Vec::new(),
        files: Vec::new(),
        moves: Vec::new(),
    };
    for (image, moving) in images.iter().zip(sources) {
        if moving {
            match claim(Path::new(image), &queued_images) {
                Ok(to) => {
                    planned.images.push(to.to_string_lossy().into_owned());
                    planned.moves.push((PathBuf::from(image), to, "images"));
                }
                Err(err) => {
                    planned.abandon();
                    return Err(err);
                }
            }
        } else {
            planned.images.push(image.clone());
        }
    }
    for file in files {
        let path = Path::new(file);
        let saved = scratch_files
            .as_deref()
            .is_some_and(|scratch| path.parent() == Some(scratch))
            && path
                .file_name()
                .is_some_and(|name| !name.to_string_lossy().starts_with('.'));
        let saved = match saved.then(|| is_file(path)).transpose() {
            Ok(saved) => saved.unwrap_or(false),
            Err(err) => {
                planned.abandon();
                return Err(err);
            }
        };
        if !saved {
            planned.files.push(file.clone());
            continue;
        }
        match claim(path, &queued_files) {
            Ok(to) => {
                planned.files.push(to.to_string_lossy().into_owned());
                planned.moves.push((path.to_path_buf(), to, "files"));
            }
            Err(err) => {
                planned.abandon();
                return Err(err);
            }
        }
    }
    Ok(planned)
}

/// [`plan`] for images alone, committed at once.
#[cfg(test)]
pub(crate) fn take_images(
    state: &AppState,
    session_id: &str,
    images: &[String],
) -> Result<Vec<String>> {
    let planned = plan(state, session_id, images, &[])?;
    let images = planned.images.clone();
    let _ = planned.commit()?;
    Ok(images)
}

/// [`plan`] for files alone, committed at once.
#[cfg(test)]
pub(crate) fn take_files(
    state: &AppState,
    session_id: &str,
    files: &[String],
) -> Result<Vec<String>> {
    let planned = plan(state, session_id, &[], files)?;
    let files = planned.files.clone();
    let _ = planned.commit()?;
    Ok(files)
}

/// Whether `path` is a file: false only where it is missing, an error where it cannot be told.
fn is_file(path: &Path) -> Result<bool> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

/// The directory `name` of the running session's scratch directory, if it runs.
fn scratch_subdirectory(state: &AppState, session_id: &str, name: &str) -> Option<PathBuf> {
    state
        .live_session(session_id)
        .and_then(|live| live.scratch_dir())
        .map(|scratch| scratch.join(name))
}

/// Claims a name for the file `from` in `directory`, created the owner's alone if missing: its
/// own name, numbered where that is taken, claimed by creating it, so two claims never take one
/// name (as `attachment::save` claims a saved file's).
fn claim(from: &Path, directory: &Path) -> Result<PathBuf> {
    crate::platform::fs::create_private_dir(directory)
        .with_context(|| format!("creating {}", directory.display()))?;
    let Some(name) = from
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    else {
        bail!("{} has no name", from.display());
    };
    for n in 1..=MAX_NAME_TRIES {
        let target = directory.join(numbered(&name, n));
        match create_private(&target, OpenOptions::new().write(true).create_new(true)) {
            Ok(claim) => {
                drop(claim);
                return Ok(target);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err).with_context(|| format!("creating {}", target.display())),
        }
    }
    bail!("{MAX_NAME_TRIES} files named like {name} are there already")
}

/// Moves `to` back to `from`, where it came from, for a refused change: the original name is
/// free again, as the move took it away.
fn move_back(to: &Path, from: &Path) -> Result<()> {
    move_onto_claim(to, from)
}

/// Moves `from` onto `to`, the name claimed for it, replacing the claim: renamed, or, across
/// file systems, copied and the original removed. A missing `from` is the `io::Error` itself.
fn move_onto_claim(from: &Path, to: &Path) -> Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::CrossesDevices => {
            std::fs::copy(from, to).with_context(|| format!("writing {}", to.display()))?;
            if let Err(err) = std::fs::remove_file(from) {
                if err.kind() != io::ErrorKind::NotFound {
                    tracing::warn!(%err, "removing a moved file's original failed");
                }
            }
            Ok(())
        }
        Err(err) => {
            Err(err).with_context(|| format!("moving {} to {}", from.display(), to.display()))
        }
    }
}

#[cfg(test)]
mod tests;
