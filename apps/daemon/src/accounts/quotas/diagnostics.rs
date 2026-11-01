//! What the daemon's log says about quota readings. A failed reading only makes the quota
//! unavailable in memory, so without this a lost sign-in or a probe that was killed leaves no trace
//! to find the cause from. The log is told of a change, not of every reading: an account going
//! from signed in to not signed in and back, a reading that starts failing or fails differently,
//! and every probe Octoboard itself ended. Everything that reaches the log passes through
//! [`redact`] first, since the text comes from an agent's CLI: no token, key or email is written.
//!
//! The decisions ([`classify`], [`advance`]) and the redaction are plain functions, tested on
//! their own; [`log_stop`] and [`log_events`] only write what they decided. The state they work
//! from, [`Observed`], is kept per account beside the readings (see `Inner::observed`).

use std::time::Duration;

use super::judgement::{is_account_state, is_failure};
use super::Key;
use crate::protocol::AccountQuota;

/// How much of a probe's stderr is logged: the end of what was kept, by bytes and by lines.
const STDERR_TAIL_BYTES: usize = 2048;
const STDERR_TAIL_LINES: usize = 8;

/// The longest a single line of agent text is logged.
pub(super) const LINE_LIMIT: usize = 300;

/// How many lines of an agent's own stdout, besides the reply, are kept (see
/// [`ProbeTrace::notes`]).
pub(super) const MAX_NOTES: usize = 3;

/// What one probe run left to log, collected while it ran. All text is already redacted.
#[derive(Debug, Default)]
pub(super) struct ProbeTrace {
    /// The agent process's pid; `None` when it was never started.
    pub pid: Option<u32>,
    pub elapsed: Duration,
    /// The end of the stderr kept; empty when the agent wrote none.
    pub stderr_tail: String,
    /// Why Octoboard ended the process before it ended by itself, if it did: a failed run it
    /// stopped, or a process that had not exited when the time it is given after its reply ran
    /// out. A process that exits by itself after its reply is the normal end of a reading and is
    /// not recorded here.
    pub stopped: Option<String>,
    /// What the agent reported that made the account read as not signed in.
    pub reason: Option<String>,
    /// Lines the agent printed besides the reply that look like an error or a sign-in status.
    pub notes: Vec<String>,
}

/// What a reading says of the account's sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Outcome {
    /// Read, or read as having no quota to report: credentials of some kind are in place.
    SignedIn,
    /// The account's directory is gone or the agent says it holds no credential.
    SignedOut,
    /// The reading failed, which says nothing of the sign-in. Holds the redacted message.
    Failed(String),
}

/// What is known of an account from the readings so far.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Observed {
    /// Whether the last reading that could tell found the account signed in.
    signed_in: Option<bool>,
    /// The message of the failure the last reading ended in, while the readings keep failing.
    failure: Option<String>,
}

/// What a reading changed that is worth a line in the log.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Event {
    /// Went from signed in to not signed in, `was` being the failure the readings before it ended
    /// in, if they were failing.
    SignedOut { was: Option<String> },
    /// Came back from not signed in.
    SignedIn,
    /// Started failing, or failed with a different message than the failure before it.
    Failing,
    /// Read as signed in again after failing, with the message it failed with last.
    Recovered(String),
}

/// Reads an account's reading as an [`Outcome`]; the message of a failure is redacted.
pub(super) fn classify(quota: &AccountQuota) -> Outcome {
    match &quota.unavailable {
        None => Outcome::SignedIn,
        Some(unavailable) if is_account_state(unavailable.code) => Outcome::SignedOut,
        Some(unavailable) if is_failure(unavailable.code) => {
            Outcome::Failed(redact(&unavailable.message))
        }
        // No quota to report: signed in with a key or a provider that has none.
        Some(_) => Outcome::SignedIn,
    }
}

/// Folds `outcome` into `observed` and returns what changed. Not the first sighting of a
/// signed-out account and not a repeated failure: only a change.
pub(super) fn advance(observed: &mut Observed, outcome: &Outcome) -> Vec<Event> {
    let mut events = Vec::new();
    if let Outcome::Failed(message) = outcome {
        if observed.failure.as_ref() != Some(message) {
            events.push(Event::Failing);
        }
        observed.failure = Some(message.clone());
        return events;
    }
    let was = observed.failure.take();
    let signed_in = *outcome == Outcome::SignedIn;
    match observed.signed_in.replace(signed_in) {
        Some(true) if !signed_in => events.push(Event::SignedOut { was }),
        previous => {
            // A failure that ends in not signed in is told by neither event: only a reading that
            // can be taken again is a recovery.
            if let (true, Some(message)) = (signed_in, was) {
                events.push(Event::Recovered(message));
            }
            if previous == Some(false) && signed_in {
                events.push(Event::SignedIn);
            }
        }
    }
    events
}

/// Writes what [`advance`] found for the reading `quota` of `key`, taken by the process `trace`
/// describes.
pub(super) fn log_events(key: &Key, quota: &AccountQuota, trace: &ProbeTrace, events: &[Event]) {
    let (agent, account) = (key.agent.label(), &key.account);
    let reason = reason_of(quota, trace);
    for event in events {
        match event {
            Event::SignedOut { was } => tracing::warn!(
                agent,
                account = ?account,
                pid = trace.pid,
                reason = %reason,
                was = was.as_deref(),
                stderr = %trace.stderr_tail,
                notes = ?trace.notes,
                "the account went from signed in to not signed in"
            ),
            Event::SignedIn => tracing::info!(
                agent,
                account = ?account,
                pid = trace.pid,
                "the account is signed in again"
            ),
            Event::Failing => tracing::warn!(
                agent,
                account = ?account,
                pid = trace.pid,
                elapsed_ms = trace.elapsed.as_millis() as u64,
                err = %reason,
                stderr = %trace.stderr_tail,
                notes = ?trace.notes,
                "reading the account's quota failed"
            ),
            Event::Recovered(was) => tracing::info!(
                agent,
                account = ?account,
                pid = trace.pid,
                was = %was,
                "the account's quota is readable again"
            ),
        }
    }
}

/// What the judgement went on: the agent's own sign-in signal when the probe noted one, else the
/// message of the reading's unavailable reason.
fn reason_of(quota: &AccountQuota, trace: &ProbeTrace) -> String {
    trace.reason.clone().unwrap_or_else(|| {
        quota
            .unavailable
            .as_ref()
            .map(|unavailable| redact(&unavailable.message))
            .unwrap_or_default()
    })
}

/// Writes that Octoboard ended the probe process, if `trace` says it did.
pub(super) fn log_stop(key: &Key, trace: &ProbeTrace) {
    if let Some(reason) = &trace.stopped {
        tracing::warn!(
            agent = key.agent.label(),
            account = ?key.account,
            pid = trace.pid,
            elapsed_ms = trace.elapsed.as_millis() as u64,
            %reason,
            stderr = %trace.stderr_tail,
            "stopped the quota probe before it finished"
        );
    }
}

/// The end of a probe's stderr as one redacted line: the last [`STDERR_TAIL_LINES`] lines of the
/// last [`STDERR_TAIL_BYTES`] bytes, joined with ` | `.
pub(super) fn stderr_tail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let mut start = text.len().saturating_sub(STDERR_TAIL_BYTES);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let lines: Vec<&str> = text[start..]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let tail = &lines[lines.len().saturating_sub(STDERR_TAIL_LINES)..];
    tail.iter()
        .map(|line| truncate(&redact(line), LINE_LIMIT))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// `text` cut to at most `limit` characters, marked when it was.
pub(super) fn truncate(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// Characters that end a word for [`redact`].
fn is_separator(c: char) -> bool {
    c.is_whitespace() || "\"'<>()[]{},;|".contains(c)
}

/// Words after which the next word is a credential.
const SCHEMES: [&str; 2] = ["bearer", "basic"];

/// Names of secrets: a word that is or ends in one (`token`, `refresh_token`, `Authorization`),
/// and the value in `name=value` and `name:value`, are masked.
const SECRET_NAMES: [&str; 8] = [
    "token",
    "secret",
    "password",
    "passwd",
    "apikey",
    "api_key",
    "authorization",
    "credential",
];

/// Prefixes of the credentials services issue.
const SECRET_PREFIXES: [&str; 9] = [
    "sk-",
    "sk_",
    "ghp_",
    "gho_",
    "github_pat_",
    "xox",
    "aiza",
    "ya29.",
    "eyj",
];

const MASK: &str = "[redacted]";

/// `text` with what looks like a credential or an email address masked: an `sk-…`-style key, a
/// JWT, any long run of key characters mixing letters and digits, the word after `Bearer` or
/// after the name of a secret (`"refresh_token": "…"`, `password: …`, `Authorization: …`), the
/// value of `token=…`-style pairs, and anything with an `@` in the shape of an address. It errs
/// on the side of masking too much; the log needs the shape of an error, never the secret in it.
pub(super) fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let mut mask_next = false;
    let mut flush = |word: &mut String, out: &mut String| {
        if word.is_empty() {
            return;
        }
        let lower = word.to_lowercase();
        let bare = lower.trim_matches([':', '=']);
        if bare.is_empty() {
            // A lone `:` or `=` between a name and its value.
            out.push_str(word);
        } else if mask_next && !SCHEMES.contains(&bare) {
            out.push_str(MASK);
            mask_next = false;
        } else {
            // A scheme right after a name keeps the mask for the credential after it.
            mask_next = mask_next
                || SCHEMES.contains(&bare)
                || SECRET_NAMES.iter().any(|name| bare.ends_with(name));
            out.push_str(&redact_word(word));
        }
        word.clear();
    };
    for c in text.chars() {
        if is_separator(c) {
            flush(&mut word, &mut out);
            out.push(c);
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn redact_word(word: &str) -> String {
    if let Some((name, value)) = word.split_once(['=', ':']) {
        let lower = name.to_lowercase();
        if !value.is_empty() && SECRET_NAMES.iter().any(|part| lower.contains(part)) {
            return format!("{name}={MASK}");
        }
    }
    if is_email(word) || has_secret_prefix(word) || is_long_key(word) {
        return MASK.to_string();
    }
    // A pair such as `error=<value>`: the value may be a credential on its own.
    if let Some((name, value)) = word.split_once('=') {
        if !value.is_empty() && (is_email(value) || has_secret_prefix(value) || is_long_key(value))
        {
            return format!("{name}={MASK}");
        }
    }
    word.to_string()
}

fn is_email(word: &str) -> bool {
    word.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
    })
}

fn has_secret_prefix(word: &str) -> bool {
    let lower = word.to_lowercase();
    SECRET_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// A long run of base64, hex or URL-safe characters mixing letters and digits, as keys, tokens and
/// ids are.
fn is_long_key(word: &str) -> bool {
    word.chars().count() >= 32
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+/=_-.".contains(c))
        && word.chars().any(|c| c.is_ascii_digit())
        && word.chars().any(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests;
