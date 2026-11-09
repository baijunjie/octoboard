//! The screens a planned stop saved for the next daemon: written by the daemon that stops to
//! `restart-screens.json` in the data directory (owner-only, replaced whole) with the handover's
//! token and the format's version, and read by the next daemon as it starts, before it adopts
//! anything. A file that cannot be read, does not read, or is of a version this build does not
//! read, is deleted, and every screen is rebuilt from its tail.
//!
//! Loaded, selected, committed. The file is read whole at start
//! ([`AppState::load_restart_screens`]) and stays until each screen in it is consumed, so that a
//! start that fails, or a holder linked to only later, still finds its screen. Selecting a screen
//! takes nothing: for an adopted terminal ([`AppState::select_exact_screen`]), or for the preview
//! of one whose holder is gone ([`AppState::select_restart_screen`]). A screen is consumed only
//! once what becomes of its terminal is settled, the screens of one holder's terminals together,
//! in one rewrite of the file:
//!
//! - For an adopted or ended terminal, by [`AppState::commit_restart_screens`], before any of its
//!   holder's terminals' readers starts, so that no later start replays the tail since the
//!   handover over one again and answers the queries in it twice. A file that cannot be rewritten
//!   is deleted, and the screens are not used.
//! - For a terminal whose holder is gone, by [`AppState::expire_restart_screens`], once the
//!   previews made of its screens are written and what becomes of the holder's sessions is
//!   settled. With them go the screens of its shell terminals and of sessions that need no
//!   preview, and those whose preview could not be written: nothing could use them any more.
//!
//! [`AppState::expire_restart_screens`] also drops the screens of a holder taken over that none of
//! its terminals took, which are of terminals it no longer runs. Any still left stay until the
//! next planned stop replaces the file ([`replace`]), which carries over, each with the token of
//! the handover it was saved at, only those of the holders still pending then
//! ([`AppState::pending_restart_screens`]): a holder linked to only after that stop still reports
//! the earlier handover.
//!
//! A screen is restored only when its terminal's handover is the one the screen was saved at
//! ([`verify`]): the holder reports the same token, its tail since the handover is whole, and the
//! terminal still has the screen's size. Then the screen is restored as it was
//! (`SavedScreen::restore_into`) and only the tail since the handover is replayed over it;
//! otherwise the adoption rebuilds the screen from the tail, as after a crash.

use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::AppState;
use crate::holder::client::{FoundTerminal, HeldAt};
use crate::holder::protocol::{HandoverTail, OfflineAnswer};
use crate::hostfs::replace::{remove_replaced, replace_beside, Permissions};
use crate::platform::term::Size;
use crate::terminal::saved_screen::{ReplayRule, SavedScreen};

/// The version of the file's format this build writes. Raised when the format, or what a saved
/// screen's bytes or marks mean, changes in a way another build would read wrongly; a file of a
/// version outside [`READ_VERSIONS`] is not read.
const VERSION: u32 = 1;

/// The versions this build reads: its own, and every earlier one a daemon it can take over from at
/// a planned restart writes, so that an upgrade across that range keeps the screens exact.
const READ_VERSIONS: RangeInclusive<u32> = 1..=VERSION;

/// Keeps two writes of the file in one process, which stage under one name beside it, from
/// running at once: the planned stop's ([`replace`]) and a start's rewrite as it consumes screens.
static FILE_WRITES: Mutex<()> = Mutex::new(());

/// What the file holds, as read once its version is known to be one this build reads
/// ([`Version`]).
#[derive(Debug, Deserialize)]
struct ScreensFile {
    /// The handover's token, which each holder reports again with the tail it kept since.
    token: String,
    screens: Vec<ScreenEntry>,
}

#[derive(Debug, Deserialize)]
struct ScreenEntry {
    /// The token of the handover this screen was saved at, where it is not the file's: a screen
    /// an earlier handover saved, carried over for a holder still pending.
    #[serde(default)]
    token: Option<String>,
    holder_id: String,
    terminal_id: String,
    screen: SavedScreen,
}

/// Only the version of a file, read first so that a file of another version is never read as
/// this one's format. Absent from a file written before the format had versions, which no build
/// reads.
#[derive(Deserialize)]
struct Version {
    #[serde(default)]
    version: u32,
}

/// [`ScreensFile`] as written, borrowing the screens; built by [`ScreensOut::new`].
#[derive(Serialize)]
struct ScreensOut<'a> {
    version: u32,
    token: &'a str,
    screens: Vec<ScreenOut<'a>>,
}

#[derive(Serialize)]
struct ScreenOut<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<&'a str>,
    holder_id: &'a str,
    terminal_id: &'a str,
    screen: &'a SavedScreen,
}

impl<'a> ScreensOut<'a> {
    /// The file of the handover of `token`, holding `screens`, each with the token of the handover
    /// it was saved at, or none for `token`'s: a screen's token is written only where it is not the
    /// file's.
    fn new(token: &'a str, screens: impl IntoIterator<Item = ScreenOut<'a>>) -> Self {
        Self {
            version: VERSION,
            token,
            screens: screens
                .into_iter()
                .map(|screen| ScreenOut {
                    token: screen.token.filter(|own| *own != token),
                    ..screen
                })
                .collect(),
        }
    }
}

/// The saved screens read at start, by holder and terminal, until each is taken.
#[derive(Default)]
pub struct RestartScreens {
    loaded: Mutex<Loaded>,
}

#[derive(Default)]
struct Loaded {
    /// The file they were read from, which is rewritten as they are taken; `None` where none
    /// was read.
    path: Option<PathBuf>,
    /// The file's token, which a rewrite keeps.
    token: String,
    screens: HashMap<(String, String), Entry>,
}

/// A screen loaded, with the token of the handover it was saved at: the file's, or its own where
/// it was carried over from an earlier one.
struct Entry {
    token: String,
    screen: SavedScreen,
}

/// A screen saved at an earlier handover and not yet taken, carried over by a planned stop for a
/// holder it could not link to ([`AppState::pending_restart_screens`]).
pub struct CarriedScreen {
    /// The token of the handover it was saved at.
    pub token: String,
    pub at: HeldAt,
    pub screen: SavedScreen,
}

/// A screen saved at the handover the adopted terminal reports, and the rule its tail is replayed
/// under: what an adoption restores the terminal from in place of the tail alone.
pub struct ExactScreen {
    pub saved: SavedScreen,
    pub rule: ReplayRule,
}

/// A screen saved for a terminal whose holder is gone, selected for the terminal's preview
/// ([`AppState::select_restart_screen`]).
pub struct GoneScreen {
    pub saved: SavedScreen,
    /// It verifies as the screen of the handover the terminal's tail reported, which the output
    /// since that handover in the tail follows.
    pub followed: bool,
}

/// Replaces the file at `path` with what a planned stop leaves for the next daemon: `screens`,
/// each saved for the terminal it names at the handover of `token`, and `carried`, each kept with
/// the token of the earlier handover it was saved at. With neither the file is removed, so that no
/// screen of an earlier handover outlives the stop. **Blocks.**
pub fn replace(
    path: &Path,
    token: &str,
    screens: &[(HeldAt, SavedScreen)],
    carried: &[CarriedScreen],
) -> std::io::Result<()> {
    let _writes = FILE_WRITES.lock().unwrap_or_else(PoisonError::into_inner);
    if screens.is_empty() && carried.is_empty() {
        return remove_replaced(path);
    }
    let saved = screens.iter().map(|(at, screen)| ScreenOut {
        token: None,
        holder_id: &at.holder_id,
        terminal_id: &at.terminal_id,
        screen,
    });
    let carried = carried.iter().map(|carried| ScreenOut {
        token: Some(&carried.token),
        holder_id: &carried.at.holder_id,
        terminal_id: &carried.at.terminal_id,
        screen: &carried.screen,
    });
    write_out(path, &ScreensOut::new(token, saved.chain(carried)))
}

/// Writes `file` to `path`, replacing whatever is there whole and owner-only, the caller holding
/// [`FILE_WRITES`]. **Blocks.**
fn write_out(path: &Path, file: &ScreensOut<'_>) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(file).map_err(std::io::Error::other)?;
    replace_beside(path, &bytes, Permissions::Private)
}

/// Reads the file at `path`, leaving it in place. A file that cannot be read, or does not read as
/// one of a version this build reads, gives nothing and is deleted: the terminals are taken over
/// without its screens, and no later start may restore one and replay the tail since over it.
/// **Blocks.**
fn load(path: &Path) -> Option<ScreensFile> {
    let why = match std::fs::read(path) {
        Ok(content) => match parse(&content) {
            Ok(file) => return Some(file),
            Err(why) => why,
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => format!("reading them failed: {err}"),
    };
    tracing::warn!(path = %path.display(), %why, "the saved screens are of no use; every screen is rebuilt from its tail");
    if let Err(err) = remove_replaced(path) {
        tracing::warn!(path = %path.display(), %err, "removing the saved screens failed");
    }
    None
}

/// The file in `content`, or why it is of no use.
fn parse(content: &[u8]) -> Result<ScreensFile, String> {
    let Version { version } =
        serde_json::from_slice::<Version>(content).map_err(|err| err.to_string())?;
    if !READ_VERSIONS.contains(&version) {
        return Err(format!(
            "its version, {version}, is not one this build reads"
        ));
    }
    serde_json::from_slice(content).map_err(|err| err.to_string())
}

/// Whether `saved`, saved at the handover of `token`, is the screen of a terminal whose holder
/// reports `handover` and the size `size`: the same handover, a whole tail since it, and the
/// screen's size, which a screen can have. Otherwise the reason it is not.
fn verify(
    token: &str,
    saved: &SavedScreen,
    handover: Option<&HandoverTail>,
    size: Option<Size>,
) -> Result<(), &'static str> {
    let Some(handover) = handover else {
        return Err("its holder reports no handover");
    };
    if handover.token != token {
        return Err("its holder reports another handover");
    }
    if handover.truncated {
        return Err("its holder kept only the end of the output since the handover");
    }
    // The restore would give a screen of a size no screen has another one.
    if !saved.has_supported_size() {
        return Err("the saved screen's size is not one a screen can have");
    }
    let saved_size = Size {
        rows: saved.rows,
        cols: saved.cols,
    };
    if size != Some(saved_size) {
        return Err("its size is not the saved screen's");
    }
    Ok(())
}

/// The rule the `len` bytes of a tail since `handover` are replayed under, its ages measured at
/// `measured_at`, for a holder that answered `answers_offline` while no daemon was linked.
fn replay_rule(
    handover: Option<&HandoverTail>,
    answers_offline: &[OfflineAnswer],
    len: u64,
    measured_at: Instant,
) -> ReplayRule {
    // A holder that answers a query this build does not know offline may have answered one in the
    // tail already, and nothing says which: the replay answers nothing, which every age missing
    // makes of every query (stale).
    let ages = if answers_offline.contains(&OfflineAnswer::Unknown) {
        Vec::new()
    } else {
        handover
            .map(|handover| {
                handover
                    .ages
                    .iter()
                    .map(|age| (age.offset, Duration::from_millis(age.age_ms)))
                    .collect()
            })
            .unwrap_or_default()
    };
    ReplayRule {
        len,
        ages,
        measured_at,
        skip_device_attributes: answers_offline.contains(&OfflineAnswer::PrimaryDeviceAttributes),
    }
}

impl AppState {
    /// Reads the screens the previous daemon's planned stop saved to `path`, leaving the file for
    /// their consumption. Called at start, before anything is adopted. **Blocks.**
    pub fn load_restart_screens(&self, path: &Path) {
        let Some(file) = load(path) else {
            return;
        };
        let screens: HashMap<(String, String), Entry> = file
            .screens
            .into_iter()
            .map(|entry| {
                let token = entry.token.unwrap_or_else(|| file.token.clone());
                let screen = Entry {
                    token,
                    screen: entry.screen,
                };
                ((entry.holder_id, entry.terminal_id), screen)
            })
            .collect();
        tracing::info!(
            screens = screens.len(),
            "loaded the screens the previous daemon saved for its successor"
        );
        *self.loaded_restart_screens() = Loaded {
            path: Some(path.to_path_buf()),
            token: file.token,
            screens,
        };
    }

    /// The screen saved for `found`, if one was, with the rule its tail is replayed under when it
    /// verifies as the screen of `found`'s handover; otherwise the reason the screen is rebuilt
    /// from the tail instead. Nothing is taken: the screen may be restored only once committed
    /// ([`Self::commit_restart_screens`]).
    pub fn select_exact_screen(&self, found: &FoundTerminal) -> Result<ExactScreen, &'static str> {
        let selected = {
            let loaded = self.loaded_restart_screens();
            let key = (found.holder_id.clone(), found.id.clone());
            loaded
                .screens
                .get(&key)
                .map(|entry| (entry.token.clone(), entry.screen.clone()))
        };
        let Some((token, saved)) = selected else {
            return Err("no screen was saved for it");
        };
        let handover = found.handover.as_ref();
        verify(&token, &saved, handover, found.size)?;
        let rule = replay_rule(
            handover,
            &found.answers_offline,
            found.since_handover().len() as u64,
            found.welcomed_at,
        );
        Ok(ExactScreen { saved, rule })
    }

    /// The screen saved for the terminal `at`, whose holder is gone, whatever handover it was
    /// saved at, for its preview: `handover` and `size` are what the terminal's tail reported, if
    /// a tail was kept, which say whether the screen is the one that tail follows
    /// ([`GoneScreen::followed`]). Nothing is taken: the holder's screens are expired once the
    /// previews are written and its sessions settled ([`Self::expire_restart_screens`]).
    pub fn select_restart_screen(
        &self,
        at: &HeldAt,
        handover: Option<&HandoverTail>,
        size: Option<Size>,
    ) -> Option<GoneScreen> {
        let loaded = self.loaded_restart_screens();
        let key = (at.holder_id.clone(), at.terminal_id.clone());
        let entry = loaded.screens.get(&key)?;
        Some(GoneScreen {
            followed: verify(&entry.token, &entry.screen, handover, size).is_ok(),
            saved: entry.screen.clone(),
        })
    }

    /// Consumes the screens saved for the terminals `at`, those that had one, in one rewrite of
    /// the file. Returns whether the file no longer holds any of them, so that no later start can
    /// select one again: only then may a terminal be restored from its screen. Called once what
    /// becomes of each of those terminals is settled: by the adoption of a holder's terminals,
    /// before any of them is acted on. **Blocks.**
    pub fn commit_restart_screens(&self, at: &[HeldAt]) -> bool {
        let mut loaded = self.loaded_restart_screens();
        let before = loaded.screens.len();
        for at in at {
            loaded
                .screens
                .remove(&(at.holder_id.clone(), at.terminal_id.clone()));
        }
        loaded.screens.len() == before || self.persist_restart_screens(&loaded)
    }

    /// Consumes, as [`Self::commit_restart_screens`] does, every screen still saved for a terminal
    /// of a holder `gone` names: called once what becomes of every terminal of those holders is
    /// settled, when what is left of them is of no use (see the module's documentation).
    /// **Blocks.**
    pub fn expire_restart_screens(&self, gone: impl Fn(&str) -> bool) {
        let mut loaded = self.loaded_restart_screens();
        let before = loaded.screens.len();
        loaded
            .screens
            .retain(|(holder_id, _), _| !gone(holder_id.as_str()));
        let expired = before - loaded.screens.len();
        if expired > 0 {
            tracing::info!(expired, "the saved screens of no more use are dropped");
            self.persist_restart_screens(&loaded);
        }
    }

    /// The screens still loaded, not taken, for a terminal of one of `holders`, the holders a
    /// planned stop could not link to, each with the token of the handover it was saved at: what
    /// the stop carries over into the file it replaces ([`replace`]), so that the next daemon
    /// linking to such a holder still finds them. **Blocks** on nothing but the loaded screens.
    pub fn pending_restart_screens(&self, holders: &[String]) -> Vec<CarriedScreen> {
        let loaded = self.loaded_restart_screens();
        loaded
            .screens
            .iter()
            .filter(|((holder, _), _)| holders.contains(holder))
            .map(|((holder_id, terminal_id), entry)| CarriedScreen {
                token: entry.token.clone(),
                at: HeldAt {
                    holder_id: holder_id.clone(),
                    terminal_id: terminal_id.clone(),
                },
                screen: entry.screen.clone(),
            })
            .collect()
    }

    /// Writes what is left of the loaded screens over the file they were read from, or removes it
    /// once none is left; one that cannot be rewritten is removed, the screens left in it rebuilt
    /// from their tails by a later start rather than one taken used twice. Returns whether the
    /// file now holds none of the screens taken from it. During a planned stop the file is the
    /// stop's to replace, and is left alone. **Blocks.**
    fn persist_restart_screens(&self, loaded: &Loaded) -> bool {
        let Some(path) = &loaded.path else {
            return true;
        };
        // Held from the check on, so that a planned stop that begins after it writes its file
        // only once this rewrite is done.
        let _writes = FILE_WRITES.lock().unwrap_or_else(PoisonError::into_inner);
        if self.is_handing_over() {
            tracing::info!("a screen saved by the previous daemon was taken during a planned stop; the file is left to it");
            return false;
        }
        let written = if loaded.screens.is_empty() {
            remove_replaced(path)
        } else {
            let screens = loaded
                .screens
                .iter()
                .map(|((holder_id, terminal_id), entry)| ScreenOut {
                    token: Some(&entry.token),
                    holder_id,
                    terminal_id,
                    screen: &entry.screen,
                });
            write_out(path, &ScreensOut::new(&loaded.token, screens))
        };
        let Err(err) = written else {
            return true;
        };
        tracing::warn!(path = %path.display(), %err, "rewriting the saved screens failed; removing them");
        match remove_replaced(path) {
            Ok(()) => true,
            Err(err) => {
                tracing::error!(path = %path.display(), %err, "removing the saved screens failed");
                false
            }
        }
    }

    fn loaded_restart_screens(&self) -> MutexGuard<'_, Loaded> {
        self.restart_screens
            .loaded
            .lock()
            .expect("restart screens lock poisoned")
    }

    /// Puts `screen` among the loaded screens as saved for `at` at the handover of `token`, as
    /// [`Self::load_restart_screens`] would from a file of that handover.
    #[cfg(test)]
    pub(crate) fn keep_restart_screen(&self, token: &str, at: HeldAt, screen: SavedScreen) {
        let mut loaded = self.loaded_restart_screens();
        loaded.token = token.to_string();
        let entry = Entry {
            token: token.to_string(),
            screen,
        };
        loaded.screens.insert((at.holder_id, at.terminal_id), entry);
    }

    /// Whether a screen is still loaded for the terminal `at`.
    #[cfg(test)]
    pub(crate) fn has_restart_screen(&self, at: &HeldAt) -> bool {
        let key = (at.holder_id.clone(), at.terminal_id.clone());
        self.loaded_restart_screens().screens.contains_key(&key)
    }
}

#[cfg(test)]
mod tests;
