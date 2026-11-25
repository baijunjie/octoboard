//! Asking an agent's own CLI for an account's quota. Each probe is a short conversation over
//! lines on the CLI's stdin and stdout (`crate::processes::subprocess::run_exchange`), measured in
//! `docs/agent-cli-reference.md` ("Plan quota"). What to run, what to write and how each reply
//! reads is the provider's own ([`QuotaProbe`], in `crate::adapter::<provider>::quota`); this
//! module runs the conversation and maps how it ends.
//!
//! The agents mark these calls experimental, so a reply that cannot be read is an
//! [`QuotaUnavailable`] for that account and nothing else. An account that is not signed in is
//! told from each agent's own structured signal and reported as its own state rather than as a
//! failure, and only where the agent's own reply rules out every other way of being signed in: an
//! API key, a cloud provider or a custom provider also leaves a quota request refused or empty.
//! A probe runs in the login-shell snapshot with the agent's configuration-directory variable
//! pointed at the account's directory, or, for a provider whose CLI would change the account's
//! configuration there, at a directory of Octoboard's own that the provider builds from it
//! (`AccountOps::run_home`); and with the CLI told to load as little of the user's setup as it can
//! without losing its sign-in.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::diagnostics::{redact, stderr_tail, truncate, ProbeTrace, LINE_LIMIT, MAX_NOTES};
use crate::adapter::account_cli::{self, BackgroundRun, NotRun, RunWait};
use crate::adapter::provider;
use crate::agents::json_rpc;
use crate::processes::subprocess::{
    run_conversation_traced, AfterReply, Bounds, Conversation, ExchangeFailure, ExchangeTrace,
    Framing, Heard, AGENT_REPLY_GRACE,
};
use crate::protocol::{
    quota_unavailable_code as code, AgentId, QuotaUnavailable, QuotaWindow, ResetCredits,
};

/// How long a probe may take. The CLIs answer in one to three seconds on an idle machine, but a
/// Claude Code start has been measured at 17 seconds under heavy load (several test suites running
/// at once); this leaves room for that and a refreshed login while keeping a hung CLI from holding
/// a slot for long.
pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// The most a probe may print. Claude Code's reply to `initialize` lists every command and runs
/// to hundreds of kilobytes.
pub(crate) const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

pub(crate) const SESSION_MINUTES: u32 = 5 * 60;
pub(crate) const WEEK_MINUTES: u32 = 7 * 24 * 60;

/// What a readable reply holds.
#[derive(Debug, PartialEq)]
pub(crate) struct Reading {
    pub plan: Option<String>,
    pub windows: Vec<QuotaWindow>,
    /// The rate limit reset credits that can be used, for a provider that has them; `None` for
    /// the others and for an account with none.
    pub reset_credits: Option<ResetCredits>,
}

/// Why a reply that arrived gives no reading.
#[derive(Debug, PartialEq)]
pub(crate) enum ReplyProblem {
    /// Not in a shape this code knows.
    Unreadable,
    /// In a shape it knows, saying the account has no quota windows.
    NoWindows,
    /// The CLI answered the request with an error.
    Error(String),
    /// The CLI answered that the account is not signed in.
    NotSignedIn,
}

/// What one run of a probe is for, given to [`QuotaProbe::prepare`] and [`QuotaProbe::launch`].
pub(crate) struct ProbeRun<'a> {
    /// The account's configuration directory; `None` for the default account, whose directory is
    /// wherever the shell environment puts it.
    pub account_dir: Option<&'a Path>,
    /// The home prepared by the account's provider for this particular run, if it needs one.
    pub own_dir: Option<&'a Path>,
}

/// How one run of a probe is started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeLaunch {
    /// The arguments after the binary.
    pub args: Vec<String>,
    /// Variables set for the run on top of the shell environment.
    pub env: Vec<(String, String)>,
}

impl ProbeLaunch {
    /// A run with `args` as the account, with nothing added to its environment.
    pub(crate) fn as_account(args: &[&str]) -> Self {
        Self {
            args: args.iter().map(|arg| arg.to_string()).collect(),
            env: Vec::new(),
        }
    }
}

/// One provider's side of the quota conversation: what to run, what to write, which line is the
/// reply, and how to read it.
pub(crate) trait QuotaProbe: Send + Sync {
    /// The arguments after the binary.
    fn args(&self) -> &'static [&'static str];

    /// The lines written to the CLI's stdin.
    fn input(&self) -> Vec<String>;

    /// A conversation bound to this run's account and working files. The default uses the
    /// provider's shared conversation unchanged. A prepared conversation is kept until the
    /// process has ended so its exchange can verify the account and own its temporary files.
    fn prepare(&self, _run: &ProbeRun<'_>) -> Result<Option<Box<dyn QuotaProbe>>, String> {
        Ok(None)
    }

    /// The arguments and additional variables for this run. A refusal here stops the reading.
    fn launch(&self, _run: &ProbeRun<'_>) -> Result<ProbeLaunch, String> {
        Ok(ProbeLaunch::as_account(self.args()))
    }

    /// How the CLI's messages are delimited, both ways. One per line unless a provider says
    /// otherwise.
    fn framing(&self) -> Framing {
        Framing::Lines
    }

    /// The input in the steps it is written in: the first at the start, each later one once
    /// [`QuotaProbe::advances`] accepts a message. By default all of [`QuotaProbe::input`] at
    /// once; a provider whose CLI answers requests in no fixed order puts a request that builds
    /// on an earlier answer in a later step.
    fn steps(&self) -> Vec<Vec<String>> {
        vec![self.input()]
    }

    /// Whether `value` is the answer the next step of [`QuotaProbe::steps`] waits for.
    fn advances(&self, _value: &Value) -> bool {
        false
    }

    /// Whether `value` is the reply that is read.
    fn is_reply(&self, value: &Value) -> bool;

    /// The part of another reply that [`QuotaProbe::read`] needs along with the reply: who is
    /// signed in, as the provider says it.
    fn companion(&self, _value: &Value) -> Option<Value> {
        None
    }

    /// Whether the exchange must hold the companion before the reply can be read, which is decided
    /// by the reply, so that a reading that needs no companion is not held up for one.
    fn needs_companion(&self, _reply: &Value) -> bool {
        false
    }

    /// Reads `reply`, with the companion when the exchange held one.
    fn read(&self, reply: &Value, companion: Option<&Value>) -> Result<Reading, ReplyProblem>;

    /// Whether a line other than the reply is an error or sign-in status message from the CLI,
    /// which the run keeps in its notes. By default none is: a provider says which of its lines are.
    fn is_error_like(&self, _line: &Value) -> bool {
        false
    }

    /// Whether the companion alone says the account is signed out, asked when the exchange failed
    /// before a reply (Claude Code's sign-in state is in a reply other than the one that is read).
    fn signed_out_without_reply(&self, _companion: Option<&Value>) -> bool {
        false
    }

    /// The configuration directory a failure's text says is missing, when the CLI refuses one that
    /// way (which is how the default account's missing directory shows, since nothing is looked
    /// for before its probe is run).
    fn missing_config_dir<'a>(&self, _detail: &'a str) -> Option<&'a str> {
        None
    }

    /// What the provider said that makes the account read as not signed in, in words fit for the
    /// log (see `diagnostics::redact`).
    fn signed_out_reason(&self, reply: Option<&Value>, companion: Option<&Value>) -> String;

    /// The replies a stand-in must give before the final request is sent.
    #[cfg(test)]
    fn test_start_replies(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    /// What the tests of the generic probe need to stand in for this provider's CLI. Every probe
    /// brings its own, so that a provider added with one is run by them.
    #[cfg(test)]
    fn test_fixture(&self) -> ProbeFixture;
}

/// A probe's stand-in data for the generic probe tests.
#[cfg(test)]
pub(crate) struct ProbeFixture {
    /// The final stdin request's marker, or `None` for a probe that sends no input.
    pub request_marker: Option<&'static str>,
    /// A reply the probe reads.
    pub reply: &'static str,
    /// A reply in the answering line's place that is in no shape the probe knows.
    pub unreadable_reply: &'static str,
    /// A line other than the reply that the CLI writes on an error, which the probe's
    /// `is_error_like` accepts and which mentions `refresh failed for me@example.com`; `None` for
    /// a probe that counts no line as an error.
    pub error_line: Option<&'static str>,
}

/// The JSON-RPC response to the request with id 2, as opposed to a notification or a request the
/// server makes of its own.
pub(crate) fn is_second_response(value: &Value) -> bool {
    json_rpc::is_reply_to(value, 2)
}

/// Runs the probe for `agent` and reads its reply. Blocking; setting `cancel` stops it, which
/// reports as a failure. Notes in `trace` what is worth logging of the run: the process's pid,
/// how long it ran, its stderr, whether Octoboard had to end it, what the agent said that made
/// the account read as not signed in, and any error-like line it printed besides the reply.
/// `run_homes` is where a provider keeps the directories of Octoboard's own it runs a probe
/// against (`AccountOps::run_home`).
pub(super) fn probe(
    agent: AgentId,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    run_homes: &Path,
    timeout: Duration,
    cancel: &AtomicBool,
    trace: &mut ProbeTrace,
) -> Result<Reading, QuotaUnavailable> {
    let Some(conversation) = provider(agent).accounts().and_then(|ops| ops.quota_probe()) else {
        return Err(failed(agent, "it has no quota to read"));
    };
    probe_with(
        agent,
        Probing {
            conversation,
            probes_dir: run_homes,
        },
        shell_env,
        config_dir,
        timeout,
        cancel,
        trace,
    )
}

/// What [`probe_with`] runs: the provider's side, and the parent of the directories probes run in
/// in place of an account's own.
pub(super) struct Probing<'a> {
    pub conversation: &'a dyn QuotaProbe,
    pub probes_dir: &'a Path,
}

/// [`probe`] with the provider's side and the parent of the probes' own directories given, which
/// is what lets a test run a probe no registered provider has, in a directory of the test's.
pub(super) fn probe_with(
    agent: AgentId,
    probing: Probing<'_>,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
    cancel: &AtomicBool,
    trace: &mut ProbeTrace,
) -> Result<Reading, QuotaUnavailable> {
    let Probing {
        conversation,
        probes_dir,
    } = probing;
    let deadline = Instant::now() + timeout;
    let admitted = account_cli::admitted_command_in(
        agent,
        shell_env,
        config_dir,
        BackgroundRun::QuotaReading,
        probes_dir,
        &RunWait {
            until: deadline,
            cancel: Some(cancel),
        },
        &[],
    );
    let mut run = admitted.map_err(|refused| match refused {
        NotRun::DirMissing(dir) => config_dir_missing(agent, &dir.to_string_lossy()),
        NotRun::DirNotExportable(..) | NotRun::HomeNotPrepared(_) => {
            failed(agent, &refused.detail())
        }
        NotRun::BinaryNotFound(binary) => QuotaUnavailable::new(
            code::BINARY_NOT_FOUND,
            refused.detail(),
            &[("binary", binary)],
        ),
        NotRun::BinaryRefused(refusal) => QuotaUnavailable {
            code: refusal.code,
            params: refusal.params,
            message: refusal.message,
        },
    })?;
    let context = ProbeRun {
        account_dir: config_dir,
        own_dir: run.home_dir(),
    };
    let prepared = conversation
        .prepare(&context)
        .map_err(|detail| failed(agent, &detail))?;
    let conversation = prepared.as_deref().unwrap_or(conversation);
    let launch = conversation
        .launch(&context)
        .map_err(|detail| failed(agent, &detail))?;
    run.command
        .args(&launch.args)
        .envs(launch.env.iter().map(|(name, value)| (name, value)));
    let steps = conversation.steps();
    let timeout = deadline.saturating_duration_since(Instant::now());
    let bounds = Bounds::agent_query(timeout, OUTPUT_LIMIT);
    // The reply that is read, and the part of another reply that says who is signed in: the two
    // arrive in either order.
    let reply: RefCell<Option<Value>> = RefCell::new(None);
    let companion: RefCell<Option<Value>> = RefCell::new(None);
    let notes: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let mut exchange = ExchangeTrace::default();
    let result = run_conversation_traced(
        &mut run.command,
        Conversation {
            framing: conversation.framing(),
            steps: &steps,
        },
        &bounds,
        cancel,
        AGENT_REPLY_GRACE,
        &mut exchange,
        |line| {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                return Heard::Nothing;
            };
            if let Some(part) = conversation.companion(&value) {
                *companion.borrow_mut() = Some(part);
            }
            // Asked before the reply is kept, which takes the message.
            let advances = conversation.advances(&value);
            if conversation.is_reply(&value) {
                *reply.borrow_mut() = Some(value);
            } else if conversation.is_error_like(&value) && notes.borrow().len() < MAX_NOTES {
                notes.borrow_mut().push(truncate(&redact(line), LINE_LIMIT));
            }
            let complete = reply.borrow().as_ref().is_some_and(|reply| {
                !conversation.needs_companion(reply) || companion.borrow().is_some()
            });
            if complete {
                Heard::Reply
            } else if advances {
                Heard::Next
            } else {
                Heard::Nothing
            }
        },
    );
    trace.pid = exchange.pid;
    trace.elapsed = exchange.elapsed;
    trace.stderr_tail = stderr_tail(&exchange.stderr);
    trace.notes = notes.take();
    match &result {
        Err(failure) => trace.stopped = stopped_by_us(failure, exchange.pid.is_some()),
        // A program that exits by itself after its reply is the normal end of a reading.
        Ok(_) => {
            if let Some(AfterReply::Killed { after, why }) = &exchange.after_reply {
                trace.stopped = Some(format!("stopped {after:?} after its reply: {why}"));
            }
        }
    }
    result.map_err(|failure| match failure {
        _ if conversation.signed_out_without_reply(companion.borrow().as_ref()) => {
            trace.reason = Some(conversation.signed_out_reason(None, companion.borrow().as_ref()));
            not_signed_in(agent)
        }
        ExchangeFailure::TimedOut(_) => QuotaUnavailable::new(
            code::TIMED_OUT,
            format!("{} did not report its quota in time", agent.label()),
            &[("agent", agent.label())],
        ),
        other => {
            let detail = other.to_string();
            match conversation.missing_config_dir(&detail) {
                Some(dir) => config_dir_missing(agent, dir),
                None => failed(agent, &detail),
            }
        }
    })?;
    let value = reply.take().ok_or_else(|| unreadable(agent))?;
    let companion = companion.take();
    conversation
        .read(&value, companion.as_ref())
        .map_err(|problem| match problem {
            ReplyProblem::NotSignedIn => {
                trace.reason =
                    Some(conversation.signed_out_reason(Some(&value), companion.as_ref()));
                not_signed_in(agent)
            }
            ReplyProblem::Unreadable => unreadable(agent),
            ReplyProblem::NoWindows => QuotaUnavailable::new(
                code::NO_WINDOWS,
                format!("{} reports no quota for this account", agent.label()),
                &[("agent", agent.label())],
            ),
            ReplyProblem::Error(detail) => failed(agent, &detail),
        })
}

/// Why Octoboard ended the process before it ended itself, from how the exchange failed;
/// `started` is whether there was a process at all. A child that closed its output by itself was
/// not ended by Octoboard. A process that had to be killed after its reply is told from the
/// run's `after_reply`, not from a failure.
fn stopped_by_us(failure: &ExchangeFailure, started: bool) -> Option<String> {
    match failure {
        ExchangeFailure::TimedOut(timeout) => Some(format!("no reply within {timeout:?}")),
        ExchangeFailure::StdoutExceeded(limit) => Some(format!("output went past {limit} bytes")),
        ExchangeFailure::Cancelled => Some("the daemon is stopping".to_string()),
        ExchangeFailure::Failed(_) if started => Some("its output could not be read".to_string()),
        ExchangeFailure::Failed(_) | ExchangeFailure::ClosedWithoutReply { .. } => None,
    }
}

/// Whether a line carries what a CLI plainly marks as an error or sign-in status: a `type` of
/// `auth_status` or `error`, or an `error` field. The reading of the providers whose CLIs mark
/// them that way, for their [`QuotaProbe::is_error_like`].
pub(crate) fn marks_error(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(Value::as_str),
        Some("auth_status" | "error")
    ) || value.get("error").is_some_and(|error| !error.is_null())
}

pub(super) fn config_dir_missing(agent: AgentId, dir: &str) -> QuotaUnavailable {
    QuotaUnavailable::new(
        code::CONFIG_DIR_MISSING,
        format!(
            "{}'s configuration directory {dir} does not exist yet",
            agent.label()
        ),
        &[("agent", agent.label()), ("dir", dir)],
    )
}
pub(super) fn not_signed_in(agent: AgentId) -> QuotaUnavailable {
    QuotaUnavailable::new(
        code::NOT_SIGNED_IN,
        format!("{} is not signed in for this account", agent.label()),
        &[("agent", agent.label())],
    )
}
pub(super) fn failed(agent: AgentId, detail: &str) -> QuotaUnavailable {
    QuotaUnavailable::new(
        code::COMMAND_FAILED,
        format!("reading {}'s quota failed: {detail}", agent.label()),
        &[("agent", agent.label()), ("detail", detail)],
    )
}

fn unreadable(agent: AgentId) -> QuotaUnavailable {
    QuotaUnavailable::new(
        code::UNREADABLE_REPLY,
        format!(
            "{} answered with a quota that could not be read",
            agent.label()
        ),
        &[("agent", agent.label())],
    )
}

/// The reading in a reply with `windows`, or the problem when there is none: no window at all is
/// `NoWindows` where the reply says plainly that the account has no limits, else `Unreadable`.
pub(crate) fn reading(
    plan: Option<&Value>,
    windows: Vec<QuotaWindow>,
    none_is_plain: bool,
) -> Result<Reading, ReplyProblem> {
    if windows.is_empty() {
        return Err(if none_is_plain {
            ReplyProblem::NoWindows
        } else {
            ReplyProblem::Unreadable
        });
    }
    Ok(Reading {
        plan: crate::text::trimmed_or_none(plan.and_then(Value::as_str)),
        windows,
        reset_credits: None,
    })
}
/// Who a refused quota request leaves the account as, told from `signed_out`: `Some(true)` when
/// the agent says nothing is signed in, `Some(false)` when it says something else is, and `None`
/// when what it says cannot be read.
pub(crate) fn refusal_problem(signed_out: Option<bool>, agent_name: &str) -> ReplyProblem {
    match signed_out {
        Some(true) => ReplyProblem::NotSignedIn,
        Some(false) => ReplyProblem::NoWindows,
        None => ReplyProblem::Error(format!(
            "{agent_name} refused the quota request and did not say whether it is signed in"
        )),
    }
}
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
