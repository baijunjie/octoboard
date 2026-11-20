//! Claude Code's background sessions as they stand in the way of a resume. A background session
//! (`claude --bg`, or `claude --bg --resume <id>` for an existing conversation) runs a conversation
//! under a process of its own, under the conversation's own id, and goes on after the terminal
//! that started it has gone. Resuming that conversation with Octoboard's launch flags then fails at
//! once: Claude Code prints "That session is running in the background (<id>). Run `claude attach
//! <id>` to open it, or `claude stop <id>` first to resume it here." and exits, and the session
//! cannot come back however often it is resumed. (A bare `claude --resume` attaches to the
//! background session instead; one with the flags a launch passes refuses. The agents view's ←,
//! by contrast, forks the conversation under a new id, which leaves the original free to resume.
//! Measured on Claude Code 2.1.329.)
//!
//! So before a resume, the background session holding the conversation is stopped, which keeps
//! its conversation, and the resume then goes ahead as usual. What it was running is ended with
//! it: the user asked for the conversation back in Octoboard, and the conversation cannot be in
//! both places. The background sessions are found through the CLI's own listing rather than by
//! reading the files Claude Code keeps for them, which are its internal, undocumented layout.

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;

use crate::adapter::account_cli::{account_command_of, run_query};
use crate::adapter::AgentVersion;
use crate::platform::shell::ResolvedBinary;
use crate::protocol::AgentId;

/// The first Claude Code measured with `claude agents --json` and `claude stop`. An older one, or
/// one whose version is unknown, is not asked: an unknown subcommand may start a session instead
/// of failing, and the launch would wait on it.
pub(super) const BACKGROUND_SESSIONS_SINCE: AgentVersion = (2, 1, 329);

/// How long each CLI call may take. Both answer in well under a second; the bound only keeps a
/// stuck CLI from holding the launch.
const CLI_TIMEOUT: Duration = Duration::from_secs(10);

/// The most output read from either call. A listing names every session of the account, a few
/// hundred bytes each.
const OUTPUT_LIMIT: usize = 1024 * 1024;

/// One entry of `claude agents --json`. Only the fields this reads: the CLI lists interactive
/// sessions (with no `id`) beside background ones, and keeps an entry for a background session it
/// has stopped, without its `pid`.
#[derive(Debug, Deserialize)]
struct Entry {
    id: String,
    kind: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    pid: Option<u64>,
}

/// The ids `claude stop` takes for the background sessions in `listing` (the output of `claude
/// agents --json`) that are running `conversation`. `claude stop` takes the short id, not the
/// conversation's own.
fn holders(listing: &[u8], conversation: &str) -> Vec<String> {
    // Each entry is read on its own, so that one this does not understand (a shape a later CLI
    // lists) does not hide a holder beside it.
    let Ok(entries) = serde_json::from_slice::<Vec<serde_json::Value>>(listing) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<Entry>(entry).ok())
        .filter(|entry| {
            entry.kind.as_deref() == Some("background")
                && entry.session_id.as_deref() == Some(conversation)
                && entry.pid.is_some()
        })
        .map(|entry| entry.id)
        .collect()
}

/// Stops every background session of the account `environment` names that is running
/// `conversation`, so that resuming it does not fail. Best effort: whatever fails is logged and
/// the launch goes on, to the refusal it would have met anyway.
pub(super) fn stop_holders(
    binary: &ResolvedBinary,
    environment: &HashMap<String, String>,
    conversation: &str,
) {
    // The environment already names the account's directory: the plan put it there. The launch's
    // claim on the account keeps its sharing from changing under these calls.
    let run = |args: &[&str]| {
        let mut command = account_command_of(binary, AgentId::CLAUDE, environment, None, args);
        run_query(&mut command, CLI_TIMEOUT, OUTPUT_LIMIT, None)
    };
    let listing = match run(&["agents", "--json"]) {
        Ok(output) if output.status.success() => output.stdout,
        Ok(output) => {
            tracing::warn!(
                status = %output.status,
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "claude: listing background sessions before a resume failed"
            );
            return;
        }
        Err(err) => {
            tracing::warn!(error = %err, "claude: listing background sessions before a resume failed");
            return;
        }
    };
    for id in holders(&listing, conversation) {
        match run(&["stop", &id]) {
            Ok(output) if output.status.success() => {
                tracing::info!(background = %id, conversation, "claude: stopped the background session holding a conversation being resumed");
            }
            Ok(output) => tracing::warn!(
                background = %id,
                status = %output.status,
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "claude: stopping the background session holding a conversation being resumed failed"
            ),
            Err(err) => tracing::warn!(
                background = %id,
                error = %err,
                "claude: stopping the background session holding a conversation being resumed failed"
            ),
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::platform::shell::resolve_binary;
    use crate::test_support::{fake_agent, ScratchDir};
    use std::collections::HashMap;
    use std::path::PathBuf;

    const CONVERSATION: &str = "a863a836-568c-49c5-9637-3c465e20cf32";

    /// What `claude agents --json --all` printed on Claude Code 2.1.329 while a background session
    /// held `HELD_CONVERSATION` (see `testdata/claude-agents-captures.md`).
    pub(in crate::adapter::claude) const HOLDING_CAPTURE: &str =
        include_str!("../../../testdata/claude_agents_2_1_329_holding.json");
    /// The same listing after `claude stop` ended that background session.
    const STOPPED_CAPTURE: &str =
        include_str!("../../../testdata/claude_agents_2_1_329_stopped.json");
    /// The conversation the background session in the captures held.
    pub(in crate::adapter::claude) const HELD_CONVERSATION: &str =
        "2a952e78-0994-4aff-a2f4-6845d8b8d77c";
    /// The short id `claude stop` takes for that background session.
    pub(in crate::adapter::claude) const HOLDER_ID: &str = "2a952e78";
    /// The conversation of the interactive entry in the captures, which has no `id` field.
    const INTERACTIVE_CONVERSATION: &str = "8386dc92-13a7-44d7-8a24-9056d52dbceb";

    fn running_holder(id: &str) -> String {
        format!(
            r#"{{"pid":72299,"id":"{id}","cwd":"/p","kind":"background","sessionId":"{CONVERSATION}","status":"idle","state":"done"}}"#
        )
    }

    #[test]
    fn a_real_listing_names_the_running_holder_though_an_interactive_entry_has_no_id() {
        // The interactive entry of the capture has no `id`: a listing read as one typed array
        // fails on it as a whole, and no holder is found.
        assert_eq!(
            holders(HOLDING_CAPTURE.as_bytes(), HELD_CONVERSATION),
            [HOLDER_ID]
        );
    }

    #[test]
    fn a_real_listing_names_nothing_for_stopped_finished_and_interactive_sessions() {
        // Once stopped, the holder's entry is still listed but has no `pid`.
        assert!(holders(STOPPED_CAPTURE.as_bytes(), HELD_CONVERSATION).is_empty());
        for capture in [HOLDING_CAPTURE, STOPPED_CAPTURE] {
            // A finished background session (no `pid`) and an interactive one (running, but
            // not a background session) hold nothing.
            for conversation in [CONVERSATION, INTERACTIVE_CONVERSATION] {
                assert!(
                    holders(capture.as_bytes(), conversation).is_empty(),
                    "{conversation}"
                );
            }
        }
    }

    #[test]
    fn a_running_background_session_of_the_conversation_is_named_by_its_short_id() {
        let listing = format!("[{}]", running_holder("a863a836"));
        assert_eq!(holders(listing.as_bytes(), CONVERSATION), ["a863a836"]);
    }

    #[test]
    fn every_running_background_session_of_the_conversation_is_named() {
        let listing = format!(
            r#"[{}, {{"pid":3,"id":"other","kind":"background","sessionId":"x"}}, {}]"#,
            running_holder("first"),
            running_holder("second")
        );
        assert_eq!(
            holders(listing.as_bytes(), CONVERSATION),
            ["first", "second"]
        );
    }

    #[test]
    fn stopped_interactive_and_other_sessions_are_left_alone() {
        let listing = format!(
            r#"[
                {{"id":"a863a836","kind":"background","sessionId":"{CONVERSATION}","state":"done"}},
                {{"pid":1,"id":"b1","kind":"interactive","sessionId":"{CONVERSATION}"}},
                {{"pid":2,"id":"c2","kind":"background","sessionId":"00000000-0000-0000-0000-000000000000"}}
            ]"#
        );
        assert!(holders(listing.as_bytes(), CONVERSATION).is_empty());
    }

    #[test]
    fn a_listing_that_does_not_read_names_nothing() {
        assert!(holders(b"Usage: claude agents", CONVERSATION).is_empty());
        assert!(holders(b"{}", CONVERSATION).is_empty());
        assert!(holders(b"", CONVERSATION).is_empty());
        assert!(holders(b"[]", CONVERSATION).is_empty());
    }

    #[test]
    fn an_entry_that_does_not_read_does_not_hide_a_holder_beside_it() {
        let listing = format!(
            r#"[
                {{"pid":1,"kind":"background","sessionId":"{CONVERSATION}"}},
                {{"pid":"not a number","id":"bad-pid","kind":"background","sessionId":"{CONVERSATION}"}},
                {{"pid":2,"id":7,"kind":"background","sessionId":"{CONVERSATION}"}},
                "a string",
                null,
                {},
                {{"pid":3,"id":"kind-is-a-list","kind":["background"],"sessionId":"{CONVERSATION}"}},
                {}
            ]"#,
            running_holder("before"),
            running_holder("after")
        );
        assert_eq!(
            holders(listing.as_bytes(), CONVERSATION),
            ["before", "after"]
        );
    }

    /// A stand-in `claude` that records what it was run with and answers `agents --json` with a
    /// canned listing, `stop` with a canned status.
    pub(in crate::adapter::claude) struct FakeClaude {
        dir: ScratchDir,
        environment: HashMap<String, String>,
        /// The name of an environment variable the test process has and `environment` does not.
        leak_name: String,
    }

    impl FakeClaude {
        pub(in crate::adapter::claude) fn new(
            listing: &str,
            listing_status: i32,
            stop_status: i32,
        ) -> Self {
            let dir = ScratchDir::new("claude-background-sessions");
            let listing_file = dir.join("listing.json");
            std::fs::write(&listing_file, listing).unwrap();
            let log = dir.join("log");
            let leak_name = std::env::vars_os()
                .filter_map(|(name, value)| Some((name.into_string().ok()?, value)))
                .find(|(name, value)| {
                    !value.is_empty()
                        && name.starts_with("CARGO")
                        && name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                })
                .map(|(name, _)| name)
                .expect("the test process has a CARGO_* variable to see leak");
            let body = format!(
                "echo \"argv: $*\" >> '{log}'\n\
                 echo \"config: $CLAUDE_CONFIG_DIR\" >> '{log}'\n\
                 echo \"leak: ${{{leak_name}-unset}}\" >> '{log}'\n\
                 case \"$1\" in\n\
                 agents) cat '{listing_file}'; exit {listing_status};;\n\
                 stop) echo 'stop refused' >&2; exit {stop_status};;\n\
                 esac\n",
                log = log.display(),
                listing_file = listing_file.display(),
            );
            let mut environment = fake_agent(&dir, "claude", &body);
            environment.insert(
                "CLAUDE_CONFIG_DIR".to_string(),
                "/accounts/work".to_string(),
            );
            Self {
                dir,
                environment,
                leak_name,
            }
        }

        pub(in crate::adapter::claude) fn binary(&self) -> ResolvedBinary {
            resolve_binary("claude", &self.environment).expect("the fake claude")
        }

        pub(in crate::adapter::claude) fn environment(&self) -> &HashMap<String, String> {
            &self.environment
        }

        fn stop_holders(&self) {
            stop_holders(&self.binary(), &self.environment, CONVERSATION);
        }

        fn log(&self) -> Vec<String> {
            let log: PathBuf = self.dir.join("log");
            std::fs::read_to_string(log)
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        pub(in crate::adapter::claude) fn calls(&self) -> Vec<String> {
            self.log()
                .iter()
                .filter_map(|line| line.strip_prefix("argv: "))
                .map(str::to_owned)
                .collect()
        }
    }

    #[test]
    fn a_running_holder_is_listed_and_then_stopped_by_its_short_id() {
        let claude = FakeClaude::new(HOLDING_CAPTURE, 0, 0);
        stop_holders(&claude.binary(), claude.environment(), HELD_CONVERSATION);
        assert_eq!(
            claude.calls(),
            ["agents --json".to_string(), format!("stop {HOLDER_ID}")]
        );
    }

    #[test]
    fn the_cli_runs_in_the_given_environment_and_nothing_of_the_daemons() {
        let claude = FakeClaude::new(&format!("[{}]", running_holder("a863a836")), 0, 0);
        claude.stop_holders();
        let log = claude.log();
        // Both calls (the listing and the stop) saw the account the environment names.
        assert_eq!(
            log.iter()
                .filter(|l| *l == "config: /accounts/work")
                .count(),
            2,
            "{log:?}"
        );
        assert_eq!(
            log.iter().filter(|l| *l == "leak: unset").count(),
            2,
            "{} reached the CLI: {log:?}",
            claude.leak_name
        );
    }

    #[test]
    fn nothing_is_stopped_when_no_session_holds_the_conversation() {
        let listing = format!(
            r#"[{{"pid":1,"id":"b1","kind":"interactive","sessionId":"{CONVERSATION}"}}, {{"id":"c2","kind":"background","sessionId":"{CONVERSATION}"}}]"#
        );
        let claude = FakeClaude::new(&listing, 0, 0);
        claude.stop_holders();
        assert_eq!(claude.calls(), ["agents --json"]);
    }

    #[test]
    fn a_failing_listing_stops_nothing_even_when_it_printed_a_holder() {
        let claude = FakeClaude::new(&format!("[{}]", running_holder("a863a836")), 1, 0);
        claude.stop_holders();
        assert_eq!(claude.calls(), ["agents --json"]);
    }

    #[test]
    fn a_listing_that_does_not_read_stops_nothing() {
        let claude = FakeClaude::new("not json at all", 0, 0);
        claude.stop_holders();
        assert_eq!(claude.calls(), ["agents --json"]);
    }

    #[test]
    fn a_failing_stop_does_not_keep_the_next_holder_from_being_stopped() {
        let listing = format!(
            "[{}, {}]",
            running_holder("first"),
            running_holder("second")
        );
        let claude = FakeClaude::new(&listing, 0, 1);
        claude.stop_holders();
        assert_eq!(
            claude.calls(),
            ["agents --json", "stop first", "stop second"]
        );
    }

    #[test]
    fn a_cli_that_cannot_be_started_is_not_an_error() {
        let dir = ScratchDir::new("claude-background-sessions-missing");
        let binary = ResolvedBinary::new(dir.join("no-such-claude"));
        stop_holders(&binary, &HashMap::new(), CONVERSATION);
    }
}
