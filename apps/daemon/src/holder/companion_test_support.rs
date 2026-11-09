//! What the tests of both halves of the holder share when they run companions: shells that make
//! the socket they are waited for on and stay up.

use std::path::PathBuf;

use super::protocol::{CompanionEndpoint, EndpointKind};
use crate::platform::term::Command;

pub(crate) fn socket_kind() -> EndpointKind {
    EndpointKind::Socket {
        prefix: "octo-test".to_string(),
        name: "rpc".to_string(),
    }
}

/// A companion that makes the file at `path`, as a server makes its socket once it listens, and
/// stays up.
pub(crate) fn touching(path: &str) -> Command {
    let script = format!("touch '{path}'; exec sleep 60");
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script.as_str()]);
    command
}

/// The directory a socket endpoint lives in.
pub(crate) fn directory_of(endpoint: &CompanionEndpoint) -> PathBuf {
    match endpoint {
        CompanionEndpoint::Socket { path } => PathBuf::from(path)
            .parent()
            .expect("a socket in a directory")
            .to_path_buf(),
        other => panic!("not a socket: {other:?}"),
    }
}
