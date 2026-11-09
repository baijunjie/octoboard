//! Process holders: processes that own the PTYs of the programs the daemon starts, so that the
//! daemon speaks to a terminal over a link instead of holding its descriptor.
//!
//! - [`protocol`]: the frames the daemon and a holder exchange.
//! - [`registry`]: the file a holder is found by.
//! - [`compat`]: which holders a daemon adopts, by version.
//! - [`quit_targets`]: the holders a quit left to end, kept in a file until each is gone.
//! - [`build`]: a build's identity, and the copy of it the holders run from.
//! - [`server`]: the holder itself, the `octoboardd holder` mode, which also answers the helpers of
//!   the launches it runs and relays what they ask to the daemon.
//! - [`client`]: the daemon's side: starting a holder, the link to it, and the terminals it holds.

use std::time::Duration;

pub use build::build_id;

pub mod build;
pub mod client;
pub mod compat;
pub mod protocol;
pub mod quit_targets;
pub mod registry;
pub mod server;

#[cfg(all(test, unix))]
mod companion_test_support;

/// How long a request a helper waits on (the revert gate's question, an MCP call) waits for a
/// daemon to link when none is. It covers an ordinary daemon restart: the desktop application
/// restarts a dead daemon after a second and a start takes a few seconds. Past it the request
/// fails as one made with no daemon running does.
pub const RELAY_DEADLINE: Duration = Duration::from_secs(30);
