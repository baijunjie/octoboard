//! Linux systemd user services: whether the daemon is the main process of one, and starting a
//! process as a transient user service of its own (`systemd-run --user`), which outlives the
//! daemon's service and delegates a cgroup of its own.

use std::fs;
use std::io;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::cgroup::unified_path;

/// How long `systemd-run` has to start a unit before whether it started counts as unknown.
const UNIT_START_DEADLINE: Duration = Duration::from_secs(15);

/// The name of the systemd user service this daemon is the main process of, when a systemd user
/// manager runs it: `managed` says the daemon is a service's main process
/// ([`super::restarted_by_service_manager`]: the environment systemd 248 and later gives it, or
/// `--service-managed`, which the unit passes on any version), and its cgroup is that of a service
/// below the calling user's manager, read from systemd's own hierarchy on cgroup v1 and from the
/// unified one on v2. `None` otherwise, as for a daemon started by hand, or by a system service.
pub(crate) fn user_service_unit(managed: bool) -> Option<String> {
    managed.then(user_service).flatten()
}

/// Whether the calling process's cgroup is that of a systemd service, a user's or the system's:
/// whatever it starts without a unit of its own is in that service, and ends when the service
/// stops.
pub(crate) fn in_systemd_service() -> bool {
    fs::read_to_string("/proc/self/cgroup")
        .ok()
        .and_then(|own| systemd_path(&own).map(is_service))
        .unwrap_or(false)
}

/// The path of the calling process in the hierarchy systemd tracks units in, from a
/// `/proc/<pid>/cgroup` content: the unified hierarchy's (`0::`) where it is listed with a path
/// other than `/`, which is where systemd tracks units on cgroup v2 and in its hybrid layout; the
/// `name=systemd` hierarchy's otherwise, which is where it does on cgroup v1. A `name=systemd` line
/// beside a unified one that names a path is no more than a stray hierarchy (container tooling
/// leaves them), so the unified line wins.
fn systemd_path(cgroup_file: &str) -> Option<&str> {
    unified_path(cgroup_file)
        .filter(|path| *path != "/")
        .or_else(|| {
            cgroup_file
                .lines()
                .find_map(|line| {
                    let (_, rest) = line.split_once(':')?;
                    rest.strip_prefix("name=systemd:")
                })
                .filter(|path| path.starts_with('/'))
        })
}

/// Whether the cgroup path `path` is that of a service: its last part is `<name>.service`.
fn is_service(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|last| last.len() > ".service".len() && last.ends_with(".service"))
}

/// Starts `command` as a transient systemd user service named `<unit>.service`
/// (`systemd-run --user`), described as `description`, ordered before the daemon's own service
/// `daemon`, so that when both stop together (at a host shutdown), which systemd does in the
/// reverse of the start order, it is stopped once that service has stopped. It is not bound to
/// `daemon` otherwise, and outlives it. When it is stopped only its main process is signalled
/// (`KillMode=process`); it delegates its cgroup (`Delegate=yes`), so that groups can be made in
/// it; its output is discarded, as a detached process's is; and it is removed once it has ended,
/// whether or not it failed. Only the program, the arguments and the working directory of
/// `command` are used: the unit runs in the user manager's environment, so nothing the daemon's
/// environment holds reaches it, nor anything systemd records of it. Only options that
/// `systemd-run --collect` (systemd 236) comes with are used (the working directory is a property,
/// not `--working-directory`, which came with 240). **Blocks** until `systemd-run` returns. An
/// error of kind `TimedOut` means it did not return within [`UNIT_START_DEADLINE`], so whether the
/// unit started is unknown; any other means it did not.
pub(crate) fn start_in_user_unit(
    command: &Command,
    unit: &str,
    description: &str,
    daemon: &str,
) -> io::Result<()> {
    start_in_user_unit_within(command, unit, description, daemon, UNIT_START_DEADLINE)
}

/// [`start_in_user_unit`] with `deadline` for `systemd-run` to return in.
fn start_in_user_unit_within(
    command: &Command,
    unit: &str,
    description: &str,
    daemon: &str,
    deadline: Duration,
) -> io::Result<()> {
    use std::io::Read as _;

    let mut run = Command::new("systemd-run");
    run.args(["--user", "--quiet", "--collect"])
        .arg(format!("--unit={unit}.service"))
        .arg(format!("--description={description}"))
        .args([
            "--property=KillMode=process",
            "--property=Delegate=yes",
            "--property=StandardOutput=null",
            "--property=StandardError=null",
        ])
        .arg(format!("--property=Before={daemon}"));
    if let Some(dir) = command.get_current_dir() {
        let mut option = std::ffi::OsString::from("--property=WorkingDirectory=");
        option.push(dir);
        run.arg(option);
    }
    run.arg("--")
        .arg(command.get_program())
        .args(command.get_args())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = run.spawn()?;
    let stderr = child.stderr.take();
    let said = std::thread::spawn(move || {
        let mut said = Vec::new();
        if let Some(mut stderr) = stderr {
            stderr.read_to_end(&mut said).ok();
        }
        String::from_utf8_lossy(&said).trim().to_string()
    });
    let deadline = Instant::now() + deadline;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().ok();
            child.wait().ok();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "systemd-run did not return in time",
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let said = said.join().unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("systemd-run {status}: {said}")))
    }
}

/// The name of the systemd user service the calling process's cgroup is the cgroup of: its last
/// part when that names a service (`<name>.service`) directly below the user manager of the
/// calling user (`user@<uid>.service`) or a slice in it, or `None` otherwise. Says nothing of
/// whether the process is that service's main process.
fn user_service() -> Option<String> {
    let own = fs::read_to_string("/proc/self/cgroup").ok()?;
    service_of(systemd_path(&own)?, uid())
}

/// The real user id of the calling process.
fn uid() -> u32 {
    // SAFETY: takes nothing and cannot fail.
    unsafe { libc::getuid() }
}

/// [`user_service`] over the cgroup path `path` and the user id `uid`.
fn service_of(path: &str, uid: u32) -> Option<String> {
    let manager = format!("user@{uid}.service");
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    parts.by_ref().find(|part| *part == manager)?;
    let below: Vec<&str> = parts.collect();
    let (service, slices) = below.split_last()?;
    (service.len() > ".service".len()
        && service.ends_with(".service")
        && slices.iter().all(|slice| slice.ends_with(".slice")))
    .then(|| (*service).to_string())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    use super::{
        is_service, service_of, start_in_user_unit, start_in_user_unit_within, systemd_path,
        user_service, user_service_unit,
    };
    use crate::platform::test_helper;
    use crate::test_support::{
        assert_gone_by, make_executable, wait_for_pid, ScratchDir, PATIENCE,
    };

    const MANAGER: &str = "/user.slice/user-1001.slice/user@1001.service";

    /// The path of a user service of uid 1001 in the hierarchy systemd tracks units in.
    const SERVICE_PATH: &str =
        "/user.slice/user-1001.slice/user@1001.service/app.slice/octoboardd.service";

    fn service(path: &str) -> Option<String> {
        service_of(path, 1001)
    }

    #[test]
    fn a_user_service_is_named_by_the_last_part_of_its_cgroup() {
        assert_eq!(
            service(&format!("{MANAGER}/app.slice/octoboardd.service")).as_deref(),
            Some("octoboardd.service")
        );
        assert_eq!(
            service(&format!("{MANAGER}/octoboardd.service")).as_deref(),
            Some("octoboardd.service"),
            "directly below the manager counts"
        );
        assert_eq!(
            service(&format!("{MANAGER}/app.slice/deeper.slice/a-b.service")).as_deref(),
            Some("a-b.service")
        );
    }

    #[test]
    fn a_cgroup_that_is_no_user_service_of_this_user_names_none() {
        // Another user's manager.
        assert_eq!(
            service_of(
                "/user.slice/user-1002.slice/user@1002.service/app.slice/octoboardd.service",
                1001
            ),
            None
        );
        // A scope, not a service.
        assert_eq!(
            service(&format!("{MANAGER}/app.slice/app-gnome-term.scope")),
            None
        );
        // A part between that is no slice.
        assert_eq!(
            service(&format!(
                "{MANAGER}/app.slice/other.scope/octoboardd.service"
            )),
            None
        );
        // Nothing after the manager.
        assert_eq!(service(MANAGER), None);
        assert_eq!(service(&format!("{MANAGER}/")), None);
        // Not below a manager at all: a system service, a session scope.
        assert_eq!(service("/system.slice/octoboardd.service"), None);
        assert_eq!(service("/user.slice/user-1001.slice/session-3.scope"), None);
        assert_eq!(service("/"), None);
        // A bare suffix is no name.
        assert_eq!(service(&format!("{MANAGER}/app.slice/.service")), None);
    }

    /// A pure cgroup v1 host (systemd before 248 on a distribution without the unified
    /// hierarchy) has no `0::` line: systemd's own hierarchy, `name=systemd`, holds the unit.
    #[test]
    fn the_systemd_hierarchy_of_a_cgroup_v1_file_is_the_one_units_are_tracked_in() {
        let legacy = format!(
            "12:cpu,cpuacct:/user.slice\n7:memory:/user.slice\n1:name=systemd:{SERVICE_PATH}\n"
        );
        assert_eq!(systemd_path(&legacy), Some(SERVICE_PATH));
        // Wherever the line is.
        let first = format!("1:name=systemd:{SERVICE_PATH}\n12:cpu:/user.slice\n");
        assert_eq!(systemd_path(&first), Some(SERVICE_PATH));
    }

    /// The hybrid layout lists both, the unified line naming no path (`/`), so the systemd
    /// hierarchy's line is the one that says where the unit is; where both name one, systemd
    /// tracks the same path in both.
    #[test]
    fn the_systemd_hierarchy_is_read_where_the_unified_line_names_no_path() {
        let hybrid = format!("12:cpu:/user.slice\n1:name=systemd:{SERVICE_PATH}\n0::/\n");
        assert_eq!(systemd_path(&hybrid), Some(SERVICE_PATH));
        let same = format!("1:name=systemd:{SERVICE_PATH}\n0::{SERVICE_PATH}\n");
        assert_eq!(systemd_path(&same), Some(SERVICE_PATH));
    }

    /// A cgroup v2 host can list a stray named hierarchy (container tooling makes one), whose
    /// path is `/` or stale: the unified line is where systemd tracks the unit.
    #[test]
    fn a_stray_systemd_named_hierarchy_does_not_hide_the_unified_line_of_a_cgroup_v2_file() {
        let stray_root = format!("1:name=systemd:/\n0::{SERVICE_PATH}\n");
        assert_eq!(systemd_path(&stray_root), Some(SERVICE_PATH));
        let stray_stale = format!("0::{SERVICE_PATH}\n1:name=systemd:/lxc/old/init.scope\n");
        assert_eq!(systemd_path(&stray_stale), Some(SERVICE_PATH));
        // With nothing else listed, the root is all there is, and no service.
        assert_eq!(systemd_path("1:name=systemd:/\n0::/\n"), Some("/"));
    }

    #[test]
    fn the_unified_line_of_a_cgroup_v2_file_is_the_path_where_there_is_no_systemd_hierarchy() {
        let unified = format!("0::{SERVICE_PATH}\n");
        assert_eq!(systemd_path(&unified), Some(SERVICE_PATH));
    }

    #[test]
    fn a_cgroup_file_with_no_usable_line_names_no_path() {
        assert_eq!(systemd_path(""), None);
        assert_eq!(systemd_path("\n"), None);
        // Controllers only.
        assert_eq!(
            systemd_path("12:cpu:/user.slice\n7:memory:/user.slice\n"),
            None
        );
        // Another named hierarchy is not systemd's.
        assert_eq!(systemd_path("5:name=systemd-other:/x/a.service\n"), None);
        assert_eq!(systemd_path("5:name=openrc:/x/a.service\n"), None);
        // A line with no path at all.
        assert_eq!(systemd_path("1:name=systemd:\n"), None);
        assert_eq!(systemd_path("0::\n"), None);
    }

    #[test]
    fn a_path_that_is_not_absolute_is_no_path() {
        assert_eq!(systemd_path("1:name=systemd:user.slice/a.service\n"), None);
        assert_eq!(systemd_path("0::user.slice/a.service\n"), None);
    }

    #[test]
    fn a_path_with_a_colon_in_it_is_kept_whole() {
        assert_eq!(
            systemd_path("1:name=systemd:/a:b/c.service\n"),
            Some("/a:b/c.service")
        );
    }

    /// The user service is found from a cgroup v1 file as it is from the unified line.
    #[test]
    fn a_user_service_is_found_in_a_cgroup_v1_file() {
        let legacy = format!("12:cpu:/user.slice\n1:name=systemd:{SERVICE_PATH}\n");
        let found = systemd_path(&legacy).and_then(service);
        assert_eq!(found.as_deref(), Some("octoboardd.service"));

        let hybrid = format!("1:name=systemd:{SERVICE_PATH}\n0::/\n");
        let found = systemd_path(&hybrid).and_then(service);
        assert_eq!(found.as_deref(), Some("octoboardd.service"));

        let unified = format!("0::{SERVICE_PATH}\n");
        let found = systemd_path(&unified).and_then(service);
        assert_eq!(found.as_deref(), Some("octoboardd.service"));
    }

    /// A v1 file of a process that is no user service names none.
    #[test]
    fn a_cgroup_v1_file_of_a_system_service_or_a_session_names_no_user_service() {
        for path in [
            "/system.slice/octoboardd.service",
            "/user.slice/user-1001.slice/session-3.scope",
        ] {
            let legacy = format!("1:name=systemd:{path}\n");
            assert_eq!(systemd_path(&legacy).and_then(service), None, "{path}");
        }
    }

    /// Any service counts, the system's as well as a user's: it is the service's stop that ends
    /// what the process started without a unit of its own.
    #[test]
    fn a_path_is_a_service_when_its_last_part_is_a_service_unit() {
        assert!(is_service(SERVICE_PATH));
        assert!(is_service("/system.slice/ssh.service"));
        assert!(is_service("/octoboardd.service"));
        assert!(is_service("/system.slice/foo.bar.service"));
        assert!(is_service("/system.slice/a@b.service"));
    }

    #[test]
    fn a_path_is_no_service_when_its_last_part_is_not_a_service_unit() {
        assert!(!is_service(""));
        assert!(!is_service("/"));
        assert!(!is_service("/user.slice/user-1001.slice/session-3.scope"));
        assert!(!is_service(
            "/user.slice/user-1001.slice/user@1001.service/app.slice"
        ));
        assert!(!is_service("/system.slice/ssh.service.d"));
        assert!(!is_service("/system.slice/ssh.services"));
        // A bare suffix is no name.
        assert!(!is_service("/system.slice/.service"));
        assert!(!is_service(".service"));
        // A group made inside a service is not the service's own.
        assert!(!is_service("/system.slice/ssh.service/sub"));
        assert!(!is_service("/system.slice/ssh.service/"));
    }

    // `start_in_user_unit` runs `systemd-run`, found on the PATH of the process that calls it, so
    // the tests run it in a process of its own (this test binary run again, as the helpers of the
    // contract tests are) whose PATH leads to a stand-in `systemd-run` that records how it was
    // called and ends as the test says.

    /// Where the helper writes what `start_in_user_unit` answered, whole, once it has.
    const RESULT: &str = "OCTOBOARD_TEST_UNIT_RESULT";
    /// The working directory the helper gives the command it starts as a unit, if any.
    const WORKING_DIRECTORY: &str = "OCTOBOARD_TEST_UNIT_CWD";
    /// What the environment of the command the helper starts as a unit holds, which must not
    /// reach `systemd-run`.
    const SECRET: &str = "OCTOBOARD_TEST_UNIT_SECRET";
    /// The deadline, in milliseconds, the helper gives `systemd-run` to return in, if not the
    /// production one.
    const DEADLINE_MILLIS: &str = "OCTOBOARD_TEST_UNIT_DEADLINE_MS";

    const UNIT: &str = "octoboard-holder-h1";
    const DESCRIPTION: &str = "Octoboard process holder";
    const DAEMON: &str = "octoboardd.service";

    /// The helper: starts a command as a unit and writes the answer. A no-op in an ordinary run.
    #[test]
    fn a_unit_starter() {
        let Some(result) = std::env::var_os(RESULT) else {
            return;
        };
        let mut command = Command::new("/opt/octoboard/octoboardd");
        command.args(["holder", "--registry-dir", "/some dir/holders", "-x"]);
        if let Some(dir) = std::env::var_os(WORKING_DIRECTORY) {
            command.current_dir(dir);
        }
        command.env(SECRET, "s3cret");
        let started = match std::env::var(DEADLINE_MILLIS)
            .ok()
            .and_then(|millis| millis.parse().ok())
        {
            Some(millis) => start_in_user_unit_within(
                &command,
                UNIT,
                DESCRIPTION,
                DAEMON,
                Duration::from_millis(millis),
            ),
            None => start_in_user_unit(&command, UNIT, DESCRIPTION, DAEMON),
        };
        let answer = match started {
            Ok(()) => "ok".to_string(),
            Err(err) => format!("{:?}\n{err}", err.kind()),
        };
        let result = PathBuf::from(result);
        let partial = result.with_extension("partial");
        std::fs::write(&partial, answer).expect("the answer is written");
        std::fs::rename(partial, result).expect("the answer is in place");
    }

    /// The helper: writes what [`user_service_unit`] says of the process it runs in, given the
    /// service-managed answer the environment gives.
    #[test]
    fn a_service_unit_probe() {
        let Some(result) = std::env::var_os(RESULT) else {
            return;
        };
        let result = PathBuf::from(result);
        let partial = result.with_extension("partial");
        let managed = crate::platform::proc::restarted_by_service_manager(false);
        std::fs::write(&partial, format!("{:?}", user_service_unit(managed))).expect("written");
        std::fs::rename(partial, result).expect("in place");
    }

    /// The stand-in `systemd-run`, which every test shares: one executable per test binary, made
    /// once and put in place by a rename (a fresh script per test costs each its first `exec`,
    /// and binaries of other worktrees run beside this one). What it does comes from its
    /// environment: it writes its arguments, a line each, to `$FAKE_ARGS` and what it saw of
    /// `SECRET` to `$FAKE_ARGS.secret`; then, as `$FAKE_HANG` names a file, writes its pid
    /// there and waits; or else writes `$FAKE_STDERR` to its standard error and exits with
    /// `$FAKE_EXIT`.
    fn stand_in_directory() -> &'static Path {
        const SCRIPT: &str = "#!/bin/sh
for arg in \"$@\"; do printf '%s\\n' \"$arg\"; done > \"$FAKE_ARGS.partial\"
printf '%s' \"${OCTOBOARD_TEST_UNIT_SECRET-unset}\" > \"$FAKE_ARGS.secret\"
mv \"$FAKE_ARGS.partial\" \"$FAKE_ARGS\"
if [ -n \"$FAKE_HANG\" ]; then
  echo $$ > \"$FAKE_HANG.partial\"
  mv \"$FAKE_HANG.partial\" \"$FAKE_HANG\"
  exec sleep 600
fi
printf '%s' \"$FAKE_STDERR\" >&2
exit \"${FAKE_EXIT:-0}\"
";
        static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
        DIRECTORY.get_or_init(|| {
            let dir = std::env::temp_dir().join("octoboardd-stand-in-systemd-run");
            std::fs::create_dir_all(&dir).expect("the directory");
            let partial = dir.join(format!("systemd-run.{}", std::process::id()));
            std::fs::write(&partial, SCRIPT).expect("the script is written");
            make_executable(&partial);
            std::fs::rename(&partial, dir.join("systemd-run")).expect("the script is in place");
            dir
        })
    }

    /// What `start_in_user_unit` did when run against the stand-in.
    struct Run {
        /// What the helper wrote: `ok`, or the error's kind and its message.
        answer: String,
        /// What the stand-in was given, as it recorded it; `None` where it never ran.
        arguments: Option<Vec<String>>,
        /// What the stand-in saw of [`SECRET`].
        secret: Option<String>,
    }

    /// How a test has the stand-in answer, and where it is looked for.
    #[derive(Default)]
    struct Setup<'a> {
        exit: Option<&'a str>,
        stderr: Option<&'a str>,
        working_directory: Option<&'a Path>,
        hang: Option<&'a Path>,
        /// How long the helper gives `systemd-run` to return.
        deadline: Option<Duration>,
        /// The PATH has no `systemd-run` on it.
        absent: bool,
    }

    /// Waits for `helper` to end, up to `PATIENCE`, and returns its status.
    fn finished(helper: &mut std::process::Child) -> std::process::ExitStatus {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(status) = helper.try_wait().expect("the helper is waited on") {
                return status;
            }
            if Instant::now() >= deadline {
                helper.kill().ok();
                helper.wait().ok();
                panic!("the helper kept running");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn run_starter(label: &str, setup: Setup<'_>) -> Run {
        let dir = ScratchDir::new(label);
        let result = dir.join("answer");
        let arguments = dir.join("arguments");
        // An empty directory alone where `systemd-run` is to be missing: the usual directories
        // would hold the host's own, which would be run instead. The helper is started by its
        // path, so it needs no PATH.
        let path = if setup.absent {
            let empty = dir.join("empty");
            std::fs::create_dir(&empty).expect("an empty directory");
            empty.display().to_string()
        } else {
            format!("{}:/usr/bin:/bin", stand_in_directory().display())
        };
        let mut helper = Command::new(test_helper::program());
        helper
            .args(test_helper::args(
                "platform::proc::systemd::tests::a_unit_starter",
            ))
            .env("PATH", path)
            .env(RESULT, &result)
            .env("FAKE_ARGS", &arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(exit) = setup.exit {
            helper.env("FAKE_EXIT", exit);
        }
        if let Some(stderr) = setup.stderr {
            helper.env("FAKE_STDERR", stderr);
        }
        if let Some(dir) = setup.working_directory {
            helper.env(WORKING_DIRECTORY, dir);
        }
        if let Some(hang) = setup.hang {
            helper.env("FAKE_HANG", hang);
        }
        if let Some(deadline) = setup.deadline {
            helper.env(DEADLINE_MILLIS, deadline.as_millis().to_string());
        }
        let mut helper = helper.spawn().expect("the test binary starts");
        finished(&mut helper);
        Run {
            answer: std::fs::read_to_string(&result).expect("the helper answered"),
            arguments: std::fs::read_to_string(&arguments)
                .ok()
                .map(|text| text.lines().map(str::to_string).collect()),
            secret: std::fs::read_to_string(dir.join("arguments.secret")).ok(),
        }
    }

    /// The unit is started with `systemd-run --user`, named and described as asked, ordered before
    /// the daemon's service, delegating its cgroup, stopped by its main process alone, collected
    /// when it ends and with its output discarded, running the command's program and arguments,
    /// each as one argument, after `--`.
    #[test]
    fn a_unit_is_started_by_a_systemd_run_with_the_properties_a_holder_needs() {
        let dir = ScratchDir::new("unit-arguments-cwd");
        let run = run_starter(
            "unit-arguments",
            Setup {
                working_directory: Some(&dir),
                ..Setup::default()
            },
        );
        assert_eq!(run.answer, "ok");
        let arguments = run.arguments.expect("systemd-run ran");
        let split = arguments
            .iter()
            .position(|argument| argument == "--")
            .expect("the program is set apart by `--`");
        assert_eq!(
            arguments[split + 1..],
            [
                "/opt/octoboard/octoboardd",
                "holder",
                "--registry-dir",
                "/some dir/holders",
                "-x"
            ]
        );
        let mut options: Vec<String> = arguments[..split].to_vec();
        options.sort();
        let mut expected = vec![
            "--user".to_string(),
            "--quiet".to_string(),
            "--collect".to_string(),
            format!("--unit={UNIT}.service"),
            format!("--description={DESCRIPTION}"),
            "--property=KillMode=process".to_string(),
            "--property=Delegate=yes".to_string(),
            "--property=StandardOutput=null".to_string(),
            "--property=StandardError=null".to_string(),
            format!("--property=Before={DAEMON}"),
            format!("--property=WorkingDirectory={}", dir.display()),
        ];
        expected.sort();
        assert_eq!(options, expected);
    }

    /// Without a working directory of its own the command's unit is given none, and what the
    /// command's environment holds is not given to `systemd-run` either.
    #[test]
    fn a_unit_without_a_working_directory_is_asked_for_none_and_gets_no_environment() {
        let run = run_starter("unit-no-cwd", Setup::default());
        assert_eq!(run.answer, "ok");
        let arguments = run.arguments.expect("systemd-run ran");
        assert!(
            !arguments
                .iter()
                .any(|argument| argument.contains("WorkingDirectory")
                    || argument.starts_with("--working-directory")),
            "{arguments:?}"
        );
        assert!(
            !arguments.iter().any(|argument| argument.contains("s3cret")),
            "{arguments:?}"
        );
        assert_eq!(run.secret.as_deref(), Some("unset"));
    }

    /// A `systemd-run` that fails is an error that says what it said, and is not a time-out, which
    /// the caller reads as a unit that may have started.
    #[test]
    fn a_systemd_run_that_fails_is_an_error_with_what_it_said() {
        let run = run_starter(
            "unit-failure",
            Setup {
                exit: Some("3"),
                stderr: Some("Failed to connect to bus: No medium found\n"),
                ..Setup::default()
            },
        );
        assert!(run.arguments.is_some());
        let (kind, message) = run.answer.split_once('\n').expect("an error");
        assert_ne!(kind, "TimedOut");
        assert!(message.contains("Failed to connect to bus"), "{message}");
        assert!(message.contains('3'), "the exit status is named: {message}");
    }

    /// A `systemd-run` that cannot be started at all is an error of its own kind, that is no
    /// time-out either, so the holder is started detached instead.
    #[test]
    fn a_missing_systemd_run_is_an_error_that_is_no_time_out() {
        let run = run_starter(
            "unit-missing",
            Setup {
                absent: true,
                ..Setup::default()
            },
        );
        assert_eq!(run.arguments, None, "a systemd-run was found");
        let (kind, _) = run.answer.split_once('\n').expect("an error");
        assert_eq!(kind, "NotFound");
    }

    /// A `systemd-run` that does not return is ended, and the answer is a time-out. Gives it a
    /// short deadline instead of the production one.
    #[test]
    fn a_systemd_run_that_does_not_return_is_killed_and_times_out() {
        let dir = ScratchDir::new("unit-timeout");
        let hang = dir.join("hang");
        let run = run_starter(
            "unit-timeout-run",
            Setup {
                hang: Some(&hang),
                deadline: Some(Duration::from_millis(200)),
                ..Setup::default()
            },
        );
        let (kind, _) = run.answer.split_once('\n').expect("an error");
        assert_eq!(kind, "TimedOut");
        // On a loaded machine the deadline may pass before the stand-in has written its pid,
        // which is no fault of the code under test; where it has, that process is gone.
        if hang.exists() {
            assert_gone_by(wait_for_pid(&hang), Instant::now() + PATIENCE);
        }
    }

    /// Runs the service probe with the environment `variables` over this process's, as the
    /// process `INVOCATION_ID` and `SYSTEMD_EXEC_PID` name when `as_the_service` is set: `sh` sets
    /// the pid to its own and executes the probe, which keeps it.
    fn probed(label: &str, as_the_service: bool, variables: &[(&str, &str)]) -> String {
        let dir = ScratchDir::new(label);
        let result = dir.join("answer");
        let probe = test_helper::args("platform::proc::systemd::tests::a_service_unit_probe");
        let mut helper = if as_the_service {
            let mut sh = Command::new("/bin/sh");
            sh.arg("-c")
                .arg("SYSTEMD_EXEC_PID=$$ INVOCATION_ID=0123abcd exec \"$0\" \"$@\"")
                .arg(test_helper::program())
                .args(probe);
            sh
        } else {
            let mut direct = Command::new(test_helper::program());
            direct.args(probe);
            direct
        };
        helper
            .env(RESULT, &result)
            .envs(variables.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut helper = helper.spawn().expect("the test binary runs");
        assert!(finished(&mut helper).success());
        std::fs::read_to_string(&result).expect("the probe answered")
    }

    /// A process the environment shows to be the one systemd started is a user service exactly
    /// where its cgroup is a user service's: the answer is the cgroup's alone, which is none in a
    /// process that no user manager runs.
    #[test]
    fn the_environment_alone_does_not_make_a_user_service() {
        let answer = probed("unit-probe-env", true, &[]);
        let from_cgroup = format!("{:?}", user_service());
        assert_eq!(answer, from_cgroup);
    }

    /// A cgroup that is a user service's does not make one either where the environment names
    /// another process as the one systemd started.
    #[test]
    fn a_process_that_only_inherited_the_environment_is_no_user_service() {
        let answer = probed(
            "unit-probe-inherited",
            false,
            &[("INVOCATION_ID", "0123abcd"), ("SYSTEMD_EXEC_PID", "1")],
        );
        assert_eq!(answer, "None");
    }

    /// A daemon that is not service-managed is no user service.
    #[test]
    fn a_daemon_that_is_not_service_managed_is_no_user_service() {
        assert_eq!(user_service_unit(false), None);
    }
}
