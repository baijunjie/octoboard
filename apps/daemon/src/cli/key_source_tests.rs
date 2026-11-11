//! Where each helper process of a session gets the key its requests carry, and what it does
//! without one. Arguments can be listed by every user of the machine, so `hook`, `status-line`
//! and `mcp` read the key from the launch's key file named by `--key-file`, and `git-ref` and
//! `gate-hook`, which git starts with the agent's environment, from the key file that environment
//! names; those two go to the daemon's own route, through the records in the data directory, when
//! the launch's route is gone.
//!
//! Each helper runs as a process of its own (this test binary, running only [`helper_child`])
//! against a stand-in daemon on a loopback port, which records what reached it: the helpers read
//! their stdin to its end, leave the process with an exit status, and `git-ref` leaves its session,
//! none of which a test may do to the test binary itself.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread::JoinHandle;

use clap::Parser;

use super::Cli;
use crate::test_support::{ClosedPort, ScratchDir, PATIENCE, TEST_KEY};

/// The environment variable the helper process takes its command line from, its arguments apart
/// by [`SEPARATOR`].
const ARGUMENTS: &str = "OCTOBOARD_TEST_HELPER_ARGUMENTS";
const SEPARATOR: char = '\u{1f}';
/// The variable the session's environment names the key file in, for `git-ref` and `gate-hook`.
const KEY_FILE_VARIABLE: &str = "OCTOBOARD_DAEMON_CREDENTIAL_PATH";

/// The key the daemon's own route takes in the tests of the git hook helpers: another key than
/// the launch's, [`TEST_KEY`].
const DAEMON_KEY: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

/// What the tests below start for `octoboardd <arguments>`: the mode runs as it does in the real
/// binary, and its exit status is the process's.
#[test]
#[ignore = "started by the tests below as the helper process"]
fn helper_child() {
    let arguments = std::env::var(ARGUMENTS).unwrap();
    let cli =
        Cli::try_parse_from(["octoboardd"].into_iter().chain(arguments.split(SEPARATOR))).unwrap();
    super::run(cli.command.expect("a mode")).unwrap();
}

/// One request the stand-in daemon received.
struct Request {
    /// `POST /hook/a?key=… HTTP/1.1`.
    line: String,
    body: String,
}

/// A loopback server that answers a request carrying `TEST_KEY` with `body` and any other with a
/// refusal, and records the requests until it is finished.
struct StandInDaemon {
    port: u16,
    thread: JoinHandle<Vec<Request>>,
}

impl StandInDaemon {
    fn start(body: &'static str) -> Self {
        Self::answering(TEST_KEY, "200 OK", body)
    }

    /// A server that answers a request carrying `key` with `status` and `body`, and any other with
    /// a refusal.
    fn answering(key: &'static str, status: &'static str, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let thread = std::thread::spawn(move || {
            let mut received = Vec::new();
            loop {
                let (stream, _) = listener.accept().unwrap();
                // A connection that sends nothing is the one that ends the server.
                let Some(request) = read_request(&stream) else {
                    return received;
                };
                let refused = !request.line.contains(&format!("?key={key} "));
                let (status, body) = if refused {
                    ("401 Unauthorized", "")
                } else {
                    (status, body)
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = (&stream).write_all(reply.as_bytes());
                received.push(request);
            }
        });
        Self { port, thread }
    }

    /// What reached the daemon, once everything that was going to has: call it after the helper
    /// that made the requests has ended, since a connection it opened is already waiting.
    fn finish(self) -> Vec<Request> {
        drop(TcpStream::connect(("127.0.0.1", self.port)).unwrap());
        self.thread.join().unwrap()
    }
}

/// The request on `stream`, or `None` for a connection that was closed without sending one.
fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok().filter(|read| *read > 0)?;
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap();
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        line: line.trim_end().to_owned(),
        body: String::from_utf8(body).unwrap(),
    })
}

/// The helper process for `octoboardd <arguments>`, with the key file variable set to `key_file` or
/// absent, and a home directory holding no data directory, so that no helper finds the
/// developer's own daemon.
fn helper(arguments: &[&str], key_file: Option<&str>) -> Command {
    helper_in(&crate::test_support::isolated_home(), arguments, key_file)
}

/// [`helper`] with `home` as its home directory, whose data directory may record a daemon.
fn helper_in(home: &Path, arguments: &[&str], key_file: Option<&str>) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "cli::key_source_tests::helper_child",
            "--ignored",
            "--nocapture",
        ])
        .env(ARGUMENTS, arguments.join(&SEPARATOR.to_string()))
        .env_remove(KEY_FILE_VARIABLE)
        .env_remove("REVERT_GATE_SKIP")
        .env("HOME", home)
        .env("USERPROFILE", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(key_file) = key_file {
        command.env(KEY_FILE_VARIABLE, key_file);
    }
    command
}

/// Runs `command` with `input` on its stdin, and returns what it left and whether all of the input
/// was taken: a helper that ended without reading it leaves the writer with a broken pipe.
fn run(mut command: Command, input: &[u8]) -> (Output, bool) {
    let mut child = command.spawn().unwrap();
    let written = child.stdin.take().unwrap().write_all(input).is_ok();
    (child.wait_with_output().unwrap(), written)
}

fn text(output: &[u8]) -> String {
    String::from_utf8_lossy(output).into_owned()
}

/// A key file in `dir` holding `content`, or the path of one that is not there.
fn key_file(dir: &Path, name: &str, content: Option<&str>) -> String {
    let path = dir.join(name);
    if let Some(content) = content {
        std::fs::write(&path, content).unwrap();
    }
    path.to_string_lossy().into_owned()
}

/// The values of the key file variable the helpers are run with, and whether each is a key: a file
/// holding the key, no variable at all, an empty variable, and a path that is a missing file, a
/// directory, or an empty or blank file.
fn key_file_cases(dir: &Path) -> [(Option<String>, bool); 7] {
    [
        (Some(key_file(dir, "key", Some(TEST_KEY))), true),
        (None, false),
        (Some(String::new()), false),
        (Some(dir.to_string_lossy().into_owned()), false),
        (Some(key_file(dir, "absent", None)), false),
        (Some(key_file(dir, "empty", Some(""))), false),
        (Some(key_file(dir, "blank", Some("\n"))), false),
    ]
}

#[test]
fn git_ref_posts_under_the_key_in_the_file_its_environment_names_and_posts_nothing_without_one() {
    let dir = ScratchDir::new("key-source-git-ref");
    for (file, posts) in key_file_cases(&dir) {
        let daemon = StandInDaemon::start("{}");
        let port = daemon.port.to_string();
        let (output, _) = run(
            helper(
                &["git-ref", "--session", "a", "--port", &port],
                file.as_deref(),
            ),
            b"",
        );
        assert!(
            output.status.success(),
            "{file:?}: {}",
            text(&output.stderr)
        );
        let requests = daemon.finish();
        let lines: Vec<&str> = requests.iter().map(|r| r.line.as_str()).collect();
        if posts {
            assert_eq!(
                lines,
                [format!("POST /git-ref/a?key={TEST_KEY} HTTP/1.1")],
                "{file:?}"
            );
        } else {
            assert!(lines.is_empty(), "{file:?}: {lines:?}");
            assert_eq!(text(&output.stderr), "", "{file:?}");
        }
    }
}

#[test]
fn gate_hook_asks_under_the_key_in_the_file_its_environment_names_and_lets_the_update_pass_without_one(
) {
    let dir = ScratchDir::new("key-source-gate-hook");
    for (file, asks) in key_file_cases(&dir) {
        // The daemon saying to skip the gate, which is a pass as well; what differs is whether it
        // was asked.
        let daemon = StandInDaemon::start("null");
        let port = daemon.port.to_string();
        let (output, _) = run(
            helper(
                &["gate-hook", "--session", "a", "--port", &port, "pre-push"],
                file.as_deref(),
            ),
            b"refs/heads/main 1 refs/heads/main 0\n",
        );
        assert_eq!(output.status.code(), Some(0), "{file:?}");
        assert_eq!(text(&output.stderr), "", "{file:?}");
        let requests = daemon.finish();
        let lines: Vec<&str> = requests.iter().map(|r| r.line.as_str()).collect();
        if asks {
            assert_eq!(
                lines,
                [format!("POST /revert-gate/a?key={TEST_KEY} HTTP/1.1")],
                "{file:?}"
            );
        } else {
            assert!(lines.is_empty(), "{file:?}: {lines:?}");
        }
    }
}

/// The launch's route a git hook helper is given in [`git_hook_route_cases`].
#[derive(Clone, Copy, Debug)]
enum Launch {
    /// A port that takes the key in the key file.
    Accepting,
    /// A port that refuses the key in the key file.
    Refusing,
    /// A holder answering that it has no daemon, or that it lost the one it had.
    Holder(&'static str),
    /// A port nothing listens on.
    Closed,
    /// No port at all.
    NoPort,
}

/// What the data directory says of the daemon in [`git_hook_route_cases`].
#[derive(Clone, Copy, Debug)]
enum Record {
    /// The stand-in daemon, running as this very process, with its key.
    Running,
    /// A daemon whose process is gone.
    Dead,
    /// Nothing.
    None,
}

/// Each case of the git hook helpers' routes: the launch's route, whether the key file holds a key,
/// what the data directory records, and how many requests reach the launch's route and the
/// daemon's.
const GIT_HOOK_ROUTE_CASES: [(Launch, bool, Record, usize, usize); 9] = [
    (Launch::Accepting, true, Record::Running, 1, 0),
    (Launch::Accepting, false, Record::Running, 0, 1),
    (Launch::Refusing, true, Record::Running, 1, 1),
    (Launch::Closed, true, Record::Running, 0, 1),
    (Launch::NoPort, false, Record::Running, 0, 1),
    (
        Launch::Holder("503 Service Unavailable"),
        true,
        Record::Running,
        1,
        0,
    ),
    (
        Launch::Holder("502 Bad Gateway"),
        true,
        Record::Running,
        1,
        0,
    ),
    (Launch::Accepting, false, Record::None, 0, 0),
    (Launch::Closed, true, Record::Dead, 0, 0),
];

/// Runs `mode` (its arguments after the session and port) with `input` once per case of
/// [`GIT_HOOK_ROUTE_CASES`], against stand-ins for the launch's route and the daemon that answer
/// with `body`, and checks which of them each request reached; `check` gets each case's output.
fn git_hook_route_cases(
    mode: &str,
    rest: &[&str],
    input: &[u8],
    body: &'static str,
    check: impl Fn(&str, &Output),
) {
    let dir = ScratchDir::new("key-source-git-hook-routes");
    for (n, (launch, keyed, record, to_launch, to_daemon)) in
        GIT_HOOK_ROUTE_CASES.into_iter().enumerate()
    {
        let case = format!("{launch:?}, key {keyed}, record {record:?}");
        let daemon = StandInDaemon::answering(DAEMON_KEY, "200 OK", body);
        let home = dir.join(format!("home-{n}"));
        let data = home.join(".octoboard");
        let running = crate::owner_record::Owner::current(daemon.port, false, true)
            .expect("this process's identity");
        match record {
            Record::Running => running.record_in(&data, DAEMON_KEY).unwrap(),
            Record::Dead => {
                let mut dead = running;
                dead.identity.start_micros = dead.identity.start_micros.wrapping_add(1);
                dead.record_in(&data, DAEMON_KEY).unwrap();
            }
            Record::None => {}
        }
        let stand_in = match launch {
            Launch::Accepting => Some(StandInDaemon::start(body)),
            Launch::Refusing => Some(StandInDaemon::answering("not-this-key", "200 OK", body)),
            Launch::Holder(status) => Some(StandInDaemon::answering(TEST_KEY, status, "")),
            Launch::Closed | Launch::NoPort => None,
        };
        // Held until the case is over, so that no other test is given the port.
        let closed = ClosedPort::new();
        let port = match (launch, &stand_in) {
            (_, Some(stand_in)) => Some(stand_in.port.to_string()),
            (Launch::Closed, None) => Some(closed.port.to_string()),
            _ => None,
        };
        let mut arguments = vec![mode, "--session", "a"];
        if let Some(port) = &port {
            arguments.extend(["--port", port.as_str()]);
        }
        arguments.extend(rest);
        let file = key_file(&dir, &format!("key-{n}"), keyed.then_some(TEST_KEY));
        let (output, _) = run(helper_in(&home, &arguments, Some(&file)), input);
        check(&case, &output);
        let at_launch = stand_in.map(StandInDaemon::finish).unwrap_or_default();
        let at_daemon = daemon.finish();
        assert_eq!(at_launch.len(), to_launch, "{case}: the launch's route");
        assert_eq!(at_daemon.len(), to_daemon, "{case}: the daemon's route");
        for request in &at_launch {
            assert!(
                !request.line.contains(DAEMON_KEY),
                "{case}: {}",
                request.line
            );
        }
        for request in &at_daemon {
            assert!(
                request.line.contains(&format!("?key={DAEMON_KEY} ")),
                "{case}: {}",
                request.line
            );
        }
    }
}

#[test]
fn git_ref_reports_to_the_daemons_record_only_when_the_launchs_route_is_gone() {
    git_hook_route_cases("git-ref", &[], b"", "{}", |case, output| {
        assert!(output.status.success(), "{case}: {}", text(&output.stderr));
    });
}

#[test]
fn gate_hook_asks_the_daemons_record_only_when_the_launchs_route_is_gone() {
    git_hook_route_cases(
        "gate-hook",
        &["pre-push"],
        b"refs/heads/main 1 refs/heads/main 0\n",
        "null",
        |case, output| {
            assert_eq!(output.status.code(), Some(0), "{case}");
            // A holder with no daemon is a daemon that is down, which says nothing; so is having
            // no route at all.
            assert_eq!(text(&output.stderr), "", "{case}");
        },
    );
}

/// A job a session left running after it ended: its scratch directory and key file are gone, and
/// nothing listens on the port its environment names. Whatever state the ended session is in, the
/// branch the job's git creates is still recorded as the session's, through the daemon's own route.
#[test]
fn a_branch_a_job_of_an_ended_session_creates_is_still_recorded_for_the_session() {
    use crate::protocol::SessionStatus;
    use crate::session_git_hooks::fixture::{session_record, Daemon, Fixture};

    for status in [SessionStatus::Archived, SessionStatus::Interrupted] {
        let fixture = Fixture::new();
        let mut ended = session_record("a", "project");
        ended.status = status;
        ended.ended_at = Some(crate::time::now_millis());
        fixture.state.store.update_session(&ended).unwrap();
        let oid = String::from_utf8(crate::test_support::git(
            &fixture.repo,
            &["rev-parse", "HEAD"],
        ))
        .unwrap()
        .trim()
        .to_string();
        crate::test_support::git(&fixture.repo, &["branch", "topic"]);
        let daemon = Daemon::start(&fixture.state);
        let home = fixture.dir.join("home");
        crate::owner_record::Owner::current(daemon.port, false, true)
            .expect("this process's identity")
            .record_in(&home.join(".octoboard"), TEST_KEY)
            .unwrap();
        let closed_port = ClosedPort::new();
        let closed = closed_port.port.to_string();
        let gone = key_file(&fixture.dir, "run-a-key", None);
        let mut command = helper_in(
            &home,
            &[
                "git-ref",
                "--session",
                "a",
                "--port",
                &closed,
                "--",
                "refs/heads/topic",
                &oid,
            ],
            Some(&gone),
        );
        command.current_dir(&*fixture.repo);

        let (output, _) = run(command, b"");

        assert!(
            output.status.success(),
            "{status:?}: {}",
            text(&output.stderr)
        );
        let deadline = std::time::Instant::now() + PATIENCE;
        while fixture.responsible("topic").is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "{status:?}: the branch was never recorded"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            fixture.responsible("topic").as_deref(),
            Some("a"),
            "{status:?}"
        );
    }
}

/// A session an earlier build launched names its key file in `OCTOBOARD_DAEMON_KEY_FILE` and always
/// gives the helpers a port: the newer helpers take those arguments, and where the variable they
/// read is not there they go to the daemon's own route instead of failing.
#[test]
fn a_helper_of_an_earlier_builds_dispatcher_is_served_through_the_daemons_record() {
    let dir = ScratchDir::new("key-source-earlier-build");
    let old_key_file = key_file(&dir, "old-key", Some(TEST_KEY));
    let closed_port = ClosedPort::new();
    let closed = closed_port.port.to_string();
    let cases: [(&str, &[&str], &[u8], &str); 2] = [
        (
            "git-ref",
            &["--head", "refs/heads/main", "--moved"],
            b"",
            "{}",
        ),
        (
            "gate-hook",
            &["pre-push"],
            b"refs/heads/main 1 refs/heads/main 0\n",
            "null",
        ),
    ];
    for (mode, rest, input, body) in cases {
        let daemon = StandInDaemon::answering(DAEMON_KEY, "200 OK", body);
        let home = dir.join(format!("home-{mode}"));
        crate::owner_record::Owner::current(daemon.port, false, true)
            .expect("this process's identity")
            .record_in(&home.join(".octoboard"), DAEMON_KEY)
            .unwrap();
        let mut arguments = vec![mode, "--session", "a", "--port", closed.as_str()];
        arguments.extend(rest);
        let mut command = helper_in(&home, &arguments, None);
        command.env("OCTOBOARD_DAEMON_KEY_FILE", &old_key_file);

        let (output, _) = run(command, input);

        assert_eq!(
            output.status.code(),
            Some(0),
            "{mode}: {}",
            text(&output.stderr)
        );
        let requests = daemon.finish();
        assert_eq!(requests.len(), 1, "{mode}");
        assert!(
            requests[0].line.contains(&format!("?key={DAEMON_KEY} ")),
            "{mode}: {}",
            requests[0].line
        );
    }
}

/// A dispatcher passes the data directory it lives in, and the helper looks for the daemon's
/// records there: the home directory of the helper's own environment, which comes from the
/// session's shell and may be another daemon's, holds a daemon that must not be asked.
#[test]
fn a_helper_given_a_data_directory_reads_the_daemons_records_there_and_not_under_its_home() {
    let dir = ScratchDir::new("key-source-data-dir");
    let cases: [(&str, &[&str], &[u8], &str); 2] = [
        ("git-ref", &[], b"", "{}"),
        (
            "gate-hook",
            &["pre-push"],
            b"refs/heads/main 1 refs/heads/main 0\n",
            "null",
        ),
    ];
    for (mode, rest, input, body) in cases {
        let at_home = StandInDaemon::answering("the-homes-key", "200 OK", body);
        let home = dir.join(format!("home-{mode}"));
        crate::owner_record::Owner::current(at_home.port, false, true)
            .expect("this process's identity")
            .record_in(&home.join(".octoboard"), "the-homes-key")
            .unwrap();
        let named = StandInDaemon::answering(DAEMON_KEY, "200 OK", body);
        let data = dir.join(format!("data-{mode}"));
        crate::owner_record::Owner::current(named.port, false, true)
            .expect("this process's identity")
            .record_in(&data, DAEMON_KEY)
            .unwrap();
        let mut arguments = vec![mode, "--session", "a", "--data-dir", data.to_str().unwrap()];
        arguments.extend(rest);

        let (output, _) = run(helper_in(&home, &arguments, None), input);

        assert_eq!(
            output.status.code(),
            Some(0),
            "{mode}: {}",
            text(&output.stderr)
        );
        let requests = named.finish();
        assert_eq!(requests.len(), 1, "{mode}");
        assert!(
            requests[0].line.contains(&format!("?key={DAEMON_KEY} ")),
            "{mode}: {}",
            requests[0].line
        );
        assert!(at_home.finish().is_empty(), "{mode}: the home's daemon");
    }
}

#[test]
fn the_hook_posts_the_payload_under_the_key_in_its_file() {
    let dir = ScratchDir::new("key-source-hook");
    // A newline after the key, as an editor leaves one, is not part of it.
    let file = key_file(&dir, "key", Some(&format!("{TEST_KEY}\r\n")));
    let daemon = StandInDaemon::start("{}");
    let port = daemon.port.to_string();
    let (output, written) = run(
        helper(
            &[
                "hook",
                "--session",
                "a",
                "--port",
                &port,
                "--key-file",
                &file,
            ],
            None,
        ),
        br#"{"hook_event_name":"Stop"}"#,
    );
    assert!(written);
    assert_eq!(output.status.code(), Some(0));
    let requests = daemon.finish();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].line,
        format!("POST /hook/a?key={TEST_KEY} HTTP/1.1")
    );
    assert_eq!(requests[0].body, r#"{"hook_event_name":"Stop"}"#);
}

#[test]
fn the_hook_without_a_usable_key_file_reads_its_input_and_posts_nothing() {
    let dir = ScratchDir::new("key-source-hook-without");
    // More than a pipe holds, so a hook that did not read it would leave the writer blocked or
    // with a broken pipe.
    let payload = vec![b'x'; 1 << 20];
    for file in [
        key_file(&dir, "absent", None),
        key_file(&dir, "empty", Some("")),
        key_file(&dir, "blank", Some("\r\n")),
    ] {
        let daemon = StandInDaemon::start("{}");
        let port = daemon.port.to_string();
        let (output, written) = run(
            helper(
                &[
                    "hook",
                    "--session",
                    "a",
                    "--port",
                    &port,
                    "--key-file",
                    &file,
                ],
                None,
            ),
            &payload,
        );
        assert!(written, "{file}: the input was not read");
        assert_eq!(output.status.code(), Some(0), "{file}");
        assert_eq!(text(&output.stderr), "", "{file}");
        assert!(daemon.finish().is_empty(), "{file}");
    }
}

/// What Claude Code gives a status line on stdin.
#[cfg(unix)]
const STATUS_INPUT: &str = r#"{"context_window":{"context_window_size":200000}}"#;

/// A config directory whose user status line prints `mine`.
#[cfg(unix)]
fn user_status_line(dir: &Path) -> String {
    let config = dir.join("config");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("settings.json"),
        r#"{"statusLine":{"type":"command","command":"printf mine"}}"#,
    )
    .unwrap();
    config.to_string_lossy().into_owned()
}

#[cfg(unix)]
fn status_line(dir: &Path, port: &str, file: &str) -> (Output, bool) {
    let config = user_status_line(dir);
    let cwd = dir.to_string_lossy().into_owned();
    run(
        helper(
            &[
                "status-line",
                "--provider",
                "claude",
                "--session",
                "a",
                "--port",
                port,
                "--key-file",
                file,
                "--cwd",
                &cwd,
                "--config-dir",
                &config,
            ],
            None,
        ),
        STATUS_INPUT.as_bytes(),
    )
}

#[cfg(unix)]
#[test]
fn the_status_line_reports_the_window_under_the_key_in_its_file_and_prints_the_users_own() {
    let dir = ScratchDir::new("key-source-status-line");
    let file = key_file(&dir, "key", Some(&format!("{TEST_KEY}\n")));
    let daemon = StandInDaemon::start("{}");
    let (output, _) = status_line(&dir, &daemon.port.to_string(), &file);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        text(&output.stdout).contains("mine"),
        "{}",
        text(&output.stdout)
    );
    let requests = daemon.finish();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].line,
        format!("POST /context-window/a?key={TEST_KEY} HTTP/1.1")
    );
    assert_eq!(requests[0].body, r#"{"window":200000}"#);
}

#[cfg(unix)]
#[test]
fn the_status_line_without_a_usable_key_file_reports_nothing_and_still_prints_the_users_own() {
    let dir = ScratchDir::new("key-source-status-line-without");
    for file in [
        key_file(&dir, "absent", None),
        key_file(&dir, "empty", Some("")),
    ] {
        let daemon = StandInDaemon::start("{}");
        let (output, _) = status_line(&dir, &daemon.port.to_string(), &file);
        assert_eq!(output.status.code(), Some(0), "{file}");
        assert!(
            text(&output.stdout).contains("mine"),
            "{file}: {}",
            text(&output.stdout)
        );
        assert!(daemon.finish().is_empty(), "{file}");
    }
}

/// Runs `octoboardd mcp` against a stand-in daemon, calls one tool of its catalogue, and returns
/// the tool's result as the agent reads it with the requests that reached the daemon.
fn mcp_call(file: &str) -> (serde_json::Value, Vec<Request>) {
    use crate::protocol::SessionKind;
    let daemon = StandInDaemon::start(r#"{"ok":true,"result":{"done":true}}"#);
    let port = daemon.port.to_string();
    let tool = crate::session_tools::catalogue::tools_for(SessionKind::Project, false)[0].name;
    let mut command = helper(
        &[
            "mcp",
            "--session",
            "a",
            "--kind",
            "project",
            "--port",
            &port,
            "--key-file",
            file,
            "--token",
            "token",
        ],
        None,
    );
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (lines, answers) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if lines.send(message).is_err() {
                return;
            }
        }
    });
    let send = |stdin: &mut std::process::ChildStdin, message: serde_json::Value| {
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    };
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0"},
        }}),
    );
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    );
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": tool, "arguments": {}}}),
    );
    let result = loop {
        let message = answers
            .recv_timeout(PATIENCE)
            .expect("the MCP server answered the call");
        if message["id"] == 2 {
            break message["result"].clone();
        }
    };
    // Closing its stdin is how the agent ends the server.
    drop(stdin);
    assert!(child.wait().unwrap().success());
    (result, daemon.finish())
}

#[test]
fn the_mcp_server_calls_the_daemon_under_the_key_in_its_file() {
    let dir = ScratchDir::new("key-source-mcp");
    let file = key_file(&dir, "key", Some(&format!("{TEST_KEY}\n")));
    let (result, requests) = mcp_call(&file);
    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].line,
        format!("POST /mcp/token?key={TEST_KEY} HTTP/1.1")
    );
}

/// With no key to read, the server still runs, and its calls go to the daemon with a key it
/// refuses, so each is reported as the daemon's refusal and never as a success.
#[test]
fn the_mcp_server_without_a_usable_key_file_has_every_call_refused_by_the_daemon() {
    let dir = ScratchDir::new("key-source-mcp-without");
    for file in [
        key_file(&dir, "absent", None),
        key_file(&dir, "empty", Some("")),
    ] {
        let (result, requests) = mcp_call(&file);
        assert_eq!(result["isError"], true, "{file}: {result}");
        assert_eq!(requests.len(), 1, "{file}");
        assert!(
            !requests[0].line.contains(TEST_KEY),
            "{file}: {}",
            requests[0].line
        );
    }
}
