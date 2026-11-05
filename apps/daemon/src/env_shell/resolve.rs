//! Resolving a binary name on a snapshot's `PATH`.

use std::collections::HashMap;

use anyhow::Result;

use crate::platform::shell::{self, ResolvedBinary};
use crate::protocol::{error_code, CodedError};

/// What starts the binary `name` resolves to on `env`'s `PATH` (`platform::shell::resolve_binary`),
/// or the coded error that it was not found.
pub fn resolve_binary(name: &str, env: &HashMap<String, String>) -> Result<ResolvedBinary> {
    shell::resolve_binary(name, env).ok_or_else(|| {
        CodedError::raised(
            error_code::BINARY_NOT_FOUND,
            format!("`{name}` was not found on PATH in the snapshotted shell environment"),
            &[("binary", name)],
        )
    })
}

/// Where the program `name` resolves to, for a tool the daemon runs with arguments of its own and
/// that is a program in its own right, never a script a host starts through an interpreter (`git`).
pub fn resolve_program(name: &str, env: &HashMap<String, String>) -> Result<String> {
    resolve_binary(name, env).map(|binary| binary.program.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binary_that_is_not_there_is_a_coded_error_naming_it() {
        let err = resolve_binary("octoboard-no-such-tool", &HashMap::new())
            .expect_err("nothing on an empty environment");
        assert!(format!("{err:#}").contains("octoboard-no-such-tool"));
        assert!(resolve_program("octoboard-no-such-tool", &HashMap::new()).is_err());
    }
}
