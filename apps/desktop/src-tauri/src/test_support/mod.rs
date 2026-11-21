//! Helpers the tests of several modules share.

pub(crate) mod closed_port;

pub(crate) use closed_port::ClosedPort;

/// How long a test waits for something it expects to happen: a machine under load can stall for seconds, and a
/// wait returns as soon as its condition holds, so a generous value costs a passing test nothing.
pub(crate) const PATIENCE: std::time::Duration = std::time::Duration::from_secs(30);
