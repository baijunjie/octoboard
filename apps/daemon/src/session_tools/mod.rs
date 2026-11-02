//! The static description of the Octoboard MCP tools and what sessions are told about them: the
//! tool catalogue ([`catalogue`]), the role description a session is launched with ([`role`]), the
//! text of the console session's instruction file ([`console_instructions`]), whom a worker reports
//! to as that text depends on it ([`Owner`]), and the limit on a forwarded call ([`CALL_TIMEOUT`]).
//! Pure data and text, with no behavior of its own: whoever announces or runs the tools, or tells a
//! session about them, reads it from here.

pub mod catalogue;
pub mod console_instructions;
pub mod role;

use std::time::Duration;

/// The limit that ends a call forwarded to the daemon. Generous on purpose: `add_project` with a
/// `git` source runs a `git clone`, which is minutes on a large repository. A launch may raise an
/// agent's own shorter limit above this one, so that this one decides; how the limits are ordered
/// is in apps/daemon/PROTOCOL.md's "`POST /mcp/:token` — Octoboard's MCP tools" section.
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(600);

/// Who a worker reports to at launch, as far as what the session is told depends on it. A binding
/// can change while the session runs, and the session is told of that change in a message or, when
/// it binds itself, in the call's result; its role text, recorded once by some agents, is written
/// so that it still holds afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Console,
    /// A project coordinator, named by Octoboard's id for it: a
    /// title can be renamed after the text naming it has been recorded for good.
    Project {
        session_id: String,
    },
}
