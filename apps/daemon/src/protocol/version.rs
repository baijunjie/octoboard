//! Which protocol the daemon speaks, as the `hello` frame states it. See "Versions and
//! compatibility" in `apps/daemon/PROTOCOL.md` for when the generation is raised.

/// The protocol generation. Within one generation the protocol only grows; a break (a removed or
/// renamed request, event, field or value, a changed type, a newly required input, a changed
/// meaning) raises it, and a client of another generation stops at the `hello`.
pub const GENERATION: u32 = 1;

/// The daemon's release version, the workspace version, for a client to name when the
/// generations differ.
pub const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");
