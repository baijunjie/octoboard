//! The daemon's own part of the update: the scheduled check, and the work each request starts.
//!
//! A request answers once its work has begun and the work runs on a thread of its own, announcing
//! its progress and end as the status; the claim it holds (`Claim`) is released when the thread
//! ends, or when it panics. Not on the runtime's blocking pool, whose tasks the runtime waits for
//! when it shuts down: a check or a download that waits on the network must not hold a stop or a
//! planned restart.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::check::{check, Checked};
use super::fetch::UreqFetcher;
use super::install::{self, InstallError, Progress, Sources};
use super::manifest::is_newer;
use super::record::Files;
use super::verify::release_key;
use super::{Claim, InstallRefusal, Setup, RUNNING};
use crate::platform::{fs, proc};
use crate::protocol::{error_code, CodedError, DaemonUpdateProgress};
use crate::state::{shutting_down_refusal, AppState, RestartRefusal};
use crate::time::now_millis;

/// How long after the daemon starts the first check is made, for the login shell's environment
/// (the proxy variables the check goes through) to have been taken.
const START_DELAY: Duration = Duration::from_secs(30);

/// How often the schedule looks at the clock. Hours, not the period itself: a timer does not
/// advance while the machine sleeps, so one of six hours fires days late on a laptop.
const WAKE: Duration = Duration::from_secs(60 * 60);

/// How long after a check the next scheduled one is due, in milliseconds.
const CHECK_EVERY_MS: i64 = 6 * 60 * 60 * 1000;

/// The bytes the archive grows by between two announcements of its progress, and the shortest time
/// between two.
const PROGRESS_STEP: u64 = 256 * 1024;
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// Turns the update on for a daemon that can carry it out, and starts its schedule: the build has
/// the `self-update` feature, the daemon follows no application, and the host is one there is a
/// release for. Notes the version the previous run recorded, for `updated_from`. Whether it is on.
/// Called at start, in a runtime, before anything is served.
pub(crate) fn start(state: &Arc<AppState>, follows_application: bool, data_dir: &Path) -> bool {
    if !cfg!(feature = "self-update") || follows_application || !proc::self_update_host() {
        return false;
    }
    let files = Files::new(data_dir);
    state.daemon_update.enable(Setup {
        files: files.clone(),
        fetcher: Arc::new(UreqFetcher::daemon()),
        key: release_key(),
        arch: std::env::consts::ARCH.to_owned(),
        launch: std::path::PathBuf::from(&state.install_exe),
        is_running_image: proc::names_running_image,
        probe: super::probe::expect_version,
    });
    state
        .daemon_update
        .set_updated_from(files.started_as(RUNNING));
    // What a stop during a download left in `updates/`, unless another process is downloading.
    files.sweep_updates();
    tokio::spawn(scheduled(state.clone()));
    true
}

fn unavailable() -> anyhow::Error {
    CodedError::raised(
        error_code::DAEMON_UPDATE_UNAVAILABLE,
        "this daemon does not update itself",
        &[],
    )
}

fn setup_of(state: &AppState) -> Result<Arc<Setup>> {
    state.daemon_update.setup().ok_or_else(unavailable)
}

/// `check_daemon_update`: starts a check unless one or the download runs, one ended a moment ago,
/// or an update is installed, and returns at once.
pub(crate) fn check_requested(state: &Arc<AppState>) -> Result<()> {
    let setup = setup_of(state)?;
    let state = state.clone();
    // Blocking all the way, the look at the disk before the check included.
    spawn_check(move || {
        forget_rolled_back(&state, &setup);
        if let Some(claim) = state.daemon_update.begin_check(&state, false) {
            run_check(&state, &setup, claim);
        }
    });
    Ok(())
}

/// Runs `check` (blocking: it reads the disk and the manifest) on a thread of its own, which the
/// runtime does not wait for at its end; see the module.
fn spawn_check(check: impl FnOnce() + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("daemon-update-check".to_owned())
        .spawn(check);
    if let Err(err) = spawned {
        tracing::warn!(%err, "the update check could not be started");
    }
}

/// `update_daemon`: starts the download and replacement of the release the last check found, and
/// returns once it has started.
pub(crate) async fn update_requested(state: &Arc<AppState>) -> Result<()> {
    let setup = setup_of(state)?;
    state.refuse_while_shutting_down()?;
    let looking = {
        let (state, setup) = (state.clone(), setup.clone());
        // Blocking: it reads files, and the lock is a file.
        tokio::task::spawn_blocking(move || {
            // The lock first: with it held, no other update is writing the record or the file.
            let lock = fs::try_lock_file(&setup.files.lock());
            if lock.is_ok() {
                // An update `octoboardd update` made while the daemon ran is learned of, so that
                // it is refused as installed rather than made again, and one it rolled back is
                // forgotten.
                forget_rolled_back(&state, &setup);
                adopt_recorded(&state, &setup);
            }
            (lock, install::replaceable(&setup.launch))
        })
        .await?
    };
    if let Some(refusal) = state.daemon_update.refusal_to_install() {
        return Err(refused(refusal));
    }
    let lock = match looking.0 {
        Ok(lock) => lock,
        Err(fs::Busy::Held) => return Err(refused(InstallRefusal::Running)),
        Err(fs::Busy::Open(err) | fs::Busy::Lock(err)) => {
            return Err(anyhow::Error::new(err).context("taking the update's lock"))
        }
    };
    if let Err(InstallError::NotWritable(exe)) = looking.1 {
        let channel = state.store.daemon_update_channel().unwrap_or_default();
        state
            .daemon_update
            .record_manual(state, super::not_writable(&exe, channel));
        let path = exe.to_string_lossy().into_owned();
        return Err(CodedError::raised(
            error_code::DAEMON_UPDATE_NOT_WRITABLE,
            format!("the daemon cannot replace {path}: the user it runs as may not write there"),
            &[("path", &path)],
        ));
    }
    // Looked at again: the stop may have begun while the lock and the executable were looked at.
    state.refuse_while_shutting_down()?;
    let (claim, release) = state.daemon_update.begin_install(state).map_err(refused)?;
    tracing::info!(version = %release.version, "updating the daemon");
    let state = state.clone();
    // A thread of its own, not the runtime's blocking pool, whose end the runtime waits for: a
    // download that stalls must not hold the daemon's exit. A stop that begins meanwhile keeps the
    // executable from being replaced (`Sources::stopping`).
    std::thread::spawn(move || run_install(&state, &setup, claim, &release, lock));
    Ok(())
}

/// `restart_daemon`: asks for the planned restart `octoboardd restart` makes, which hands every
/// session over to the daemon that starts from the file now at the launch path.
pub(crate) fn restart_requested(state: &Arc<AppState>) -> Result<()> {
    setup_of(state)?;
    state.refuse_while_shutting_down()?;
    // A restart under a download would end it, and the update with it.
    if state.daemon_update.download_running() {
        return Err(refused(InstallRefusal::Running));
    }
    match state.request_planned_restart_unless_stopping() {
        Ok(()) => {
            tracing::info!("a planned restart was asked for to run the daemon's update");
            Ok(())
        }
        Err(RestartRefusal::StopInProgress) => Err(shutting_down_refusal()),
        // Only a daemon that follows an application refuses otherwise, and that one never enables
        // the update.
        Err(_) => Err(unavailable()),
    }
}

fn refused(refusal: InstallRefusal) -> anyhow::Error {
    let (code, message) = match refusal {
        InstallRefusal::Installed => (
            error_code::DAEMON_UPDATE_INSTALLED,
            "an update is installed already; restart the daemon to run it",
        ),
        InstallRefusal::Running => (
            error_code::DAEMON_UPDATE_RUNNING,
            "an update is being downloaded already",
        ),
        InstallRefusal::NothingToInstall => (
            error_code::DAEMON_UPDATE_NOTHING_TO_INSTALL,
            "no update has been found to install",
        ),
    };
    CodedError::raised(code, message, &[])
}

/// A check, under `claim`. Blocking.
fn run_check(state: &Arc<AppState>, setup: &Setup, claim: Claim) {
    if adopt_recorded(state, setup) {
        return;
    }
    // The channel changed while the manifest was read: what it says is of the old one (the claim
    // says so, `record_check`), and the manifest of the new one is read, under the same claim (the
    // automatic check that follows the change may be off, and this one would otherwise end with
    // no result). Not more than a few times, for a channel that keeps changing.
    for _ in 0..3 {
        let channel = state.store.daemon_update_channel().unwrap_or_default();
        let found = check(&*setup.fetcher, channel, RUNNING);
        let (found, manual) = match found {
            Ok(Checked::Available(release)) => {
                // An update the user would have to install by hand is shown as such from the check on.
                let manual = match install::replaceable(&setup.launch) {
                    Err(InstallError::NotWritable(exe)) => Some(super::not_writable(&exe, channel)),
                    _ => None,
                };
                (Ok(Some(release)), manual)
            }
            Ok(Checked::UpToDate) => (Ok(None), None),
            Err(detail) => (Err(detail), None),
        };
        if state.daemon_update.record_check(&claim, found, manual) {
            return;
        }
    }
}

/// The download and replacement, under `claim` and the update's `lock`, which is held until the end
/// is recorded. Blocking.
fn run_install(
    state: &Arc<AppState>,
    setup: &Setup,
    claim: Claim,
    release: &super::manifest::Release,
    lock: std::fs::File,
) {
    let stopping = || state.stop_asked();
    let sources = Sources {
        fetcher: &*setup.fetcher,
        key: &setup.key,
        arch: &setup.arch,
        is_running_image: setup.is_running_image,
        stopping: &stopping,
        probe: setup.probe,
    };
    let mut throttle = Throttle::default();
    let outcome = install::install(
        &sources,
        release,
        &setup.launch,
        &setup.files,
        &mut |progress| {
            if throttle.due(progress.downloaded, Instant::now()) {
                let Progress { downloaded, total } = progress;
                state
                    .daemon_update
                    .record_progress(&claim, DaemonUpdateProgress { downloaded, total });
            }
        },
    );
    if let Err(failure) = &outcome {
        tracing::warn!(detail = %failure.detail(), "updating the daemon failed");
    }
    state
        .daemon_update
        .record_installed(&claim, release, outcome.map(|done| done.replaced));
    drop(lock);
}

/// Learns of an update `octoboardd update` installed over this daemon's executable: the record it
/// left names this daemon's file and a version newer than this build. Whether one was found, in
/// which case the status says installed. Blocking.
fn adopt_recorded(state: &AppState, setup: &Setup) -> bool {
    let Some(record) = setup.files.read_installed() else {
        return false;
    };
    let ours = crate::hostfs::canonical(&setup.launch)
        .ok()
        .map(|path| path.to_string_lossy().into_owned());
    if !is_newer(&record.version, RUNNING) || ours.as_deref() != Some(record.path.as_str()) {
        return false;
    }
    state
        .daemon_update
        .record_installed_elsewhere(state, record.version, record.holder_floor)
}

/// Forgets an installed update once the launch path leads to the running build again, as after
/// `octoboardd update --rollback`: there is nothing to restart into. Blocking, as a look at the
/// disk.
fn forget_rolled_back(state: &AppState, setup: &Setup) {
    if !state.daemon_update.installed() {
        return;
    }
    let back =
        crate::hostfs::canonical(&setup.launch).is_ok_and(|real| (setup.is_running_image)(&real));
    if back {
        state.daemon_update.forget_installed(state);
    }
}

/// Whether a check is due `now` (milliseconds since the epoch) when the last ended at `last`:
/// none yet, six hours have passed, or the wall clock went backwards past it.
pub(super) fn due(now: i64, last: Option<i64>) -> bool {
    match last {
        None => true,
        Some(last) => now < last || now - last >= CHECK_EVERY_MS,
    }
}

/// The schedule: a check after [`START_DELAY`], then whenever one is due at an hourly look, and at
/// once when the channel changes or the check is turned back on.
async fn scheduled(state: Arc<AppState>) {
    tokio::time::sleep(START_DELAY).await;
    let mut forced = false;
    loop {
        scheduled_check(&state, forced).await;
        forced = false;
        tokio::select! {
            () = tokio::time::sleep(WAKE) => {}
            () = state.daemon_update.again.notified() => forced = true,
        }
    }
}

/// One look of the schedule: checks if the setting is on, nothing is installed, and a check is due
/// or `forced`. The check has run when this returns.
pub(super) async fn scheduled_check(state: &Arc<AppState>, forced: bool) {
    let Some(setup) = state.daemon_update.setup() else {
        return;
    };
    let state = state.clone();
    let (done, ended) = tokio::sync::oneshot::channel::<()>();
    // Blocking all the way: the look at the disk and the store's reads come before the check.
    spawn_check(move || {
        let _done = done;
        let updates = &state.daemon_update;
        forget_rolled_back(&state, &setup);
        if updates.installed() || !state.store.daemon_update_check().unwrap_or(false) {
            return;
        }
        if !forced && !due(now_millis(), updates.last_check_at()) {
            return;
        }
        if let Some(claim) = updates.begin_check(&state, forced) {
            run_check(&state, &setup, claim);
        }
    });
    // Ended, or the thread is gone: either way the check has run, or never will.
    ended.await.ok();
}

/// Tells when the progress of the archive is worth announcing: [`PROGRESS_STEP`] bytes more than
/// at the last announcement, and [`PROGRESS_EVERY`] since it.
#[derive(Default)]
pub(super) struct Throttle {
    sent: u64,
    at: Option<Instant>,
}

impl Throttle {
    pub(super) fn due(&mut self, downloaded: u64, now: Instant) -> bool {
        let far_enough = downloaded.saturating_sub(self.sent) >= PROGRESS_STEP;
        let late_enough = self
            .at
            .is_none_or(|at| now.duration_since(at) >= PROGRESS_EVERY);
        if far_enough && late_enough {
            self.sent = downloaded;
            self.at = Some(now);
            return true;
        }
        false
    }
}
