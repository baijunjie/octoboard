//! `octoboardd status-line` — the status line command the Claude Code adapter injects. Claude Code
//! hands a status line its session's figures on stdin, the context window among them, which its
//! transcript never records; this posts the window to the daemon and runs the user's own status
//! line in its place.
//!
//! Everything the user sees has to be what their own status line would show: it is run with the
//! very bytes read from stdin, through `/bin/sh -c` as Claude Code runs one on Unix, with the
//! environment and working directory Claude Code gave this process, its stdout and stderr this
//! process's own, and its exit status returned as this process's. No status line of their own
//! prints nothing and exits 0. Which one is theirs is read from their settings files on every run,
//! so an edit made while the session runs is followed. The window is posted while that command
//! runs, so a daemon that is slow or gone delays nothing beyond the post's own short bound, and
//! nothing about the post is ever printed.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Bound on each step of the post. The daemon answers before doing anything with the report, so
/// this is only ever reached by one that is wedged.
const TIMEOUT: Duration = Duration::from_millis(500);

/// Runs the status line and returns the exit status to leave with.
pub fn run(session: &str, port: u16, cwd: &Path, config_dir: &Path) -> i32 {
    let mut input = Vec::new();
    // Claude Code writes the whole input and closes the pipe; what could be read is passed on.
    let _ = std::io::stdin().read_to_end(&mut input);
    let report = window_of(&input).map(|window| report(port, session, window));
    let status = match user_command(config_dir, cwd) {
        Some(command) => pass_through(&command, &input, Stdio::inherit()),
        None => 0,
    };
    if let Some(report) = report {
        let _ = report.join();
    }
    status
}

/// Posts `window` for `session` on a thread of its own, whose answer is of no interest.
fn report(port: u16, session: &str, window: u64) -> std::thread::JoinHandle<()> {
    let path = format!("/context-window/{session}");
    let body = serde_json::json!({ "window": window }).to_string();
    std::thread::spawn(move || {
        let _ = crate::loopback::post(port, &path, body.as_bytes(), TIMEOUT);
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
    let line = crate::adapter::claude::settings::user_status_line(config_dir, cwd)?;
    (line.get("type")?.as_str()? == "command").then_some(())?;
    Some(line.get("command")?.as_str()?.to_owned())
}

/// Runs `command` through the shell with `input` on its stdin and `stdout` as its stdout, and
/// returns its exit status: 1 when it was ended by a signal or could not be started, which is what
/// Claude Code makes of either.
fn pass_through(command: &str, input: &[u8], stdout: Stdio) -> i32 {
    let Ok(mut child) = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(stdout)
        .stderr(Stdio::inherit())
        .spawn()
    else {
        return 1;
    };
    if let Some(mut stdin) = child.stdin.take() {
        // A command that exits without reading its input closes the pipe early; that is its
        // business, and it is still waited for.
        let _ = stdin.write_all(input);
    }
    child
        .wait()
        .ok()
        .and_then(|status| status.code())
        .unwrap_or(1)
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
        let dir = crate::test_support::ScratchDir::new("status-line-pass-through");
        let output = dir.join("output");
        let input = format!("{INPUT}\n");
        let file = || std::fs::File::create(&output).unwrap();
        assert_eq!(
            pass_through("cat; printf '\\033[1m!'", input.as_bytes(), file().into()),
            0
        );
        assert_eq!(
            std::fs::read(&output).unwrap(),
            [input.as_bytes(), b"\x1b[1m!"].concat()
        );
        assert_eq!(pass_through("echo partial; exit 3", b"", file().into()), 3);
        assert_eq!(std::fs::read(&output).unwrap(), b"partial\n");
        // One that never reads its input is not held up by it.
        let large = vec![b'x'; 1 << 20];
        assert_eq!(pass_through("exit 0", &large, file().into()), 0);
        assert_eq!(pass_through("kill -9 $$", b"", file().into()), 1);
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
            let (mut stream, _) = listener.accept().unwrap();
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
        report(port, "session-1", 200_000).join().unwrap();
        assert!(server
            .join()
            .unwrap()
            .starts_with("POST /context-window/session-1 HTTP/1.1\r\n"));

        let gone = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = gone.local_addr().unwrap().port();
        drop(gone);
        let started = std::time::Instant::now();
        report(port, "session-1", 200_000).join().unwrap();
        assert!(started.elapsed() < TIMEOUT);
    }
}
