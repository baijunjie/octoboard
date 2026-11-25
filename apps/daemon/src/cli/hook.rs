//! `octoboardd hook` — what the per-session hook script execs, and what Claude Code's command hook
//! execs directly. It reads the agent's hook payload from stdin and posts it to the daemon. A
//! Claude Code that takes HTTP hooks posts to the daemon itself and never runs this.
//!
//! Its only hard requirement is to be invisible when anything goes wrong: every one of the three
//! agents renders a failing hook to the user (an error block on Claude Code, a scrollback line on
//! Grok, a history cell on Codex), so this exits 0 unconditionally and writes nothing to stdout or
//! stderr. Stdout silence also matters for correctness, not just cosmetics: a `PreToolUse` hook on
//! Grok that emits JSON can deny a tool call, and on Claude Code stdout can steer the turn.
//!
//! **Whose hook it is.** An agent that names its launch's settings in a variable hands them to
//! every agent started inside the session too, whose hooks would then report another
//! conversation as this session's. A provider with such an agent gives the hook a mark
//! (`--fired-by`), text only the session's own agent process has on its command line, and the
//! payload is forwarded only when the process that fired the hook has it ([`fired_by_marked`]).
//! The command lines are read from the process table (`crate::platform::proc_table`), so the
//! check starts no process: it runs on every hook event. The cross-project guard's mode
//! (`octoboardd guard-hook --fired-by`) makes the same check ([`fired_by`]).

use std::io::Read;
use std::path::Path;
use std::time::Duration;

/// Hard deadline for the whole round trip ([`crate::adapter::HOOK_ROUND_TRIP_TIMEOUT`]).
const TIMEOUT: Duration = crate::adapter::HOOK_ROUND_TRIP_TIMEOUT;

/// How many shells wrapping the hook command are looked past for the process that fired it. An
/// agent runs a hook through one shell, and a provider may wrap the command in a subshell or two.
const MAX_WRAPPERS: usize = 4;

/// Whether the process that fired this hook has `mark` on its command line, so that the hook is
/// the session's own. That process is the nearest ancestor, from `parent` up, that is not a shell
/// wrapping the hook command; a wrapping shell is told by `wraps`, text of the hook command's own
/// (the path of the key file it names), which a shell given the command to run has on its command
/// line and the agent does not. `parent_of` and `argv_of` read the process table.
///
/// Whatever cannot be told runs the hook: a command line that cannot be read, a parent that
/// cannot be found, more wrapping shells than [`MAX_WRAPPERS`]. A status for every session counts
/// for more than the events of an agent started inside one.
///
/// Not for an agent whose own command line carries its hook command or the key file's path: it
/// would read as a wrapping shell, and the process above it has no mark. A provider gives such
/// an agent's hook no mark.
fn fired_by_marked(
    parent: Option<i32>,
    wraps: &str,
    mark: &str,
    parent_of: impl Fn(i32) -> Option<i32>,
    argv_of: impl Fn(i32) -> Option<Vec<String>>,
) -> bool {
    let mut next = parent;
    for _ in 0..=MAX_WRAPPERS {
        // Pid 1 and below is no process that fired a hook: the chain was cut.
        let Some(pid) = next.filter(|pid| *pid > 1) else {
            return true;
        };
        let Some(argv) = argv_of(pid) else {
            return true;
        };
        let holds = |text: &str| argv.iter().any(|arg| arg.contains(text));
        if wraps.is_empty() || !holds(wraps) {
            return holds(mark);
        }
        next = parent_of(pid);
    }
    true
}

/// Whether the process that fired the hook this process runs for has `mark` on its command line
/// ([`fired_by_marked`]), read from the process table; `wraps` is text of the hook command's own
/// that tells a shell wrapping it (the path of a file the command names, which the agent's own
/// command line does not hold).
pub(crate) fn fired_by(wraps: &str, mark: &str) -> bool {
    use crate::platform::proc_table;
    let own = i32::try_from(std::process::id()).ok();
    fired_by_marked(
        own.and_then(proc_table::parent_of),
        wraps,
        mark,
        proc_table::parent_of,
        proc_table::argv,
    )
}

/// Runs the hook and returns. Never fails: the caller exits 0 regardless. A key file that cannot
/// be read is a daemon that refuses the post: nothing is sent. The route names the launch
/// when the script does. `event` is posted as the hook route's
/// `event` query parameter, the name of an event whose payload names none. With `fired_by`, the
/// payload is read and then dropped unless the process that fired the hook has that text on its
/// command line ([`fired_by_marked`]).
pub fn run(
    session: &str,
    launch: Option<&str>,
    port: u16,
    key_file: &Path,
    event: Option<&str>,
    fired_by: Option<&str>,
) {
    // The key first: the file goes with the session's scratch directory when the agent exits, and
    // a hook the agent does not wait for can start just before that.
    let key = crate::key_file::read(key_file);
    let mut payload = Vec::new();
    if std::io::stdin().read_to_end(&mut payload).is_err() {
        return;
    }
    let Some(key) = key else {
        return;
    };
    // After the payload is read, so the agent's write to this process ends as it would.
    if let Some(mark) = fired_by {
        if !self::fired_by(&key_file.to_string_lossy(), mark) {
            return;
        }
    }
    // The response is discarded: the daemon never steers the agent through it.
    let mut path = match launch {
        Some(launch) => crate::holder::protocol::hook_path(session, launch),
        None => format!("/hook/{session}"),
    };
    if let Some(event) = event {
        path.push_str("?event=");
        path.push_str(&crate::text::percent_encode(event, ""));
    }
    let _ = crate::loopback::post(port, &key, &path, &payload, TIMEOUT);
}

#[cfg(test)]
mod tests {
    use super::fired_by_marked;

    const KEY_FILE: &str = "/run/s/key";
    const MARK: &str = "/run/s/mcp.json";

    /// A process table of `(pid, parent, command line)` rows; a row with no command line is a
    /// process whose arguments cannot be read.
    fn fired(table: &[(i32, i32, Option<&[&str]>)], parent: i32) -> bool {
        let row = |pid: i32| table.iter().find(|row| row.0 == pid);
        fired_by_marked(
            Some(parent),
            KEY_FILE,
            MARK,
            |pid| row(pid).map(|row| row.1),
            |pid| {
                row(pid)?
                    .2
                    .map(|argv| argv.iter().map(|arg| arg.to_string()).collect())
            },
        )
    }

    /// The hook command as a shell is given it: it names the key file, and the mark as its own
    /// argument.
    const WRAPPER: &[&str] = &[
        "bash",
        "-c",
        "( octoboardd hook --session s --launch l --port 1 --key-file /run/s/key --fired-by /run/s/mcp.json ) >/dev/null 2>&1; exit 0",
    ];
    const OWN_AGENT: &[&str] = &["agent", "--mcp-config", "/run/s/mcp.json"];
    const NESTED_AGENT: &[&str] = &["agent", "-p", "summarise this"];

    /// The session's own agent fired the hook, whether the hook's process replaced the shell the
    /// agent started or runs under one or two shells that wrap the command.
    #[test]
    fn a_hook_the_sessions_own_agent_fired_is_forwarded() {
        let direct = [(10, 2, Some(OWN_AGENT))];
        assert!(fired(&direct, 10));
        let wrapped = [
            (10, 2, Some(OWN_AGENT)),
            (20, 10, Some(WRAPPER)),
            (21, 20, Some(WRAPPER)),
        ];
        assert!(fired(&wrapped, 20));
        assert!(fired(&wrapped, 21));
    }

    /// An agent started inside the session fired the hook: its command line has no mark, though
    /// the shell that wraps the hook command names the mark, and though the session's own agent,
    /// further up, has it.
    #[test]
    fn a_hook_an_agent_started_inside_the_session_fired_is_dropped() {
        let table = [
            (10, 2, Some(OWN_AGENT)),
            (
                30,
                10,
                Some(&["bash", "-c", "agent -p 'summarise this'"][..]),
            ),
            (40, 30, Some(NESTED_AGENT)),
            (50, 40, Some(WRAPPER)),
        ];
        assert!(!fired(&table, 50));
        assert!(!fired(&table, 40), "with no shell left between them either");
    }

    /// What cannot be told runs the hook: a command line that cannot be read, a parent that is
    /// not in the table, a chain cut at the first process, and more wrapping shells than are
    /// looked past.
    #[test]
    fn a_hook_whose_firing_process_cannot_be_told_is_forwarded() {
        assert!(fired(&[(10, 2, None)], 10), "an unreadable command line");
        assert!(fired(&[], 10), "a process that is gone");
        assert!(fired(&[(20, 1, Some(WRAPPER))], 20), "a chain cut at pid 1");
        assert!(
            fired(&[(20, 99, Some(WRAPPER))], 20),
            "a wrapper whose parent is gone"
        );
        let deep: Vec<_> = (0..8)
            .map(|depth| (100 + depth, 99 + depth, Some(WRAPPER)))
            .chain([(99, 2, Some(NESTED_AGENT))])
            .collect();
        assert!(fired(&deep, 107), "more wrappers than are looked past");
        assert!(
            fired_by_marked(None, KEY_FILE, MARK, |_| None, |_| None),
            "no parent at all"
        );
    }

    /// A shell that wraps the command is told by the key file's path alone: one that names only
    /// the mark is the process that fired the hook.
    #[test]
    fn a_process_naming_the_mark_and_not_the_key_file_is_the_one_that_fired() {
        let table = [(10, 2, Some(&["agent", "--settings", "/run/s/mcp.json"][..]))];
        assert!(fired(&table, 10));
        let unmarked = [(
            10,
            2,
            Some(&["agent", "--settings", "/run/other/mcp.json"][..]),
        )];
        assert!(!fired(&unmarked, 10));
    }
}
