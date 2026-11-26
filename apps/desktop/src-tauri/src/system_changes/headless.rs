//! The clean-up as the Windows uninstaller has the application run it, before it removes anything
//! (`windows/installer-hooks.nsh`): the executable started with [`ARGUMENT`] and the path of a file
//! for what is left. It clears what the Settings row clears, through the same exceptions in the
//! same order, with this computer's own daemon's part and the taskbar's jump list, and opens no
//! window: nobody is asked anything, and the uninstall goes on whatever this comes to.
//!
//! It builds the application without a window, an icon or the single-instance hand-over, so that
//! the exceptions reach what they reach in the running application, and starts the bundled daemon
//! for the daemon's part once the shell's own are through, since the uninstaller has ended the
//! application and its daemon by then. That daemon makes a clean-up run (`StartFor::CleanUp`): it
//! takes over what the ended application left running and clears, and does nothing else a daemon
//! does at its start. It is stopped again before this exits, with the ordinary stop: nothing of
//! the application runs on after its uninstall.
//!
//! It refuses to run, clearing nothing, where it was not started as the uninstaller starts it:
//! with a file anywhere but in the installation's own folder, or while the application runs
//! ([`refusal`]).
//!
//! What it tells: its exit status ([`Status`]), the items left as text in the file the uninstaller
//! named in the installation's own folder (for its message box, so kept short; the uninstaller
//! deletes it once read), and the same in full in `uninstall-clean-up.log`
//! beside the shell's own log. A clean-up that leaves nothing writes neither, and the log of an
//! earlier one is removed before anything is cleared. Both are in the language the uninstaller
//! shows, which it passes after the file (English where it passes none), from the shell's native
//! catalogs (`localizations.rs`); what the shell's own log gets (`diagnostic!`), the reasons the
//! features give (an error's text, the daemon's message) and a feature's kind where no message
//! names it are for diagnosis and stay as they are.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tauri::AppHandle;
use tokio::sync::watch;

use super::daemon::{self, Daemon};
use super::{Change, Cleared, ClearedAll, Exception, Failed, Listed, Outcome};
use crate::diagnostics::diagnostic;
use crate::localizations::Catalog;
use crate::remote::{KeyRemoval, LineRemoval};
use crate::sidecar::{self, StartFor};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

pub use platform::has_uninstaller;

/// The argument the uninstaller starts the executable with to have the clean-up run and nothing
/// else done, followed by the path of the file for what is left and, optionally, the tag of the
/// language the uninstaller shows (`packages/ui/src/i18n/languages.ts`), which the report is
/// written in; the installer hooks name the same one, which
/// `apps/desktop/scripts/installer-hooks.test.mjs` checks.
pub const ARGUMENT: &str = "--clean-up-before-uninstall";

/// The log of what the last clean-up for an uninstall left, under the data directory's `logs`.
const LOG_NAME: &str = "uninstall-clean-up.log";

/// How long clearing is given in all. A device key's host that does not answer takes about 40
/// seconds, the hosts being asked side by side, and the daemon then ends and resumes the idle
/// sessions of each account around its change.
const CLEAR_BOUND: Duration = Duration::from_secs(120);

/// How long reading what is still in effect is given once clearing ran out of time.
const LIST_BOUND: Duration = Duration::from_secs(15);

/// How long the daemon is given to start. The uninstaller ends the application just before, and
/// the daemon that application ran may still be handing its sessions over, which holds the lock a
/// new one needs for some 30 seconds at most (`PLANNED_STOP_GRACE` in `sidecar/stop.rs`).
const DAEMON_START_BOUND: Duration = Duration::from_secs(45);

/// How long stopping the daemon waits for a start of it that is still under way to end, which it
/// does once it sees the stop: the attempt it is in kills its daemon and waits for it to be gone.
const START_END_WAIT: Duration = Duration::from_secs(15);

/// How long the daemon is given to end once it is asked to stop, before it is killed: its ordinary
/// stop ends every session it took over, each given its own time to end, and the holders, and
/// nothing has asked it to stop before; it also waits for a change of an account's sharing that
/// the clearing left at work on disk once it ran out of time. A daemon killed in the middle of
/// that leaves agents running in their holders after the uninstall, or that account's directory
/// half-changed ([`out_of_time`] says how to restore it by hand).
const DAEMON_STOP_BOUND: Duration = Duration::from_secs(30);

/// How long the application the uninstaller has just ended is given to be gone, its
/// single-instance lock with it, before the clean-up takes it for running and refuses.
const APPLICATION_END_WAIT: Duration = Duration::from_secs(10);

/// When the process ends whatever it is doing, so that the uninstaller is never kept waiting on
/// it: past the bounds above and the daemon's stop, which nothing should outlast.
const HARD_BOUND: Duration = Duration::from_secs(240);
// Every wait of a clean-up that runs its longest, in their order: for the application to be gone,
// the clearing (the daemon's start falls within it, and no start is begun after it), the reading
// of what is left, a start still under way to end, and the daemon's whole stop.
const _: () = assert!(
    APPLICATION_END_WAIT.as_secs()
        + CLEAR_BOUND.as_secs()
        + LIST_BOUND.as_secs()
        + START_END_WAIT.as_secs()
        + sidecar::stop_daemon_unasked_bound(DAEMON_STOP_BOUND).as_secs()
        < HARD_BOUND.as_secs()
);
const _: () = assert!(DAEMON_START_BOUND.as_secs() < CLEAR_BOUND.as_secs());

/// How many UTF-16 code units the uninstaller's message box holds: an NSIS string is 1024 with its
/// end.
const MESSAGE_UNITS: usize = 1023;

/// How many of those the uninstaller's own heading and the blank line after it may take
/// (`OCTOBOARD_CLEAN_UP_LEFT_HEADING` in the installer hooks, whose test holds it to this).
const HEADING_UNITS: usize = 160;

/// How much text goes into the file the uninstaller shows in its message box, in UTF-16 code
/// units: what the box holds after the uninstaller's heading.
const SHOWN_LIMIT: usize = MESSAGE_UNITS - HEADING_UNITS;

/// What the single-instance plugin adds to the bundle's identifier to name its lock, the named
/// mutex the running application holds (`platform_impl/windows.rs` of
/// `tauri-plugin-single-instance`, which `lifecycle/windows.rs` registers).
#[cfg(any(windows, test))]
const SINGLE_INSTANCE_LOCK_SUFFIX: &str = "-sim";

/// The name of the lock the running application whose bundle identifier is `identifier` holds.
#[cfg(any(windows, test))]
fn single_instance_lock_name(identifier: &str) -> String {
    format!("{identifier}{SINGLE_INSTANCE_LOCK_SUFFIX}")
}

/// What the clean-up came to, which is the process's exit status; the installer hooks read it.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Status {
    /// Nothing is left in effect.
    Cleared = 0,
    /// Something is left, or may be: the file names it.
    Left = 1,
    /// The clean-up could not run, so what is left is not known.
    CouldNotRun = 2,
}

impl Status {
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// What the clean-up came to and, where something is left, a line for each item.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Report {
    pub status: Status,
    pub lines: Vec<String>,
}

/// What the uninstaller asked the clean-up for.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Asked {
    /// The file for what is left.
    pub shown_file: PathBuf,
    /// The language tag the report is written in; English where there is none, or none the
    /// catalogs know.
    pub language: Option<String>,
}

/// What `args`, a process's own with the executable first, ask of the clean-up when they ask for
/// it and nothing else: the argument followed by the path of the file and, optionally, a language
/// tag. `None` for any other arguments.
pub fn asked<I, S>(args: I) -> Option<Asked>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut rest = args.into_iter().skip(1);
    if rest.next()?.as_ref() != OsStr::new(ARGUMENT) {
        return None;
    }
    let shown_file = PathBuf::from(rest.next()?.as_ref());
    let language = rest
        .next()
        .map(|tag| tag.as_ref().to_string_lossy().into_owned());
    rest.next().is_none().then_some(Asked {
        shown_file,
        language,
    })
}

/// Why the clean-up does not run, when it was not started as the uninstaller starts it: `None`
/// when it may. `shown_file` has to be in the folder of `executable`, this process's own, which is
/// the only place the clean-up writes outside the data directory; and no application may be
/// running, behind whose back its keys and login entry would be cleared, and whose daemon holds
/// the data directory. `application_running` is asked until it says no or `wait` is over, the
/// uninstaller having ended the application a moment before.
fn refusal(
    shown_file: &Path,
    executable: std::io::Result<PathBuf>,
    application_running: impl Fn() -> bool,
    wait: Duration,
) -> Option<String> {
    let folder = |path: &Path| path.parent().and_then(|dir| dir.canonicalize().ok());
    let installation = executable.ok().and_then(|path| folder(&path));
    if installation.is_none() || folder(shown_file) != installation {
        return Some(
            "the file for what is left is not in the installation's own folder".to_owned(),
        );
    }
    if !sidecar::wait_for(wait, || !application_running()) {
        return Some("the application is running".to_owned());
    }
    None
}

/// Clears everything `exceptions` and `own_daemon` have in effect, as the Settings row does, within
/// `bound`. Past it the clearing is left where it stands, and what is then still in effect is read
/// and reported as left.
pub(super) async fn clean_up<C: Sync>(
    exceptions: &[&dyn Exception<C>],
    app: &C,
    own_daemon: &dyn Daemon,
    bound: Duration,
    texts: Catalog,
) -> Report {
    match tokio::time::timeout(bound, super::clear_with(exceptions, app, own_daemon)).await {
        Ok(cleared) => report_of(&cleared, texts),
        Err(_) => {
            let listing = super::list_with(exceptions, app, own_daemon);
            let listed = tokio::time::timeout(LIST_BOUND, listing).await.ok();
            out_of_time(bound, listed.as_ref(), texts)
        }
    }
}

/// The code of the daemon's failure for a change of sharing that did not complete
/// (`account_sharing_failed` in `apps/daemon/PROTOCOL.md`): the account's directory is left as far
/// as the change got.
const SHARING_FAILED: &str = "account_sharing_failed";

/// What a clearing that ended came to: a line for each item of which something stays, with how to
/// restore by hand an account's directory a change of sharing left part-way, and for each feature
/// that could not tell what it has.
fn report_of(cleared: &ClearedAll, texts: Catalog) -> Report {
    let mut lines: Vec<String> = cleared
        .results
        .iter()
        .filter_map(|result| left_line(result, texts))
        .collect();
    let part_way = cleared.results.iter().any(|result| {
        matches!(&result.outcome, Outcome::AccountSharingLeft { failure } if failure.code == SHARING_FAILED)
    });
    if part_way {
        lines.push(texts.text("uninstallCleanUp.accountSharing.handRestore", &[]));
    }
    lines.extend(
        cleared
            .failed
            .iter()
            .map(|failed| failed_line(failed, texts)),
    );
    let status = if lines.is_empty() {
        Status::Cleared
    } else {
        Status::Left
    };
    Report { status, lines }
}

/// What a clearing that ran out of time came to: what `listed` says was still in effect when it
/// was read, or that it could not be read. Nothing listed, with every feature having answered, is
/// nothing left. An account that shares may have been in the middle of its change then, which the
/// daemon's stop waits for only until the daemon is killed (`DAEMON_STOP_BOUND`, [`HARD_BOUND`]):
/// how to restore its directory by hand follows.
fn out_of_time(bound: Duration, listed: Option<&Listed>, texts: Catalog) -> Report {
    let mut lines: Vec<String> = match listed {
        Some(listed) => {
            let sharing = listed
                .changes
                .iter()
                .any(|change| matches!(change, Change::AccountSharing { .. }));
            listed
                .changes
                .iter()
                .map(|change| in_effect_line(change, texts))
                .chain(
                    sharing.then(|| texts.text("uninstallCleanUp.accountSharing.handRestore", &[])),
                )
                .chain(
                    listed
                        .failed
                        .iter()
                        .map(|failed| failed_line(failed, texts)),
                )
                .collect()
        }
        None => vec![texts.text("uninstallCleanUp.notRead", &[])],
    };
    if lines.is_empty() {
        return Report {
            status: Status::Cleared,
            lines,
        };
    }
    let seconds = bound.as_secs().to_string();
    lines.insert(
        0,
        texts.text("uninstallCleanUp.outOfTime", &[("seconds", &seconds)]),
    );
    Report {
        status: Status::Left,
        lines,
    }
}

/// The line for an item of which something stays; none for one that is cleared.
fn left_line(cleared: &Cleared, texts: Catalog) -> Option<String> {
    let line = match &cleared.outcome {
        Outcome::Cleared => return None,
        Outcome::DeviceKeyLeft { removal } => key_left_line(&cleared.change, removal, texts),
        Outcome::LaunchAtLoginLeft { detail } => {
            texts.text("uninstallCleanUp.launchAtLogin.left", &[("detail", detail)])
        }
        Outcome::CommandLineLeft { error } => match &error.path {
            Some(path) => texts.text("uninstallCleanUp.commandLine.leftAt", &[("path", path)]),
            None => texts.text("uninstallCleanUp.commandLine.left", &[]),
        },
        Outcome::AccountSharingLeft { failure } => {
            let (name, agent) = match &cleared.change {
                Change::AccountSharing { name, agent, .. } => (name.as_str(), agent.as_str()),
                _ => ("", ""),
            };
            texts.text(
                "uninstallCleanUp.accountSharing.left",
                &[
                    ("name", name),
                    ("agent", agent),
                    ("message", &failure.message),
                ],
            )
        }
    };
    Some(line)
}

/// The line for a device key of which `removal` says a part stays, each part with where it is.
fn key_left_line(change: &Change, removal: &KeyRemoval, texts: Catalog) -> String {
    let name = match change {
        Change::DeviceKey { name, .. } => name.as_str(),
        _ => "",
    };
    let mut parts = Vec::new();
    match &removal.line {
        LineRemoval::NotInstalled | LineRemoval::Removed => {}
        LineRemoval::LeftOnHost {
            mark,
            file,
            host,
            user,
            ..
        } => parts.push(match file {
            Some(file) => texts.text(
                "uninstallCleanUp.deviceKey.lineLeftInFile",
                &[
                    ("mark", mark),
                    ("file", file),
                    ("user", user),
                    ("host", host),
                ],
            ),
            None => texts.text(
                "uninstallCleanUp.deviceKey.lineLeft",
                &[("mark", mark), ("user", user), ("host", host)],
            ),
        }),
        LineRemoval::FileIncomplete {
            host, user, copy, ..
        } => parts.push(texts.text(
            "uninstallCleanUp.deviceKey.fileIncomplete",
            &[("user", user), ("host", host), ("copy", copy)],
        )),
    }
    if let Some(left) = &removal.private_key_left {
        parts.push(texts.text(
            "uninstallCleanUp.deviceKey.privateKeyLeft",
            &[("service", &left.service), ("account", &left.account)],
        ));
    }
    if parts.is_empty() {
        parts.push(texts.text("uninstallCleanUp.deviceKey.partLeft", &[]));
    }
    let separator = texts.text("uninstallCleanUp.deviceKey.partSeparator", &[]);
    texts.text(
        "uninstallCleanUp.deviceKey.left",
        &[("name", name), ("parts", &parts.join(&separator))],
    )
}

/// The line for an item that is in effect.
fn in_effect_line(change: &Change, texts: Catalog) -> String {
    match change {
        Change::DeviceKey {
            name,
            host,
            user,
            on_server: true,
            ..
        } => texts.text(
            "uninstallCleanUp.deviceKey.inEffectOnServer",
            &[("name", name), ("user", user), ("host", host)],
        ),
        Change::DeviceKey { name, .. } => {
            texts.text("uninstallCleanUp.deviceKey.inEffect", &[("name", name)])
        }
        Change::LaunchAtLogin { .. } => texts.text("uninstallCleanUp.launchAtLogin.inEffect", &[]),
        Change::CommandLine { path } => {
            texts.text("uninstallCleanUp.commandLine.inEffect", &[("path", path)])
        }
        Change::AccountSharing { name, agent, .. } => texts.text(
            "uninstallCleanUp.accountSharing.inEffect",
            &[("name", name), ("agent", agent)],
        ),
    }
}

/// The line for a feature that could not tell what it has in effect.
fn failed_line(failed: &Failed, texts: Catalog) -> String {
    let key = match failed.kind {
        "device_key" => "uninstallCleanUp.failed.deviceKey",
        "launch_at_login" => "uninstallCleanUp.failed.launchAtLogin",
        "command_line" => "uninstallCleanUp.failed.commandLine",
        daemon::KIND => "uninstallCleanUp.failed.accountSharing",
        _ => "uninstallCleanUp.failed.other",
    };
    texts.text(key, &[("feature", failed.kind), ("detail", &failed.detail)])
}

/// The text for the uninstaller's message box, within `limit` UTF-16 code units in all: as many
/// of the lines as fit, then how many more there are, then where `log` holds all of them.
fn shown(lines: &[String], log: Option<&Path>, limit: usize, texts: Catalog) -> String {
    let units = |text: &str| text.encode_utf16().count();
    let saved = log
        .map(|log| {
            let path = log.display().to_string();
            texts.text("uninstallCleanUp.saved", &[("path", &path)]) + "\r\n"
        })
        .unwrap_or_default();
    let more =
        |left: usize| texts.text("uninstallCleanUp.more", &[("count", &left.to_string())]) + "\r\n";
    let room = limit.saturating_sub(units(&saved));
    let all: usize = lines.iter().map(|line| units(line) + 2).sum();
    // Where not every line fits, the count of the rest has to, at its longest.
    let room = if all <= room {
        room
    } else {
        room.saturating_sub(units(&more(lines.len())))
    };
    let mut text = String::new();
    let mut used = 0;
    let mut fitted = 0;
    for line in lines {
        let length = units(line) + 2;
        if used + length > room {
            break;
        }
        text.push_str(line);
        text.push_str("\r\n");
        used += length;
        fitted += 1;
    }
    if fitted < lines.len() {
        text.push_str(&more(lines.len() - fitted));
    }
    text.push_str(&saved);
    text
}

/// Removes the log an earlier clean-up left, before this one clears anything: what the log holds
/// is then this clean-up's, or nothing.
fn forget_earlier(log: Option<&Path>) {
    if let Some(log) = log {
        let _ = std::fs::remove_file(log);
    }
}

/// Writes what `report` left: every line to `log`, and to `shown_file`, as UTF-16 without a byte
/// order mark (what the uninstaller's `FileReadUTF16LE` reads), as much as its message box holds.
/// With nothing left neither is written.
fn write(report: &Report, shown_file: &Path, log: Option<&Path>, texts: Catalog) {
    if report.status == Status::Cleared {
        return;
    }
    let log = log.filter(|log| {
        let written = log
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(log, report.lines.join("\n") + "\n"));
        if let Err(err) = &written {
            diagnostic!("the clean-up's log could not be written: {err}");
        }
        written.is_ok()
    });
    let bytes: Vec<u8> = shown(&report.lines, log, SHOWN_LIMIT, texts)
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    if let Err(err) = std::fs::write(shown_file, bytes) {
        diagnostic!("what the clean-up left could not be written for the uninstaller: {err}");
    }
}

/// A start made at most once, on a thread of its own, which whoever asks for it waits for: giving
/// the wait up leaves the start running, and asking again never starts a second one.
struct OnceStart {
    /// What the start came to, once it has; set when the start is begun.
    begun: OnceLock<watch::Receiver<Option<Result<(), String>>>>,
    /// From when on no start is begun any more.
    not_after: Instant,
}

impl OnceStart {
    fn new(not_after: Instant) -> Self {
        Self {
            begun: OnceLock::new(),
            not_after,
        }
    }

    /// What the start came to, beginning it with `start` when none was begun and it is not too
    /// late for one.
    async fn started(
        &self,
        start: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) -> Result<(), String> {
        if self.begun.get().is_none() && Instant::now() >= self.not_after {
            return Err("it was not started: the clean-up had run out of time".to_owned());
        }
        let mut settled = self
            .begun
            .get_or_init(|| {
                let (tell, settled) = watch::channel(None);
                std::thread::spawn(move || {
                    let _ = tell.send(Some(start()));
                });
                settled
            })
            .clone();
        Self::outcome(&mut settled).await
    }

    /// Waits up to `wait` for a start that was begun to end; whether none is under way then.
    async fn ended(&self, wait: Duration) -> bool {
        match self.begun.get() {
            Some(settled) => {
                let mut settled = settled.clone();
                tokio::time::timeout(wait, Self::outcome(&mut settled))
                    .await
                    .is_ok()
            }
            None => true,
        }
    }

    fn begun(&self) -> bool {
        self.begun.get().is_some()
    }

    async fn outcome(
        settled: &mut watch::Receiver<Option<Result<(), String>>>,
    ) -> Result<(), String> {
        match settled.wait_for(Option::is_some).await {
            Ok(outcome) => match &*outcome {
                Some(outcome) => outcome.clone(),
                None => Ok(()),
            },
            // The thread ended without an answer: the start panicked.
            Err(_) => Err("starting it failed".to_owned()),
        }
    }
}

/// This computer's own daemon, started for the clean-up when its part is first asked for.
struct Started<'a> {
    app: &'a AppHandle,
    start: OnceStart,
}

impl<'a> Started<'a> {
    /// The daemon is started only when asked for before `not_after`: past the clearing's bound
    /// none is started for the reading of what is left, whose part is then not known.
    fn new(app: &'a AppHandle, not_after: Instant) -> Self {
        Self {
            app,
            start: OnceStart::new(not_after),
        }
    }

    /// Stops the daemon, if a start of it was begun, and waits until it is gone. A start still
    /// under way is told first, and waited for, so that it starts no daemon after this.
    async fn stop(&self) {
        if !self.start.begun() {
            return;
        }
        sidecar::mark_daemon_stopping(self.app);
        if !self.start.ended(START_END_WAIT).await {
            diagnostic!("the daemon's start for the clean-up did not end in time");
        }
        let app = self.app.clone();
        let stopped = tauri::async_runtime::spawn_blocking(move || {
            sidecar::stop_daemon_unasked(&app, DAEMON_STOP_BOUND);
        });
        if let Err(err) = stopped.await {
            diagnostic!("stopping the daemon after the clean-up failed: {err}");
        }
    }
}

impl Daemon for Started<'_> {
    fn post<'a>(&'a self, path: &'static str, wait: Duration) -> daemon::Asked<'a> {
        Box::pin(async move {
            let app = self.app.clone();
            self.start
                .started(move || {
                    sidecar::start_daemon(&app, StartFor::CleanUp(DAEMON_START_BOUND)).map(drop)
                })
                .await
                .map_err(|err| format!("this computer's daemon could not be asked: {err}"))?;
            daemon::Local(self.app).post(path, wait).await
        })
    }
}

/// Runs the clean-up in the application `app` is the handle of, writes what it left and answers
/// what it came to, which is the process's status from the moment it is written. The daemon it
/// started is gone when this returns.
async fn run_in(app: AppHandle, shown_file: PathBuf, texts: Catalog) -> Status {
    let log = crate::diagnostics::logs_dir().map(|dir| dir.join(LOG_NAME));
    forget_earlier(log.as_deref());
    let own_daemon = Started::new(&app, Instant::now() + CLEAR_BOUND);
    let report = clean_up(super::REGISTERED, &app, &own_daemon, CLEAR_BOUND, texts).await;
    for line in &report.lines {
        diagnostic!("the clean-up for the uninstall left: {line}");
    }
    settle(
        || super::remove_jump_list(&app),
        || write(&report, &shown_file, log.as_deref(), texts),
        |status| EXIT_STATUS.store(status, Ordering::SeqCst),
        report.status,
    );
    own_daemon.stop().await;
    report.status
}

/// Settles what the process answers once the clean-up is over: the jump list removed first, then
/// what was left written and the status set to `status`. A status of the clean-up's own (0 or 1)
/// tells the uninstaller's hooks that the jump list is gone, so it is set only once it is: the
/// hard time limit ending the process in between answers [`Status::CouldNotRun`], for which the
/// hooks remove the list themselves. All of it before the daemon is stopped, so that a stop that
/// hangs loses nothing of it.
fn settle(
    remove_jump_list: impl FnOnce(),
    write: impl FnOnce(),
    set: impl FnOnce(i32),
    status: Status,
) {
    remove_jump_list();
    write();
    set(status.code());
}

/// The status the process ends with, which the clean-up sets once it has written what it left.
static EXIT_STATUS: AtomicI32 = AtomicI32::new(Status::CouldNotRun as i32);

/// The application the clean-up runs in, for the last bound to end its daemon through.
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Runs the clean-up `asked` asks for in the application `build` makes, which has no window and
/// whose bundle identifier is `identifier`, and ends the process with its [`Status`]. Never
/// returns: on the main thread, in place of the application's own run.
pub fn run(build: impl FnOnce() -> tauri::Result<tauri::App>, asked: Asked, identifier: &str) -> ! {
    let Asked {
        shown_file,
        language,
    } = asked;
    let texts = Catalog::of(language.as_deref());
    std::thread::spawn(|| {
        std::thread::sleep(HARD_BOUND);
        diagnostic!("the clean-up for the uninstall was ended: it did not finish in time");
        // Not left running: nothing stops it once this process is gone.
        if let Some(app) = APP.get() {
            sidecar::kill_daemon(app);
        }
        std::process::exit(EXIT_STATUS.load(Ordering::SeqCst));
    });
    let refused = refusal(
        &shown_file,
        std::env::current_exe(),
        || platform::application_running(identifier),
        APPLICATION_END_WAIT,
    );
    if let Some(why) = refused {
        diagnostic!("the clean-up for the uninstall did not run: {why}");
        std::process::exit(Status::CouldNotRun.code());
    }
    let app = match build() {
        Ok(app) => app,
        Err(err) => {
            diagnostic!("the clean-up for the uninstall could not start: {err}");
            std::process::exit(Status::CouldNotRun.code());
        }
    };
    let handle = app.handle().clone();
    let _ = APP.set(handle.clone());
    tauri::async_runtime::spawn(async move {
        // In a task of its own, so that a panic in it is an answer and not a wait without end.
        let work = tauri::async_runtime::spawn(run_in(handle.clone(), shown_file, texts));
        let status = work.await.map_or_else(
            // What it had written before it panicked stands.
            |_| EXIT_STATUS.load(Ordering::SeqCst),
            Status::code,
        );
        handle.exit(status);
    });
    app.run(|_, event| {
        if let tauri::RunEvent::Exit = event {
            std::process::exit(EXIT_STATUS.load(Ordering::SeqCst));
        }
    });
    std::process::exit(EXIT_STATUS.load(Ordering::SeqCst))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    /// The documentation site names the clean-up's log as this fixture holds it.
    #[test]
    fn the_log_is_the_one_the_docs_name() {
        let quoted: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/docs_system_paths.json")).unwrap();
        assert_eq!(quoted["clean_up_log_name"].as_str(), Some(super::LOG_NAME));
    }

    /// The status that tells the hooks the jump list is gone is set only after it is removed,
    /// and after what was left is written.
    #[test]
    fn the_status_is_set_only_once_the_jump_list_is_removed_and_the_list_written() {
        for status in [super::Status::Cleared, super::Status::Left] {
            let done = std::cell::RefCell::new(Vec::new());
            super::settle(
                || done.borrow_mut().push("jump list removed".to_owned()),
                || done.borrow_mut().push("written".to_owned()),
                |code| done.borrow_mut().push(format!("status {code}")),
                status,
            );
            assert_eq!(
                *done.borrow(),
                [
                    "jump list removed".to_owned(),
                    "written".to_owned(),
                    format!("status {}", status.code())
                ]
            );
        }
    }

    use super::super::daemon::tests::{sharing, StandIn as OwnDaemon, SHARING};
    use super::super::tests::{link, stand_in, LOGIN};
    use super::super::DaemonFailure;
    use super::*;

    /// The catalog a run with no language writes in.
    fn en() -> Catalog {
        Catalog::of(None)
    }
    use crate::config_dir::test_support::Scratch;
    use crate::remote::PrivateKeyLeft;

    fn gone(change: Change) -> Cleared {
        Cleared {
            change,
            outcome: Outcome::Cleared,
        }
    }

    fn key(name: &str) -> Change {
        Change::DeviceKey {
            id: "e".into(),
            name: name.into(),
            host: "build.example".into(),
            user: "dev".into(),
            on_server: true,
        }
    }

    /// An exception whose clearing never ends, as a host that never answers would make it.
    struct Stalled;

    impl Exception<()> for Stalled {
        fn kind(&self) -> &'static str {
            "stalled"
        }

        fn in_effect<'a>(&'a self, _: &'a ()) -> super::super::Answer<'a, Change> {
            Box::pin(async { Ok(vec![key("Build server")]) })
        }

        fn clear_all<'a>(&'a self, _: &'a ()) -> super::super::Answer<'a, Cleared> {
            Box::pin(std::future::pending())
        }
    }

    #[test]
    fn only_the_argument_with_one_path_and_perhaps_a_language_asks_for_the_clean_up() {
        assert_eq!(
            asked(["app.exe", ARGUMENT, "C:\\t\\left.txt"]),
            Some(Asked {
                shown_file: PathBuf::from("C:\\t\\left.txt"),
                language: None,
            })
        );
        assert_eq!(
            asked(["app.exe", ARGUMENT, "C:\\t\\left.txt", "zh-CN"]),
            Some(Asked {
                shown_file: PathBuf::from("C:\\t\\left.txt"),
                language: Some("zh-CN".into()),
            })
        );
        assert_eq!(asked(["app.exe", ARGUMENT]), None);
        assert_eq!(asked(["app.exe", ARGUMENT, "a", "b", "c"]), None);
        assert_eq!(asked(["app.exe", "--open", ARGUMENT, "a"]), None);
        assert_eq!(asked(["app.exe"]), None);
    }

    #[tokio::test]
    async fn everything_cleared_is_the_status_zero_with_nothing_to_tell() {
        let shell = stand_in(Ok(vec![]), Ok(vec![gone(LOGIN), gone(link("/a"))]));
        let own_daemon = OwnDaemon::answering(&format!(
            r#"{{"results":[{{"change":{SHARING}}}],"failed":[]}}"#
        ));
        let report = clean_up(&[&shell], &(), &own_daemon, CLEAR_BOUND, en()).await;
        assert_eq!(
            report,
            Report {
                status: Status::Cleared,
                lines: vec![]
            }
        );
        assert_eq!(report.status.code(), 0);
        assert_eq!(own_daemon.routes(), ["/system-changes/clear"]);
    }

    #[tokio::test]
    async fn an_item_left_is_the_status_one_and_named_with_where_it_is() {
        let left = Cleared {
            change: key("Build server"),
            outcome: Outcome::DeviceKeyLeft {
                removal: KeyRemoval {
                    line: LineRemoval::LeftOnHost {
                        mark: "octoboard-device".into(),
                        file: Some("~/.ssh/authorized_keys".into()),
                        host: "build.example".into(),
                        user: "dev".into(),
                        detail: "unreachable".into(),
                    },
                    private_key_left: Some(PrivateKeyLeft {
                        service: "dev.octoboard.desktop".into(),
                        account: "e".into(),
                        host: "build.example".into(),
                        user: "dev".into(),
                        detail: "refused".into(),
                    }),
                },
            },
        };
        let shell = stand_in(Ok(vec![]), Ok(vec![gone(LOGIN), left]));
        let body = format!(
            r#"{{"results":[{{"change":{SHARING},"failure":{{"code":"account_in_use","params":{{}},"message":"in use"}}}}],"failed":[]}}"#
        );
        let report = clean_up(
            &[&shell],
            &(),
            &OwnDaemon::answering(&body),
            CLEAR_BOUND,
            en(),
        )
        .await;
        assert_eq!(report.status.code(), 1);
        assert_eq!(
            report.lines,
            [
                "This computer’s key for “Build server”: its line, marked “octoboard-device”, is \
                 still in ~/.ssh/authorized_keys of dev on build.example, to be removed there; \
                 its private key is still in this computer’s credential store, under \
                 “dev.octoboard.desktop” with the account e.",
                "The account “Work” (claude) shares its agent’s default configuration. It \
                 still does: in use",
            ]
        );
    }

    #[test]
    fn each_kind_of_item_left_has_its_line() {
        let line = |change, outcome| left_line(&Cleared { change, outcome }, en()).unwrap();
        assert_eq!(left_line(&gone(LOGIN), en()), None);
        assert_eq!(
            line(
                LOGIN,
                Outcome::LaunchAtLoginLeft {
                    detail: "access denied".into()
                }
            ),
            "Launch at login: the entry is still in the system’s startup list (access denied)."
        );
        assert_eq!(
            line(
                link("/a"),
                Outcome::CommandLineLeft {
                    error: crate::command_line::Error::failed("busy")
                }
            ),
            "The command-line tool: its link is still there."
        );
        assert_eq!(
            line(
                key("Build server"),
                Outcome::DeviceKeyLeft {
                    removal: KeyRemoval {
                        line: LineRemoval::FileIncomplete {
                            host: "build.example".into(),
                            user: "dev".into(),
                            copy: "~/.ssh/authorized_keys.octoboard".into(),
                            detail: "disk full".into(),
                        },
                        private_key_left: None,
                    }
                }
            ),
            "This computer’s key for “Build server”: the authorized keys of dev on build.example \
             may be incomplete, to be restored from ~/.ssh/authorized_keys.octoboard on that host."
        );
        assert_eq!(
            line(
                sharing(),
                Outcome::AccountSharingLeft {
                    failure: DaemonFailure {
                        code: "account_in_use".into(),
                        params: BTreeMap::new(),
                        message: "in use".into(),
                    }
                }
            ),
            "The account “Work” (claude) shares its agent’s default configuration. It still \
             does: in use"
        );
    }

    #[tokio::test]
    async fn an_exception_that_fails_as_a_whole_is_told_as_not_known_beside_the_rest() {
        let broken = stand_in(Ok(vec![]), Err("unreadable".into()));
        let shell = stand_in(Ok(vec![]), Ok(vec![gone(LOGIN)]));
        let own_daemon = OwnDaemon::failing("this computer's daemon could not be started: no");
        let report = clean_up(&[&broken, &shell], &(), &own_daemon, CLEAR_BOUND, en()).await;
        assert_eq!(report.status, Status::Left);
        assert_eq!(
            report.lines,
            [
                "Could not tell what is left of stand_in (unreadable).",
                "Could not tell what is left of the accounts that share a default configuration \
                 (this computer's daemon could not be started: no).",
            ]
        );
    }

    #[tokio::test]
    async fn clearing_that_outlasts_its_bound_is_left_and_tells_what_is_still_in_effect() {
        let quick = stand_in(Ok(vec![LOGIN]), Ok(vec![gone(LOGIN)]));
        let own_daemon = OwnDaemon::answering(r#"{"changes":[]}"#);
        let bound = Duration::from_millis(20);
        let report = clean_up(&[&Stalled, &quick], &(), &own_daemon, bound, en()).await;
        assert_eq!(report.status, Status::Left);
        assert_eq!(
            report.lines,
            [
                "The clean-up did not finish within 0 seconds and was ended. This was still in \
                 effect then, and some of it may have been removed since it was read:",
                "This computer’s key for “Build server”: its line in the authorized keys of dev on \
                 build.example, and its private key in this computer’s credential store.",
                "Launch at login: the entry in the system’s startup list.",
            ]
        );
        // The exception after the stalled one was never reached, and the daemon only read.
        assert_eq!(own_daemon.routes(), ["/system-changes"]);
    }

    #[test]
    fn a_clearing_out_of_time_that_reads_nothing_in_effect_left_nothing() {
        let bound = CLEAR_BOUND;
        assert_eq!(
            out_of_time(bound, Some(&Listed::default()), en()),
            Report {
                status: Status::Cleared,
                lines: vec![]
            }
        );
        // A feature that could not tell, or a reading that did not come back, is not nothing.
        let unread = Listed {
            changes: vec![],
            failed: vec![Failed {
                kind: daemon::KIND,
                detail: "no answer".into(),
            }],
        };
        let report = out_of_time(bound, Some(&unread), en());
        assert_eq!(report.status, Status::Left);
        assert_eq!(report.lines.len(), 2);
        assert_eq!(
            out_of_time(bound, None, en()).lines[1],
            "What is still in effect could not be read."
        );
    }

    /// The line that tells how to restore by hand an account's directory a change of sharing may
    /// have left half-changed.
    fn hand_restore() -> String {
        en().text("uninstallCleanUp.accountSharing.handRestore", &[])
    }

    /// A change of sharing the daemon left part-way is followed by how to restore the directory by
    /// hand, and an account the daemon refused to change is not: its directory was not touched.
    #[tokio::test]
    async fn a_change_of_sharing_left_part_way_tells_how_to_restore_it_by_hand() {
        let shell = stand_in(Ok(vec![]), Ok(vec![gone(LOGIN)]));
        let body = format!(
            r#"{{"results":[{{"change":{SHARING},"failure":{{"code":"account_sharing_failed","params":{{}},"message":"settings.json left linked"}}}}],"failed":[]}}"#
        );
        let report = clean_up(
            &[&shell],
            &(),
            &OwnDaemon::answering(&body),
            CLEAR_BOUND,
            en(),
        )
        .await;
        assert_eq!(report.status, Status::Left);
        assert_eq!(
            report.lines,
            [
                "The account “Work” (claude) shares its agent’s default configuration. It still \
                 does: settings.json left linked"
                    .to_owned(),
                hand_restore(),
            ]
        );
        assert!(hand_restore().contains("<entry>.bak-<stamp>"));
    }

    /// A clearing out of time may have cut an account's change of sharing short, so an account
    /// still listed is followed by how to restore its directory by hand; nothing else is.
    #[test]
    fn a_clearing_out_of_time_with_an_account_still_sharing_tells_how_to_restore_it() {
        let listed = Listed {
            changes: vec![sharing()],
            failed: vec![],
        };
        let report = out_of_time(CLEAR_BOUND, Some(&listed), en());
        assert_eq!(report.status, Status::Left);
        assert_eq!(report.lines.len(), 3, "{:?}", report.lines);
        assert_eq!(
            report.lines[1],
            "The account “Work” (claude) shares its agent’s default configuration."
        );
        assert_eq!(report.lines[2], hand_restore());
        let others = Listed {
            changes: vec![LOGIN],
            failed: vec![],
        };
        let report = out_of_time(CLEAR_BOUND, Some(&others), en());
        assert!(
            !report.lines.contains(&hand_restore()),
            "{:?}",
            report.lines
        );
    }

    #[test]
    fn every_registered_feature_that_cannot_tell_is_named_in_words() {
        let kinds = super::super::REGISTERED
            .iter()
            .map(|exception| exception.kind())
            .chain([daemon::KIND]);
        for kind in kinds {
            let line = failed_line(
                &Failed {
                    kind,
                    detail: "why".into(),
                },
                en(),
            );
            assert!(
                !line.contains(&format!("of {kind} could")),
                "{kind} has no words of its own: {line}"
            );
        }
        assert_eq!(
            failed_line(
                &Failed {
                    kind: "later_kind",
                    detail: "why".into()
                },
                en()
            ),
            "Could not tell what is left of later_kind (why)."
        );
    }

    #[test]
    fn the_clean_up_refuses_a_file_outside_the_installation_and_a_running_application() {
        let scratch = Scratch::new("clean-up", "refusal");
        let dir = &scratch.0;
        let (installation, elsewhere) = (dir.join("app"), dir.join("elsewhere"));
        std::fs::create_dir_all(&installation).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        let exe = || Ok(installation.join("octoboard.exe"));
        let none = Duration::ZERO;
        let inside = installation.join("left.txt");
        assert_eq!(refusal(&inside, exe(), || false, none), None);
        // The same folder, named another way.
        let roundabout = elsewhere.join("..").join("app").join("left.txt");
        assert_eq!(refusal(&roundabout, exe(), || false, none), None);
        let outside = "the file for what is left is not in the installation's own folder";
        for file in [
            elsewhere.join("left.txt"),
            dir.join("absent").join("left.txt"),
            PathBuf::from("left.txt"),
        ] {
            let refused = refusal(&file, exe(), || false, none);
            assert_eq!(refused.as_deref(), Some(outside), "{}", file.display());
        }
        // Where this process's own executable is not known, no folder is the installation's.
        let unknown = Err(std::io::Error::other("unknown"));
        assert_eq!(
            refusal(&inside, unknown, || false, none).as_deref(),
            Some(outside)
        );
        assert_eq!(
            refusal(&inside, exe(), || true, none).as_deref(),
            Some("the application is running")
        );
        // An application that is just ending is waited for.
        let asked = std::sync::atomic::AtomicUsize::new(0);
        let ending = || asked.fetch_add(1, Ordering::SeqCst) < 2;
        assert_eq!(refusal(&inside, exe(), ending, APPLICATION_END_WAIT), None);
    }

    #[test]
    fn the_running_application_s_lock_is_named_as_the_single_instance_plugin_names_it() {
        assert_eq!(
            single_instance_lock_name("dev.octoboard.desktop"),
            "dev.octoboard.desktop-sim"
        );
    }

    #[tokio::test]
    async fn a_start_is_made_once_and_goes_on_when_its_wait_is_given_up() {
        let start = OnceStart::new(Instant::now() + HARD_BOUND);
        assert!(!start.begun() && start.ended(Duration::ZERO).await);
        let made = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (entered, mut under_way) = tokio::sync::mpsc::unbounded_channel();
        let (release, held) = std::sync::mpsc::channel::<()>();
        let slow = {
            let made = made.clone();
            move || {
                made.fetch_add(1, Ordering::SeqCst);
                let _ = entered.send(());
                held.recv().map_err(|err| err.to_string())
            }
        };
        // The wait is given up once the start is under way, as the clearing's bound gives it up.
        {
            let waiting = start.started(slow);
            tokio::pin!(waiting);
            tokio::select! {
                answered = &mut waiting => panic!("the start ended while it was held: {answered:?}"),
                entered = under_way.recv() => assert_eq!(entered, Some(())),
            }
        }
        // It is held, so it cannot have ended: stopping the daemon would wait for it.
        assert!(start.begun() && !start.ended(Duration::ZERO).await);
        // Asked for again, it is the same start that is waited for, and no other is made.
        let again = {
            let made = made.clone();
            move || {
                made.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        };
        release.send(()).unwrap();
        assert_eq!(start.started(again).await, Ok(()));
        assert_eq!(made.load(Ordering::SeqCst), 1);
        assert!(start.ended(Duration::ZERO).await);
    }

    #[tokio::test]
    async fn no_start_is_begun_once_it_is_too_late_and_one_begun_before_is_still_answered() {
        let late = OnceStart::new(Instant::now());
        let refused = late.started(|| panic!("started all the same")).await;
        assert_eq!(
            refused,
            Err("it was not started: the clean-up had run out of time".to_owned())
        );
        assert!(!late.begun());

        let mut start = OnceStart::new(Instant::now() + HARD_BOUND);
        assert_eq!(
            start.started(|| Err("no".to_owned())).await,
            Err("no".into())
        );
        // Past the moment, what the start begun before it came to is answered, with none made.
        start.not_after = Instant::now();
        let answered = start.started(|| panic!("started a second time")).await;
        assert_eq!(answered, Err("no".into()));
    }

    #[test]
    fn what_is_shown_is_cut_to_what_the_message_box_holds_and_says_where_the_rest_is() {
        let lines = vec!["a".repeat(30), "b".repeat(30), "c".repeat(30)];
        let log = Path::new("logs").join(LOG_NAME);
        let all = shown(&lines, Some(&log), SHOWN_LIMIT, en());
        assert_eq!(
            all,
            format!(
                "{}\r\n{}\r\n{}\r\nThis list is saved in {}.\r\n",
                lines[0],
                lines[1],
                lines[2],
                log.display()
            )
        );
        // 32 units a line: two fit beside the count of the rest, which is counted too.
        let cut = shown(&lines, None, 80, en());
        assert_eq!(
            cut,
            format!("{}\r\n{}\r\nAnd 1 more.\r\n", lines[0], lines[1])
        );
        assert_eq!(
            shown(&lines, None, 70, en()),
            format!("{}\r\nAnd 2 more.\r\n", lines[0])
        );
        // Where the list is saved is counted as well, and is what stays when room is short.
        let saved = format!("This list is saved in {}.\r\n", log.display());
        let units = |text: &str| text.encode_utf16().count();
        let tight = shown(&lines, Some(&log), units(&saved) + 50, en());
        assert_eq!(tight, format!("{}\r\nAnd 2 more.\r\n{saved}", lines[0]));
        assert_eq!(
            shown(&lines, Some(&log), 10, en()),
            format!("And 3 more.\r\n{saved}")
        );
    }

    #[test]
    fn every_language_s_report_is_its_own_and_fits_the_message_box_with_the_heading() {
        let log = Path::new("C:\\Users")
            .join("u".repeat(200))
            .join(".octoboard")
            .join("logs")
            .join(LOG_NAME);
        let key_left = KeyRemoval {
            line: LineRemoval::FileIncomplete {
                host: "build.example".into(),
                user: "dev".into(),
                copy: "~/.ssh/authorized_keys.octoboard-backup".into(),
                detail: "the disk is full".into(),
            },
            private_key_left: Some(PrivateKeyLeft {
                service: "dev.octoboard.desktop".into(),
                account: "0123456789abcdef".into(),
                host: "build.example".into(),
                user: "dev".into(),
                detail: "refused".into(),
            }),
        };
        let english = Catalog::of(None);
        for tag in crate::localizations::tags() {
            let texts = Catalog::of(Some(tag));
            let mut lines = vec![
                key_left_line(&key("Build server"), &key_left, texts),
                in_effect_line(&key("Build server"), texts),
                in_effect_line(&LOGIN, texts),
                failed_line(
                    &Failed {
                        kind: "device_key",
                        detail: "why".into(),
                    },
                    texts,
                ),
            ];
            lines.extend(out_of_time(CLEAR_BOUND, None, texts).lines);
            for line in &lines {
                assert!(!line.contains('{'), "{tag}: {line}");
            }
            if tag != "en" {
                assert_ne!(
                    in_effect_line(&LOGIN, texts),
                    in_effect_line(&LOGIN, english),
                    "{tag} has its own words"
                );
            }
            // Its longest: lines that do not all fit, the count of the rest, and where the log is.
            let long: Vec<String> = (0..40).map(|_| lines[0].clone()).collect();
            let text = shown(&long, Some(&log), SHOWN_LIMIT, texts);
            assert!(
                text.encode_utf16().count() + HEADING_UNITS <= MESSAGE_UNITS,
                "{tag}"
            );
            let path = log.display().to_string();
            assert!(
                text.ends_with(
                    &(texts.text("uninstallCleanUp.saved", &[("path", &path)]) + "\r\n")
                ),
                "{tag}"
            );
        }
    }

    #[test]
    fn the_out_of_time_line_names_the_bound_its_translations_are_worded_for() {
        // The catalogs write the word for seconds in the form 120 takes in each language.
        assert_eq!(CLEAR_BOUND.as_secs(), 120);
        assert!(
            out_of_time(CLEAR_BOUND, None, Catalog::of(Some("ru"))).lines[0].contains("120 секунд")
        );
    }

    #[test]
    fn what_is_shown_with_the_uninstaller_s_heading_fits_its_message_box() {
        let lines: Vec<String> = (0..40)
            .map(|n| format!("{n} {}", "服".repeat(60)))
            .collect();
        let log = Path::new("C:\\Users")
            .join("u".repeat(200))
            .join(".octoboard")
            .join("logs")
            .join(LOG_NAME);
        let text = shown(&lines, Some(&log), SHOWN_LIMIT, en());
        assert!(text.encode_utf16().count() + HEADING_UNITS <= MESSAGE_UNITS);
        assert!(text.ends_with(&format!("This list is saved in {}.\r\n", log.display())));
        assert!(text.contains(" more.\r\n"), "{text}");
    }

    #[test]
    fn what_is_left_is_written_for_the_uninstaller_and_to_the_log_and_nothing_once_cleared() {
        let scratch = Scratch::new("clean-up", "written");
        let dir = &scratch.0;
        std::fs::create_dir_all(dir).unwrap();
        let shown_file = dir.join("left.txt");
        let log = dir.join("logs").join(LOG_NAME);
        let left = Report {
            status: Status::Left,
            lines: vec!["The device key for \"服务器\".".into(), "second".into()],
        };
        write(&left, &shown_file, Some(&log), en());
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "The device key for \"服务器\".\nsecond\n"
        );
        let units: Vec<u16> = std::fs::read(&shown_file)
            .unwrap()
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(
            String::from_utf16(&units).unwrap(),
            format!(
                "The device key for \"服务器\".\r\nsecond\r\nThis list is saved in {}.\r\n",
                log.display()
            )
        );
        std::fs::remove_file(&shown_file).unwrap();
        // The next clean-up forgets the earlier one's log before it clears, and writes none where
        // it leaves nothing.
        forget_earlier(Some(&log));
        assert!(!log.exists(), "the log of the earlier clean-up is removed");
        forget_earlier(Some(&log));
        let cleared = Report {
            status: Status::Cleared,
            lines: vec![],
        };
        write(&cleared, &shown_file, Some(&log), en());
        assert!(!log.exists() && !shown_file.exists());
    }

    #[test]
    fn the_statuses_are_the_exit_codes_the_installer_hooks_read() {
        assert_eq!(
            [Status::Cleared, Status::Left, Status::CouldNotRun].map(Status::code),
            [0, 1, 2]
        );
    }
}
