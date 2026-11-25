//! A provider's helpers: small commands of the provider's own that its launch names for the agent
//! to run where the agent takes a command (a title command, say), run as a mode of the daemon's
//! own executable (`octoboardd agent-helper`) so that no script of the host's is needed for them.
//! The command line is a POSIX shell line, which the agent runs through its own shell as it runs
//! the status hooks' command; the shell then hands over to the daemon's executable. Core only
//! builds the command line and dispatches the mode to the provider
//! ([`super::AgentProvider::run_helper`]).

use std::path::Path;

use crate::platform::shell::{self, Arg, Stderr};
use crate::protocol::AgentId;

/// The command line that runs `agent`'s helper `name` with `args`, through a POSIX shell as the
/// status hooks' command is: the shell is replaced by the daemon's executable, `self_exe`, the
/// installation's entry point that an update replaces in place. Stderr is dropped, since the agent
/// may show it; stdout is what the agent reads.
pub fn helper_command(self_exe: &str, agent: AgentId, name: &str, args: &[&str]) -> String {
    let mut line = vec![
        Arg::Word("agent-helper"),
        Arg::Word("--provider"),
        Arg::Word(agent.descriptor().id),
        Arg::Word("--helper"),
        Arg::Value(name),
    ];
    if !args.is_empty() {
        line.push(Arg::Word("--"));
        line.extend(args.iter().map(|arg| Arg::Value(arg)));
    }
    shell::command_line(Path::new(self_exe), &line, Stderr::Discard)
}

/// Runs the helper `name` of the provider whose id is `provider` with `args`, returning the exit
/// status to leave with. A provider that is not registered, or has no such helper, prints nothing:
/// the agent's command then shows nothing, as an absent one would. Whatever the agent wrote to the
/// input and the helper did not read is drained after it, whichever ran, so that the agent never
/// meets a closed pipe for writing more than the helper read.
pub fn run(provider: &str, name: &str, args: &[String]) -> i32 {
    let ran = AgentId::from_wire_name(provider)
        .and_then(|agent| super::provider(agent).run_helper(name, args));
    let _ = std::io::copy(&mut std::io::stdin(), &mut std::io::sink());
    ran.unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The command names the mode, the provider by its id and the helper, then the helper's own
    /// arguments after `--`, each value quoted as one shell word.
    #[test]
    fn the_command_names_the_provider_the_helper_and_its_arguments() {
        let command = helper_command(
            "/opt/octoboard/octoboardd",
            AgentId::TEST,
            "title",
            &["/run/it's here"],
        );
        let id = AgentId::TEST.descriptor().id;
        assert_eq!(
            command,
            format!(
                "exec '/opt/octoboard/octoboardd' agent-helper --provider {id} --helper 'title' -- \
                 '/run/it'\\''s here' 2>/dev/null"
            )
        );
        let bare = helper_command("/opt/octoboard/octoboardd", AgentId::TEST, "title", &[]);
        assert!(!bare.contains(" -- "), "{bare}");
    }
}
