//! The processes a Windows test starts where a Unix test has perl or a shell script start them: a
//! stand-in agent that starts commands in the background, and the commands, that write their pids,
//! wait for a file, listen, sleep, answer what they read and start others.
//!
//! Each is this test binary run as [`process`] over a list of steps, as a process of its own that
//! Windows starts directly: no shell and no perl between it and what started it, so the tree the
//! ledger reads is exactly the one the test describes, and every pid is the one Windows gives. A
//! step that starts another process starts it without anything that waits for it, so it outlives
//! the process that started it; without a console, as an agent's shell command starts its own,
//! unless the step says otherwise.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
use windows_sys::Win32::System::Threading::{ExitProcess, DETACHED_PROCESS};

use super::programs::Program;
use super::{is_running, kill, wait_for_pid, PATIENCE};

/// The environment variable that holds the pid of the process that started a fixture process.
const PARENT_VARIABLE: &str = "OCTOBOARDD_TEST_FIXTURE_PARENT";

/// The pid [`Op::EndChildOnClose`]'s handler ends, 0 until it is set.
static ENDED_ON_CLOSE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Whose pid [`Op::WritePid`] writes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum Whose {
    Own,
    /// The process the last [`Op::Spawn`] or [`Op::Run`] started.
    LastChild,
}

/// A step of a fixture process.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum Op {
    /// Writes the pid to `file` whole, through a rename, so a reader never reads half of it.
    WritePid {
        file: PathBuf,
        whose: Whose,
    },
    /// Waits until `file` exists.
    WaitFor {
        file: PathBuf,
    },
    Sleep {
        seconds: u64,
    },
    /// Starts a fixture process called `name`, working in `cwd` when given, as `start` says.
    Spawn {
        name: String,
        cwd: Option<PathBuf>,
        steps: Vec<Op>,
        start: Start,
    },
    /// Starts any program, with the variables of `env` set or, for `None`, removed.
    Run {
        program: PathBuf,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        env: Vec<(String, Option<String>)>,
    },
    /// Listens on a port of the loopback address, which it writes to `file` whole, until it ends.
    Listen {
        file: PathBuf,
    },
    /// Ends once the process that started it has.
    EndWithParent,
    /// Ends the last child it started, and itself, when its console closes or takes Ctrl-C: what
    /// a process does when it is asked to terminate, and takes a child with it.
    EndChildOnClose,
    /// Writes `text` and a line break to its output.
    Say {
        text: String,
    },
    /// Writes `text` to its output, with no line break.
    Print {
        text: String,
    },
    /// Outlives the close of its console for as long as the host lets a process, which is
    /// [`CLOSE_HELD`] at most: the console's close ends a process that does not.
    HoldOnClose,
    /// Reads its input until it closes, as an agent waiting for a message does.
    ReadInput,
    /// Reads a line of its input and writes `reply `, the line and a line break.
    Reply,
    /// Copies its input to its output, byte for byte, until the input closes, with its console's
    /// line editing, echo and Ctrl-C handling off: what `stty raw -echo; cat` is on a Unix
    /// terminal.
    EchoRaw,
    Exit,
}

/// How [`Op::Spawn`] starts its process.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum Start {
    /// Without a console and with nowhere to write: apart from the process starting it, as an
    /// agent's shell command starts its own.
    Detached,
    /// On the starting process's console, with nowhere to write: in its session, as a program an
    /// agent runs in the foreground is.
    OnConsole,
    /// On the starting process's console, with its standard streams: holding open whatever they
    /// are, as a background job of a shell does.
    WithStreams,
}

/// The steps of a fixture process, built as the test tells them.
#[derive(Debug, Clone, Default)]
pub(crate) struct Steps(Vec<Op>);

impl Steps {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Writes its own pid to `file`.
    pub(crate) fn write_pid(self, file: &Path) -> Self {
        self.with(Op::WritePid {
            file: file.into(),
            whose: Whose::Own,
        })
    }

    /// Writes the pid of the process it started last to `file`.
    pub(crate) fn write_child_pid(self, file: &Path) -> Self {
        self.with(Op::WritePid {
            file: file.into(),
            whose: Whose::LastChild,
        })
    }

    pub(crate) fn wait_for(self, file: &Path) -> Self {
        self.with(Op::WaitFor { file: file.into() })
    }

    pub(crate) fn sleep(self, seconds: u64) -> Self {
        self.with(Op::Sleep { seconds })
    }

    /// Starts `steps` as a process called `name`, which shows in its command line, apart from
    /// this one ([`Start::Detached`]).
    pub(crate) fn spawn(self, name: &str, steps: Steps) -> Self {
        self.spawn_in(None, name, steps)
    }

    /// [`Self::spawn`] working in `cwd`.
    pub(crate) fn spawn_in(self, cwd: Option<&Path>, name: &str, steps: Steps) -> Self {
        self.with(Op::Spawn {
            name: name.to_string(),
            cwd: cwd.map(Into::into),
            steps: steps.0,
            start: Start::Detached,
        })
    }

    /// [`Self::spawn`] on this process's console ([`Start::OnConsole`]).
    pub(crate) fn spawn_on_console(self, name: &str, steps: Steps) -> Self {
        self.with(Op::Spawn {
            name: name.to_string(),
            cwd: None,
            steps: steps.0,
            start: Start::OnConsole,
        })
    }

    /// [`Self::spawn`] on this process's console, with its standard streams
    /// ([`Start::WithStreams`]).
    pub(crate) fn spawn_with_streams(self, name: &str, steps: Steps) -> Self {
        self.with(Op::Spawn {
            name: name.to_string(),
            cwd: None,
            steps: steps.0,
            start: Start::WithStreams,
        })
    }

    /// Listens on the loopback address, writing the port to `file`.
    pub(crate) fn listen(self, file: &Path) -> Self {
        self.with(Op::Listen { file: file.into() })
    }

    pub(crate) fn end_with_parent(self) -> Self {
        self.with(Op::EndWithParent)
    }

    pub(crate) fn end_child_on_close(self) -> Self {
        self.with(Op::EndChildOnClose)
    }

    pub(crate) fn say(self, text: &str) -> Self {
        self.with(Op::Say {
            text: text.to_string(),
        })
    }

    pub(crate) fn print(self, text: &str) -> Self {
        self.with(Op::Print {
            text: text.to_string(),
        })
    }

    /// Holds on when its console closes, rather than ending: the host ends it after a grace.
    pub(crate) fn hold_on_close(self) -> Self {
        self.with(Op::HoldOnClose)
    }

    pub(crate) fn read_input(self) -> Self {
        self.with(Op::ReadInput)
    }

    pub(crate) fn reply(self) -> Self {
        self.with(Op::Reply)
    }

    pub(crate) fn echo_raw(self) -> Self {
        self.with(Op::EchoRaw)
    }

    pub(crate) fn exit(self) -> Self {
        self.with(Op::Exit)
    }

    fn with(mut self, op: Op) -> Self {
        self.0.push(op);
        self
    }

    /// The steps as a program called `name`, to start as a child or on a terminal.
    pub(crate) fn program(&self, name: &str) -> Program {
        let exe = std::env::current_exe().expect("the test binary");
        Program::of(exe, entry_args(name, &self.0))
    }
}

/// The arguments that run the test binary as [`process`] over `steps`. The name comes first, so
/// that a command line shown cut short, as a notice of what a restart ended shows one, still names
/// the process. The name and what follows `--`, which [`process`] reads, count as test names to
/// the harness, which none of them is. The harness is quiet: otherwise it writes the test's name
/// before running it, and the first line the steps write continues that one.
fn entry_args(name: &str, steps: &[Op]) -> Vec<String> {
    let steps = serde_json::to_string(steps).expect("the steps are JSON");
    [
        name,
        "--exact",
        "test_support::fixture::process",
        "--ignored",
        "--nocapture",
        "--quiet",
        "--test-threads=1",
        "--",
        &steps,
    ]
    .map(str::to_string)
    .to_vec()
}

/// Runs `program` with `args` in `cwd`, with `env` set or, for `None`, removed, as a process whose
/// parent has already exited, and answers its pid: what an agent's leftover is, which no
/// process of the test's holds. The process that starts it gets no pipe to write its answer to,
/// since a pipe it gave to the process would stay open until that ended: the answer is a file.
fn detached(cwd: &Path, program: &Path, args: &[&str], env: &[(&str, Option<&str>)]) -> i32 {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let pid_file = std::env::temp_dir().join(format!(
        "octoboardd-detached-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let steps = vec![
        Op::Run {
            program: program.into(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            cwd: Some(cwd.into()),
            env: env
                .iter()
                .map(|(key, value)| (key.to_string(), value.map(str::to_string)))
                .collect(),
        },
        Op::WritePid {
            file: pid_file.clone(),
            whose: Whose::LastChild,
        },
    ];
    let exe = std::env::current_exe().expect("the test binary");
    let status = Command::new(exe)
        .args(entry_args("starter", &steps))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("the starter ran");
    assert!(status.success(), "the starter failed: {status}");
    let pid = wait_for_pid(&pid_file);
    std::fs::remove_file(&pid_file).ok();
    pid
}

/// A process called `sleep` that sleeps `seconds`, working in `cwd`, whose parent has already
/// exited ([`detached`]).
pub(crate) fn detached_sleep(cwd: &Path, seconds: u32) -> i32 {
    let exe = std::env::current_exe().expect("the test binary");
    // Answered once the process runs, which is when it holds its directory: before, a directory
    // could still be removed from under it.
    let started = std::env::temp_dir().join(format!(
        "octoboardd-detached-sleep-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos())
    ));
    let steps = [
        Op::WritePid {
            file: started.clone(),
            whose: Whose::Own,
        },
        Op::Sleep {
            seconds: u64::from(seconds),
        },
    ];
    let args = entry_args("sleep", &steps);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    detached(cwd, &exe, &args, &[]);
    let pid = wait_for_pid(&started);
    std::fs::remove_file(&started).ok();
    pid
}

/// Runs the steps after `--` in the arguments. The tests that start a fixture process run it, so
/// that it is a test of this binary only by the harness's own account of it.
#[test]
#[ignore = "run as a process of its own by the tests that start fixture processes"]
fn process() {
    let args: Vec<String> = std::env::args().collect();
    let Some(at) = args.iter().position(|arg| arg == "--") else {
        return;
    };
    let steps: Vec<Op> =
        serde_json::from_str(args.get(at + 1).expect("the steps")).expect("the steps are JSON");
    run(&steps);
}

fn run(steps: &[Op]) {
    let mut child: Option<std::process::Child> = None;
    let mut listeners = Vec::new();
    for step in steps {
        match step {
            Op::WritePid { file, whose } => {
                let pid = match whose {
                    Whose::Own => std::process::id(),
                    Whose::LastChild => child.as_ref().expect("a child was started").id(),
                };
                let partial = file.with_extension("tmp");
                std::fs::write(&partial, pid.to_string()).expect("write the pid");
                std::fs::rename(&partial, file).expect("rename the pid file into place");
            }
            Op::WaitFor { file } => {
                while !file.exists() {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
            Op::Sleep { seconds } => std::thread::sleep(std::time::Duration::from_secs(*seconds)),
            Op::Spawn {
                name,
                cwd,
                steps,
                start: how,
            } => {
                let exe = std::env::current_exe().expect("the test binary");
                let mut command = Command::new(exe);
                command.args(entry_args(name, steps));
                child = Some(start(&mut command, cwd, *how));
            }
            Op::Run {
                program,
                args,
                cwd,
                env,
            } => {
                let mut command = Command::new(program);
                command.args(args);
                for (key, value) in env {
                    match value {
                        Some(value) => command.env(key, value),
                        None => command.env_remove(key),
                    };
                }
                child = Some(start(&mut command, cwd, Start::Detached));
            }
            Op::Listen { file } => {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listen");
                let port = listener.local_addr().expect("the address").port();
                let partial = file.with_extension("tmp");
                std::fs::write(&partial, port.to_string()).expect("write the port");
                std::fs::rename(&partial, file).expect("rename the port file into place");
                listeners.push(listener);
            }
            Op::EndWithParent => {
                let parent = std::env::var(PARENT_VARIABLE)
                    .ok()
                    .and_then(|pid| pid.parse().ok())
                    .expect("started by a fixture process");
                crate::platform::proc::watch_parent(parent, || std::process::exit(0));
            }
            Op::EndChildOnClose => {
                ENDED_ON_CLOSE.store(
                    child.as_ref().expect("a child was started").id(),
                    std::sync::atomic::Ordering::SeqCst,
                );
                // SAFETY: the handler is a function of this process that never returns.
                assert_ne!(
                    unsafe { SetConsoleCtrlHandler(Some(end_child_and_exit), 1) },
                    0
                );
            }
            Op::Say { text } => {
                println!("{text}");
            }
            Op::Print { text } => {
                use std::io::Write;
                let mut output = std::io::stdout().lock();
                output.write_all(text.as_bytes()).expect("write the text");
                output.flush().expect("flush the output");
            }
            Op::HoldOnClose => {
                // SAFETY: the handler is a function of this process.
                assert_ne!(unsafe { SetConsoleCtrlHandler(Some(hold_on), 1) }, 0);
            }
            Op::ReadInput => {
                let mut line = String::new();
                while std::io::stdin()
                    .read_line(&mut line)
                    .is_ok_and(|read| read > 0)
                {
                    line.clear();
                }
            }
            Op::Reply => {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).expect("read a line");
                println!("reply {}", line.trim_end_matches(['\r', '\n']));
            }
            Op::EchoRaw => echo_raw(),
            Op::Exit => std::process::exit(0),
        }
    }
}

/// Starts `command` as a process that outlives the one starting it, as `how` says: detached, with
/// no console, no input and nowhere to write, or on this process's console, with nothing of its
/// streams or with all of them.
fn start(command: &mut Command, cwd: &Option<PathBuf>, how: Start) -> std::process::Child {
    use std::os::windows::process::CommandExt;
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.env(PARENT_VARIABLE, std::process::id().to_string());
    if !matches!(how, Start::WithStreams) {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }
    if matches!(how, Start::Detached) {
        command.creation_flags(DETACHED_PROCESS);
    }
    command.spawn().expect("start the process")
}

/// What [`Op::EchoRaw`] runs.
fn echo_raw() {
    use std::io::{Read, Write};
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
        ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_INPUT, STD_INPUT_HANDLE,
    };
    // SAFETY: plain calls on this process's standard input, and `mode` outlives them.
    unsafe {
        let input = GetStdHandle(STD_INPUT_HANDLE);
        let mut mode = 0;
        assert_ne!(
            GetConsoleMode(input, &mut mode),
            0,
            "the input is a console"
        );
        let raw = (mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT))
            | ENABLE_VIRTUAL_TERMINAL_INPUT;
        assert_ne!(SetConsoleMode(input, raw), 0, "the input is made raw");
    }
    let (mut input, mut output) = (std::io::stdin().lock(), std::io::stdout().lock());
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let read = input.read(&mut buf).expect("read the input");
        if read == 0 {
            return;
        }
        output.write_all(&buf[..read]).expect("write the output");
        output.flush().expect("flush the output");
    }
}

/// How long [`Op::HoldOnClose`] holds on: past the grace the host gives a process whose console
/// closes, so that the host is what ends it.
const CLOSE_HELD: std::time::Duration = std::time::Duration::from_secs(60);

/// What [`Op::HoldOnClose`] runs: takes the event and does not return.
unsafe extern "system" fn hold_on(_event: u32) -> i32 {
    std::thread::sleep(CLOSE_HELD);
    1
}

/// What [`Op::EndChildOnClose`] runs: ends the child and waits for it to be gone, then exits.
unsafe extern "system" fn end_child_and_exit(_event: u32) -> i32 {
    let pid = ENDED_ON_CLOSE.load(std::sync::atomic::Ordering::SeqCst) as i32;
    kill(pid);
    let deadline = std::time::Instant::now() + PATIENCE;
    while is_running(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // SAFETY: a plain call that ends this process.
    unsafe { ExitProcess(0) }
}
