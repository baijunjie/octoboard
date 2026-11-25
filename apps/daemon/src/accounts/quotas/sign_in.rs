//! What a provider that Octoboard reads no plan quota for says of an account's sign-in
//! (`AccountOps::sign_in_state`). The account poll (`poll.rs`) takes this reading in the quota
//! reading's place and publishes it in the same [`AccountQuota`](crate::protocol::AccountQuota):
//! no plan and no windows, the email, and the not-signed-in reason when the account is signed
//! out. Everything that follows a reading's sign-in state follows it without knowing which kind
//! of reading it came from.

/// Whether an account is signed in, as the provider's own record or CLI says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SignInReading {
    /// Signed in, as the email the provider's record gives, when it gives one.
    SignedIn { email: Option<String> },
    /// The agent holds no sign-in for the account.
    SignedOut,
    /// The provider cannot tell: no record to read, one in a shape it does not know, or a query
    /// that failed. Says nothing about the account, so the reading carries no reason.
    CannotTell,
}
