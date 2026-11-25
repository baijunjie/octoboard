//! The Claude Code status line (`octoboardd status-line --provider claude`) the adapter injects.
//! Claude Code hands a status line its session's figures on stdin, the context window among them,
//! which its transcript never records; this posts the window to the daemon and runs the user's own
//! status line in its place.
//!
//! Everything the user sees has to be what their own status line would show: it is run with the
//! very bytes read from stdin, through a POSIX shell as Claude Code runs one on Unix, with the
//! environment and working directory Claude Code gave this process, with bounded output relayed
//! on the corresponding streams and its exit status returned. No status line of their own
//! prints nothing and exits 0. Which one is theirs is read from their settings files on every run,
//! so an edit made while the session runs is followed. The window is posted while that command
//! runs, so a daemon that is slow or gone delays nothing beyond the post's own short bound, and
//! nothing about the post is ever printed.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::adapter::StatusLine;
use crate::platform::proc::Reach;
use crate::platform::shell::{self, Dialect};
use crate::processes::subprocess::{run_bounded_input, Bounds};

/// Bound on each step of the post. The daemon answers before doing anything with the report, so
/// this is only ever reached by one that is wedged.
const TIMEOUT: Duration = Duration::from_millis(500);

/// A stuck or flooding command must not hold up the agent's status line.
const COMMAND_BOUNDS: Bounds = Bounds {
    timeout: Duration::from_secs(5),
    stdout_limit: 64 * 1024,
    stderr_limit: 64 * 1024,
    reach: Reach::Group,
    term_grace: Duration::ZERO,
};

fn command_bounds() -> Bounds {
    let bounds = COMMAND_BOUNDS;
    #[cfg(test)]
    let bounds = Bounds {
        timeout: crate::test_support::PATIENCE,
        ..bounds
    };
    bounds
}

/// Claude Code's status line, with the `--cwd` and `--config-dir` the injected command names.
pub struct ClaudeStatusLine;

impl StatusLine for ClaudeStatusLine {
    fn run(&self, session: &str, port: u16, key_file: &Path, args: &[String]) -> i32 {
        let value = |name: &str| {
            let at = args.iter().position(|arg| arg == name)?;
            args.get(at + 1).map(PathBuf::from)
        };
        // Without the directories there is no user status line to find; the window is still
        // reported.
        let dirs = value("--cwd").zip(value("--config-dir"));
        run(session, port, key_file, dirs.as_ref())
    }
}

/// Runs the status line, with the `(cwd, config_dir)` its user's own is looked for in, and
/// returns the exit status to leave with.
/// A key file that cannot be read is a daemon that refuses the post: nothing is reported.
fn run(session: &str, port: u16, key_file: &Path, dirs: Option<&(PathBuf, PathBuf)>) -> i32 {
    let mut input = Vec::new();
    // Claude Code writes the whole input and closes the pipe; what could be read is passed on.
    let _ = std::io::stdin().read_to_end(&mut input);
    let report = window_of(&input)
        .zip(crate::key_file::read(key_file))
        .map(|(window, key)| report(port, key, session, window));
    let status = match dirs.and_then(|(cwd, config_dir)| user_command(config_dir, cwd)) {
        Some(command) => pass_through(&command, input, std::io::stdout(), std::io::stderr()),
        None => 0,
    };
    if let Some(report) = report {
        let _ = report.join();
    }
    status
}

/// Posts `window` for `session` on a thread of its own, whose answer is of no interest.
fn report(port: u16, key: String, session: &str, window: u64) -> std::thread::JoinHandle<()> {
    let path = format!("/context-window/{session}");
    let body = serde_json::json!({ "window": window }).to_string();
    std::thread::spawn(move || {
        let _ = crate::loopback::post(port, &key, &path, body.as_bytes(), TIMEOUT);
    })
}

/// The context window Claude Code gave the status line, when its input carries one.
fn window_of(input: &[u8]) -> Option<u64> {
    let input: serde_json::Value = serde_json::from_slice(input).ok()?;
    input
        .get("context_window")?
        .get("context_window_size")?
        .as_u64()
        .filter(|window| *window > 0)
}

/// The command of the user's own status line, as the settings files under `config_dir` and `cwd`
/// give it now. Claude Code runs only a `command` status line.
fn user_command(config_dir: &Path, cwd: &Path) -> Option<String> {
    let line = super::settings::user_status_line(config_dir, cwd)?;
    (line.get("type")?.as_str()? == "command").then_some(())?;
    Some(line.get("command")?.as_str()?.to_owned())
}

/// Passes through the user's command output and exit code. An unsuccessful run returns 1.
fn pass_through(
    command: &str,
    input: Vec<u8>,
    mut stdout: impl Write,
    mut stderr: impl Write,
) -> i32 {
    let runner = shell::command_runner(Dialect::Posix);
    let mut shell = Command::new(&runner.program);
    shell.args(&runner.args).arg(command);
    let Ok(output) = run_bounded_input(&mut shell, &command_bounds(), input) else {
        return 1;
    };
    let _ = stdout.write_all(&output.stdout);
    let _ = stderr.write_all(&output.stderr);
    output.status.code().unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INPUT: &str = r#"{"session_id":"x","context_window":{"total_input_tokens":10,
        "total_output_tokens":2,"context_window_size":200000,"current_usage":null,
        "used_percentage":null,"remaining_percentage":null}}"#;

    #[test]
    fn the_window_is_read_from_the_input_and_a_missing_one_is_none() {
        assert_eq!(window_of(INPUT.as_bytes()), Some(200_000));
        for input in [
            r#"{"session_id":"x"}"#,
            r#"{"context_window":{"total_input_tokens":10}}"#,
            r#"{"context_window":{"context_window_size":null}}"#,
            r#"{"context_window":{"context_window_size":0}}"#,
            "not json",
            "",
        ] {
            assert_eq!(window_of(input.as_bytes()), None, "{input}");
        }
    }

    /// The user's command sees the same input and its output, byte for byte, and its exit status
    /// is the one returned.
    #[test]
    fn the_users_command_gets_the_same_input_and_its_output_and_status_pass_through() {
        let input = format!("{INPUT}\n").into_bytes();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            pass_through(
                "cat; printf '\\033[1m!'; printf diagnostic >&2; exit 7",
                input.clone(),
                &mut stdout,
                &mut stderr,
            ),
            7,
        );
        assert_eq!(stdout, [input.as_slice(), b"\x1b[1m!"].concat());
        assert_eq!(stderr, b"diagnostic");
    }

    #[test]
    fn reports_the_exit_status_even_when_the_command_ignores_input() {
        let sink = std::io::sink;
        assert_eq!(
            pass_through("exit 3", vec![b'x'; 1 << 20], sink(), sink()),
            3
        );
        #[cfg(unix)]
        assert_eq!(pass_through("kill -9 $$", Vec::new(), sink(), sink()), 1);
    }

    #[test]
    fn output_past_the_limit_is_refused_without_a_partial_status_line() {
        let mut stdout = Vec::new();
        let status = pass_through(
            &format!("printf '%0{}d' 0", COMMAND_BOUNDS.stdout_limit + 1),
            Vec::new(),
            &mut stdout,
            std::io::sink(),
        );
        assert_eq!(status, 1);
        assert!(stdout.is_empty());
    }

    /// The local settings outrank the project's, which outrank the user's; an object is merged key
    /// by key, and anything but a `command` status line runs nothing.
    #[test]
    fn the_users_command_is_the_one_their_settings_give_now() {
        let dir = crate::test_support::ScratchDir::new("status-line-resolution");
        let (user, cwd) = (dir.join("user"), dir.join("project"));
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(cwd.join(".claude")).unwrap();
        let write = |path: std::path::PathBuf, line: serde_json::Value| {
            std::fs::write(path, serde_json::json!({ "statusLine": line }).to_string()).unwrap();
        };
        assert_eq!(user_command(&user, &cwd), None);

        write(
            user.join("settings.json"),
            serde_json::json!({"type":"command","command":"user","padding":2}),
        );
        assert_eq!(user_command(&user, &cwd).as_deref(), Some("user"));
        write(
            cwd.join(".claude/settings.json"),
            serde_json::json!({"type":"command","command":"project"}),
        );
        assert_eq!(user_command(&user, &cwd).as_deref(), Some("project"));
        write(
            cwd.join(".claude/settings.local.json"),
            serde_json::json!({"refreshInterval":5}),
        );
        assert_eq!(user_command(&user, &cwd).as_deref(), Some("project"));
        let merged = crate::adapter::claude::settings::user_status_line(&user, &cwd).unwrap();
        assert_eq!(
            serde_json::Value::Object(merged),
            serde_json::json!({"type":"command","command":"project","padding":2,
                "refreshInterval":5})
        );
        write(
            cwd.join(".claude/settings.local.json"),
            serde_json::json!({"type":"command","command":"local"}),
        );
        assert_eq!(user_command(&user, &cwd).as_deref(), Some("local"));

        // A broken file counts for nothing; a status line of another type runs nothing.
        std::fs::write(cwd.join(".claude/settings.local.json"), "{").unwrap();
        assert_eq!(user_command(&user, &cwd).as_deref(), Some("project"));
        write(
            cwd.join(".claude/settings.local.json"),
            serde_json::json!({"type":"other"}),
        );
        assert_eq!(user_command(&user, &cwd), None);
    }

    /// The window reaches the daemon's route for the session as the body the route reads, and a
    /// daemon that is not there costs nothing.
    #[test]
    fn the_window_is_posted_for_the_session() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut stream = crate::test_support::accept_past_probes(&listener);
            stream.set_read_timeout(Some(TIMEOUT)).unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !request.ends_with(b"{\"window\":200000}") {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "the request ended early");
                request.extend_from_slice(&buffer[..read]);
            }
            stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
            String::from_utf8(request).unwrap()
        });
        // A running daemon may probe the port at any point: the probe is not the report.
        assert_eq!(
            crate::test_support::probe_port(port),
            "HTTP/1.1 404 Not Found"
        );
        report(port, "k".to_owned(), "session-1", 200_000)
            .join()
            .unwrap();
        assert!(server
            .join()
            .unwrap()
            .starts_with("POST /context-window/session-1?key=k HTTP/1.1\r\n"));

        let gone = crate::test_support::ClosedPort::new();
        let started = std::time::Instant::now();
        report(gone.port(), "k".to_owned(), "session-1", 200_000)
            .join()
            .unwrap();
        // Bounded by the timeout, which a refused connection on Windows takes up in full.
        assert!(started.elapsed() < TIMEOUT * 4);
    }
}
