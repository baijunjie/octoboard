//! The wire types of the daemon's only external interface. `apps/daemon/PROTOCOL.md` is the
//! specification; this module is its Rust form, split into a file per concern, most of them one
//! section of it. The sections are re-exported here, as the protocol is one namespace: a type is
//! named `crate::protocol::X` whichever section declares it.

mod accounts;
mod agents;
mod browse;
mod codes;
mod errors;
mod events;
mod fields;
mod history;
mod models;
mod path_links;
mod quotas;
mod records;
mod requests;
mod session_branches;
mod settings;
mod terminal;

pub use accounts::*;
pub use agents::*;
pub use browse::*;
pub use codes::*;
pub use errors::*;
pub use events::*;
pub use fields::*;
pub use history::*;
pub use models::*;
pub use path_links::*;
pub use quotas::*;
pub use records::*;
pub use requests::*;
pub use session_branches::*;
pub use settings::*;
pub use terminal::*;

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod bindings;
