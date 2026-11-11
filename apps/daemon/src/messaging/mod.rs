//! The messages Octoboard writes into sessions: the delivery API and the per-session outbox every
//! one passes through, the fixed texts and openings it writes in its own name, the reports a bound
//! session sends its owner, the retry of a Codex turn that stopped at its model's capacity, the
//! notices to an owner (one of them about a child left idle after its last report), how a message
//! names a session, and information one session shares with another.
//! [`write_queue`] is `AppState`'s side of the outbox: queuing, flushing and holding the queues,
//! and the reasons nothing is written into a session for now; [`restart_outbox`] carries what is
//! still queued over a planned stop to the next daemon; [`usage_limit`] holds what is written into
//! a session after a turn of it stopped on its account's usage limit.

pub(crate) mod capacity_retry;
pub(crate) mod delivery;
pub(crate) mod idle_children;
pub(crate) mod naming;
pub(crate) mod outbox;
pub(crate) mod owner_notices;
pub(crate) mod reporting;
pub(crate) mod restart_outbox;
pub(crate) mod sharing;
pub(crate) mod texts;
pub(crate) mod usage_limit;
pub(crate) mod write_queue;
