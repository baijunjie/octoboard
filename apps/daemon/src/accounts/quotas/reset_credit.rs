//! Using one of an account's rate limit reset credits for the user, for a provider that has them
//! (`Capabilities::reset_credits`): the provider's own request, run the way a quota probe is. Only
//! the user's own action starts it; nothing else in the daemon calls it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::probe::PROBE_TIMEOUT;
use crate::protocol::ResetCreditOutcome;

/// How long a use may take: the CLI's start and one request to the backend, as long as a probe may.
pub(crate) const CONSUME_TIMEOUT: Duration = PROBE_TIMEOUT;

/// Why a credit could not be used.
#[derive(Debug, PartialEq)]
pub(crate) enum Failure {
    /// The provider's binary does not resolve; carries its name.
    BinaryNotFound(String),
    /// The provider says the account is signed in to nothing it could use a credit with.
    NotSignedIn,
    /// The provider did not answer in time. The credit may have been used all the same.
    TimedOut,
    /// The provider could not be run, ended without answering, refused, or answered with a reply
    /// that could not be read; carries what it said.
    Failed(String),
}

/// How a provider uses a reset credit.
pub(crate) trait ResetCredits: Send + Sync {
    /// Asks the provider to use the credit `credit_id`, or the next one available when it is
    /// `None`, under `idempotency_key`. Blocking; setting `cancel` stops it, and it is refused
    /// before anything is run when it is set already.
    fn consume(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        credit_id: Option<&str>,
        idempotency_key: &str,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<ResetCreditOutcome, Failure>;
}
