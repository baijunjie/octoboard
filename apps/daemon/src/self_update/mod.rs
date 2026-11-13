//! The server form's self-update: a Linux daemon that follows no application checks the release
//! manifest, and downloads, verifies and replaces its own executable when the user asks. See
//! "Daemon updates" in `apps/daemon/PROTOCOL.md`.
//!
//! It behaves as the desktop application's update does: nothing is downloaded and nothing is
//! restarted on the daemon's own. It only checks, on a schedule (once shortly after it starts, so
//! the login shell's proxy variables are in the environment snapshot, then every six hours, and at
//! once when the channel changes or the check is turned back on), and each check is announced as
//! the status. Downloading and replacing the executable is `update_daemon`, and the restart that
//! makes it the running build is `restart_daemon` (the planned restart `octoboardd restart` makes,
//! which keeps every session running); both are the user's choice, and no more checks are made once
//! an update has been installed.
//!
//! - `manifest.rs`, `check.rs`: the release manifest and the check against it.
//! - `fetch.rs`: the HTTP, behind a seam so that no test touches the network.
//! - `verify.rs`, `unpack.rs`, `replace.rs`, `install.rs`: the download, the checks that make the
//!   archive the release it claims to be, and the replacement of the executable, which
//!   `octoboardd update` shares (`crate::cli::update`).
//! - `probe.rs`: running a build with `--version`, which a new build has to answer before it
//!   replaces the executable.
//! - `record.rs`: the files the update leaves in the data directory.
//! - `rollback.rs`: what going back to the previous build checks first (its version, and that it
//!   reads the database).
//! - `successor.rs`: the next daemon of a planned restart without a service manager, which falls
//!   back to the build that ran when an update does not start.
//! - `helpers.rs`: the executable the helpers of a launch in the daemon itself run.
//! - `task.rs`: the daemon's own part: the scheduled check and the work a request starts.
//!
//! The status is runtime state held in [`DaemonUpdates`] on `AppState`, never stored. A daemon
//! where the update is [not enabled](DaemonUpdates::enabled) has no status (`null` in the
//! snapshot) and refuses the requests.

pub(crate) mod check;
pub(crate) mod fetch;
pub(crate) mod helpers;
pub(crate) mod install;
pub(crate) mod manifest;
pub(crate) mod probe;
pub(crate) mod record;
pub(crate) mod replace;
pub(crate) mod rollback;
mod successor;
mod task;
#[cfg(test)]
mod tests;
pub(crate) mod unpack;
pub(crate) mod verify;

pub(crate) use successor::start_successor;
pub(crate) use task::{check_requested, restart_requested, start, update_requested};

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use minisign_verify::PublicKey;

use crate::protocol::{
    daemon_update_code, DaemonUpdateAvailable, DaemonUpdateCheck, DaemonUpdateCheckOutcome,
    DaemonUpdateInstall, DaemonUpdateInstallState, DaemonUpdateInstalled, DaemonUpdateManual,
    DaemonUpdateManualReason, DaemonUpdatePhase, DaemonUpdateProgress, DaemonUpdateStatus, Event,
};
use crate::state::AppState;
use crate::time::now_millis;
use fetch::Fetcher;
use manifest::Release;
use record::Files;

/// The version of this build, which an update is newer or older than.
pub(crate) const RUNNING: &str = env!("CARGO_PKG_VERSION");

/// How soon after a check ended a request to check again is left alone. Several clients can be
/// connected and each asks when its user does; the floor keeps a button pressed twice, or in two
/// windows, one request to the host.
const CHECK_FLOOR: std::time::Duration = std::time::Duration::from_secs(30);

/// The longest failure detail carried to clients, in characters.
const DETAIL_LIMIT: usize = 300;

/// What the update reaches outside the daemon's own state: its files, where the archive comes from,
/// the key it is signed with and the executable that is replaced. Present once the update is
/// enabled.
pub(crate) struct Setup {
    pub files: Files,
    pub fetcher: Arc<dyn Fetcher>,
    pub key: PublicKey,
    /// The architecture of the release this build is replaced by, as the manifest's platform keys
    /// and the archive's name spell it.
    pub arch: String,
    /// The path the planned restart starts the successor from (`AppState::install_exe`).
    pub launch: PathBuf,
    pub is_running_image: fn(&std::path::Path) -> bool,
    /// How a new build is run to see that it starts (`install::Sources::probe`).
    pub probe: fn(&std::path::Path, &str) -> Result<(), String>,
}

/// The update's status and what is running, held on `AppState`.
#[derive(Default)]
pub struct DaemonUpdates {
    setup: OnceLock<Arc<Setup>>,
    inner: Mutex<Inner>,
    /// Woken when the channel changes or the check is turned back on: the scheduled check runs
    /// again at once.
    again: tokio::sync::Notify,
}

struct Inner {
    status: DaemonUpdateStatus,
    /// The release the last check found, with the address and the signature the status leaves
    /// out.
    release: Option<Release>,
    working: Option<Work>,
    /// When a check or download last ended, for [`CHECK_FLOOR`].
    last_ended: Option<Instant>,
    /// A forced check was asked for while work ran: the schedule runs it once that ends.
    recheck: bool,
    /// Moved on whenever the channel changes: a check that began before it read the old channel's
    /// manifest, and its result is dropped.
    channel_generation: u64,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            status: DaemonUpdateStatus::idle(),
            release: None,
            working: None,
            last_ended: None,
            recheck: false,
            channel_generation: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Work {
    Check,
    Download,
}

/// The right to check, or to download and install, one at a time and never both, released when
/// dropped: the work runs on a blocking task that can panic, and a claim that stays held would
/// leave the update unusable for the rest of the daemon's life. A claim dropped with its work still
/// showing as under way ends it as failed.
pub(crate) struct Claim {
    state: Arc<AppState>,
    work: Work,
    /// The channel generation (`Inner::channel_generation`) the claim's work is of.
    channel_generation: std::sync::atomic::AtomicU64,
}

impl Drop for Claim {
    fn drop(&mut self) {
        let updates = &self.state.daemon_update;
        let mut inner = updates.lock();
        inner.working = None;
        inner.last_ended = Some(Instant::now());
        if std::mem::take(&mut inner.recheck) {
            updates.check_again();
        }
        let phase = inner.status.phase;
        match (self.work, phase) {
            (Work::Check, DaemonUpdatePhase::Checking) => {
                inner.status.phase = DaemonUpdatePhase::Idle;
            }
            (Work::Download, DaemonUpdatePhase::Downloading) => {
                inner.status.phase = DaemonUpdatePhase::Idle;
                inner.status.progress = None;
                let started_at = inner
                    .status
                    .install
                    .as_ref()
                    .map_or_else(now_millis, |install| install.started_at);
                inner.status.install = Some(DaemonUpdateInstall {
                    state: DaemonUpdateInstallState::Failed,
                    started_at,
                    finished_at: Some(now_millis()),
                    error: Some(daemon_update_code::REPLACE_FAILED),
                    detail: Some("internal error".to_owned()),
                });
            }
            _ => return,
        }
        updates.announce(&self.state, &inner);
    }
}

impl DaemonUpdates {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("daemon update lock poisoned")
    }

    /// Turns the update on, with what it reaches. Called once, at start, before anything is
    /// served; a daemon that never calls it has no update. A second call changes nothing.
    pub(crate) fn enable(&self, setup: Setup) {
        let _ = self.setup.set(Arc::new(setup));
    }

    /// Whether this daemon updates itself.
    pub fn enabled(&self) -> bool {
        self.setup.get().is_some()
    }

    pub(crate) fn setup(&self) -> Option<Arc<Setup>> {
        self.setup.get().cloned()
    }

    /// The status, `None` where the daemon does not update itself.
    pub fn status(&self) -> Option<DaemonUpdateStatus> {
        self.enabled().then(|| self.lock().status.clone())
    }

    /// Notes the version this daemon replaced when it started, for the window that opens later to
    /// be told once.
    pub(crate) fn set_updated_from(&self, version: Option<String>) {
        self.lock().status.updated_from = version;
    }

    /// Has the scheduled check run again at once.
    pub fn check_again(&self) {
        self.again.notify_one();
    }

    fn announce(&self, state: &AppState, inner: &Inner) {
        state.broadcast(Event::DaemonUpdateStatusUpdated {
            status: inner.status.clone(),
        });
    }

    /// The claim to check, or `None` while another check or the download runs, once an update is
    /// installed and, unless `forced`, within [`CHECK_FLOOR`] of the last work that ended. A forced
    /// check asked for while work runs is not lost: it is asked for again when that work ends.
    /// Marks the check as started.
    pub(crate) fn begin_check(&self, state: &Arc<AppState>, forced: bool) -> Option<Claim> {
        let mut inner = self.lock();
        let recent = inner
            .last_ended
            .is_some_and(|ended| ended.elapsed() < CHECK_FLOOR);
        if !self.enabled() || inner.status.phase == DaemonUpdatePhase::Installed {
            return None;
        }
        if inner.working.is_some() {
            inner.recheck |= forced;
            return None;
        }
        if recent && !forced {
            return None;
        }
        inner.working = Some(Work::Check);
        inner.status.phase = DaemonUpdatePhase::Checking;
        self.announce(state, &inner);
        Some(Claim {
            state: state.clone(),
            work: Work::Check,
            channel_generation: inner.channel_generation.into(),
        })
    }

    /// Records how a check taken under `claim` ended and announces it. `manual` says the update
    /// found is one the user has to install by hand, and how. False, with nothing recorded,
    /// where the channel changed since the check began (or since the last time it was told so):
    /// what it read is of the old channel, the claim now stands for the new one, and the check is
    /// made again. True where it ended, recorded or not.
    pub(crate) fn record_check(
        &self,
        claim: &Claim,
        found: Result<Option<Release>, String>,
        manual: Option<DaemonUpdateManual>,
    ) -> bool {
        use std::sync::atomic::Ordering::SeqCst;
        let mut inner = self.lock();
        // Something else decided meanwhile (an update the command line installed was learned of).
        if inner.status.phase != DaemonUpdatePhase::Checking {
            return true;
        }
        if claim
            .channel_generation
            .swap(inner.channel_generation, SeqCst)
            != inner.channel_generation
        {
            return false;
        }
        let at = now_millis();
        let (outcome, error, detail) = match found {
            Ok(Some(release)) => {
                inner.status.available = Some(DaemonUpdateAvailable {
                    version: release.version.clone(),
                    notes: release.notes.clone(),
                    holder_floor: release.holder_floor.clone(),
                    channel: release.channel,
                });
                inner.status.manual = manual;
                inner.release = Some(release);
                (DaemonUpdateCheckOutcome::Available, None, None)
            }
            Ok(None) => {
                inner.status.available = None;
                inner.status.manual = None;
                inner.release = None;
                (DaemonUpdateCheckOutcome::UpToDate, None, None)
            }
            // What an earlier check found stays: a failed look says nothing about it.
            Err(detail) => (
                DaemonUpdateCheckOutcome::Failed,
                Some(daemon_update_code::CHECK_FAILED),
                Some(bounded(&detail)),
            ),
        };
        inner.status.last_check = Some(DaemonUpdateCheck {
            outcome,
            at,
            error,
            detail,
        });
        inner.status.phase = DaemonUpdatePhase::Idle;
        self.announce(&claim.state, &inner);
        true
    }

    /// What stands in the way of an installation, if anything: an update installed already, work
    /// running, none found.
    fn refusal_of(inner: &Inner) -> Option<InstallRefusal> {
        if inner.status.phase == DaemonUpdatePhase::Installed {
            Some(InstallRefusal::Installed)
        } else if inner.working.is_some() {
            Some(InstallRefusal::Running)
        } else if inner.release.is_none() {
            Some(InstallRefusal::NothingToInstall)
        } else {
            None
        }
    }

    /// [`Self::refusal_of`], as the refusal of the request before anything is taken.
    pub(crate) fn refusal_to_install(&self) -> Option<InstallRefusal> {
        Self::refusal_of(&self.lock())
    }

    /// Whether a download and installation runs now.
    pub(crate) fn download_running(&self) -> bool {
        self.lock().working == Some(Work::Download)
    }

    /// The claim to download and install the release the last check found, and that release, or
    /// the refusal. Marks the installation as started.
    pub(crate) fn begin_install(
        &self,
        state: &Arc<AppState>,
    ) -> Result<(Claim, Release), InstallRefusal> {
        let mut inner = self.lock();
        if let Some(refusal) = Self::refusal_of(&inner) {
            return Err(refusal);
        }
        let release = inner
            .release
            .clone()
            .ok_or(InstallRefusal::NothingToInstall)?;
        inner.working = Some(Work::Download);
        inner.status.phase = DaemonUpdatePhase::Downloading;
        inner.status.progress = Some(DaemonUpdateProgress {
            downloaded: 0,
            total: None,
        });
        inner.status.install = Some(DaemonUpdateInstall {
            state: DaemonUpdateInstallState::Running,
            started_at: now_millis(),
            finished_at: None,
            error: None,
            detail: None,
        });
        self.announce(state, &inner);
        Ok((
            Claim {
                state: state.clone(),
                work: Work::Download,
                channel_generation: inner.channel_generation.into(),
            },
            release,
        ))
    }

    /// Records how far the archive has come, and announces it.
    pub(crate) fn record_progress(&self, claim: &Claim, progress: DaemonUpdateProgress) {
        let mut inner = self.lock();
        if inner.status.phase != DaemonUpdatePhase::Downloading {
            return;
        }
        inner.status.progress = Some(progress);
        self.announce(&claim.state, &inner);
    }

    /// Notes that the update is one the user has to install by hand, as `manual` says, and
    /// announces it.
    pub(crate) fn record_manual(&self, state: &AppState, manual: DaemonUpdateManual) {
        let mut inner = self.lock();
        if inner.status.manual.as_ref() == Some(&manual) {
            return;
        }
        inner.status.manual = Some(manual);
        self.announce(state, &inner);
    }

    /// Records the end of an installation taken under `claim`: the executable replaced (or found
    /// replaced already, with `replaced` false and the version not known), or why it was not.
    pub(crate) fn record_installed(
        &self,
        claim: &Claim,
        release: &Release,
        outcome: Result<bool, install::InstallError>,
    ) {
        let mut inner = self.lock();
        if inner.status.phase != DaemonUpdatePhase::Downloading {
            return;
        }
        let started_at = inner
            .status
            .install
            .as_ref()
            .map_or_else(now_millis, |install| install.started_at);
        let finished_at = Some(now_millis());
        inner.status.progress = None;
        match outcome {
            Ok(replaced) => {
                inner.status.phase = DaemonUpdatePhase::Installed;
                inner.status.installed = Some(if replaced {
                    DaemonUpdateInstalled {
                        version: Some(release.version.clone()),
                        holder_floor: release.holder_floor.clone(),
                    }
                } else {
                    DaemonUpdateInstalled {
                        version: None,
                        holder_floor: None,
                    }
                });
                inner.status.available = None;
                inner.status.manual = None;
                inner.release = None;
                inner.status.install = Some(DaemonUpdateInstall {
                    state: DaemonUpdateInstallState::Succeeded,
                    started_at,
                    finished_at,
                    error: None,
                    detail: None,
                });
            }
            Err(failure) => {
                inner.status.phase = DaemonUpdatePhase::Idle;
                inner.status.install = Some(DaemonUpdateInstall {
                    state: DaemonUpdateInstallState::Failed,
                    started_at,
                    finished_at,
                    error: Some(failure.code()),
                    detail: Some(bounded(&failure.detail())),
                });
            }
        }
        self.announce(&claim.state, &inner);
    }

    /// Notes that an update the command line installed has replaced the executable, and announces
    /// it. Whatever check is under way ends without a word. Nothing is noted while this daemon
    /// downloads one itself, whose own end says what was installed. Whether it was noted.
    pub(crate) fn record_installed_elsewhere(
        &self,
        state: &AppState,
        version: String,
        holder_floor: Option<String>,
    ) -> bool {
        let mut inner = self.lock();
        if inner.status.phase == DaemonUpdatePhase::Installed
            || inner.working == Some(Work::Download)
        {
            return false;
        }
        inner.status.phase = DaemonUpdatePhase::Installed;
        inner.status.installed = Some(DaemonUpdateInstalled {
            version: Some(version),
            holder_floor,
        });
        inner.status.available = None;
        inner.status.manual = None;
        inner.status.progress = None;
        inner.release = None;
        self.announce(state, &inner);
        true
    }

    /// Forgets an installed update once the launch path leads to the running build again (a
    /// rollback from the command line put it back): there is nothing to restart into, and checks
    /// resume. Whether it was forgotten.
    pub(crate) fn forget_installed(&self, state: &AppState) -> bool {
        let mut inner = self.lock();
        if inner.status.phase != DaemonUpdatePhase::Installed {
            return false;
        }
        inner.status.phase = DaemonUpdatePhase::Idle;
        inner.status.installed = None;
        inner.status.install = None;
        self.announce(state, &inner);
        true
    }

    /// Forgets the release found on the channel the daemon followed and what depends on it (the
    /// update to download, that it has to be installed by hand), which says nothing of the new
    /// channel, and announces it. Nothing is forgotten while an update installs or once one is
    /// installed.
    pub(crate) fn channel_changed(&self, state: &AppState) {
        let mut inner = self.lock();
        inner.channel_generation += 1;
        let held = inner.release.is_some()
            || inner.status.available.is_some()
            || inner.status.manual.is_some();
        if !held
            || inner.working == Some(Work::Download)
            || inner.status.phase == DaemonUpdatePhase::Installed
        {
            return;
        }
        inner.release = None;
        inner.status.available = None;
        inner.status.manual = None;
        self.announce(state, &inner);
    }

    /// The time the last check ended, in milliseconds since the epoch, which the schedule counts
    /// from.
    pub(crate) fn last_check_at(&self) -> Option<i64> {
        self.lock().status.last_check.as_ref().map(|check| check.at)
    }

    /// Whether an update has been installed, after which nothing is checked until the restart.
    pub(crate) fn installed(&self) -> bool {
        self.lock().status.phase == DaemonUpdatePhase::Installed
    }
}

/// Why `update_daemon` is refused beyond the update being unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstallRefusal {
    Installed,
    Running,
    NothingToInstall,
}

/// The update of `channel` the user has to install by hand over `exe`, which the daemon's user may
/// not replace.
pub(crate) fn not_writable(
    exe: &std::path::Path,
    channel: crate::protocol::DaemonUpdateChannel,
) -> DaemonUpdateManual {
    DaemonUpdateManual {
        reason: DaemonUpdateManualReason::NotWritable,
        path: exe.to_string_lossy().into_owned(),
        command: manifest::install_command_for(exe, channel),
    }
}

/// `text` trimmed and cut to [`DETAIL_LIMIT`] characters.
fn bounded(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(DETAIL_LIMIT) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}
