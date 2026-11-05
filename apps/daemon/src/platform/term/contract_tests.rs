//! What every host's terminal backend must do, written once against the neutral signatures. The
//! process each test starts on a terminal is this test binary run again as a helper (see
//! [`a_terminal_helper`] and `platform::test_helper`), so that no test needs a program of the
//! host's own.

use std::io::BufRead;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;
use crate::platform::proc::{ChildHandle, Polite};
use crate::platform::test_helper::{self, PID_FILE, ROLE};
use crate::test_support::{KillOnDrop, ScratchDir, PATIENCE};

/// What the helper prints once it is set up and reading.
const READY: &str = "helper-ready";

/// The process a test starts on a terminal: run as `ROLE` says, and a no-op in an ordinary test
/// run.
///
/// - `echo`: prints each line it reads back, escaped, after `got:`.
/// - `size`: prints the terminal's size for each line it reads, after `size:`.
/// - `park`: waits to be ended.
/// - `park_deaf`: waits to be ended, and takes neither a polite request to terminate nor a
///   hang-up.
/// - `quit_keys`: reads its input key by key and, at the second Ctrl-C, writes `quit` to the file
///   `PID_FILE` names and exits.
#[test]
fn a_terminal_helper() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    if role == "park_deaf" {
        ignore_polite_requests();
    }
    read_terminal_input(role == "quit_keys");
    println!("{READY}");
    let stdin = std::io::stdin();
    match role.as_str() {
        "echo" => {
            for line in stdin.lock().lines() {
                println!("got:{:?}", line.expect("a line"));
            }
        }
        "size" => {
            for line in stdin.lock().lines() {
                line.expect("a line");
                println!("size:{}", terminal_size());
            }
        }
        "park" | "park_deaf" => std::thread::sleep(PATIENCE * 2),
        "quit_keys" => {
            let marker = std::env::var(PID_FILE).expect("the marker's path");
            let mut presses = 0;
            let mut key = [0u8; 1];
            let mut stdin = stdin.lock();
            while std::io::Read::read(&mut stdin, &mut key).is_ok_and(|n| n == 1) {
                if key[0] == 0x03 {
                    presses += 1;
                    if presses == 2 {
                        std::fs::write(&marker, "quit").expect("the marker is written");
                        return;
                    }
                }
            }
        }
        other => panic!("unknown role {other}"),
    }
}

/// The size of the terminal the helper's stdin is, as `<rows>x<cols>`.
#[cfg(unix)]
fn terminal_size() -> String {
    // SAFETY: a zeroed `winsize` is a valid buffer for the request to fill in.
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: the request only fills in `size`, through the helper's own stdin.
    if unsafe { libc::ioctl(0, libc::TIOCGWINSZ, &mut size) } != 0 {
        return "unknown".into();
    }
    format!("{}x{}", size.ws_row, size.ws_col)
}

#[cfg(windows)]
fn terminal_size() -> String {
    use windows_sys::Win32::System::Console::{
        GetConsoleScreenBufferInfo, GetStdHandle, CONSOLE_SCREEN_BUFFER_INFO, STD_OUTPUT_HANDLE,
    };
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    // SAFETY: the call only fills in `info`, from the helper's own console.
    if unsafe { GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &mut info) } == 0 {
        return "unknown".into();
    }
    let window = info.srWindow;
    format!(
        "{}x{}",
        window.Bottom - window.Top + 1,
        window.Right - window.Left + 1
    )
}

/// What a helper that takes neither a polite request to terminate nor a hang-up does to ask for
/// it.
#[cfg(unix)]
fn ignore_polite_requests() {
    // SAFETY: ignoring signals in a process of the test's own.
    unsafe {
        libc::signal(libc::SIGTERM, libc::SIG_IGN);
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }
}

/// On Windows the polite request is the close of the console, which a process can hold off only
/// for as long as its handler runs, and only until the host gives up on it.
#[cfg(windows)]
fn ignore_polite_requests() {
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

    unsafe extern "system" fn hold(_event: u32) -> windows_sys::core::BOOL {
        std::thread::sleep(PATIENCE * 2);
        1
    }
    // SAFETY: installs a handler of the helper's own.
    unsafe { SetConsoleCtrlHandler(Some(hold), 1) };
}

/// Has the helper read its console's input as the terminal sends it, escape sequences included,
/// as the agents do, and with `raw` key by key, a Ctrl-C included, rather than line by line.
#[cfg(windows)]
fn read_terminal_input(raw: bool) {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
        ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_INPUT, STD_INPUT_HANDLE,
    };
    let mut mode = 0;
    // SAFETY: plain calls on the helper's own console input.
    unsafe {
        let input = GetStdHandle(STD_INPUT_HANDLE);
        if GetConsoleMode(input, &mut mode) != 0 {
            if raw {
                mode &= !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT);
            }
            SetConsoleMode(input, mode | ENABLE_VIRTUAL_TERMINAL_INPUT);
        }
    }
}

/// Nothing to set: a Unix host presses no quit keys, so the helper's line-by-line input serves.
#[cfg(unix)]
fn read_terminal_input(_raw: bool) {}

/// A helper running as `role` on a terminal of `size`, with what its reader has read so far, and
/// a guard that kills it if the test ends without having.
struct Helper {
    spawned: Spawned,
    output: Arc<Mutex<Vec<u8>>>,
    /// Sent to once the reader has returned, with what its last read returned.
    reader_done: mpsc::Receiver<io::Result<Option<usize>>>,
    guard: KillOnDrop,
}

fn start(role: &str, size: Size) -> Helper {
    start_with(role, size, |_| {})
}

/// [`start`] with `configure` having its say on the helper's command first.
fn start_with(role: &str, size: Size, configure: impl FnOnce(&mut Command)) -> Helper {
    let mut command = Command::new(test_helper::program());
    command
        .args(test_helper::args(
            "platform::term::contract_tests::a_terminal_helper",
        ))
        .env(ROLE, role);
    configure(&mut command);
    let spawned = spawn(command, size, None).expect("the helper starts on a terminal");
    let guard = KillOnDrop(vec![spawned.pid.0 as i32]);
    let output = Arc::new(Mutex::new(Vec::new()));
    let (done, reader_done) = mpsc::channel();
    let reader = spawned.pty.reader();
    let read_into = output.clone();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 4096];
        let last = loop {
            match reader.read_chunk(&mut buf) {
                Ok(Some(n)) if n > 0 => read_into.lock().unwrap().extend_from_slice(&buf[..n]),
                other => break other,
            }
        };
        done.send(last).ok();
    });
    let helper = Helper {
        spawned,
        output,
        reader_done,
        guard,
    };
    helper.wait_for(READY);
    helper
}

impl Helper {
    /// Waits until the output holds `needle`, for up to [`PATIENCE`].
    fn wait_for(&self, needle: &str) {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let output = String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned();
            if output.contains(needle) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{needle:?} never appeared in {output:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Waits for the helper to exit, then reaps it.
    fn reap(mut self) {
        reap(&self.spawned.child);
        self.guard.0.clear();
    }
}

/// Waits until `child` has exited, then reaps it.
fn reap(child: &ChildHandle) {
    let deadline = Instant::now() + PATIENCE;
    while !child.reap_if_exited().expect("the child is reaped") {
        assert!(Instant::now() < deadline, "the child kept running");
        std::thread::sleep(Duration::from_millis(20));
    }
}

const SIZE: Size = Size { rows: 24, cols: 80 };

/// The daemon's terminal tests are written for the bundled console host, whose output is the
/// program's own bytes. Without its files beside the test binary the system's host runs instead and
/// dozens of them fail unrelated to what they test; this one says why first.
#[cfg(windows)]
#[test]
fn the_tests_run_on_the_bundled_console_host() {
    assert!(
        facts().host_waits_on_queries,
        "the pseudoconsoles run on the system's console host: run `node scripts/fetch-conpty.mjs` to \
         place the bundled conpty.dll and OpenConsole.exe beside the test binary"
    );
}

/// What the host writes before the process prints is [`host_preamble`], and where the output is
/// the process's own bytes, all of it: after it, the output up to the process's first line holds
/// nothing of the host's.
#[test]
fn the_host_writes_only_its_preamble_before_the_process_prints() {
    let helper = start("park", SIZE);
    let output = helper.output.lock().unwrap().clone();
    let printed = output
        .strip_prefix(host_preamble())
        .unwrap_or_else(|| panic!("{output:?} starts with the preamble"));
    if !facts().host_renders {
        let ready = printed
            .windows(READY.len())
            .position(|window| window == READY.as_bytes())
            .expect("the helper's line");
        assert!(!printed[..ready].contains(&0x1b), "{output:?}");
    }

    assert!(helper.spawned.child.kill().delivered());
    helper.reap();
}

/// What is written reaches the process on the terminal, raw bytes as they are and the daemon's
/// inputs as the terminal sends them, and what it prints reaches the reader.
#[test]
fn a_process_reads_what_is_written_and_its_output_is_read() {
    let helper = start("echo", SIZE);
    let pty = &helper.spawned.pty;
    let expected = if facts().foreground_job {
        Foreground::Own
    } else {
        Foreground::Unsupported
    };
    assert_eq!(pty.foreground_job(), expected);

    pty.write_raw(b"raw bytes\r").expect("the write");
    helper.wait_for(r#"got:"raw bytes""#);
    pty.write_input(&[Input::Text("typed".into()), Input::Enter])
        .expect("the write");
    helper.wait_for(r#"got:"typed""#);
    pty.write_input(&[Input::Paste("pasted".into()), Input::Enter])
        .expect("the write");
    helper.wait_for(r#"got:"\u{1b}[200~pasted\u{1b}[201~""#);

    assert!(helper.spawned.child.kill().delivered());
    helper.reap();
}

/// A Ctrl-C written into the terminal interrupts the program on it, as the daemon's own key and as
/// a client's keystroke alike: the helper takes Ctrl-C as a program does by default, and ends.
#[test]
fn a_ctrl_c_interrupts_the_program_on_the_terminal() {
    let helper = start("park", SIZE);
    helper
        .spawned
        .pty
        .write_input(&[Input::Ctrl('c')])
        .expect("the write");
    helper.reap();

    let helper = start("park", SIZE);
    helper.spawned.pty.write_raw(b"\x03").expect("the write");
    helper.reap();
}

/// A resize reaches the process on the terminal, and the terminal reports its new size.
#[test]
fn a_resize_reaches_the_process() {
    let helper = start("size", SIZE);
    let pty = &helper.spawned.pty;
    assert_eq!(pty.size().expect("the size"), SIZE);
    pty.write_input(&[Input::Enter]).expect("the write");
    helper.wait_for("size:24x80");

    let resized = Size {
        rows: 30,
        cols: 100,
    };
    pty.resize(resized).expect("the resize");
    assert_eq!(pty.size().expect("the size"), resized);
    pty.write_input(&[Input::Enter]).expect("the write");
    helper.wait_for("size:30x100");

    assert!(helper.spawned.child.kill().delivered());
    helper.reap();
}

/// A hang-up ends the reader within a bound even while the process ignores it and runs on,
/// holding the terminal open, and every write after it fails.
#[test]
fn a_hang_up_ends_the_reader_of_a_process_still_running() {
    let helper = start("park_deaf", SIZE);
    helper.spawned.pty.hang_up();
    let last = helper
        .reader_done
        .recv_timeout(PATIENCE)
        .expect("the reader returned after the hang-up");
    assert!(
        matches!(last, Ok(None | Some(0))),
        "the reader ended with {last:?}"
    );
    assert!(helper.spawned.pty.write_raw(b"x").is_err());
    assert!(helper.spawned.pty.write_input(&[Input::Enter]).is_err());
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(helper.spawned.child.exited_unreaped(), Some(false));

    assert!(helper.spawned.child.kill().delivered());
    helper.reap();
}

/// A polite request ends a process on a terminal that takes it; one that does not take it is
/// left running by the request and ends with the kill.
#[test]
fn a_polite_stop_is_followed_by_a_kill() {
    let polite = start("park", SIZE);
    assert!(polite.spawned.child.stop(Polite::Terminate).delivered());
    polite.reap();

    let deaf = start("park_deaf", SIZE);
    assert!(deaf.spawned.child.stop(Polite::Terminate).delivered());
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(deaf.spawned.child.exited_unreaped(), Some(false));
    assert!(deaf.spawned.child.kill().delivered());
    deaf.reap();
}

/// A polite request to terminate a process that declared quit keys presses them first where the
/// host's own request would end the process without its shutdown work, and only there; the process
/// ends either way.
#[test]
fn a_polite_stop_presses_the_quit_keys_where_the_host_does() {
    let dir = ScratchDir::new("quit-keys");
    let marker = dir.join("quit");
    let keys = QuitKeys {
        key: &[Input::Ctrl('c')],
        presses: 2,
        apart: Duration::from_millis(200),
        wait: PATIENCE,
    };
    let helper = start_with("quit_keys", SIZE, |command| {
        command.env(PID_FILE, &marker).quit_keys(keys);
    });
    let presses = !helper
        .spawned
        .pty
        .polite_stop_lead(Polite::Terminate)
        .is_zero();
    assert!(helper.spawned.child.stop(Polite::Terminate).delivered());
    helper.reap();
    assert_eq!(marker.exists(), presses, "the keys were pressed: {presses}");
}
