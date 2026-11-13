//! `octoboardd update`: checking for a newer daemon, installing it over the one at this
//! installation's path, and putting the previous one back. The same download, checks and
//! replacement as the daemon's own (`crate::self_update`), run in this process.
//!
//! None of it restarts the daemon: the file is replaced under a running one, which goes on running
//! the build it started as until `octoboardd restart` (a planned restart that keeps every session
//! running). The update lock (`update.lock` in the data directory) keeps this command and the
//! daemon's own update from replacing the file together.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use crate::platform::{fs, proc};
use crate::protocol::DaemonUpdateChannel;
use crate::self_update::check::{check, Checked};
use crate::self_update::fetch::UreqFetcher;
use crate::self_update::install::{self, InstallError, Sources};
use crate::self_update::manifest::{install_command, is_newer, Release};
use crate::self_update::record::Files;
use crate::self_update::replace::{self, ReplaceError};
use crate::self_update::verify::release_key;
use crate::self_update::RUNNING;

/// The exit status of `update --check` when a newer daemon is published.
const UPDATE_AVAILABLE: i32 = 10;

/// What the command was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Task {
    Install,
    Check,
    Rollback,
}

/// What the command reaches, handed in so that a test reaches none of the real network, key or
/// installation.
pub(crate) struct Environment<'a> {
    pub sources: Sources<'a>,
    pub files: &'a Files,
    /// The installation's path, as the daemon relaunches itself from it (`launch_path`).
    pub launch: &'a Path,
    pub channel: DaemonUpdateChannel,
    /// Whether a daemon runs against this data directory: the build it runs is the one an update
    /// replaces, which may not be the build of this process.
    pub daemon_running: bool,
}

/// Runs `task` as `octoboardd update` does, returning the exit status.
pub(super) fn run(task: Task, channel: Option<DaemonUpdateChannel>) -> Result<i32> {
    if !cfg!(feature = "self-update") || !proc::self_update_host() {
        eprintln!(
            "this build of {} does not update itself; upgrade by hand with `{}`",
            crate::APP_NAME,
            install_command()
        );
        return Ok(1);
    }
    if crate::instance_lock::is_running()? {
        let follows = crate::owner_record::Owner::recorded()
            .is_some_and(|owner| owner.follows_application == Some(true));
        if follows {
            eprintln!(
                "the running {} follows the desktop application, which updates it itself; not \
                 updating it",
                crate::APP_NAME
            );
            return Ok(1);
        }
    }
    let launch = super::launch_path()?;
    let files = Files::new(&crate::paths::data_dir());
    let channel = channel
        .or_else(|| crate::store::Store::peek_daemon_update_channel(&crate::paths::db_path()))
        .unwrap_or_default();
    let key = release_key();
    let daemon_running = crate::instance_lock::is_running()?;
    let environment = Environment {
        sources: Sources {
            fetcher: &UreqFetcher::command_line(),
            key: &key,
            arch: std::env::consts::ARCH,
            is_running_image: proc::names_running_image,
            // Nothing of a daemon's stops a command line process.
            stopping: &|| false,
        },
        files: &files,
        launch: &launch,
        channel,
        daemon_running,
    };
    let (stdout, stderr) = (&mut std::io::stdout(), &mut std::io::stderr());
    perform(task, &environment, stdout, stderr)
}

/// Runs `task` in `environment`, writing what it says to `out` and the reasons it did not do it to
/// `err`; the exit status.
pub(crate) fn perform(
    task: Task,
    environment: &Environment<'_>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<i32> {
    match task {
        Task::Check => check_only(environment, out, err),
        Task::Install => {
            let Some(_lock) = take_lock(environment.files, err)? else {
                return Ok(1);
            };
            install_newer(environment, out, err)
        }
        Task::Rollback => {
            let Some(_lock) = take_lock(environment.files, err)? else {
                return Ok(1);
            };
            roll_back(environment, out, err)
        }
    }
}

/// The update lock, or `None` after saying that the daemon or another command holds it.
fn take_lock(files: &Files, err: &mut dyn Write) -> Result<Option<std::fs::File>> {
    if let Some(dir) = files.lock().parent() {
        std::fs::create_dir_all(dir).context("creating the data directory")?;
    }
    match fs::try_lock_file(&files.lock()) {
        Ok(lock) => Ok(Some(lock)),
        Err(fs::Busy::Held) => {
            writeln!(
                err,
                "an update is under way already (in the running {} or in another command); try \
                 again once it has ended",
                crate::APP_NAME
            )?;
            Ok(None)
        }
        Err(fs::Busy::Open(cause) | fs::Busy::Lock(cause)) => {
            Err(cause).context("taking the update lock")
        }
    }
}

fn check_only(
    environment: &Environment<'_>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<i32> {
    match check(environment.sources.fetcher, environment.channel, RUNNING) {
        Ok(Checked::UpToDate) => {
            writeln!(out, "{} {RUNNING} is up to date", crate::APP_NAME)?;
            Ok(0)
        }
        Ok(Checked::Available(release)) => {
            writeln!(
                out,
                "{} {} is available (this is {RUNNING}); run `octoboardd update` to install it",
                crate::APP_NAME,
                release.version
            )?;
            Ok(UPDATE_AVAILABLE)
        }
        Err(detail) => {
            writeln!(err, "could not check for an update: {detail}")?;
            Ok(1)
        }
    }
}

fn install_newer(
    environment: &Environment<'_>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<i32> {
    let release = match check(environment.sources.fetcher, environment.channel, RUNNING) {
        Ok(Checked::UpToDate) => {
            writeln!(out, "{} {RUNNING} is up to date", crate::APP_NAME)?;
            return Ok(0);
        }
        Ok(Checked::Available(release)) => release,
        Err(detail) => {
            writeln!(err, "could not check for an update: {detail}")?;
            return Ok(1);
        }
    };
    let mut downloading = false;
    let installed = install::install(
        &environment.sources,
        &release,
        environment.launch,
        environment.files,
        &mut |_| {
            // Said when the archive begins to come, not before: an installation that cannot be
            // replaced is refused before anything is downloaded.
            if !downloading {
                downloading = true;
                writeln!(out, "downloading {} {}", crate::APP_NAME, release.version).ok();
            }
        },
    );
    match installed {
        Ok(done) if done.replaced => {
            writeln!(
                out,
                "updated {} from {RUNNING} to {}; the previous build is kept as `{}`",
                done.path.display(),
                release.version,
                replace::previous_path(&done.path)
                    .unwrap_or_default()
                    .display()
            )?;
            writeln!(
                out,
                "{}",
                restart_advice(&release, &restarting_from(environment))
            )?;
            Ok(0)
        }
        Ok(done) => {
            writeln!(
                err,
                "{} is not the build this command runs any more (something else replaced it); \
                 nothing was changed",
                done.path.display()
            )?;
            Ok(1)
        }
        Err(InstallError::NotWritable(path)) => {
            writeln!(
                err,
                "{} cannot be replaced by this user; upgrade by hand with `{}`",
                path.display(),
                install_command()
            )?;
            Ok(1)
        }
        Err(failure) => {
            writeln!(err, "the update failed: {}", failure.detail())?;
            Ok(1)
        }
    }
}

/// The version a restart would leave: the running daemon's, which `daemon.version` records at its
/// start, else this build's.
fn restarting_from(environment: &Environment<'_>) -> String {
    environment
        .daemon_running
        .then(|| environment.files.read_daemon_version())
        .flatten()
        .unwrap_or_else(|| RUNNING.to_owned())
}

/// What to do next after an update, and what the restart would cost: a restart is planned and keeps
/// every session running, except those in a holder older than the release's floor, which the
/// daemon at version `running` adopts from.
fn restart_advice(release: &Release, running: &str) -> String {
    let interrupts = release
        .holder_floor
        .as_deref()
        .is_some_and(|floor| is_newer(floor, running));
    let mut advice =
        "run `octoboardd restart` to run it, which keeps every session running".to_owned();
    if interrupts {
        advice.push_str(&format!(
            "; sessions started by a build older than {} cannot be handed over, so the restart \
             interrupts them",
            release.holder_floor.as_deref().unwrap_or_default()
        ));
    }
    advice
}

fn roll_back(
    environment: &Environment<'_>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<i32> {
    let real = match install::replaceable(environment.launch) {
        Ok(real) => real,
        Err(InstallError::NotWritable(path)) => {
            writeln!(
                err,
                "{} cannot be replaced by this user, so it cannot be rolled back",
                path.display()
            )?;
            return Ok(1);
        }
        Err(failure) => {
            writeln!(err, "could not roll back: {}", failure.detail())?;
            return Ok(1);
        }
    };
    match replace::swap_with_previous(&real) {
        Ok(()) => {
            // What `update-installed.json` says was installed no longer holds for this file.
            environment.files.forget_installed().ok();
            writeln!(
                out,
                "rolled {} back to the previous build, which was kept as `{}`",
                real.display(),
                replace::previous_path(&real).unwrap_or_default().display()
            )?;
            writeln!(
                out,
                "run `octoboardd restart` to run it, which keeps every session running"
            )?;
            Ok(0)
        }
        Err(ReplaceError::Failed(detail) | ReplaceError::Filling(detail)) => {
            writeln!(err, "could not roll back: {detail}")?;
            Ok(1)
        }
        Err(ReplaceError::NotWritable(path)) => {
            writeln!(
                err,
                "{} cannot be replaced by this user, so it cannot be rolled back",
                path.display()
            )?;
            Ok(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use minisign_verify::PublicKey;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::self_update::fetch::{FetchError, Fetcher, Sink};
    use crate::self_update::manifest::{manifest_url, platform_key};
    use crate::test_support::ScratchDir;

    const ARCHIVE: &[u8] =
        include_bytes!("../../testdata/self_update/octoboardd_9.9.9_linux_x86_64.tar.gz");
    const SIGNATURE: &str =
        include_str!("../../testdata/self_update/octoboardd_9.9.9_linux_x86_64.tar.gz.sig");
    const TEST_KEY: &str = include_str!("../../testdata/self_update/test-release-key.txt");
    const ARCHIVE_URL: &str = "https://downloads.example/octoboardd_9.9.9_linux_x86_64.tar.gz";

    /// Serves one manifest, or the failure to, and the archive.
    struct Host {
        manifest: Result<String, FetchError>,
        asked: Mutex<Vec<String>>,
    }

    impl Host {
        fn publishing(version: &str, floor: &str) -> Self {
            let sha256: String = Sha256::digest(ARCHIVE)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let manifest = serde_json::json!({
                "version": version,
                "holder_floor": floor,
                "platforms": { platform_key(): {
                    "url": ARCHIVE_URL,
                    "signature": SIGNATURE.trim(),
                    "sha256": sha256,
                }},
            });
            Self {
                manifest: Ok(manifest.to_string()),
                asked: Mutex::default(),
            }
        }
    }

    impl Fetcher for Host {
        fn get(
            &self,
            url: &str,
            _max_bytes: u64,
            _timeout: Duration,
            sink: &mut Sink<'_>,
        ) -> Result<(), FetchError> {
            self.asked.lock().unwrap().push(url.to_owned());
            let body = if url == manifest_url(DaemonUpdateChannel::Stable) {
                self.manifest.clone()?.into_bytes()
            } else if url == ARCHIVE_URL {
                ARCHIVE.to_vec()
            } else {
                return Err(FetchError::Status(404));
            };
            sink(&body, Some(body.len() as u64)).map_err(|err| FetchError::Failed(err.to_string()))
        }
    }

    fn is_running(_: &Path) -> bool {
        true
    }

    struct Fixture {
        _dir: ScratchDir,
        files: Files,
        exe: std::path::PathBuf,
        key: PublicKey,
        daemon_running: bool,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let dir = ScratchDir::new(label);
            std::fs::create_dir_all(dir.join("data")).unwrap();
            std::fs::create_dir_all(dir.join("bin")).unwrap();
            let exe = dir.join("bin").join("octoboardd");
            std::fs::write(&exe, b"the daemon that runs").unwrap();
            Self {
                files: Files::new(&dir.join("data")),
                _dir: dir,
                exe,
                key: PublicKey::from_base64(TEST_KEY.trim()).unwrap(),
                daemon_running: false,
            }
        }

        /// Runs `task` against `host`: the exit status, what was said and what was complained of.
        fn run(&self, task: Task, host: &Host) -> (i32, String, String) {
            self.run_on(task, host, DaemonUpdateChannel::Stable)
        }

        fn run_on(
            &self,
            task: Task,
            host: &Host,
            channel: DaemonUpdateChannel,
        ) -> (i32, String, String) {
            let environment = Environment {
                sources: Sources {
                    fetcher: host,
                    key: &self.key,
                    arch: "x86_64",
                    is_running_image: is_running,
                    stopping: &|| false,
                },
                files: &self.files,
                launch: &self.exe,
                channel,
                daemon_running: self.daemon_running,
            };
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let status = perform(task, &environment, &mut out, &mut err).unwrap();
            (
                status,
                String::from_utf8(out).unwrap(),
                String::from_utf8(err).unwrap(),
            )
        }
    }

    #[test]
    fn a_check_exits_0_up_to_date_10_for_a_newer_release_and_1_when_it_failed() {
        let fixture = Fixture::new("cli-update-check");
        let (status, out, _) = fixture.run(Task::Check, &Host::publishing(RUNNING, "0.0.0"));
        assert_eq!((status, out.contains("up to date")), (0, true), "{out}");

        let (status, out, _) = fixture.run(Task::Check, &Host::publishing("9.9.9", "0.0.0"));
        assert_eq!(status, 10);
        assert!(out.contains("9.9.9 is available"), "{out}");

        let failing = Host {
            manifest: Err(FetchError::Failed("offline".into())),
            asked: Mutex::default(),
        };
        let (status, out, err) = fixture.run(Task::Check, &failing);
        assert_eq!(status, 1);
        assert!(out.is_empty() && err.contains("offline"), "{err}");
        // A check replaces nothing and takes no lock.
        assert_eq!(
            std::fs::read(&fixture.exe).unwrap(),
            b"the daemon that runs"
        );
        assert!(!fixture.files.lock().exists());
    }

    #[test]
    fn the_channel_decides_which_manifest_is_looked_in() {
        let fixture = Fixture::new("cli-update-channel");
        // The host publishes a newer release on the stable line only.
        let host = Host::publishing("9.9.9", "0.0.0");
        for task in [Task::Check, Task::Install] {
            let (status, out, _) = fixture.run_on(task, &host, DaemonUpdateChannel::Beta);
            assert_eq!(status, 0, "{task:?}: nothing is published on beta");
            assert!(out.contains("up to date"), "{out}");
        }
        assert_eq!(
            host.asked.lock().unwrap().as_slice(),
            [
                manifest_url(DaemonUpdateChannel::Beta),
                manifest_url(DaemonUpdateChannel::Beta)
            ]
        );
        assert_eq!(
            std::fs::read(&fixture.exe).unwrap(),
            b"the daemon that runs"
        );
        let (status, _, _) = fixture.run_on(Task::Check, &host, DaemonUpdateChannel::Stable);
        assert_eq!(status, 10);
    }

    #[test]
    fn an_update_replaces_the_executable_and_says_to_restart() {
        let fixture = Fixture::new("cli-update-install");
        let host = Host::publishing("9.9.9", "0.0.0");
        let (status, out, err) = fixture.run(Task::Install, &host);
        assert_eq!(status, 0, "{err}");
        assert!(out.contains("to 9.9.9"), "{out}");
        assert!(out.contains("octoboardd restart"), "{out}");
        assert!(!out.contains("interrupts"), "{out}");
        assert_eq!(
            std::fs::read(&fixture.exe).unwrap(),
            b"#!/bin/sh\necho new daemon\n"
        );
        assert_eq!(
            std::fs::read(fixture.exe.with_file_name("octoboardd.previous")).unwrap(),
            b"the daemon that runs"
        );
        assert_eq!(fixture.files.read_installed().unwrap().version, "9.9.9");
    }

    #[test]
    fn the_restart_is_said_to_interrupt_what_a_holder_floor_above_this_build_leaves_behind() {
        let fixture = Fixture::new("cli-update-floor");
        let (status, out, _) = fixture.run(Task::Install, &Host::publishing("9.9.9", "9.0.0"));
        assert_eq!(status, 0);
        assert!(out.contains("interrupts them"), "{out}");
        assert!(out.contains("older than 9.0.0"), "{out}");
    }

    /// A daemon that runs is restarted from the version it runs, which `daemon.version` records,
    /// not from this process's build.
    #[test]
    fn the_warning_compares_the_floor_with_the_running_daemons_version() {
        // Below this build's version, so no warning when no daemon runs.
        let host = Host::publishing("9.9.9", "0.1.0-beta.1");
        let (_, out, _) = Fixture::new("cli-update-floor-none").run(Task::Install, &host);
        assert!(!out.contains("interrupts"), "{out}");

        let mut running = Fixture::new("cli-update-floor-running");
        running.daemon_running = true;
        running.files.write_daemon_version("0.0.9").unwrap();
        let (_, out, _) = running.run(Task::Install, &host);
        assert!(out.contains("interrupts them"), "{out}");
    }

    #[test]
    fn an_update_that_finds_nothing_newer_changes_nothing() {
        let fixture = Fixture::new("cli-update-none");
        let host = Host::publishing(RUNNING, "0.0.0");
        let (status, out, _) = fixture.run(Task::Install, &host);
        assert_eq!(status, 0);
        assert!(out.contains("up to date"), "{out}");
        assert_eq!(
            host.asked.lock().unwrap().len(),
            1,
            "no archive was fetched"
        );
        assert_eq!(
            std::fs::read(&fixture.exe).unwrap(),
            b"the daemon that runs"
        );
    }

    #[test]
    fn an_update_is_busy_while_the_daemon_or_another_command_holds_the_lock() {
        let fixture = Fixture::new("cli-update-busy");
        let _held = fs::try_lock_file(&fixture.files.lock()).unwrap();
        let host = Host::publishing("9.9.9", "0.0.0");
        for task in [Task::Install, Task::Rollback] {
            let (status, out, err) = fixture.run(task, &host);
            assert_eq!(status, 1);
            assert!(out.is_empty() && err.contains("under way already"), "{err}");
        }
        assert!(host.asked.lock().unwrap().is_empty());
        assert_eq!(
            std::fs::read(&fixture.exe).unwrap(),
            b"the daemon that runs"
        );
    }

    #[test]
    fn a_failed_update_says_why_and_exits_1() {
        let fixture = Fixture::new("cli-update-unverified");
        // The SHA-256 the manifest names is not the archive's: the archive must be refused.
        let mut host = Host::publishing("9.9.9", "0.0.0");
        let mut manifest: serde_json::Value =
            serde_json::from_str(host.manifest.as_ref().unwrap()).unwrap();
        manifest["platforms"][platform_key()]["sha256"] = "0".repeat(64).into();
        host.manifest = Ok(manifest.to_string());
        let (status, out, err) = fixture.run(Task::Install, &host);
        assert_eq!(status, 1);
        assert!(out.contains("downloading"), "{out}");
        assert!(
            err.contains("the update failed") && err.contains("SHA-256"),
            "{err}"
        );
        assert_eq!(
            std::fs::read(&fixture.exe).unwrap(),
            b"the daemon that runs"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_the_user_may_not_write_has_the_install_script_named() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: `geteuid` has no preconditions.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let fixture = Fixture::new("cli-update-readonly");
        let bin = fixture.exe.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o555)).unwrap();
        let host = Host::publishing("9.9.9", "0.0.0");
        let (status, out, err) = fixture.run(Task::Install, &host);
        let (rollback_status, _, rollback_err) = fixture.run(Task::Rollback, &host);
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(status, 1);
        assert!(err.contains("install.sh | sh"), "{err}");
        assert!(!out.contains("downloading"), "{out}");
        assert_eq!(rollback_status, 1);
        assert!(
            rollback_err.contains("cannot be rolled back"),
            "{rollback_err}"
        );
        assert!(
            host.asked.lock().unwrap().len() <= 1,
            "no archive was fetched"
        );
    }

    #[test]
    fn a_rollback_puts_the_previous_build_back_and_forgets_the_record() {
        let fixture = Fixture::new("cli-update-rollback");
        let host = Host::publishing("9.9.9", "0.0.0");
        let (status, _, _) = fixture.run(Task::Rollback, &host);
        assert_eq!(status, 1, "there is nothing to go back to before an update");

        fixture.run(Task::Install, &host);
        assert!(fixture.files.read_installed().is_some());
        let (status, out, err) = fixture.run(Task::Rollback, &host);
        assert_eq!(status, 0, "{err}");
        assert!(out.contains("octoboardd restart"), "{out}");
        assert_eq!(
            std::fs::read(&fixture.exe).unwrap(),
            b"the daemon that runs"
        );
        assert_eq!(
            std::fs::read(fixture.exe.with_file_name("octoboardd.previous")).unwrap(),
            b"#!/bin/sh\necho new daemon\n"
        );
        assert!(fixture.files.read_installed().is_none());
    }
}
