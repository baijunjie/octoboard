//! Telling the `octoboardd` sidecar the application is quitting: the quit record left in its data
//! directory, and the quit note written to its standard input. A daemon that finds the application
//! gone makes an ordinary stop after the note and a planned one without it, which is how a crash
//! leaves the sessions running for the next launch. Stopping the daemon is `stop.rs`.
//!
//! The record is the durable half: it is left on every quit noted, before the note is written and
//! whether or not the daemon is up or the write succeeds, as a successful write proves only that
//! the pipe took the bytes, not that the daemon acted on them. Every daemon start reconciles it, so
//! a daemon that ended, never read the note or was killed first still has its sessions interrupted,
//! as the quit meant, when the next one starts. The one exception is a quit with no daemon of this
//! application's own that may hold holders: none stored, none up this run, none killed while it
//! still ran. A daemon that exited on its own before its startup line, as one does when another
//! `octoboardd` holds the instance lock, leaves no record, which would end that other daemon's
//! sessions at its next start.

use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Manager};

use super::{mark_daemon_stopping, ChildSlot, DaemonState};
use crate::diagnostics::{data_dir, diagnostic};

/// What the application tells the daemon, on its standard input, when it is quitting. A daemon that
/// finds the application gone after reading it makes an ordinary stop; without it, a planned one
/// that leaves the sessions running for the next launch.
pub(super) const QUIT_NOTE: &[u8] = b"quit\n";

/// The name of the quit record in the daemon's data directory, which has to be the name the daemon
/// looks for.
const QUIT_RECORD_FILE: &str = "application-quit";

/// The content of the quit record: a versioned JSON record naming no scope, which has the daemon's
/// next start end every holder its registry lists, leaving the sessions interrupted as an ordinary
/// stop does.
const QUIT_RECORD: &[u8] = b"{\"version\":1}\n";

/// Whether the quit note is still to be written, has been, or is never to be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum QuitNote {
    /// No quit has been noted yet.
    #[default]
    Open,
    /// A quit has been noted: the record is left, the note has gone to the daemon there was, and
    /// goes to any daemon stored from now on.
    Written,
    /// The application is making a planned stop for an update restart, which is no quit
    /// (`skip_quit_note`): the note is never written, unless the system ends the session
    /// meanwhile (`begin_session_end`).
    Skipped,
}

impl QuitNote {
    /// Records a quit; true for the first one, which is to leave the record and write the note.
    /// A quit after an update restart's skip is no quit.
    fn begin_quit(&mut self) -> bool {
        if *self != Self::Open {
            return false;
        }
        *self = Self::Written;
        true
    }

    /// Records the system ending the user's session, which is a quit even where an update restart's
    /// planned stop is under way. Returns what the note was before: the record and the note are due
    /// unless that was `Written`, and `Skipped` says a planned stop is under way.
    fn begin_session_end(&mut self) -> Self {
        std::mem::replace(self, Self::Written)
    }

    /// Leaves the note unwritten from now on, unless a quit has been noted already, which cannot be
    /// undone.
    fn skip(&mut self) {
        if *self == Self::Open {
            *self = Self::Skipped;
        }
    }

    /// Whether a daemon started from now on is told the quit.
    pub(super) fn owed_to_new_daemon(self) -> bool {
        self == Self::Written
    }
}

/// Leaves the quit record when `record_due`, then writes the note: in this order, and the note
/// whatever became of the record.
fn announce(record_due: bool, leave_record: impl FnOnce(), write_note: impl FnOnce()) {
    if record_due {
        leave_record();
    }
    write_note();
}

impl ChildSlot {
    /// Tells the daemon the quit: leaves the quit record when a daemon of this application's own is
    /// behind the quit, one stored now (alive or still starting) or `may_hold_holders` during this
    /// run, then writes `QUIT_NOTE` to the one stored. Both paths of a quit (`note_quit`,
    /// `note_session_end`) do it here, under the lock the child is stored under.
    fn announce_quit(&mut self, may_hold_holders: bool) {
        let record_due = may_hold_holders || self.child.is_some();
        announce(record_due, leave_quit_record, || {
            if let Some(child) = self.child.as_mut() {
                let _ = child.write(QUIT_NOTE);
            }
        });
    }
}

/// Tells the daemon the application is quitting: leaves the quit record (`QUIT_RECORD_FILE`) for
/// the daemon's next start, writes `QUIT_NOTE` to the standard input of the running one, and
/// records that no daemon is to be started from now on (`ChildSlot::announce_quit`; no record where
/// this application never had a daemon of its own). The daemon makes an ordinary stop when it finds
/// the application gone after this, and a planned one otherwise. Done at most once, under the lock
/// the child is stored under, so a daemon spawned around the quit is told too (`ChildSlot::store`);
/// never waits for the daemon and ignores a write that fails. Called on every way out that is a
/// quit, a daemon already gone included.
pub fn note_quit(app: &AppHandle) {
    mark_daemon_stopping(app);
    let state = app.state::<DaemonState>();
    let mut slot = state.child.lock().unwrap_or_else(|e| e.into_inner());
    // Read under the slot lock: a start attempt sets it before it takes its child out under the
    // same lock, so a quit that finds no child finds the flag already set.
    let may_hold_holders = state.may_hold_holders.load(Ordering::SeqCst);
    if slot.note.begin_quit() {
        slot.announce_quit(may_hold_holders);
    }
}

/// `note_quit` for the system ending the user's session, which is a quit even while an update
/// restart's planned stop is under way (`skip_quit_note`): the record and the note go out unless
/// an earlier quit wrote them, under the same rule about a daemon of the application's own.
/// Returns whether a planned stop was under way, which a request for the stop does not reach if the
/// UI's had not arrived (`stop_daemon_at_session_end`).
pub(super) fn note_session_end(app: &AppHandle) -> bool {
    mark_daemon_stopping(app);
    let state = app.state::<DaemonState>();
    let mut slot = state.child.lock().unwrap_or_else(|e| e.into_inner());
    // Read under the slot lock: a start attempt sets it before it takes its child out under the
    // same lock, so a quit that finds no child finds the flag already set.
    let may_hold_holders = state.may_hold_holders.load(Ordering::SeqCst);
    let before = slot.note.begin_session_end();
    if before != QuitNote::Written {
        slot.announce_quit(may_hold_holders);
    }
    before == QuitNote::Skipped
}

/// True while an update restart's planned stop is under way and nothing has been noted over it:
/// no quit, no session end.
pub(super) fn planned_stop_under_way(app: &AppHandle) -> bool {
    app.state::<DaemonState>()
        .child
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .note
        == QuitNote::Skipped
}

/// Leaves the quit note unwritten from now on: the application is making an update restart's
/// planned stop, and its exit into the relaunch must not read as a quit to the daemon.
pub fn skip_quit_note(app: &AppHandle) {
    app.state::<DaemonState>()
        .child
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .note
        .skip();
}

/// Leaves the quit record in the daemon's data directory; a failure is logged, as nothing else can
/// be done about it.
fn leave_quit_record() {
    let Some(dir) = data_dir() else {
        diagnostic!("the home directory is unknown: the quit record is not left");
        return;
    };
    if let Err(err) = write_quit_record(&dir) {
        diagnostic!("cannot leave the quit record in {}: {err}", dir.display());
    }
}

/// Writes the quit record atomically, a temporary file in the same directory renamed over the
/// target, so the daemon never reads half of it. A record that is there is replaced.
fn write_quit_record(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let temp = dir.join(format!("{QUIT_RECORD_FILE}.{}.tmp", std::process::id()));
    let written = std::fs::File::create(&temp)
        .and_then(|mut file| {
            file.write_all(QUIT_RECORD)?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temp, dir.join(QUIT_RECORD_FILE)));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn the_first_quit_is_recorded_once() {
        let mut note = QuitNote::default();
        assert!(note.begin_quit());
        assert!(!note.begin_quit());
        assert!(note.owed_to_new_daemon());
    }

    #[test]
    fn an_update_restart_is_no_quit() {
        let mut note = QuitNote::default();
        note.skip();
        assert!(!note.begin_quit());
        assert!(!note.owed_to_new_daemon());
    }

    #[test]
    fn a_quit_noted_cannot_be_skipped_and_is_owed_to_a_daemon_stored_after_it() {
        let mut note = QuitNote::default();
        assert!(!note.owed_to_new_daemon());
        note.begin_quit();
        assert!(note.owed_to_new_daemon());
        note.skip();
        assert!(note.owed_to_new_daemon());
    }

    #[test]
    fn a_session_end_is_a_quit_even_over_an_update_restarts_skip() {
        let mut note = QuitNote::default();
        note.skip();
        assert_eq!(note.begin_session_end(), QuitNote::Skipped);
        assert!(note.owed_to_new_daemon());
        // The restart's own later skip changes nothing.
        note.skip();
        assert!(note.owed_to_new_daemon());
    }

    #[test]
    fn a_session_end_after_a_quit_finds_the_note_written() {
        let mut note = QuitNote::default();
        note.begin_quit();
        assert_eq!(note.begin_session_end(), QuitNote::Written);
        let mut open = QuitNote::default();
        assert_eq!(open.begin_session_end(), QuitNote::Open);
        assert!(open.owed_to_new_daemon());
    }

    #[test]
    fn the_record_is_left_before_the_note_is_written() {
        let order = RefCell::new(Vec::new());
        announce(
            true,
            || order.borrow_mut().push("record"),
            || order.borrow_mut().push("note"),
        );
        assert_eq!(order.into_inner(), ["record", "note"]);
    }

    #[test]
    fn no_record_is_left_for_a_quit_with_no_daemon_of_the_applications_own() {
        let order = RefCell::new(Vec::new());
        announce(
            false,
            || order.borrow_mut().push("record"),
            || order.borrow_mut().push("note"),
        );
        assert_eq!(order.into_inner(), ["note"]);
    }

    #[test]
    fn the_record_is_an_unscoped_versioned_json_record_that_replaces_any_there() {
        let dir =
            std::env::temp_dir().join(format!("octoboard-quit-record-{}", std::process::id()));
        write_quit_record(&dir).unwrap();
        let path = dir.join(QUIT_RECORD_FILE);
        let record: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(record, serde_json::json!({ "version": 1 }));
        // Writing it again, over one of another content, is no error, and leaves nothing behind.
        std::fs::write(&path, b"stale").unwrap();
        write_quit_record(&dir).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), QUIT_RECORD);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
