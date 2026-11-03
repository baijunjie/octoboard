//! Carrying the trust a provider records in a per-session home over to the user's own record
//! (`Capabilities::per_session_trust`): what core asks of the provider's carrier, and the failure
//! it answers with.

use std::path::Path;
use std::time::Duration;

use crate::protocol::trust_reason;

/// Where a provider records the trust its confirmation grants when that record cannot reach the
/// user's own configuration by itself, and how it is carried there. Once the confirmation is
/// accepted — by Octoboard's press or by the person in the terminal — the entry the provider wrote
/// is carried, unchanged, into the user's own store, every other entry there left as it is.
pub trait TrustCarrier: Send + Sync {
    /// Waits for the entry the provider writes once its confirmation is accepted and carries it
    /// into the user's own store.
    fn carry(&self) -> std::result::Result<(), CarryFailure>;
    /// The longest a carry can take once the hook that starts it has come.
    fn longest(&self) -> Duration;
    /// The user's own store the entry is carried into, for the error that says it was not.
    fn user_store(&self) -> &Path;
}

/// Why a provider's entry did not reach the user's store: one of [`trust_reason`]'s carry codes,
/// the English account of it, and what that account is filled with, if anything.
#[derive(Debug, Clone)]
pub struct CarryFailure {
    pub(crate) reason_code: &'static str,
    pub(crate) reason: String,
    pub(crate) detail: Option<String>,
}

impl CarryFailure {
    pub(crate) fn new(
        reason_code: &'static str,
        reason: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            reason_code,
            reason: reason.into(),
            detail,
        }
    }

    pub(crate) fn io(err: impl std::fmt::Display) -> Self {
        let detail = format!("{err:#}");
        Self::new(
            trust_reason::STORE_IO_FAILED,
            format!("the file could not be read or written: {detail}"),
            Some(detail),
        )
    }
}
