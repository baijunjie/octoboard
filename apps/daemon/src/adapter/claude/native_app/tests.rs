use std::collections::HashMap;

use super::*;
use crate::protocol::{error_code, CodedError, Session, SessionKind};
use crate::test_support::{bare_session, fake_agent, ScratchDir};

const CONVERSATION: &str = "0b8f1c52-3d1e-4a7b-9c41-5e6d7f8091a2";

fn session(config_dir: Option<&std::path::Path>) -> Session {
    Session {
        has_conversation: true,
        agent_session_id: Some(CONVERSATION.to_string()),
        config_dir: config_dir.map(|dir| dir.to_string_lossy().into_owned()),
        ..bare_session("s", SessionKind::Console)
    }
}

fn open(
    session: &Session,
    cwd: &std::path::Path,
    shell: &HashMap<String, String>,
) -> anyhow::Result<()> {
    ClaudeApp.open(&NativeAppOpen {
        session,
        conversation: CONVERSATION,
        cwd,
        shell,
        scratch: &cwd.join("scratch"),
    })
}

fn refusal(result: anyhow::Result<()>) -> CodedError {
    CodedError::of(&result.expect_err("refused"))
}

/// The CLI is asked to hand the conversation to the application as `claude --desktop --resume
/// <id>`, in the session's working directory, as the session's account.
#[cfg(unix)]
#[test]
fn the_cli_is_asked_to_resume_the_conversation_in_the_desktop_app() {
    let dir = ScratchDir::new("claude-app-open");
    let bin = dir.join("bin");
    let account = dir.join("account");
    let cwd = dir.join("work");
    for path in [&bin, &account, &cwd] {
        std::fs::create_dir_all(path).unwrap();
    }
    let seen = dir.join("seen");
    let shell = fake_agent(
        &bin,
        "claude",
        &format!(
            "{{ echo \"args=$*\"; echo \"cwd=$(pwd -P)\"; echo \"config=$CLAUDE_CONFIG_DIR\"; }} > '{}'",
            seen.display()
        ),
    );

    open(&session(Some(&account)), &cwd, &shell).expect("opened");

    let seen = std::fs::read_to_string(&seen).unwrap();
    assert!(
        seen.contains(&format!("args=--desktop --resume {CONVERSATION}\n")),
        "{seen}"
    );
    assert!(
        seen.lines()
            .filter_map(|line| line.strip_prefix("cwd="))
            .any(|reported| {
                crate::hostfs::canonical(reported).unwrap()
                    == crate::hostfs::canonical(&cwd).unwrap()
            }),
        "{seen}"
    );
    assert!(
        seen.contains(&format!("config={}\n", account.display())),
        "{seen}"
    );
}

/// A non-zero exit fails as `native_app_unavailable` with what the CLI printed as the detail.
#[test]
fn a_failing_cli_is_reported_with_its_output() {
    let dir = ScratchDir::new("claude-app-fails");
    let shell = fake_agent(
        &dir,
        "claude",
        "echo 'the app is not signed in' >&2; exit 3",
    );

    let refused = refusal(open(&session(None), &dir, &shell));

    assert_eq!(refused.code, error_code::NATIVE_APP_UNAVAILABLE);
    assert!(
        refused.params["detail"].contains("the app is not signed in"),
        "{:?}",
        refused.params
    );
}

/// A CLI that fails without a word is still refused with some detail.
#[test]
fn a_failing_cli_that_prints_nothing_is_still_refused_with_a_detail() {
    let dir = ScratchDir::new("claude-app-silent");
    let shell = fake_agent(&dir, "claude", "exit 3");

    let refused = refusal(open(&session(None), &dir, &shell));

    assert_eq!(refused.code, error_code::NATIVE_APP_UNAVAILABLE);
    assert!(!refused.params["detail"].is_empty());
}

/// Without the CLI on the shell's `PATH`, nothing can be asked.
#[test]
fn a_cli_that_cannot_be_found_is_refused_as_unavailable() {
    let dir = ScratchDir::new("claude-app-no-cli");
    let shell = HashMap::from([("PATH".to_string(), dir.to_string_lossy().into_owned())]);

    let refused = refusal(open(&session(None), &dir, &shell));

    assert_eq!(refused.code, error_code::NATIVE_APP_UNAVAILABLE);
    assert!(!refused.params["detail"].is_empty());
}

/// An account whose directory is gone is refused before the CLI is run.
#[test]
fn an_account_whose_directory_is_gone_is_refused_without_running_the_cli() {
    let dir = ScratchDir::new("claude-app-no-account");
    let ran = dir.join("ran");
    let shell = fake_agent(&dir, "claude", &format!("touch '{}'", ran.display()));
    let gone = dir.join("gone");

    let refused = refusal(open(&session(Some(&gone)), &dir, &shell));

    assert_eq!(refused.code, error_code::NATIVE_APP_UNAVAILABLE);
    assert!(!ran.exists());
}

/// What the CLI handed the conversation to must outlive it: a process the CLI started and left
/// running is still there once the open has returned, as the application is.
#[cfg(unix)]
#[test]
fn what_the_cli_started_is_left_running_when_it_exits() {
    let dir = ScratchDir::new("claude-app-descendants");
    let pid_file = dir.join("app.pid");
    let shell = fake_agent(
        &dir,
        "claude",
        &format!("sleep 300 &\necho $! > '{}'", pid_file.display()),
    );

    open(&session(None), &dir, &shell).expect("opened");

    let pid: i32 = std::fs::read_to_string(&pid_file)
        .expect("the stand-in application started")
        .trim()
        .parse()
        .unwrap();
    let _guard = crate::test_support::KillOnDrop(vec![pid]);
    assert!(
        crate::test_support::is_running(pid),
        "the process the CLI started was ended with it"
    );
}

/// The output of `claude --desktop` goes to one file in the session's scratch directory while it
/// runs, and the file is gone afterwards, whether it succeeded or not.
#[cfg(unix)]
#[test]
fn the_cli_s_output_is_kept_in_the_sessions_scratch_directory_only_while_it_runs() {
    let dir = ScratchDir::new("claude-app-log");
    let scratch = dir.join("scratch");
    let log = scratch.join(LOG_FILE);
    assert_eq!(LOG_FILE, "open-in-app.log");
    for (exit, name) in [(0, "ok"), (3, "failed")] {
        let marker = dir.join(format!("log-existed-{name}"));
        let shell = fake_agent(
            &dir,
            "claude",
            &format!(
                "echo hello; if [ -f '{}' ]; then touch '{}'; fi; exit {exit}",
                log.display(),
                marker.display()
            ),
        );

        let result = open(&session(None), &dir, &shell);

        assert_eq!(result.is_ok(), exit == 0, "{name}");
        assert!(
            marker.exists(),
            "{name}: the CLI's output was going to the scratch file"
        );
        assert!(!log.exists(), "{name}: the file is removed afterwards");
    }
}

/// Only the end of what the CLI printed is read for the reason it failed, which is why a long output
/// still ends in the line the user needs.
#[test]
fn the_tail_of_a_long_output_is_what_a_failure_reports() {
    let dir = ScratchDir::new("claude-app-tail");
    let shell = fake_agent(
        &dir,
        "claude",
        &format!(
            "echo first-line; i=0; while [ $i -lt {} ]; do echo filler-line-number-$i; i=$((i+1)); done; echo the-last-line; exit 3",
            OUTPUT_LIMIT / 8
        ),
    );

    let refused = refusal(open(&session(None), &dir, &shell));

    assert_eq!(refused.code, error_code::NATIVE_APP_UNAVAILABLE);
    assert!(
        refused.params["detail"].contains("the-last-line"),
        "{:?}",
        refused.params
    );
    assert!(!refused.params["detail"].contains("first-line"));
}

/// `tail_of` reads at most the last `limit` bytes, all of a shorter file, and nothing of one that
/// cannot be read.
#[test]
fn the_tail_is_at_most_the_last_bytes_of_the_file() {
    let dir = ScratchDir::new("claude-app-tail-of");
    let path = dir.join("out.log");
    std::fs::write(&path, "0123456789abcdef").unwrap();

    assert_eq!(tail_of(&path, 6), b"abcdef");
    assert_eq!(tail_of(&path, 16), b"0123456789abcdef");
    assert_eq!(tail_of(&path, 1000), b"0123456789abcdef");
    assert_eq!(tail_of(&path, 0), b"");
    assert_eq!(tail_of(&dir.join("missing.log"), 6), b"");
}
