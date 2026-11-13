//! The revert gate: blocks updates that quietly undo changes already merged into a guarded branch.
//! The product behavior (which branches are guarded, what is refused, what is allowed, how it
//! fails) is specified in `docs/product/revert-gate.md`; what follows is how this implementation
//! goes about it. The session hook dispatchers start `octoboardd gate-hook`, which asks for the
//! [`GateInputs`] and calls [`check`] in-process. It asks the holder of its terminal, which relays
//! the question to the daemon; with no holder, the daemon answers it directly. Its first step,
//! before any `git` runs, is to see whether a ref update moves an existing branch at all (for a
//! push, whether it has a guarded or target branch to check against), and it allows at once when
//! none does.
//!
//! - `local` decides a `reference-transaction` update: which fork points a move is held against
//!   and whether the move is allowed. `push` decides a push and `report` words a rejection. The
//!   content criteria are `content`'s, and every `git` read is `git`'s.
//! - A non-fast-forward is allowed only when none of the dropped commits were ever published, and
//!   it still has to pass the content check, since an unpushed commit in the same clone may also
//!   be one someone else just merged. A dropped unpushed commit that is still on another local
//!   branch does not count as dropped when the branch being moved is neither checked out in any
//!   worktree nor being rebased (first `git switch -c` to a new branch to carry the local commits,
//!   then `git branch -f` to move it back to the remote; git refuses to let `branch -f` touch a
//!   checked-out branch). Otherwise another branch pointing at those commits is most likely only
//!   temporary (a scratch branch, a feature branch already merged and about to be deleted), and
//!   deleting it loses the commits. Known gap: `switch -c` to a scratch branch, `branch -f` back,
//!   then switching back and deleting the scratch branch, or `switch --detach` first and then
//!   `branch -f` back and deleting the feature branch afterwards, have the same shape as the
//!   legitimate flows; they are a deliberate bypass, the gate cannot tell them apart, and they are
//!   allowed.
//! - The fork point cannot be found from git history alone: a squash that changed the base is
//!   exactly one that rewrote the parent to the latest target branch, so the one recorded when the
//!   branch was created is passed in as [`GateInputs::fork_points`]. The reflog's creation entry
//!   is the fallback, and the merge base is no use, since after the branch is rebased onto the
//!   target it is only the old target tip.
//!
//! The exit status is 0 to allow and 3 to reject, with the reason on stderr; the caller treats
//! only 3 as a rejection. When the gate itself fails (git failing, a deadline, bad input) the
//! update is allowed, with a line on stderr saying so: better to miss one check than to let it
//! wedge every ref update or push. `REVERT_GATE_SKIP=1` in the environment skips the check, only
//! for cases a human confirmed. The `reference-transaction` hook needs git 2.28+.

mod content;
mod git;
mod local;
mod push;
mod report;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};

use anyhow::Result;

use git::Git;
use local::decide;

/// The exit status of a rejection, which the hooks of a session's git take as the refusal.
pub(crate) const REJECTED: i32 = 3;

/// The hook the gate is run for.
#[derive(clap::Subcommand)]
pub enum Hook {
    /// The `reference-transaction` hook at `state` (`prepared`, `committed` or `aborted`; only
    /// `prepared` is checked); the ref updates come on stdin.
    ReferenceTransaction { state: String },
    /// The `pre-push` hook; the refs being pushed come on stdin. The gate judges the refs, not
    /// where they go, so neither the remote nor its URL is taken.
    PrePush,
}

/// What the gate knows about the repository's branches besides git's own state. It is also what the
/// daemon answers the `gate-hook` mode with, so it is the wire form too.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GateInputs {
    /// Resident guarded branches.
    pub guard: Vec<String>,
    /// `(branch, target branch)` records; every target branch named is guarded.
    pub targets: Vec<(String, String)>,
    /// `(branch, fork point)` records; the fork point is anything `git rev-parse` resolves.
    pub fork_points: Vec<(String, String)>,
}

/// [`GateInputs`] looked up by branch.
struct Inputs {
    guarded: HashSet<String>,
    /// The target branch of a pushed branch that records none: the first resident guarded branch.
    default_target: Option<String>,
    targets: HashMap<String, String>,
    fork_points: HashMap<String, String>,
}

impl Inputs {
    fn new(inputs: &GateInputs) -> Self {
        let guarded = inputs
            .guard
            .iter()
            .chain(inputs.targets.iter().map(|(_, target)| target))
            .filter(|branch| !branch.is_empty())
            .cloned()
            .collect();
        // The last record given for a branch wins.
        Self {
            guarded,
            default_target: inputs
                .guard
                .iter()
                .find(|branch| !branch.is_empty())
                .cloned(),
            targets: inputs.targets.iter().cloned().collect(),
            fork_points: inputs.fork_points.iter().cloned().collect(),
        }
    }
}

/// Runs the gate for `hook`, with the hook's stdin, and returns the exit status for the hook. The
/// first step, before any `git` runs, is to see whether a ref update moves an existing branch (or
/// a push has a guarded branch or a target branch to check against) at all.
pub fn check(hook: &Hook, inputs: &GateInputs, mut stdin: impl Read) -> i32 {
    // Always drained: git writes the update lines to the hook's stdin.
    let mut input = Vec::new();
    let _ = stdin.read_to_end(&mut input);
    let (status, message) = run_in(&Git::hook(), skip_requested(), hook, inputs, &input);
    let _ = std::io::stderr().write_all(message.as_bytes());
    status
}

/// Whether the environment asks to skip the check (`REVERT_GATE_SKIP=1`).
pub fn skip_requested() -> bool {
    std::env::var_os("REVERT_GATE_SKIP").is_some_and(|value| value == "1")
}

/// The exit status and the stderr text for `hook` run with `git`; `skipped` is the request to
/// skip the check.
fn run_in(
    git: &Git,
    skipped: bool,
    hook: &Hook,
    inputs: &GateInputs,
    input: &[u8],
) -> (i32, String) {
    if skipped {
        return (0, String::new());
    }
    let inputs = Inputs::new(inputs);
    outcome(match hook {
        Hook::ReferenceTransaction { state } => {
            decide(git, &inputs, state, input).map(Verdict::rejection)
        }
        Hook::PrePush => push::decide_push(git, &inputs, input),
    })
}

/// What the gate says about an update: whether it is rejected, and the stderr text, which can be
/// there for an allowed update too (a check that could not be made).
#[derive(Default)]
struct Verdict {
    rejected: bool,
    text: String,
}

impl Verdict {
    /// The verdict for `Some(message)` rejecting, `None` allowing.
    fn rejection(message: Option<String>) -> Self {
        Self {
            rejected: message.is_some(),
            text: message.unwrap_or_default(),
        }
    }

    fn reject(&mut self, message: String) {
        self.rejected = true;
        self.text.push_str(&message);
    }

    fn note(&mut self, message: String) {
        self.text.push_str(&message);
    }
}

/// The exit status and the stderr text for the result of [`decide`] or [`push::decide_push`]. A
/// failure of the gate itself allows the update.
fn outcome(result: Result<Verdict>) -> (i32, String) {
    match result {
        Ok(verdict) if verdict.rejected => (REJECTED, verdict.text),
        Ok(verdict) => (0, verdict.text),
        Err(err) => (
            0,
            format!("[revert-gate] Check failed, allowed anyway: {err:#}\n"),
        ),
    }
}

/// What moves a guarded branch.
#[derive(Clone, Copy, PartialEq)]
enum Route {
    /// A ref update in this clone.
    Local,
    /// A push, which moves the branch on the remote.
    Push,
}
