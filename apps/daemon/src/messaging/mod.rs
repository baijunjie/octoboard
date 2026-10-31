//! The messages Octoboard writes into sessions: the delivery API and the per-session outbox every
//! one passes through, the fixed texts and openings it writes in its own name, the reports a bound
//! session sends its owner, the notices to an owner, and information one session shares with
//! another.

pub(crate) mod delivery;
pub(crate) mod outbox;
pub(crate) mod owner_notices;
pub(crate) mod reporting;
pub(crate) mod sharing;
pub(crate) mod texts;
