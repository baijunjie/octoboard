//! The wire types of the daemon's only external interface. `apps/daemon/PROTOCOL.md` is the
//! specification; this module is its Rust form, split into a file per concern, most of them one
//! section of it. The sections are re-exported here, as the protocol is one namespace: a type is
//! named `crate::protocol::X` whichever section declares it.

mod accounts;
mod agent_cli;
mod agents;
mod browse;
mod codes;
mod configuration;
mod daemon_update;
mod errors;
mod events;
mod fields;
mod git_workflow;
mod history;
mod host;
mod mcp_inventory;
mod models;
mod page_content;
mod path_links;
mod processes;
mod providers;
mod quotas;
mod records;
mod requests;
mod session_branches;
mod settings;
mod terminal;
mod version;

pub use accounts::*;
pub use agent_cli::*;
pub use agents::*;
pub use browse::*;
pub use codes::*;
pub use configuration::*;
pub use daemon_update::*;
pub use errors::*;
pub use events::*;
pub use fields::*;
pub use git_workflow::*;
pub use history::*;
pub use mcp_inventory::*;
pub use models::*;
pub use page_content::*;
pub use path_links::*;
pub use processes::*;
pub use providers::*;
pub use quotas::*;
pub use records::*;
pub use requests::*;
pub use session_branches::*;
pub use settings::*;
pub use terminal::*;
pub use version::{DAEMON_VERSION, GENERATION};

#[cfg(test)]
mod bindings;
#[cfg(test)]
mod compatibility;
#[cfg(test)]
mod crosscheck;
#[cfg(test)]
mod schema;
