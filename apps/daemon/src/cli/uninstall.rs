//! `uninstall`: removes the server form's installation, on the machine it is installed on, with
//! nothing fetched. In order, and stopping before anything is removed where a step cannot be
//! done:
//!
//! 1. it looks at everything that could stop it half way: the systemd user unit, with its
//!    drop-ins, runs this executable; the files, the unit and its drop-ins are the running user's
//!    and their directories can be written; a daemon that runs is this installation's, and its
//!    stop would not be refused;
//! 2. it clears what Octoboard's features have in effect outside Octoboard's own directories
//!    ([`super::cleanup`]), where something is. The clearing is done by a daemon started for a
//!    clean-up run, which does nothing else (`crate::state::StartWork`) and so resumes no session
//!    whose relaunch would hold what is being cleared: by this command itself and not through the
//!    unit, which would start the ordinary daemon. An ordinary daemon that runs is stopped first,
//!    as `stop` stops it. Where something stays in effect, nothing is removed, and a daemon this
//!    stopped is started again as it ran (through its unit, or in the background), so that the
//!    machine is as it was. `--keep-changes` skips the step;
//! 3. it stops the daemon as `stop` does, refused the same way where work would be interrupted:
//!    the clean-up run, or with nothing cleared, the ordinary daemon;
//! 4. it removes what the install set up: the unit, where it runs this executable; lingering,
//!    where the install noted turning it on and no other enabled service of the user's needs it;
//!    the executable, `octoboardd.previous` and the hidden files an update left beside them, the
//!    executable last.
//!
//! The data directory stays, and is named with the command that removes it. The desktop
//! application's own daemon is not uninstalled this way: that installation is the application's.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::platform::{fs, proc};

use super::background::HandBack;
use super::stop_guard::Refusal;
use super::{application, background, cleanup};

/// What `uninstall` was asked.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Options {
    /// Uninstall without asking.
    pub(super) yes: bool,
    /// Stop the daemon whatever work that interrupts.
    pub(super) force: bool,
    /// Do not clear what the features have in effect first.
    pub(super) keep_changes: bool,
    /// Only say what would be done.
    pub(super) dry_run: bool,
}

/// The exit status when nothing, or not everything, was removed.
const NOT_UNINSTALLED: i32 = 1;

/// The word the user types to confirm, as the application's dialogs have it typed.
const WORD: &str = "REMOVE";

/// The unit the install script sets up.
const UNIT: &str = "octoboardd.service";

/// What follows the unit's name in the name of the directory of its drop-ins.
const DROP_INS: &str = ".d";

/// The file of the data directory in which the install script notes that it turned lingering on.
const LINGER_RECORD: &str = "install.lingering";

/// How long `systemctl` or `loginctl` is given to answer.
const TOOL_WAIT: Duration = Duration::from_secs(30);

/// What stopping the daemon as `stop` does came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stop {
    /// It is stopped, or none ran.
    Stopped,
    /// `stop` refused to interrupt work, which it named: nothing was asked of the daemon, and it
    /// runs as before.
    Refused,
    /// `stop` refused since the daemon could not be asked what stopping it would interrupt, as it
    /// said: nothing is known of what the daemon runs, and it runs as before.
    NotAsked,
    /// The stop was asked for and did not come to its end, or could not be asked, as `stop` said.
    Failed,
}

/// A tool of the system's that the uninstall asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tool {
    Systemctl,
    Loginctl,
}

impl Tool {
    fn program(self) -> &'static str {
        match self {
            Tool::Systemctl => "systemctl",
            Tool::Loginctl => "loginctl",
        }
    }
}

/// Where the installation is: the paths the uninstall reads and removes.
#[derive(Debug, Clone)]
pub(super) struct Installation {
    /// This executable, as it was launched: the file the unit runs and the last one removed.
    pub(super) binary: PathBuf,
    /// The user's systemd unit directory.
    pub(super) unit_dir: PathBuf,
    /// The daemon's data directory, which stays.
    pub(super) data_dir: PathBuf,
    /// The user's home, which a unit names as `%h`.
    pub(super) home: PathBuf,
    /// The user's runtime directory, where the units a generator makes are enabled.
    pub(super) runtime_dir: Option<PathBuf>,
}

impl Installation {
    /// The installation of this process: its launch path, and the user's own directories.
    fn here() -> Result<Self> {
        let home = crate::paths::home_dir();
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|dir| !dir.is_empty())
            .map_or_else(|| home.join(".config"), PathBuf::from);
        Ok(Self {
            binary: super::launch_path()?,
            unit_dir: config.join("systemd").join("user"),
            data_dir: crate::paths::data_dir(),
            runtime_dir: fs::user_runtime_dir(),
            home,
        })
    }

    fn bin_dir(&self) -> &Path {
        self.binary.parent().unwrap_or(Path::new("/"))
    }

    fn unit(&self) -> PathBuf {
        self.unit_dir.join(UNIT)
    }

    fn drop_in_dir(&self) -> PathBuf {
        self.unit_dir.join(format!("{UNIT}{DROP_INS}"))
    }

    fn linger_record(&self) -> PathBuf {
        self.data_dir.join(LINGER_RECORD)
    }

    /// The installation's files that are there, the executable last: what an update left beside
    /// it (`octoboardd.previous`, the hidden `.octoboardd.*` files), then the executable itself.
    fn files(&self) -> Vec<PathBuf> {
        let mut beside: Vec<PathBuf> = std::fs::read_dir(self.bin_dir())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| *path != self.binary)
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| self.is_file_name(&name.to_string_lossy()))
            })
            .filter(|path| is_file_or_link(path))
            .collect();
        beside.sort();
        if is_file_or_link(&self.binary) {
            beside.push(self.binary.clone());
        }
        beside
    }

    /// Whether `image`, which a daemon runs, is this installation's: one of its files by device
    /// and inode, the one an update replaced under a running daemon included, which it keeps as
    /// `octoboardd.previous` (Linux then already names the image `<executable> (deleted)`, the
    /// executable's path holding another file); or, where none of them is that file any more (a
    /// second update, or `octoboardd.previous` removed), a file that is gone and was one of them
    /// by its name. Each file
    /// is compared as it is, a link not followed: a link among them leads to a file that is not
    /// the installation's own (an executable run through a link is refused before this). An image
    /// the file system tells from no other is none of them.
    fn runs(&self, image: &proc::Image, files: &[PathBuf]) -> bool {
        if !image.identity.is_known() {
            return false;
        }
        let one_of_them = files.iter().any(|file| {
            fs::identity(file, fs::Links::Keep).is_ok_and(|file| file == image.identity)
        });
        one_of_them
            || image.removed_from().is_some_and(|was| {
                let here = crate::hostfs::canonical(self.bin_dir()).ok();
                was.parent().is_some_and(|dir| Some(dir) == here.as_deref())
                    && was
                        .file_name()
                        .is_some_and(|name| self.is_file_name(&name.to_string_lossy()))
            })
    }

    /// Whether a file named `name` in the executable's directory is the installation's: the
    /// executable, `octoboardd.previous`, or a hidden file an update left.
    fn is_file_name(&self, name: &str) -> bool {
        let own = self
            .binary
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        !own.is_empty()
            && (name == own
                || name == format!("{own}.previous")
                || name.starts_with(&format!(".{own}.")))
    }
}

/// Whether a file or a link is at `path`, whatever a link leads to.
fn is_file_or_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_dir())
}

/// What the uninstall reaches beyond the installation's own files, handed in so that a test
/// reaches no daemon and none of the system's tools.
pub(super) trait Host {
    /// What the daemon's features have in effect outside Octoboard's own directories. With
    /// `unchanged`, read so that nothing of the data directory changes, as `--dry-run` needs it.
    fn in_effect(&self, unchanged: bool) -> Result<Vec<Value>>;
    /// Has the running daemon clear all of it, and what became of each item.
    fn clear(&self) -> Result<Value>;
    /// Whether a daemon runs against the data directory; an error where that cannot be told.
    fn daemon_running(&self) -> Result<bool>;
    /// The executable the daemon that runs is running; an error where that cannot be told.
    fn daemon_image(&self) -> Result<proc::Image>;
    /// Whether `stop` would refuse to stop the daemon that runs, saying what `stop` says: asked
    /// before anything is cleared. A daemon that cannot be asked is one whose stop is refused.
    fn stop_refusal(&self) -> Option<Refusal>;
    /// Starts the daemon in the background for a clean-up run, showing nothing of its startup
    /// line; whether it runs.
    fn start_daemon(&self) -> bool;
    /// The port the daemon that runs listens on, as its record says.
    fn daemon_port(&self) -> Option<u16>;
    /// Starts the ordinary daemon in the background again, at `port` where one is given, showing
    /// nothing of its startup line; its pid where it started.
    fn start_daemon_again(&self, port: Option<u16>) -> Option<u32>;
    /// Stops the daemon as `stop` does, saying what `stop` says; what it came to.
    fn stop_daemon(&self, force: bool) -> Stop;
    /// Ends the clean-up run this command started with a planned stop, which leaves what it runs
    /// to the holders; what that came to.
    fn hand_back_daemon(&self) -> HandBack;
    /// Runs `tool` with `args`: whether it succeeded, or `None` where it could not be run.
    fn tool(&self, tool: Tool, args: &[&str]) -> Option<bool>;
    /// The login name of the user, for the system's lingering.
    fn user_name(&self) -> Option<String>;
    /// Whether what is at `path` belongs to the user running this.
    fn owned_by_user(&self, path: &Path) -> bool;
}

/// This machine; with `unchanged`, looked at so that nothing of the data directory changes, as
/// `--dry-run` needs it.
struct Machine {
    unchanged: bool,
}

impl Host for Machine {
    fn in_effect(&self, unchanged: bool) -> Result<Vec<Value>> {
        cleanup::in_effect(unchanged)
    }

    fn clear(&self) -> Result<Value> {
        cleanup::clear()
    }

    fn daemon_running(&self) -> Result<bool> {
        daemon_runs(self.unchanged)
    }

    fn daemon_image(&self) -> Result<proc::Image> {
        let owner = background::recorded_daemon()?.context("the daemon has ended meanwhile")?;
        let pid = owner.identity.pid;
        proc::image_of(pid).with_context(|| format!("reading which executable pid {} runs", pid.0))
    }

    fn stop_refusal(&self) -> Option<Refusal> {
        match background::recorded_daemon() {
            Ok(Some(owner)) => super::stop_guard::refused(&owner, false),
            Ok(None) => None,
            Err(err) => {
                eprintln!("{err:#}");
                Some(Refusal::NotAsked)
            }
        }
    }

    fn start_daemon(&self) -> bool {
        background::start_for_clean_up().unwrap_or_else(|err| {
            eprintln!("{err:#}");
            false
        })
    }

    fn daemon_port(&self) -> Option<u16> {
        background::recorded_daemon()
            .ok()
            .flatten()
            .map(|owner| owner.port)
    }

    fn start_daemon_again(&self, port: Option<u16>) -> Option<u32> {
        background::start_again(port).unwrap_or_else(|err| {
            eprintln!("{err:#}");
            None
        })
    }

    fn stop_daemon(&self, force: bool) -> Stop {
        match background::stop_telling(force) {
            Ok((0, _)) => Stop::Stopped,
            Ok((_, Some(Refusal::Work))) => Stop::Refused,
            Ok((_, Some(Refusal::NotAsked))) => Stop::NotAsked,
            Ok((_, None)) => Stop::Failed,
            Err(err) => {
                eprintln!("{err:#}");
                Stop::Failed
            }
        }
    }

    fn hand_back_daemon(&self) -> HandBack {
        background::hand_back().unwrap_or_else(|err| {
            eprintln!("{err:#}");
            HandBack::NotAsked
        })
    }

    fn tool(&self, tool: Tool, args: &[&str]) -> Option<bool> {
        let mut command = std::process::Command::new(tool.program());
        command.args(args);
        crate::processes::subprocess::run_with_timeout(&mut command, TOOL_WAIT)
            .ok()
            .map(|output| output.status.success())
    }

    fn user_name(&self) -> Option<String> {
        fs::account_name().map(|name| name.to_string_lossy().into_owned())
    }

    fn owned_by_user(&self, path: &Path) -> bool {
        fs::owned_by_user(path)
    }
}

/// The terminal the command talks to.
pub(super) struct Io<'a> {
    pub(super) input: &'a mut dyn BufRead,
    pub(super) out: &'a mut dyn Write,
    pub(super) err: &'a mut dyn Write,
    /// Whether the input is a terminal, where the command may ask.
    pub(super) terminal: bool,
}

/// What `uninstall` was asked of: which executable this is, and whose daemon runs.
#[derive(Debug, Clone, Copy)]
struct AskedOf {
    /// Whether this build is one the install script installs, on a host it installs on.
    script_installed: bool,
    /// Whether this executable is the desktop application's bundled copy.
    application_copy: bool,
    /// Whether the daemon that runs against the data directory is the desktop application's.
    applications_daemon_runs: bool,
}

impl AskedOf {
    /// Why this is not an installation the command uninstalls, with the data directory `data`
    /// named where a daemon is; `None` where it is one.
    fn refusal(self, data: &Path) -> Option<String> {
        let name = crate::APP_NAME;
        if !self.script_installed {
            return Some(format!(
                "this build of {name} was not installed by the install script, so there is \
                 nothing of that to uninstall here; remove it the way it was installed"
            ));
        }
        if self.application_copy {
            return Some(format!(
                "this {name} is the desktop application's own, which is uninstalled with the \
                 application (clean up in its Settings first); nothing was removed"
            ));
        }
        // This executable is the server form's, and the daemon is not its own.
        self.applications_daemon_runs.then(|| {
            format!(
                "the daemon running against {} is the desktop application's, which this command \
                 neither stops nor uninstalls: it is uninstalled with the application (clean up \
                 in its Settings first). To uninstall this octoboardd, quit the application and \
                 run `octoboardd uninstall` again; nothing was removed",
                data.display()
            )
        })
    }
}

/// `uninstall` of this installation, and its exit status: 0 once it is uninstalled (or nothing
/// was installed, or with `--dry-run`), 1 when nothing or not everything was removed.
pub(super) fn run(options: Options) -> Result<i32> {
    let asked_of = AskedOf {
        script_installed: cfg!(feature = "self-update") && proc::self_update_host(),
        application_copy: application::is_application_copy(),
        applications_daemon_runs: daemon_runs(options.dry_run)?
            && crate::owner_record::Owner::recorded()
                .is_some_and(|owner| owner.follows_application == Some(true)),
    };
    if let Some(why) = asked_of.refusal(&crate::paths::data_dir()) {
        eprintln!("{why}");
        return Ok(NOT_UNINSTALLED);
    }
    let install = Installation::here()?;
    let stdin = std::io::stdin();
    let terminal = stdin.is_terminal();
    let mut io = Io {
        input: &mut stdin.lock(),
        out: &mut std::io::stdout(),
        err: &mut std::io::stderr(),
        terminal,
    };
    let machine = Machine {
        unchanged: options.dry_run,
    };
    Ok(run_on(options, &install, &machine, &mut io))
}

/// Whether a daemon runs against the data directory; with `unchanged`, without creating its lock
/// file where it is not there.
fn daemon_runs(unchanged: bool) -> Result<bool> {
    match unchanged {
        true => crate::instance_lock::is_running_unchanged(),
        false => crate::instance_lock::is_running(),
    }
}

/// Writes `line` and its line end; a terminal that is gone is not an error of the command.
fn say(to: &mut dyn Write, line: &str) {
    let _ = writeln!(to, "{line}");
}

/// Says why nothing (more) was removed, and answers the status the command ends with.
fn refuse(io: &mut Io<'_>, why: &str) -> i32 {
    say(io.err, why);
    NOT_UNINSTALLED
}

/// Removes the file or link at `path`; one that is gone already, as another `uninstall` running
/// at once leaves it, is removed.
fn remove(path: &Path) -> std::io::Result<()> {
    match fs::remove_file_or_link(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        removed => removed,
    }
}

/// Removes the unit's drop-in directory `dir` with the files `files` in it; one that is not
/// there is removed.
fn remove_drop_ins(dir: &Path, files: &[PathBuf]) -> std::io::Result<()> {
    for file in files {
        remove(file)?;
    }
    match std::fs::remove_dir(dir) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        removed => removed,
    }
}

/// The files of the unit's drop-in directory `dir`, in the order systemd reads them, and none
/// where there is no such directory. An error, worded for the user, where the directory is not
/// the command's to read and remove whole: it is a link, holds a directory, or cannot be read.
fn drop_ins(dir: &Path) -> Result<Vec<PathBuf>, String> {
    match std::fs::symlink_metadata(dir) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("{} cannot be read: {err}", dir.display())),
        Ok(metadata) if !metadata.is_dir() => {
            return Err(format!(
                "{} is not a directory of the unit's drop-ins, which this command would remove \
                 with the unit",
                dir.display()
            ))
        }
        Ok(_) => {}
    }
    let unreadable = |err: std::io::Error| format!("{} cannot be read: {err}", dir.display());
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(unreadable)? {
        let path = entry.map_err(unreadable)?.path();
        if !is_file_or_link(&path) {
            return Err(format!(
                "{} holds a directory ({}), which is not this command's to remove",
                dir.display(),
                path.display()
            ));
        }
        files.push(path);
    }
    files.sort();
    Ok(files)
}

/// The executable the unit at `unit` runs once the drop-ins `drop_ins` are applied to it, as
/// systemd applies them: every `ExecStart` of the unit, then of each `.conf` drop-in in order,
/// an empty one dropping those before it. The first word of the one that is left, with `%h` as
/// `home`; `None` where a file cannot be read, or none or more than one is left.
fn unit_program(unit: &Path, drop_ins: &[PathBuf], home: &Path) -> Option<PathBuf> {
    let mut commands: Vec<String> = Vec::new();
    let applied = drop_ins.iter().filter(|file| {
        file.extension()
            .is_some_and(|extension| extension == "conf")
    });
    for file in std::iter::once(unit).chain(applied.map(PathBuf::as_path)) {
        let text = std::fs::read_to_string(file).ok()?;
        for command in text
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix("ExecStart="))
        {
            match command.trim() {
                "" => commands.clear(),
                command => commands.push(command.to_owned()),
            }
        }
    }
    let [command] = commands.as_slice() else {
        return None;
    };
    let program = command.split_whitespace().next()?;
    Some(match program.strip_prefix("%h/") {
        Some(below_home) => home.join(below_home),
        None => PathBuf::from(program),
    })
}

/// The other units of the user's that are enabled: the links among the units a target wants,
/// requires or upholds, in the user's unit directory and, for a unit a generator makes, in the
/// generators' directories of the runtime directory. Lingering may be what starts them at boot.
fn other_enabled_units(install: &Installation) -> Vec<String> {
    let generators = install
        .runtime_dir
        .as_ref()
        .map(|runtime| runtime.join("systemd"));
    let dirs = std::iter::once(install.unit_dir.clone()).chain(
        ["generator", "generator.early", "generator.late"]
            .into_iter()
            .filter_map(|name| generators.as_ref().map(|dir| dir.join(name))),
    );
    let mut units = Vec::new();
    for dir in dirs {
        for wanted in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = wanted.file_name();
            let name = name.to_string_lossy();
            if ![".wants", ".requires", ".upholds"]
                .iter()
                .any(|kind| name.ends_with(kind))
            {
                continue;
            }
            for link in std::fs::read_dir(wanted.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                let unit = link.file_name().to_string_lossy().into_owned();
                if unit != UNIT && !units.contains(&unit) {
                    units.push(unit);
                }
            }
        }
    }
    units.sort();
    units
}

/// How to go on where the clean-up did not clear everything.
fn how_to_go_on() -> String {
    format!(
        "Finish or end the work that holds what is left (an account stops sharing only while \
         none of its sessions is running) and run `octoboardd uninstall` again. Or run \
         `octoboardd uninstall --keep-changes` to uninstall and leave it in place: {}, which \
         nothing clears afterwards but deleting them by hand.",
        cleanup::LEFT
    )
}

/// What became of the daemon the clean-up started, once it was to be stopped again, each said by
/// what is known of it: stopped; or, its stop refused (for work it would interrupt, or since it
/// could not be asked), ended with a planned stop, since a clean-up run never outlives the
/// command, which leaves what it still runs to the holders, with how to go on; or, its stop
/// failed otherwise, as `stop` said. With `again`, the ordinary daemon this stopped is to be
/// started again ([`start_again`]), so how to go on does not say to start it. Whether it was
/// stopped as asked.
fn end_clean_up_run(host: &dyn Host, io: &mut Io<'_>, force: bool, again: bool) -> bool {
    let refused = match host.stop_daemon(force) {
        Stop::Stopped => return true,
        Stop::Failed => {
            say(
                io.err,
                "The daemon this had started for the clean-up did not stop as asked (see above). \
                 Run `octoboardd status` to see whether a daemon still runs before running \
                 `octoboardd uninstall` again.",
            );
            return false;
        }
        refused => refused,
    };
    // What it runs, as far as the refusal told.
    let (left, how) = match (refused, again) {
        (Stop::Refused, false) => (
            "the sessions that were working are left running in their holders",
            "Start the daemon (`octoboardd start`), finish or end that work, and run `octoboardd \
             uninstall` again; `octoboardd uninstall --force` ends it instead.",
        ),
        (Stop::Refused, true) => (
            "the sessions that were working are left running in their holders",
            "Finish or end that work and run `octoboardd uninstall` again; `octoboardd \
             uninstall --force` ends it instead.",
        ),
        (_, false) => (
            "whatever it ran is left to the holders",
            "Start the daemon (`octoboardd start`) to see what runs, and run `octoboardd \
             uninstall` again; `octoboardd uninstall --force` stops it without asking.",
        ),
        (_, true) => (
            "whatever it ran is left to the holders",
            "Run `octoboardd status` to see what runs, and `octoboardd uninstall` again; \
             `octoboardd uninstall --force` stops it without asking.",
        ),
    };
    let ended = match host.hand_back_daemon() {
        HandBack::Ended => {
            format!("The daemon this had started for the clean-up was ended again, and {left}.")
        }
        // It follows this command's process, and has taken the request besides.
        HandBack::Ending => format!(
            "The daemon this had started for the clean-up is still ending; it ends by itself \
             once this command has, and {left}."
        ),
        HandBack::NotAsked => "The daemon that runs could not be asked to end (see above), and \
                               runs as before; `octoboardd status` says which daemon that is."
            .to_owned(),
    };
    say(io.err, &format!("{ended} {how}"));
    false
}

/// Why the command stops where it cannot tell whether a daemon runs, `nothing` being what it has
/// not done: one that may be running is neither started beside nor removed under.
fn not_told(err: &anyhow::Error, nothing: &str) -> String {
    format!("whether a daemon is running could not be told: {err:#}; nothing was {nothing}.")
}

/// How the ordinary daemon ran before this command stopped it for the clean-up, to start it again
/// the same way where the uninstall does not go on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ran {
    /// Through the unit, which was active.
    Unit,
    /// In the background, as `start` starts it, listening on `port` where it is known.
    Background { port: Option<u16> },
}

impl Ran {
    /// How to start it by hand.
    fn command(self) -> &'static str {
        match self {
            Ran::Unit => "systemctl --user start octoboardd",
            Ran::Background { .. } => "octoboardd start",
        }
    }
}

/// Starts again the ordinary daemon this stopped for the clean-up, the way it `ran`, where the
/// uninstall does not go on, so that the machine is as before the command; says what came of it.
/// Not while a daemon still runs (the clean-up run, ending), or where that cannot be told: the
/// daemon is then said to be stopped, with how to start it.
fn start_again(host: &dyn Host, io: &mut Io<'_>, ran: Ran) {
    let stopped = |why: &str, when: &str| {
        format!(
            "The daemon this had stopped for the clean-up {why}, so it is stopped. {when} with \
             `{}`.",
            ran.command()
        )
    };
    match host.daemon_running() {
        Ok(false) => {}
        Ok(true) => {
            return say(
                io.err,
                &stopped(
                    "was not started again while the daemon started for the clean-up has not \
                     ended",
                    "Once `octoboardd status` says none runs, start it",
                ),
            )
        }
        Err(err) => {
            return say(
                io.err,
                &stopped(
                    &format!(
                        "was not started again, since whether a daemon still runs could not be \
                         told ({err:#})"
                    ),
                    "Once `octoboardd status` says none runs, start it",
                ),
            )
        }
    }
    let started = match ran {
        Ran::Unit => (host.tool(Tool::Systemctl, &["--user", "start", UNIT]) == Some(true))
            .then(|| format!("through its service, {UNIT}")),
        Ran::Background { port } => host
            .start_daemon_again(port)
            .map(|pid| format!("in the background (pid {pid})")),
    };
    match started {
        Some(how) => say(
            io.out,
            &format!("Started the daemon again {how}, as it ran before this command."),
        ),
        None => say(
            io.err,
            &stopped("could not be started again (see above)", "Start it"),
        ),
    }
}

/// The clean-up: clears what is in effect through a daemon started for a clean-up run, with none
/// running (an ordinary one that ran was stopped first, and `ran` says how it ran). `Err` with the
/// exit status where something stays in effect or the clean-up could not run; the clean-up run is
/// then ended again, and the daemon that was stopped for it started again.
fn clean_up(host: &dyn Host, io: &mut Io<'_>, ran: Option<Ran>) -> Result<(), i32> {
    say(
        io.out,
        "Clearing what Octoboard's features have in effect outside its own directories:",
    );
    // Where the uninstall does not go on, the daemon stopped for it is started again.
    let give_back = |io: &mut Io<'_>, why: &str| {
        let status = refuse(io, why);
        if let Some(ran) = ran {
            start_again(host, io, ran);
        }
        status
    };
    match host.daemon_running() {
        Ok(false) => {}
        // Something started one after the stop: the clearing would go through it.
        Ok(true) => {
            let status = refuse(
                io,
                "a daemon was started meanwhile, by something other than this command, so \
                 nothing was cleared or removed; run `octoboardd uninstall` again.",
            );
            if let Some(ran) = ran {
                say(
                    io.err,
                    &format!(
                        "The daemon this had stopped for the clean-up was not started again, \
                         since another daemon now runs; once `octoboardd status` says none runs, \
                         start it with `{}`.",
                        ran.command()
                    ),
                );
            }
            return Err(status);
        }
        Err(err) => return Err(give_back(io, &not_told(&err, "cleared or removed"))),
    }
    if !host.start_daemon() {
        return Err(give_back(
            io,
            &format!(
                "the daemon, which does the clearing, could not be started for it (see above; \
                 `octoboardd start` says why a daemon does not start); nothing was cleared or \
                 removed. Run `octoboardd uninstall` again once it starts, or run `octoboardd \
                 uninstall --keep-changes` to uninstall and leave in place {}.",
                cleanup::LEFT
            ),
        ));
    }
    say(
        io.out,
        "Started the daemon for a clean-up run, which does the clearing and runs no sessions.",
    );
    let failure = match host.clear() {
        Ok(cleared) => {
            say(io.out, &cleanup::result_text(&cleared));
            if cleanup::left_in_effect(&cleared) == 0 {
                say(io.out, cleanup::KEPT);
                return Ok(());
            }
            "the clean-up did not clear everything (see above), so nothing was removed.".to_owned()
        }
        Err(err) => format!(
            "the clean-up did not finish: {err:#}; some of it may have been cleared, and \
             nothing was removed."
        ),
    };
    if end_clean_up_run(host, io, false, ran.is_some()) {
        say(io.out, "Stopped the daemon again, which this had started.");
    }
    Err(give_back(io, &format!("{failure} {}", how_to_go_on())))
}

/// [`run`] on the installation `install`, with what it reaches (`host`) and the terminal (`io`)
/// given.
pub(super) fn run_on(
    options: Options,
    install: &Installation,
    host: &dyn Host,
    io: &mut Io<'_>,
) -> i32 {
    let unit = install.unit();
    let has_unit = is_file_or_link(&unit);
    let files = install.files();
    let record = install.linger_record();
    let has_record = record.is_file();
    let bin_dir = install.bin_dir().display().to_string();
    let data_left = |out: &mut dyn Write| {
        if install.data_dir.is_dir() {
            say(
                out,
                &format!(
                    "{dir}, which holds the daemon's own data (your consoles, session records, \
                     settings and logs), is left as it is. To remove it as well: rm -rf {dir}",
                    dir = install.data_dir.display()
                ),
            );
        }
    };
    // Run through a link, this is not the installation: that is where the link leads, and removing
    // the link alone would leave it in place.
    if std::fs::symlink_metadata(&install.binary).is_ok_and(|meta| meta.file_type().is_symlink()) {
        let leads_to = crate::hostfs::canonical(&install.binary).map_or_else(
            |_| "a file that cannot be found".to_owned(),
            |target| target.display().to_string(),
        );
        return refuse(
            io,
            &format!(
                "{link} is a link to {leads_to}, not an installation of its own; nothing was \
                 cleared, stopped or removed. To uninstall the installation it leads to, run \
                 `{leads_to} uninstall`; the link is yours to remove (rm {link}).",
                link = install.binary.display()
            ),
        );
    }
    if !has_unit && files.is_empty() && !has_record {
        say(
            io.out,
            &format!(
                "octoboardd is not installed in {bin_dir} and has no service; nothing was removed."
            ),
        );
        data_left(&mut *io.out);
        return 0;
    }

    // Everything that could stop the removal half way, before anything is cleared or stopped.
    let drop_in_dir = install.drop_in_dir();
    let has_drop_in_dir = has_unit && std::fs::symlink_metadata(&drop_in_dir).is_ok();
    let mut drop_ins = Vec::new();
    if has_unit {
        drop_ins = match self::drop_ins(&drop_in_dir) {
            Ok(drop_ins) => drop_ins,
            Err(why) => return refuse(io, &format!("{why}; nothing was removed.")),
        };
        let program = unit_program(&unit, &drop_ins, &install.home);
        if program.as_deref() != Some(install.binary.as_path()) {
            let runs = program.map_or_else(
                || "nothing this command can read".to_owned(),
                |program| program.display().to_string(),
            );
            let with_drop_ins = match drop_ins.is_empty() {
                true => String::new(),
                false => format!(", with its drop-ins in {},", drop_in_dir.display()),
            };
            return refuse(
                io,
                &format!(
                    "{}{with_drop_ins} runs {runs}, not {}; nothing was removed. If octoboardd \
                     was installed into another directory, run that directory's `octoboardd \
                     uninstall`. If the unit is one you wrote yourself, it is not this command's \
                     to remove: remove it yourself.",
                    unit.display(),
                    install.binary.display()
                ),
            );
        }
    }
    let unit_files = has_unit
        .then_some(&unit)
        .into_iter()
        .chain(has_drop_in_dir.then_some(&drop_in_dir))
        .chain(&drop_ins);
    if let Some(foreign) = files
        .iter()
        .chain(unit_files)
        .find(|file| !host.owned_by_user(file))
    {
        return refuse(
            io,
            &format!(
                "{} does not belong to the user running this; nothing was removed. Run this as \
                 the user who installed octoboardd, without sudo.",
                foreign.display()
            ),
        );
    }
    // The directories the removal takes entries out of.
    let mut dirs = Vec::new();
    if !files.is_empty() {
        dirs.push(install.bin_dir());
    }
    if has_unit {
        dirs.push(install.unit_dir.as_path());
    }
    if !drop_ins.is_empty() {
        dirs.push(drop_in_dir.as_path());
    }
    if let Some(closed) = dirs.into_iter().find(|dir| !fs::directory_writable(dir)) {
        return refuse(
            io,
            &format!(
                "{} cannot be written by the user running this, so the files in it could not be \
                 removed; nothing was removed.",
                closed.display()
            ),
        );
    }
    let running = match host.daemon_running() {
        Ok(running) => running,
        Err(err) => return refuse(io, &not_told(&err, "cleared, stopped or removed")),
    };
    if running {
        // The daemon is asked to clear and is stopped: only this installation's own is.
        match host.daemon_image() {
            Ok(image) if install.runs(&image, &files) => {}
            Ok(image) => {
                return refuse(
                    io,
                    &format!(
                        "the daemon that is running runs {}, which is not this installation's \
                         ({}); nothing was cleared, stopped or removed. Run that installation's \
                         own `octoboardd uninstall`, or stop its daemon first to remove only \
                         this one.",
                        image.path.display(),
                        install.binary.display()
                    ),
                )
            }
            Err(err) => {
                return refuse(
                    io,
                    &format!(
                        "which executable the running daemon runs could not be told: {err:#}; \
                         it may not be this installation's ({}), so nothing was cleared, stopped \
                         or removed.",
                        install.binary.display()
                    ),
                )
            }
        }
        // Asked before the clearing, which is not taken back where the stop is then refused.
        if !options.force && host.stop_refusal().is_some() {
            return refuse(
                io,
                "the daemon would not be stopped (see above), so nothing was cleared or removed. \
                 `octoboardd uninstall --force` stops it all the same and ends its sessions as \
                 interrupted.",
            );
        }
    }

    let changes = match host.in_effect(options.dry_run) {
        Ok(changes) => changes,
        // Nothing is cleared with `--keep-changes`, so what is in effect need not be known.
        Err(_) if options.keep_changes => Vec::new(),
        Err(err) => {
            return refuse(
                io,
                &format!(
                    "what Octoboard's features have in effect outside its own directories could \
                     not be read: {err:#}; nothing was removed. Run `octoboardd uninstall` \
                     again, or with `--keep-changes` to uninstall without clearing it."
                ),
            )
        }
    };

    // Whether the clearing runs, in a clean-up run, with any ordinary daemon stopped first.
    let clearing = !changes.is_empty() && !options.keep_changes;

    // What will be done, said before anything is.
    say(io.out, "Uninstalling octoboardd will:");
    if running {
        say(
            io.out,
            "  stop the daemon, which ends every session it runs as interrupted",
        );
    }
    if !changes.is_empty() {
        let lead = if options.keep_changes {
            "  leave in effect, as --keep-changes was given:"
        } else {
            "  start the daemon for a clean-up run, which runs no sessions, and clear what \
             Octoboard's features have in effect outside its own directories:"
        };
        say(io.out, lead);
        for line in cleanup::list_lines(&changes) {
            say(io.out, &format!("    {line}"));
        }
    }
    if clearing {
        say(io.out, "  then stop that daemon again");
        if running {
            say(
                io.out,
                "  where something stays in effect, remove nothing and start the daemon again as \
                 it ran (through its service, or in the background)",
            );
        }
    }
    if has_unit {
        say(
            io.out,
            &format!("  stop, disable and remove the service {}", unit.display()),
        );
        if has_drop_in_dir {
            say(
                io.out,
                &format!("  remove its drop-ins, {}", drop_in_dir.display()),
            );
        }
    }
    if has_record {
        say(
            io.out,
            "  turn lingering off, which the install turned on, unless another enabled service \
             of yours needs it",
        );
    }
    for file in &files {
        // A link is removed as a link.
        let leads_to = std::fs::read_link(file).map_or_else(
            |_| String::new(),
            |target| format!(", a link (what it leads to, {}, stays)", target.display()),
        );
        say(io.out, &format!("  remove {}{leads_to}", file.display()));
    }
    say(
        io.out,
        &format!(
            "  leave {}, the daemon's own data",
            install.data_dir.display()
        ),
    );
    if options.dry_run {
        say(io.out, "Nothing was changed (--dry-run).");
        return 0;
    }
    if !options.yes {
        if !io.terminal {
            return refuse(
                io,
                "nothing was removed: `octoboardd uninstall` asks before it uninstalls, and \
                 there is no terminal to ask in; run it with `--yes` to uninstall without being \
                 asked, or with `--dry-run` to only see what it would do",
            );
        }
        if !confirm_from(io.input, io.out) {
            say(io.out, "nothing was removed");
            return NOT_UNINSTALLED;
        }
    }

    // The clean-up, before anything is removed, in a clean-up run, the ordinary daemon stopped
    // first: one that runs would relaunch the sessions it ends, whose start can hold what is being
    // cleared.
    let mut stopped_for_it = None;
    if clearing && running {
        let ran = if has_unit
            && host.tool(Tool::Systemctl, &["--user", "is-active", "--quiet", UNIT]) == Some(true)
        {
            Ran::Unit
        } else {
            Ran::Background {
                port: host.daemon_port(),
            }
        };
        if host.stop_daemon(options.force) != Stop::Stopped {
            return refuse(
                io,
                "the daemon was not stopped (see above), so nothing was cleared or removed. Where \
                 it named work that stopping would interrupt, `octoboardd uninstall --force` \
                 stops it all the same and ends its sessions as interrupted.",
            );
        }
        say(
            io.out,
            "Stopped the daemon, to clear in a clean-up run; it is started again where the \
             clean-up leaves something.",
        );
        if let Err(status) = clean_up(host, io, Some(ran)) {
            return status;
        }
        stopped_for_it = Some(ran);
    } else if !changes.is_empty() {
        if options.keep_changes {
            say(
                io.err,
                &format!(
                    "warning: what is in effect was left in place, as --keep-changes was given: \
                     {}. Once octoboardd is removed nothing clears them; they can be deleted by \
                     hand.",
                    cleanup::LEFT
                ),
            );
        } else if let Err(status) = clean_up(host, io, None) {
            return status;
        }
    }

    let ran_up_front = running;
    let running = match host.daemon_running() {
        Ok(running) => running,
        // The daemon stopped for the clean-up is not left stopped unsaid.
        Err(err) => {
            return match stopped_for_it {
                // The clean-up run, which this started, is ended first (a stop with none running
                // comes to its end too), so that the daemon can be started beside nothing.
                Some(ran) => {
                    let status = refuse(io, &not_told(&err, "removed"));
                    end_clean_up_run(host, io, options.force, true);
                    start_again(host, io, ran);
                    status
                }
                None => refuse(io, &not_told(&err, "stopped or removed")),
            };
        }
    };
    if running {
        // A daemon that runs after a clean-up is the clean-up run; with none, one that runs and
        // did not before was started meanwhile, and was not checked.
        if !clearing && !ran_up_front {
            return refuse(
                io,
                "a daemon was started meanwhile, by something other than this command, so nothing \
                 was stopped or removed; run `octoboardd uninstall` again.",
            );
        }
        if clearing {
            if !end_clean_up_run(host, io, options.force, stopped_for_it.is_some()) {
                let status = refuse(
                    io,
                    "the daemon was not stopped (see above), so nothing was removed.",
                );
                if let Some(ran) = stopped_for_it {
                    start_again(host, io, ran);
                }
                return status;
            }
        } else if host.stop_daemon(options.force) != Stop::Stopped {
            return refuse(
                io,
                "the daemon was not stopped (see above), so nothing was removed. Where it named \
                 work that stopping would interrupt, `octoboardd uninstall --force` stops it \
                 all the same and ends its sessions as interrupted.",
            );
        }
        say(io.out, "Stopped the daemon.");
    }

    if has_unit {
        let disabled = host.tool(Tool::Systemctl, &["--user", "disable", "--now", UNIT]);
        let removed = remove(&unit).map_err(|err| (&unit, err)).and_then(|()| {
            remove_drop_ins(&drop_in_dir, &drop_ins).map_err(|err| (&drop_in_dir, err))
        });
        if disabled == Some(true) {
            host.tool(Tool::Systemctl, &["--user", "daemon-reload"]);
            host.tool(Tool::Systemctl, &["--user", "reset-failed", UNIT]);
            say(
                io.out,
                &format!(
                    "Stopped and disabled the service, and removed {}.",
                    unit.display()
                ),
            );
        } else {
            // What enabling made is a link among the units the default target wants, removed
            // here with the unit itself.
            let _ = remove(&install.unit_dir.join("default.target.wants").join(UNIT));
            let why = match disabled {
                // It ran and failed: a manager that is there is told the unit is gone.
                Some(_) => {
                    host.tool(Tool::Systemctl, &["--user", "daemon-reload"]);
                    "`systemctl --user disable --now octoboardd.service` failed (your user's \
                     service manager could not be reached, or refused it)"
                }
                None => {
                    "systemctl could not be run (it is not installed, or did not answer in time)"
                }
            };
            say(
                io.err,
                &format!(
                    "warning: {why}, so {} and the link that enabled it were removed without \
                     it. If the service still runs, it stops when you log out, or with \
                     `systemctl --user stop octoboardd`.",
                    unit.display()
                ),
            );
        }
        if let Err((path, err)) = removed {
            return refuse(io, &format!("could not remove {}: {err}", path.display()));
        }
    }

    if has_record {
        let user = host.user_name().unwrap_or_default();
        let others = other_enabled_units(install);
        if !others.is_empty() {
            say(
                io.out,
                &format!(
                    "The install turned lingering on for {user}. It was left on, since other \
                     services of yours are enabled and may need it: {}. To turn it off all the \
                     same: `loginctl disable-linger {user}` (it may need sudo).",
                    others.join(" ")
                ),
            );
        } else if !user.is_empty()
            && host.tool(
                Tool::Loginctl,
                &["--no-ask-password", "disable-linger", &user],
            ) == Some(true)
        {
            let _ = remove(&record);
            say(
                io.out,
                &format!("Turned lingering off for {user}, which the install had turned on."),
            );
        } else {
            say(
                io.out,
                &format!(
                    "The install turned lingering on for {user}, and it could not be turned off \
                     here. Turn it off with `loginctl disable-linger {user}` (it may need sudo)."
                ),
            );
        }
    }

    // The executable is the last of them: this process runs from it.
    for file in &files {
        if let Err(err) = remove(file) {
            return refuse(io, &format!("could not remove {}: {err}", file.display()));
        }
    }
    if !files.is_empty() {
        say(io.out, &format!("Removed octoboardd from {bin_dir}."));
    }
    data_left(&mut *io.out);
    0
}

/// Asks on `out` for the word, reading the answer from `input`: only the word itself uninstalls,
/// and the end of the input is a no.
fn confirm_from(input: &mut dyn BufRead, out: &mut dyn Write) -> bool {
    if write!(out, "Type {WORD} to uninstall: ")
        .and_then(|()| out.flush())
        .is_err()
    {
        return false;
    }
    let mut answer = String::new();
    if input.read_line(&mut answer).unwrap_or(0) == 0 {
        writeln!(out).ok();
        return false;
    }
    answer.trim() == WORD
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    /// The documentation site quotes the word to type as this fixture holds it.
    #[test]
    fn the_word_to_type_is_the_one_the_docs_quote() {
        let quoted: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/docs_uninstall_command.json"))
                .unwrap();
        assert_eq!(quoted["confirmation_word"].as_str(), Some(super::WORD));
    }

    use super::*;
    use crate::cli::cleanup::tests::{all_cleared, one_left, sharing};
    use crate::test_support::ScratchDir;

    /// A scratch installation: the executable with what an update left beside it, the data
    /// directory, and with `unit`, the unit the install wrote for it.
    struct Fixture {
        dir: ScratchDir,
        install: Installation,
    }

    impl Fixture {
        fn new(label: &str, unit: bool) -> Self {
            let dir = ScratchDir::new(label);
            let bin = dir.join("bin");
            let install = Installation {
                binary: bin.join("octoboardd"),
                unit_dir: dir.join("config").join("systemd").join("user"),
                data_dir: dir.join("data"),
                home: dir.to_path_buf(),
                runtime_dir: Some(dir.join("run")),
            };
            for path in [&bin, &install.unit_dir, &install.data_dir] {
                std::fs::create_dir_all(path).unwrap();
            }
            for name in ["octoboardd", "octoboardd.previous", ".octoboardd.staged"] {
                std::fs::write(bin.join(name), name).unwrap();
            }
            // Not the installation's: it stays.
            std::fs::write(bin.join("other-tool"), "other").unwrap();
            let fixture = Self { dir, install };
            if unit {
                fixture.write_unit(&format!(
                    "{} --service-managed",
                    fixture.install.binary.display()
                ));
            }
            fixture
        }

        fn write_unit(&self, exec_start: &str) {
            std::fs::write(
                self.install.unit(),
                format!(
                    "[Unit]\nDescription=Octoboard daemon\n\n[Service]\nExecStart={exec_start}\n"
                ),
            )
            .unwrap();
        }

        fn note_lingering(&self) {
            std::fs::write(self.install.linger_record(), "tester\n").unwrap();
        }

        fn nothing_removed(&self) -> bool {
            self.install.files().len() == 3
        }

        fn removed(&self) -> bool {
            self.install.files().is_empty()
                && self.install.bin_dir().join("other-tool").is_file()
                && self.install.data_dir.is_dir()
        }
    }

    /// A machine that notes what it was asked, in order.
    struct StandIn {
        changes: Result<Vec<Value>, String>,
        cleared: Result<Value, String>,
        running: RefCell<bool>,
        /// Why whether a daemon runs cannot be told, where it cannot.
        untold: Option<String>,
        /// How many times `daemon_running` was asked.
        running_asked: RefCell<usize>,
        /// What `daemon_running` answers at the asking of that number (from 1), in place of the
        /// stand-in's state, which an answer that a daemon runs or not also sets.
        answers: RefCell<std::collections::HashMap<usize, Result<bool, String>>>,
        /// The executable the running daemon runs; the fixture's own unless set.
        image: RefCell<Option<Result<PathBuf, String>>>,
        /// Whether `in_effect` was asked to change nothing.
        read_unchanged: RefCell<Option<bool>>,
        /// Whether `start_daemon` brings a daemon up.
        starts: bool,
        /// Whether the unit is active, as `systemctl --user is-active` answers.
        unit_active: Option<bool>,
        /// Whether starting the ordinary daemon again, in the background or through the unit,
        /// brings it up.
        starts_again: bool,
        /// What asking the stop check before the clearing comes to.
        check: Option<Refusal>,
        stop: Stop,
        /// What asking the daemon for a planned stop comes to.
        hands_back: HandBack,
        /// What `systemctl --user disable` answers; `None` as with no service manager to ask.
        disable: Option<bool>,
        linger_off: Option<bool>,
        foreign: Option<PathBuf>,
        calls: RefCell<Vec<String>>,
    }

    impl StandIn {
        fn new(changes: Vec<Value>, running: bool) -> Self {
            Self {
                changes: Ok(changes),
                cleared: Ok(all_cleared()),
                running: RefCell::new(running),
                untold: None,
                running_asked: RefCell::new(0),
                answers: RefCell::new(std::collections::HashMap::new()),
                image: RefCell::new(None),
                read_unchanged: RefCell::new(None),
                starts: true,
                unit_active: Some(true),
                starts_again: true,
                check: None,
                stop: Stop::Stopped,
                hands_back: HandBack::Ended,
                disable: Some(true),
                linger_off: Some(true),
                foreign: None,
                calls: RefCell::new(Vec::new()),
            }
        }

        fn note(&self, call: impl Into<String>) {
            self.calls.borrow_mut().push(call.into());
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }

        fn position(&self, call: &str) -> usize {
            self.calls()
                .iter()
                .position(|made| made == call)
                .unwrap_or_else(|| panic!("`{call}` was not made: {:?}", self.calls()))
        }

        fn made(&self, call: &str) -> bool {
            self.calls().iter().any(|made| made.starts_with(call))
        }
    }

    impl Host for StandIn {
        fn in_effect(&self, unchanged: bool) -> Result<Vec<Value>> {
            *self.read_unchanged.borrow_mut() = Some(unchanged);
            self.changes.clone().map_err(|err| anyhow::anyhow!(err))
        }

        fn clear(&self) -> Result<Value> {
            self.note("clear");
            assert!(*self.running.borrow(), "cleared with no daemon running");
            self.cleared.clone().map_err(|err| anyhow::anyhow!(err))
        }

        fn daemon_running(&self) -> Result<bool> {
            *self.running_asked.borrow_mut() += 1;
            match self
                .answers
                .borrow_mut()
                .remove(&*self.running_asked.borrow())
            {
                Some(Ok(running)) => {
                    *self.running.borrow_mut() = running;
                    return Ok(running);
                }
                Some(Err(why)) => return Err(anyhow::anyhow!(why)),
                None => {}
            }
            match &self.untold {
                Some(why) => Err(anyhow::anyhow!(why.clone())),
                None => Ok(*self.running.borrow()),
            }
        }

        fn daemon_image(&self) -> Result<proc::Image> {
            let path = self
                .image
                .borrow()
                .clone()
                .expect("the test names what the daemon runs")
                .map_err(|err| anyhow::anyhow!(err))?;
            // A file that is gone is told by where it was alone.
            let at = path
                .to_str()
                .and_then(|path| path.strip_suffix(" (deleted)"))
                .map_or(path.clone(), |_| std::env::current_exe().unwrap());
            Ok(proc::Image {
                identity: fs::identity(&at, fs::Links::Follow)?,
                path,
            })
        }

        fn stop_refusal(&self) -> Option<Refusal> {
            self.note("check stop");
            self.check
        }

        fn start_daemon(&self) -> bool {
            self.note("start");
            *self.running.borrow_mut() = self.starts;
            self.starts
        }

        fn daemon_port(&self) -> Option<u16> {
            self.running.borrow().then_some(7531)
        }

        fn start_daemon_again(&self, port: Option<u16>) -> Option<u32> {
            self.note(format!("start again {port:?}"));
            *self.running.borrow_mut() = self.starts_again;
            self.starts_again.then_some(4242)
        }

        fn stop_daemon(&self, force: bool) -> Stop {
            self.note(if force { "stop --force" } else { "stop" });
            // A stop that is forced is refused for no work.
            let came_to = match (self.stop, force) {
                (Stop::Refused | Stop::NotAsked, true) => Stop::Stopped,
                (stop, _) => stop,
            };
            if came_to == Stop::Stopped {
                *self.running.borrow_mut() = false;
            }
            came_to
        }

        fn hand_back_daemon(&self) -> HandBack {
            self.note("hand back");
            if self.hands_back == HandBack::Ended {
                *self.running.borrow_mut() = false;
            }
            self.hands_back
        }

        fn tool(&self, tool: Tool, args: &[&str]) -> Option<bool> {
            self.note(format!("{} {}", tool.program(), args.join(" ")));
            match (tool, args) {
                (Tool::Systemctl, ["--user", "disable", ..]) => {
                    if self.disable == Some(true) {
                        *self.running.borrow_mut() = false;
                    }
                    self.disable
                }
                (Tool::Systemctl, ["--user", "is-active", ..]) => self.unit_active,
                (Tool::Systemctl, ["--user", "start", ..]) => {
                    *self.running.borrow_mut() = self.starts_again;
                    Some(self.starts_again)
                }
                (Tool::Loginctl, _) => self.linger_off,
                _ => Some(true),
            }
        }

        fn user_name(&self) -> Option<String> {
            Some("tester".to_owned())
        }

        fn owned_by_user(&self, path: &Path) -> bool {
            self.foreign.as_deref() != Some(path)
        }
    }

    const YES: Options = Options {
        yes: true,
        force: false,
        keep_changes: false,
        dry_run: false,
    };

    /// Runs the command and gives back its status, what it wrote on stdout and on stderr.
    fn uninstall(
        options: Options,
        fixture: &Fixture,
        host: &StandIn,
        typed: &str,
        terminal: bool,
    ) -> (i32, String, String) {
        host.image
            .borrow_mut()
            .get_or_insert_with(|| Ok(fixture.install.binary.clone()));
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut input = typed.as_bytes();
        let status = run_on(
            options,
            &fixture.install,
            host,
            &mut Io {
                input: &mut input,
                out: &mut out,
                err: &mut err,
                terminal,
            },
        );
        (
            status,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    /// The whole order with a daemon running under its unit: the clean-up, the stop, the unit,
    /// lingering, then the files; the data directory and what is not the installation's stay.
    #[test]
    fn it_clears_then_stops_then_removes_the_unit_lingering_and_the_files() {
        let fixture = Fixture::new("uninstall-order", true);
        fixture.note_lingering();
        let host = StandIn::new(vec![sharing("work")], true);
        let (status, out, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 0, "{err}");
        assert!(err.is_empty(), "{err}");
        // The ordinary daemon is stopped first, and the clearing done in a clean-up run, which is
        // stopped in turn.
        assert_eq!(
            host.calls()[..6],
            [
                "check stop",
                "systemctl --user is-active --quiet octoboardd.service",
                "stop",
                "start",
                "clear",
                "stop",
            ]
        );
        let order = [
            "check stop",
            "clear",
            "systemctl --user disable --now octoboardd.service",
            "systemctl --user daemon-reload",
            "loginctl --no-ask-password disable-linger tester",
        ]
        .map(|call| host.position(call));
        assert!(
            order.windows(2).all(|pair| pair[0] < pair[1]),
            "{:?}",
            host.calls()
        );
        // Everything was cleared: the daemon is not started again.
        assert!(!host.made("start again") && !host.made("systemctl --user start"));
        assert!(fixture.removed());
        assert!(!fixture.install.unit().exists());
        assert!(!fixture.install.linger_record().exists());
        for said in [
            "cleared: Claude Code account \"work\" no longer shares the default configuration",
            "kept: its conversation history stays in /home/u/.claude (projects, todos)",
            "Stopped the daemon, to clear in a clean-up run",
            "1 cleared; nothing is left in effect.",
            "Stopped the daemon.",
            "Stopped and disabled the service",
            "Turned lingering off for tester",
            "Removed octoboardd from",
            "is left as it is. To remove it as well: rm -rf",
        ] {
            assert!(out.contains(said), "{said}: {out}");
        }
        // What it will do is said before anything is done.
        assert!(
            out.find("Uninstalling octoboardd will:").unwrap() < out.find("cleared: ").unwrap(),
            "{out}"
        );
    }

    /// With nothing in effect nothing is cleared and no daemon is started for it.
    #[test]
    fn with_nothing_in_effect_no_daemon_is_started_and_nothing_is_cleared() {
        let fixture = Fixture::new("uninstall-nothing", false);
        let host = StandIn::new(Vec::new(), false);
        let (status, out, _) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 0);
        assert!(host.calls().is_empty(), "{:?}", host.calls());
        assert!(!out.contains("Clearing"), "{out}");
        assert!(fixture.removed());
    }

    /// Something in effect and no daemon running: one is started for the clean-up by the command
    /// itself, also where there is a unit, which would start the ordinary daemon, and stopped
    /// afterwards.
    #[test]
    fn the_daemon_is_started_for_the_clean_up_when_none_runs() {
        let fixture = Fixture::new("uninstall-start", false);
        let host = StandIn::new(vec![sharing("work")], false);
        let (status, out, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 0, "{err}");
        assert_eq!(host.calls(), ["start", "clear", "stop"]);
        assert!(
            out.contains("Started the daemon for a clean-up run, which does the clearing"),
            "{out}"
        );
        assert!(fixture.removed());

        let fixture = Fixture::new("uninstall-start-unit", true);
        let host = StandIn::new(vec![sharing("work")], false);
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 0, "{err}");
        assert!(
            host.position("start") < host.position("clear")
                && host.position("clear") < host.position("stop")
                && host.position("stop")
                    < host.position("systemctl --user disable --now octoboardd.service")
        );
        assert!(!host.made("systemctl --user start"), "{:?}", host.calls());
        assert!(fixture.removed());
    }

    /// Something left in effect stops the uninstall before anything is removed, with what is left,
    /// why, and how to go on; the daemon that ran, stopped for the clean-up, is started again the
    /// way it ran: through its unit where that was active, and said so.
    #[test]
    fn something_left_in_effect_removes_nothing_and_starts_the_daemon_again() {
        let fixture = Fixture::new("uninstall-left", true);
        let mut host = StandIn::new(vec![sharing("work"), sharing("side")], true);
        host.cleared = Ok(one_left());
        let (status, out, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert_eq!(
            host.calls(),
            [
                "check stop",
                "systemctl --user is-active --quiet octoboardd.service",
                "stop",
                "start",
                "clear",
                "stop",
                "systemctl --user start octoboardd.service",
            ]
        );
        assert!(*host.running.borrow());
        assert!(fixture.nothing_removed() && fixture.install.unit().exists());
        assert!(
            out.contains(
                "not cleared: Claude Code account \"work\" shares the default configuration"
            ) && out.contains("one of its sessions is running or starting"),
            "{out}"
        );
        assert!(
            out.contains(
                "Started the daemon again through its service, octoboardd.service, as it ran \
                 before this command."
            ),
            "{out}"
        );
        for said in [
            "the clean-up did not clear everything",
            "nothing was removed",
            "run `octoboardd uninstall` again",
            "octoboardd uninstall --keep-changes",
            "the links in each such account's directory",
        ] {
            assert!(err.contains(said), "{said}: {err}");
        }
    }

    /// A daemon that ran in the background is started again in the background, at the port it
    /// listened on; the same where the clean-up could not run at all.
    #[test]
    fn a_daemon_that_ran_in_the_background_is_started_again_there() {
        for unit in [false, true] {
            let fixture = Fixture::new("uninstall-left-background", unit);
            let mut host = StandIn::new(vec![sharing("work")], true);
            host.cleared = Ok(one_left());
            // A unit that is not active did not run the daemon.
            host.unit_active = Some(false);
            let (status, out, _) = uninstall(YES, &fixture, &host, "", false);
            assert_eq!(status, 1);
            assert_eq!(
                host.calls().last().map(String::as_str),
                Some("start again Some(7531)")
            );
            assert!(!host.made("systemctl --user start"));
            assert!(
                out.contains(
                    "Started the daemon again in the background (pid 4242), as it ran before \
                     this command."
                ),
                "{out}"
            );
            assert!(*host.running.borrow() && fixture.nothing_removed());
        }

        for (cleared, starts) in [
            (Err("timed out".to_owned()), true),
            (Ok(all_cleared()), false),
        ] {
            let fixture = Fixture::new("uninstall-no-run-background", false);
            let mut host = StandIn::new(vec![sharing("work")], true);
            host.cleared = cleared;
            host.starts = starts;
            let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
            assert_eq!(status, 1);
            assert!(err.contains("nothing was"), "{err}");
            assert!(host.made("start again Some(7531)"), "{:?}", host.calls());
            assert!(*host.running.borrow() && fixture.nothing_removed());
        }
    }

    /// Where the daemon cannot be started again, or a daemon still runs, it is said plainly to be
    /// stopped, with how to start it.
    #[test]
    fn a_daemon_that_cannot_be_started_again_is_said_to_be_stopped() {
        for (unit, command) in [
            (true, "systemctl --user start octoboardd"),
            (false, "octoboardd start"),
        ] {
            let fixture = Fixture::new("uninstall-not-again", unit);
            let mut host = StandIn::new(vec![sharing("work")], true);
            host.cleared = Ok(one_left());
            host.starts_again = false;
            let (status, out, err) = uninstall(YES, &fixture, &host, "", false);
            assert_eq!(status, 1);
            assert!(!out.contains("Started the daemon again"), "{out}");
            assert!(
                err.contains(&format!(
                    "The daemon this had stopped for the clean-up could not be started again \
                     (see above), so it is stopped. Start it with `{command}`."
                )),
                "{err}"
            );
            assert!(!*host.running.borrow() && fixture.nothing_removed());
        }

        // The clean-up run, its stop refused, is still ending: nothing is started beside it.
        let fixture = Fixture::new("uninstall-not-again-ending", false);
        let mut host = StandIn::new(vec![sharing("work")], true);
        host.cleared = Ok(one_left());
        host.stop = Stop::Refused;
        host.hands_back = HandBack::Ending;
        // The ordinary daemon's own stop is forced, so it is refused for no work.
        let forced = Options { force: true, ..YES };
        let (status, _, err) = uninstall(forced, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(!host.made("start again"), "{:?}", host.calls());
        assert!(
            err.contains(
                "was not started again while the daemon started for the clean-up has not ended"
            ) && err.contains(
                "Once `octoboardd status` says none runs, start it with `octoboardd start`."
            ),
            "{err}"
        );
    }

    /// A daemon the uninstall started for a clean-up that leaves something is stopped again.
    #[test]
    fn a_daemon_started_for_a_clean_up_that_leaves_something_is_stopped_again() {
        let fixture = Fixture::new("uninstall-left-started", false);
        let mut host = StandIn::new(vec![sharing("work")], false);
        host.cleared = Ok(one_left());
        let (status, out, _) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert_eq!(host.calls(), ["start", "clear", "stop"]);
        assert!(
            out.contains("Stopped the daemon again, which this had started."),
            "{out}"
        );
        assert!(fixture.nothing_removed());
    }

    /// A daemon the uninstall started whose ordinary stop is refused (a session it took over is
    /// working) is ended with a planned stop, so that it never outlives the command: after a
    /// clean-up that cleared everything and after one that left something, with how to go on and
    /// nothing removed. A daemon that ran already is never handed back.
    #[test]
    fn a_started_daemon_whose_stop_is_refused_is_handed_back_and_nothing_is_removed() {
        for cleared in [all_cleared(), one_left()] {
            let fixture = Fixture::new("uninstall-hand-back", true);
            let mut host = StandIn::new(vec![sharing("work")], false);
            host.cleared = Ok(cleared);
            host.stop = Stop::Refused;
            let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
            assert_eq!(status, 1);
            assert_eq!(host.calls(), ["start", "clear", "stop", "hand back"]);
            assert!(!*host.running.borrow());
            for said in [
                "The daemon this had started for the clean-up was ended again, and the sessions \
                 that were working are left running in their holders.",
                "Start the daemon (`octoboardd start`)",
                "run `octoboardd uninstall` again",
                "octoboardd uninstall --force",
                "nothing was removed",
            ] {
                assert!(err.contains(said), "{said}: {err}");
            }
            assert!(fixture.nothing_removed() && fixture.install.unit().exists());
        }

        // A stop that was asked for and did not come to its end is no refusal: nothing is handed
        // back, and nothing is said of work left running, which that stop has ended.
        let fixture = Fixture::new("uninstall-stop-failed", false);
        let mut host = StandIn::new(vec![sharing("work")], false);
        host.stop = Stop::Failed;
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert_eq!(host.calls(), ["start", "clear", "stop"]);
        assert!(
            err.contains("did not stop as asked (see above)")
                && !err.contains("left running in")
                && !err.contains("ends by itself"),
            "{err}"
        );
        assert!(!err.contains("--force"), "{err}");
        assert!(fixture.nothing_removed());

        // One that does not end in time ends with the command, which is said.
        let fixture = Fixture::new("uninstall-hand-back-slow", false);
        let mut host = StandIn::new(vec![sharing("work")], false);
        host.stop = Stop::Refused;
        host.hands_back = HandBack::Ending;
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains("is still ending; it ends by itself once this command has"),
            "{err}"
        );
        assert!(fixture.nothing_removed());

        // A daemon that could not be asked what it runs is handed back all the same, and nothing
        // is said of sessions that were working, none having been named.
        let fixture = Fixture::new("uninstall-hand-back-not-asked", false);
        let mut host = StandIn::new(vec![sharing("work")], false);
        host.stop = Stop::NotAsked;
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert_eq!(host.calls(), ["start", "clear", "stop", "hand back"]);
        assert!(
            err.contains("was ended again, and whatever it ran is left to the holders")
                && err.contains("`octoboardd start`) to see what runs")
                && !err.contains("sessions that were working"),
            "{err}"
        );

        // One that was not asked to end, or did not take the request, is not said to end.
        let fixture = Fixture::new("uninstall-hand-back-declined", false);
        let mut host = StandIn::new(vec![sharing("work")], false);
        host.stop = Stop::Refused;
        host.hands_back = HandBack::NotAsked;
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains("could not be asked to end (see above), and runs as before")
                && !err.contains("ends by itself")
                && !err.contains("was ended again"),
            "{err}"
        );
        assert!(*host.running.borrow() && fixture.nothing_removed());

        let fixture = Fixture::new("uninstall-no-hand-back", true);
        let mut host = StandIn::new(vec![sharing("work")], true);
        host.stop = Stop::Refused;
        let (status, _, _) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(!host.made("hand back") && *host.running.borrow());
    }

    /// A clean-up that cannot run (the daemon does not start, the list cannot be read, the
    /// clearing does not come back) removes nothing.
    #[test]
    fn a_clean_up_that_cannot_run_removes_nothing() {
        let fixture = Fixture::new("uninstall-no-start", false);
        let mut host = StandIn::new(vec![sharing("work")], false);
        host.starts = false;
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(!host.made("clear"));
        assert!(
            err.contains("could not be started") && err.contains("--keep-changes"),
            "{err}"
        );
        assert!(fixture.nothing_removed());

        let mut host = StandIn::new(Vec::new(), true);
        host.changes = Err("its key is not recorded".into());
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains("its key is not recorded") && err.contains("nothing was removed"),
            "{err}"
        );
        assert!(!host.made("clear") && !host.made("stop") && fixture.nothing_removed());

        let mut host = StandIn::new(vec![sharing("work")], true);
        host.cleared = Err("timed out".into());
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains("timed out") && err.contains("nothing was removed"),
            "{err}"
        );
        assert_eq!(
            host.calls()[..5],
            ["check stop", "stop", "start", "clear", "stop"]
        );
        assert!(fixture.nothing_removed());
    }

    /// A stop that would be refused, as where work would be interrupted, stops the uninstall
    /// before anything is cleared, naming `--force`, which asks no check and stops regardless.
    /// A daemon that cannot be asked is refused the same way.
    #[test]
    fn a_stop_that_would_be_refused_clears_nothing_and_names_force() {
        for refusal in [Refusal::Work, Refusal::NotAsked] {
            let fixture = Fixture::new("uninstall-stop-refused", true);
            let mut host = StandIn::new(vec![sharing("work")], true);
            host.check = Some(refusal);
            let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
            assert_eq!(status, 1);
            assert_eq!(host.calls(), ["check stop"]);
            assert!(
                err.contains("nothing was cleared or removed")
                    && err.contains("octoboardd uninstall --force"),
                "{err}"
            );
            assert!(fixture.nothing_removed() && fixture.install.unit().exists());

            let forced = Options { force: true, ..YES };
            assert_eq!(uninstall(forced, &fixture, &host, "", false).0, 0);
            assert_eq!(
                host.calls()[1..5],
                [
                    "systemctl --user is-active --quiet octoboardd.service",
                    "stop --force",
                    "start",
                    "clear"
                ]
            );
            assert!(fixture.removed());
        }
    }

    /// Work that began after the check refuses the stop itself: nothing is removed, and
    /// `--force` is named.
    #[test]
    fn a_stop_refused_after_the_check_removes_nothing_and_names_force() {
        let fixture = Fixture::new("uninstall-stop-refused-late", true);
        let mut host = StandIn::new(Vec::new(), true);
        host.stop = Stop::Refused;
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert_eq!(host.calls(), ["check stop", "stop"]);
        assert!(
            err.contains("the daemon was not stopped (see above), so nothing was removed")
                && err.contains("octoboardd uninstall --force"),
            "{err}"
        );
        assert!(fixture.nothing_removed() && fixture.install.unit().exists());
    }

    /// A daemon that runs another executable than the installation's is not this command's to
    /// clear through or stop: nothing is asked of it, and the file it runs is named. One that
    /// runs a file of the installation's, as the previous build after an update, or one that is
    /// gone from the installation's directory, is its own; where it cannot be told, nothing is
    /// done.
    #[test]
    fn a_daemon_that_runs_another_executable_stops_the_uninstall() {
        let fixture = Fixture::new("uninstall-other-daemon", false);
        let other = fixture.dir.join("downloads");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("octoboardd"), "another copy").unwrap();
        let host = StandIn::new(vec![sharing("work")], true);
        *host.image.borrow_mut() = Some(Ok(other.join("octoboardd")));
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains(&format!(
                "the daemon that is running runs {}, which is not this installation's",
                other.join("octoboardd").display()
            )) && err.contains("nothing was cleared, stopped or removed"),
            "{err}"
        );
        assert!(host.calls().is_empty(), "{:?}", host.calls());
        assert!(fixture.nothing_removed());

        let host = StandIn::new(vec![sharing("work")], true);
        *host.image.borrow_mut() = Some(Err("permission denied".into()));
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains("could not be told: permission denied")
                && err.contains("nothing was cleared, stopped or removed"),
            "{err}"
        );
        assert!(host.calls().is_empty() && fixture.nothing_removed());

        // A file gone from another directory is not the installation's either.
        let host = StandIn::new(Vec::new(), true);
        *host.image.borrow_mut() = Some(Ok(PathBuf::from(format!(
            "{} (deleted)",
            other.join("octoboardd").display()
        ))));
        assert_eq!(uninstall(YES, &fixture, &host, "", false).0, 1);
        assert!(host.calls().is_empty() && fixture.nothing_removed());

        let own: [fn(&Fixture) -> PathBuf; 2] = [
            |fixture| fixture.install.bin_dir().join("octoboardd.previous"),
            |fixture| {
                let dir = crate::hostfs::canonical(fixture.install.bin_dir()).unwrap();
                PathBuf::from(format!(
                    "{} (deleted)",
                    dir.join(".octoboardd.update-1").display()
                ))
            },
        ];
        for image in own {
            let fixture = Fixture::new("uninstall-own-daemon", false);
            let host = StandIn::new(Vec::new(), true);
            *host.image.borrow_mut() = Some(Ok(image(&fixture)));
            let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
            assert_eq!(status, 0, "{err}");
            assert!(fixture.removed());
        }
    }

    /// Where whether a daemon runs cannot be told, nothing is cleared, stopped or removed.
    #[test]
    fn a_lock_that_cannot_be_read_stops_the_uninstall() {
        let fixture = Fixture::new("uninstall-untold", true);
        let mut host = StandIn::new(vec![sharing("work")], false);
        host.untold = Some("locking daemon.lock: input/output error".into());
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains("whether a daemon is running could not be told: locking daemon.lock")
                && err.contains("nothing was cleared, stopped or removed"),
            "{err}"
        );
        assert!(host.calls().is_empty(), "{:?}", host.calls());
        assert!(fixture.nothing_removed() && fixture.install.unit().exists());
    }

    /// A unit that runs another file is not this installation's: nothing is cleared, stopped or
    /// removed. The archive's own unit, which names the executable by `%h`, is.
    #[test]
    fn a_unit_that_runs_another_executable_stops_the_uninstall() {
        let fixture = Fixture::new("uninstall-foreign-unit", true);
        fixture.write_unit("/opt/elsewhere/octoboardd --service-managed");
        let host = StandIn::new(vec![sharing("work")], true);
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains("runs /opt/elsewhere/octoboardd, not")
                && err.contains("nothing was removed"),
            "{err}"
        );
        assert!(host.calls().is_empty(), "{:?}", host.calls());
        assert!(fixture.nothing_removed() && fixture.install.unit().exists());

        fixture.write_unit("%h/bin/octoboardd start");
        let host = StandIn::new(Vec::new(), false);
        assert_eq!(uninstall(YES, &fixture, &host, "", false).0, 0);
        assert!(fixture.removed() && !fixture.install.unit().exists());
    }

    /// A file of another user's stops the uninstall before anything is cleared or stopped.
    #[test]
    fn a_file_of_another_users_stops_the_uninstall() {
        let fixture = Fixture::new("uninstall-foreign-file", true);
        let mut host = StandIn::new(vec![sharing("work")], true);
        host.foreign = Some(fixture.install.bin_dir().join("octoboardd.previous"));
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains("octoboardd.previous does not belong to the user running this"),
            "{err}"
        );
        assert!(host.calls().is_empty(), "{:?}", host.calls());
        assert!(fixture.nothing_removed());
    }

    /// Lingering stays on, with the units named, while another enabled unit of the user's may
    /// need it, in the unit directory or a generator's; and the install's note stays with it.
    #[test]
    fn lingering_is_kept_for_another_enabled_unit() {
        for generated in [false, true] {
            let fixture = Fixture::new("uninstall-linger-kept", true);
            fixture.note_lingering();
            let wants = if generated {
                fixture
                    .dir
                    .join("run")
                    .join("systemd")
                    .join("generator")
                    .join("default.target.wants")
            } else {
                fixture.install.unit_dir.join("timers.target.upholds")
            };
            std::fs::create_dir_all(&wants).unwrap();
            std::fs::write(wants.join("backup.service"), "").unwrap();
            // Its own link does not count.
            let own = fixture.install.unit_dir.join("default.target.wants");
            std::fs::create_dir_all(&own).unwrap();
            std::fs::write(own.join(UNIT), "").unwrap();
            let host = StandIn::new(Vec::new(), false);
            let (status, out, _) = uninstall(YES, &fixture, &host, "", false);
            assert_eq!(status, 0);
            assert!(!host.made("loginctl"), "{:?}", host.calls());
            assert!(
                out.contains("It was left on, since other services of yours are enabled and may need it: backup.service."),
                "{out}"
            );
            assert!(fixture.install.linger_record().exists());
            assert!(fixture.removed());
        }
    }

    /// Lingering the system refuses to turn off is said, with the command, and the uninstall
    /// goes on; with no note of the install's, lingering is not touched.
    #[test]
    fn lingering_that_cannot_be_turned_off_is_said_and_without_a_note_it_is_left_alone() {
        let fixture = Fixture::new("uninstall-linger-refused", true);
        fixture.note_lingering();
        let mut host = StandIn::new(Vec::new(), false);
        host.linger_off = Some(false);
        let (status, out, _) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 0);
        assert!(
            out.contains(
                "could not be turned off here. Turn it off with `loginctl disable-linger tester`"
            ),
            "{out}"
        );
        assert!(fixture.install.linger_record().exists() && fixture.removed());

        let fixture = Fixture::new("uninstall-no-linger", true);
        let host = StandIn::new(Vec::new(), false);
        assert_eq!(uninstall(YES, &fixture, &host, "", false).0, 0);
        assert!(!host.made("loginctl"));
    }

    /// With no service manager to reach, the unit and the link that enabled it are removed by
    /// hand, with a warning.
    #[test]
    fn with_no_service_manager_the_unit_and_its_link_are_removed_by_hand() {
        let fixture = Fixture::new("uninstall-no-manager", true);
        let wants = fixture.install.unit_dir.join("default.target.wants");
        std::fs::create_dir_all(&wants).unwrap();
        std::fs::write(wants.join(UNIT), "").unwrap();
        let mut host = StandIn::new(Vec::new(), false);
        host.disable = None;
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 0);
        assert!(
            err.contains("warning: systemctl could not be run")
                && err.contains("were removed without it"),
            "{err}"
        );
        assert!(!host.made("systemctl --user daemon-reload"));
        assert!(!fixture.install.unit().exists() && !wants.join(UNIT).exists());
        assert!(fixture.removed());

        // One that ran and failed is said as that, and a manager that is there is told.
        let fixture = Fixture::new("uninstall-manager-refused", true);
        let mut host = StandIn::new(Vec::new(), false);
        host.disable = Some(false);
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 0);
        assert!(
            err.contains("`systemctl --user disable --now octoboardd.service` failed")
                && !err.contains("could not be run"),
            "{err}"
        );
        assert!(host.made("systemctl --user daemon-reload"));
        assert!(!host.made("systemctl --user reset-failed"));
        assert!(!fixture.install.unit().exists() && fixture.removed());
    }

    /// The unit's drop-ins are read with it and removed with it: one that has the unit run
    /// another executable stops the uninstall, and one that leaves it running this one goes.
    #[test]
    fn the_units_drop_ins_are_read_and_removed_with_it() {
        let fixture = Fixture::new("uninstall-drop-ins", true);
        let drop_ins = fixture.install.drop_in_dir();
        std::fs::create_dir_all(&drop_ins).unwrap();
        std::fs::write(
            drop_ins.join("override.conf"),
            "[Service]\nExecStart=\nExecStart=/opt/elsewhere/octoboardd\n",
        )
        .unwrap();
        let host = StandIn::new(Vec::new(), false);
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains("with its drop-ins in")
                && err.contains("runs /opt/elsewhere/octoboardd, not"),
            "{err}"
        );
        assert!(host.calls().is_empty() && fixture.nothing_removed());
        assert!(fixture.install.unit().exists() && drop_ins.join("override.conf").exists());

        std::fs::write(
            drop_ins.join("override.conf"),
            "[Service]\nEnvironment=A=1\n",
        )
        .unwrap();
        // Not a drop-in systemd applies, and removed with the directory all the same.
        std::fs::write(drop_ins.join("notes.txt"), "ExecStart=/bin/false\n").unwrap();
        let (status, out, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 0, "{err}");
        assert!(out.contains("remove its drop-ins,"), "{out}");
        assert!(!drop_ins.exists() && !fixture.install.unit().exists());
        assert!(fixture.removed());
    }

    /// Drop-ins the command cannot remove whole stop it before anything is done: a directory
    /// among them, or one of another user's.
    #[test]
    fn drop_ins_that_are_not_the_commands_to_remove_stop_the_uninstall() {
        let fixture = Fixture::new("uninstall-drop-ins-foreign", true);
        let drop_ins = fixture.install.drop_in_dir();
        std::fs::create_dir_all(drop_ins.join("kept")).unwrap();
        let host = StandIn::new(vec![sharing("work")], true);
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains("holds a directory") && err.contains("nothing was removed"),
            "{err}"
        );
        assert!(host.calls().is_empty() && fixture.nothing_removed());

        std::fs::remove_dir(drop_ins.join("kept")).unwrap();
        std::fs::write(drop_ins.join("override.conf"), "").unwrap();
        for foreign in [
            drop_ins.join("override.conf"),
            drop_ins.clone(),
            fixture.install.unit(),
        ] {
            let mut host = StandIn::new(vec![sharing("work")], true);
            host.foreign = Some(foreign.clone());
            let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
            assert_eq!(status, 1);
            assert!(
                err.contains(&format!(
                    "{} does not belong to the user running this",
                    foreign.display()
                )),
                "{err}"
            );
            assert!(host.calls().is_empty() && fixture.nothing_removed());
        }
    }

    /// A unit directory the user cannot write stops the uninstall before anything is done, as
    /// the executable's directory does.
    #[cfg(unix)]
    #[test]
    fn a_unit_directory_that_cannot_be_written_stops_the_uninstall() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = Fixture::new("uninstall-unit-dir", true);
        let unit_dir = &fixture.install.unit_dir;
        let closed = |mode| {
            std::fs::set_permissions(unit_dir, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        closed(0o500);
        // Root writes whatever the mode says.
        if fs::directory_writable(unit_dir) {
            closed(0o700);
            return;
        }
        let host = StandIn::new(vec![sharing("work")], true);
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        closed(0o700);
        assert_eq!(status, 1);
        assert!(
            err.contains(&format!(
                "{} cannot be written by the user running this",
                unit_dir.display()
            )),
            "{err}"
        );
        assert!(host.calls().is_empty() && fixture.nothing_removed());
    }

    /// A file that is gone by the time it is removed, as another `uninstall` at once leaves it,
    /// is removed.
    #[test]
    fn a_file_that_is_gone_already_is_removed() {
        let fixture = Fixture::new("uninstall-gone", false);
        let gone = fixture.dir.join("gone");
        assert!(remove(&gone).is_ok());
        assert!(remove_drop_ins(&gone, std::slice::from_ref(&gone)).is_ok());
        // Anything else is still an error.
        assert!(remove(fixture.install.bin_dir()).is_err());
    }

    /// The command refuses what it does not uninstall in the words of each case: a build the
    /// install script did not install, the application's bundled copy, and the server form's
    /// own executable while the application's daemon runs, which is not told it is the
    /// application's.
    #[test]
    fn what_the_command_does_not_uninstall_is_refused_in_its_own_words() {
        let data = Path::new("/home/u/.octoboard");
        let server = AskedOf {
            script_installed: true,
            application_copy: false,
            applications_daemon_runs: false,
        };
        assert_eq!(server.refusal(data), None);
        let not_installed = AskedOf {
            script_installed: false,
            application_copy: true,
            applications_daemon_runs: true,
        };
        assert!(not_installed
            .refusal(data)
            .unwrap()
            .contains("was not installed by the install script"));
        let copy = AskedOf {
            application_copy: true,
            applications_daemon_runs: true,
            ..server
        };
        let own = format!("this {} is the desktop application's own", crate::APP_NAME);
        assert!(copy.refusal(data).unwrap().starts_with(&own));
        let beside = AskedOf {
            applications_daemon_runs: true,
            ..server
        };
        let why = beside.refusal(data).unwrap();
        assert!(
            why.starts_with(
                "the daemon running against /home/u/.octoboard is the desktop application's"
            ) && why.contains("quit the application and run `octoboardd uninstall` again")
                && !why.contains(&own),
            "{why}"
        );
    }

    /// An install with no unit asks nothing of systemd.
    #[test]
    fn an_install_with_no_unit_asks_nothing_of_systemd() {
        let fixture = Fixture::new("uninstall-no-unit", false);
        let host = StandIn::new(Vec::new(), true);
        let (status, out, _) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 0);
        assert_eq!(host.calls(), ["check stop", "stop"]);
        assert!(!out.contains("service"), "{out}");
        assert!(fixture.removed());
    }

    /// `--keep-changes` clears nothing and starts no daemon for it; what stays is named.
    #[test]
    fn keep_changes_uninstalls_without_clearing_and_names_what_stays() {
        let fixture = Fixture::new("uninstall-keep", false);
        let host = StandIn::new(vec![sharing("work")], false);
        let keep = Options {
            keep_changes: true,
            ..YES
        };
        let (status, out, err) = uninstall(keep, &fixture, &host, "", false);
        assert_eq!(status, 0);
        assert!(host.calls().is_empty(), "{:?}", host.calls());
        assert!(
            out.contains("leave in effect, as --keep-changes was given:")
                && out.contains("Claude Code account \"work\" shares the default configuration"),
            "{out}"
        );
        assert!(
            err.contains("warning: what is in effect was left in place")
                && err.contains("the links in each such account's directory")
                && err.contains("the backups of the account's own configuration"),
            "{err}"
        );
        assert!(fixture.removed());

        // With a daemon running, nothing being cleared, it is stopped once, before the removal,
        // and not started again.
        let fixture = Fixture::new("uninstall-keep-running", true);
        let host = StandIn::new(vec![sharing("work")], true);
        assert_eq!(uninstall(keep, &fixture, &host, "", false).0, 0);
        assert_eq!(host.calls()[..2], ["check stop", "stop"]);
        assert!(!host.made("start") && !host.made("systemctl --user start"));
        assert!(fixture.removed());

        // What is in effect need not be readable to leave it.
        let fixture = Fixture::new("uninstall-keep-unread", false);
        let mut host = StandIn::new(Vec::new(), false);
        host.changes = Err("unreadable".into());
        assert_eq!(uninstall(keep, &fixture, &host, "", false).0, 0);
        assert!(fixture.removed());
    }

    /// With no terminal the command needs `--yes`; in a terminal only the typed word uninstalls.
    #[test]
    fn without_a_terminal_it_needs_yes_and_in_one_only_the_word_uninstalls() {
        let fixture = Fixture::new("uninstall-confirm", true);
        let ask = Options { yes: false, ..YES };
        let host = StandIn::new(vec![sharing("work")], true);
        let (status, _, err) = uninstall(ask, &fixture, &host, "REMOVE\n", false);
        assert_eq!(status, 1);
        assert!(
            err.contains("--yes") && err.contains("nothing was removed"),
            "{err}"
        );
        for typed in ["", "\n", "y\n", "yes\n", "remove\n"] {
            let (status, out, _) = uninstall(ask, &fixture, &host, typed, true);
            assert_eq!(status, 1, "{typed:?}");
            assert!(out.ends_with("nothing was removed\n"), "{typed:?}: {out}");
        }
        assert!(!host.made("clear") && !host.made("stop") && fixture.nothing_removed());

        let (status, out, err) = uninstall(ask, &fixture, &host, "REMOVE\n", true);
        assert_eq!(status, 0, "{err}");
        // The plan, with what will be cleared and removed, comes before the question.
        let asked = out.find("Type REMOVE to uninstall: ").expect(&out);
        for said in [
            "clear what Octoboard's features have in effect",
            "account \"work\" shares",
            "stop the daemon",
            "remove the service",
            "octoboardd.previous",
            "the daemon's own data",
        ] {
            assert!(out[..asked].contains(said), "{said}: {out}");
        }
        assert!(fixture.removed());
    }

    /// `--dry-run` says what would be cleared, stopped and removed, and changes nothing.
    #[test]
    fn a_dry_run_says_what_it_would_do_and_changes_nothing() {
        let fixture = Fixture::new("uninstall-dry-run", true);
        fixture.note_lingering();
        let host = StandIn::new(vec![sharing("work")], false);
        let dry = Options {
            dry_run: true,
            yes: false,
            ..YES
        };
        let (status, out, err) = uninstall(dry, &fixture, &host, "", false);
        assert_eq!((status, err.as_str()), (0, ""));
        for said in [
            "start the daemon for a clean-up run, which runs no sessions, and clear",
            "account \"work\" shares",
            "then stop that daemon again",
            "remove the service",
            "turn lingering off",
            "bin/octoboardd",
            "Nothing was changed (--dry-run).",
        ] {
            assert!(out.contains(said), "{said}: {out}");
        }
        assert!(host.calls().is_empty(), "{:?}", host.calls());
        assert!(fixture.nothing_removed() && fixture.install.unit().exists());
        assert!(fixture.install.linger_record().exists());
        // What is in effect is read without changing the data directory, and only then.
        assert_eq!(*host.read_unchanged.borrow(), Some(true));
        assert_eq!(uninstall(YES, &fixture, &host, "", false).0, 0);
        assert_eq!(*host.read_unchanged.borrow(), Some(false));

        // A daemon that runs is not stopped; the plan says it would be, and started again where
        // the clean-up leaves something.
        let fixture = Fixture::new("uninstall-dry-run-running", true);
        let host = StandIn::new(vec![sharing("work")], true);
        let (status, out, _) = uninstall(dry, &fixture, &host, "", false);
        assert_eq!(status, 0);
        assert_eq!(host.calls(), ["check stop"]);
        assert!(*host.running.borrow() && fixture.nothing_removed());
        let stop = out
            .find("  stop the daemon, which ends every session")
            .expect(&out);
        assert!(
            stop < out.find("start the daemon for a clean-up run").unwrap(),
            "{out}"
        );
        assert!(
            out.contains(
                "where something stays in effect, remove nothing and start the daemon again"
            ),
            "{out}"
        );
    }

    /// Run through a link, nothing is done: the installation is where the link leads, which is
    /// named, and the link and the installation stay.
    #[cfg(unix)]
    #[test]
    fn a_link_to_the_executable_is_not_uninstalled_and_names_where_it_leads() {
        let fixture = Fixture::new("uninstall-link", false);
        let linked = fixture.dir.join("home-bin");
        std::fs::create_dir_all(&linked).unwrap();
        let link = linked.join("octoboardd");
        std::os::unix::fs::symlink(&fixture.install.binary, &link).unwrap();
        let through_link = Fixture {
            install: Installation {
                binary: link.clone(),
                ..fixture.install.clone()
            },
            dir: ScratchDir::new("uninstall-link-unused"),
        };
        let host = StandIn::new(vec![sharing("work")], true);
        let (status, _, err) = uninstall(YES, &through_link, &host, "", false);
        assert_eq!(status, 1);
        let target = crate::hostfs::canonical(&fixture.install.binary).unwrap();
        assert!(
            err.contains(&format!(
                "{} is a link to {}, not an installation of its own; nothing was cleared, stopped \
                 or removed.",
                link.display(),
                target.display()
            )) && err.contains(&format!("`{} uninstall`", target.display())),
            "{err}"
        );
        assert!(host.calls().is_empty(), "{:?}", host.calls());
        assert!(std::fs::symlink_metadata(&link).is_ok() && fixture.nothing_removed());
    }

    /// A daemon started by something else after the checks is neither cleared through nor
    /// stopped: nothing is done, and running the command again is named.
    #[test]
    fn a_daemon_started_after_the_checks_is_not_used() {
        for changes in [vec![sharing("work")], Vec::new()] {
            let fixture = Fixture::new("uninstall-started-meanwhile", false);
            let host = StandIn::new(changes, false);
            // As the plan is said, something starts the daemon.
            let mut typed = "REMOVE\n".as_bytes();
            struct Starting<'a> {
                input: &'a mut &'a [u8],
                host: &'a StandIn,
            }
            impl std::io::Read for Starting<'_> {
                fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                    *self.host.running.borrow_mut() = true;
                    self.input.read(buf)
                }
            }
            let mut input = std::io::BufReader::new(Starting {
                input: &mut typed,
                host: &host,
            });
            let ask = Options { yes: false, ..YES };
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let status = run_on(
                ask,
                &fixture.install,
                &host,
                &mut Io {
                    input: &mut input,
                    out: &mut out,
                    err: &mut err,
                    terminal: true,
                },
            );
            let err = String::from_utf8(err).unwrap();
            assert_eq!(status, 1);
            assert!(
                err.contains(
                    "a daemon was started meanwhile, by something other than this command"
                ),
                "{err}"
            );
            assert!(!host.made("clear") && !host.made("stop") && !host.made("start"));
            assert!(fixture.nothing_removed());
        }
    }

    /// `--dry-run` on this machine, run in a home of the parent test's (`HOME`), on the
    /// installation `bin/octoboardd` under it: prints the status it ended with.
    #[test]
    #[ignore = "run by a_dry_run_on_this_machine_creates_no_lock_file, in a home of its own"]
    fn dry_run_child() {
        let home = crate::paths::home_dir();
        let install = Installation {
            binary: home.join("bin").join("octoboardd"),
            unit_dir: home.join(".config").join("systemd").join("user"),
            data_dir: crate::paths::data_dir(),
            home: home.clone(),
            runtime_dir: None,
        };
        let dry = Options {
            dry_run: true,
            ..Options::default()
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = run_on(
            dry,
            &install,
            &Machine { unchanged: true },
            &mut Io {
                input: &mut "".as_bytes(),
                out: &mut out,
                err: &mut err,
                terminal: false,
            },
        );
        println!("dry run status: {status}");
        eprintln!(
            "{}{}",
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&err)
        );
    }

    /// The whole dry run, on this machine's own reads (the lock, the owner record, the store),
    /// leaves no `daemon.lock` where there was none: with no store, and with one of this build's.
    #[test]
    fn a_dry_run_on_this_machine_creates_no_lock_file() {
        for with_store in [false, true] {
            let home = ScratchDir::new("uninstall-dry-run-lock");
            let data = home.join(".octoboard");
            std::fs::create_dir_all(home.join("bin")).unwrap();
            std::fs::write(home.join("bin").join("octoboardd"), "octoboardd").unwrap();
            std::fs::create_dir_all(&data).unwrap();
            if with_store {
                drop(crate::store::Store::open(&data.join("octoboard.db")).unwrap());
            }
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "cli::uninstall::tests::dry_run_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("HOME", &*home)
                .env("USERPROFILE", &*home)
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            let said = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(said.contains("dry run status: 0"), "{said}");
            assert!(said.contains("Nothing was changed (--dry-run)."), "{said}");
            assert!(
                !data.join("daemon.lock").exists(),
                "the dry run created daemon.lock (store: {with_store}): {said}"
            );
        }
    }

    /// The daemon stopped for a clean-up that cleared everything, where whether a daemon runs then
    /// cannot be told: nothing is removed, the daemon is not said to be unstopped, and it is
    /// started again.
    #[test]
    fn a_daemon_stopped_for_the_clean_up_is_started_again_where_the_stop_after_cannot_tell() {
        let fixture = Fixture::new("uninstall-untold-after", false);
        let host = StandIn::new(vec![sharing("work")], true);
        // Asked up front, in the clean-up, then before the removal.
        host.answers
            .borrow_mut()
            .insert(3, Err("daemon.lock: busy".into()));
        let (status, out, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            err.contains(
                "whether a daemon is running could not be told: daemon.lock: busy; \
                          nothing was removed."
            ) && !err.contains("nothing was stopped"),
            "{err}"
        );
        assert_eq!(
            host.calls()[2..],
            ["start", "clear", "stop", "start again Some(7531)"]
        );
        assert!(
            out.contains("Started the daemon again in the background"),
            "{out}"
        );
        assert!(fixture.nothing_removed());
    }

    /// A daemon something else started after the stop is not cleared through, and the daemon
    /// this stopped is said not to be started again, with how to start it.
    #[test]
    fn a_daemon_started_after_the_stop_is_not_used_and_the_stopped_one_is_named() {
        let fixture = Fixture::new("uninstall-started-after-stop", true);
        let host = StandIn::new(vec![sharing("work")], true);
        host.answers.borrow_mut().insert(2, Ok(true));
        let (status, _, err) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 1);
        assert!(
            !host.made("clear") && !host.made("start"),
            "{:?}",
            host.calls()
        );
        assert!(
            err.contains("a daemon was started meanwhile")
                && err.contains(
                    "The daemon this had stopped for the clean-up was not started again, since \
                     another daemon now runs; once `octoboardd status` says none runs, start it \
                     with `systemctl --user start octoboardd`."
                ),
            "{err}"
        );
        assert!(fixture.nothing_removed());
    }

    /// With nothing installed where it looks, nothing is removed and it fails nothing.
    #[test]
    fn with_nothing_installed_nothing_is_removed() {
        let fixture = Fixture::new("uninstall-absent", false);
        for file in fixture.install.files() {
            std::fs::remove_file(file).unwrap();
        }
        let host = StandIn::new(Vec::new(), false);
        let (status, out, _) = uninstall(YES, &fixture, &host, "", false);
        assert_eq!(status, 0);
        assert!(
            out.contains("is not installed in") && out.contains("nothing was removed"),
            "{out}"
        );
        assert!(host.calls().is_empty());
    }

    /// The executable a unit runs is the first word of its `ExecStart`, `%h` being the home.
    #[test]
    fn the_units_executable_is_read_from_its_exec_start() {
        let fixture = Fixture::new("uninstall-unit-program", true);
        let unit = fixture.install.unit();
        let home = Path::new("/home/u");
        fixture.write_unit("/opt/bin/octoboardd --port 7531 --service-managed");
        assert_eq!(
            unit_program(&unit, &[], home),
            Some(PathBuf::from("/opt/bin/octoboardd"))
        );
        fixture.write_unit("%h/.local/bin/octoboardd start");
        assert_eq!(
            unit_program(&unit, &[], home),
            Some(PathBuf::from("/home/u/.local/bin/octoboardd"))
        );
        // A drop-in adds to the unit's own unless it drops it first, and two are none to run;
        // only a `.conf` file is one, and one that cannot be read tells nothing.
        let drop_in = |name: &str, text: &str| {
            let path = fixture.dir.join(name);
            std::fs::write(&path, text).unwrap();
            path
        };
        let adds = drop_in("10-adds.conf", "[Service]\nExecStart=/opt/other\n");
        let replaces = drop_in(
            "20-replaces.conf",
            "[Service]\nExecStart=\nExecStart=/opt/other --flag\n",
        );
        let ignored = drop_in("30-ignored.txt", "ExecStart=/opt/ignored\n");
        assert_eq!(unit_program(&unit, std::slice::from_ref(&adds), home), None);
        assert_eq!(
            unit_program(&unit, &[adds, replaces.clone()], home),
            Some(PathBuf::from("/opt/other"))
        );
        assert_eq!(
            unit_program(&unit, &[ignored], home),
            Some(PathBuf::from("/home/u/.local/bin/octoboardd"))
        );
        assert_eq!(
            unit_program(&unit, &[fixture.dir.join("absent.conf")], home),
            None
        );
        std::fs::write(&unit, "[Service]\n").unwrap();
        assert_eq!(unit_program(&unit, &[], home), None);
        assert_eq!(
            unit_program(&unit, &[replaces], home),
            Some("/opt/other".into())
        );
        assert_eq!(
            unit_program(&fixture.dir.join("absent.service"), &[], home),
            None
        );
    }
}
