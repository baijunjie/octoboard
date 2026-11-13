//! The guard that keeps a project session to its own project, in two layers behind one setting.
//!
//! The edit layer ([`list`], [`hook`]): a list written into the session's scratch directory at
//! launch, and `octoboardd guard-hook`, the synchronous `PreToolUse` command an agent runs before
//! a file-edit tool call and which denies one that targets a path inside another registered
//! project. The git layer ([`repository`]): the verdict on whether the repository a session's git
//! is updating belongs to another registered project, which the session's git hooks act on.
//!
//! The edit layer's hook decides from the list alone ([`hook`]); it never contacts the daemon, so
//! it costs no round trip per edit and still works while the daemon is unreachable. The price is
//! that a project registered after the session started is not in its list. The list is written
//! only for a project session launched with the setting that keeps sessions to their own project
//! on ([`list::for_session`]); the git layer reads the setting at every ref update.

mod hook;
mod list;
mod repository;

pub use hook::run;
pub use list::{for_session, write, GuardList};
pub use repository::{foreign_repository, ForeignRepository};
