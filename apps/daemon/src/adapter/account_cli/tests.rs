use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{account_dir, admitted_command, default_account_dir, NotRun};
use crate::protocol::Agent;
use crate::test_support::ScratchDir;

/// Each agent's configuration directory: the variable it reads it from, set in the shell
/// snapshot, else its own directory under the snapshot's home; an empty variable counts as
/// unset.
#[test]
fn each_agents_default_account_is_its_variable_or_its_directory_under_home() {
    for (agent, var, dir) in [
        (Agent::Claude, "CLAUDE_CONFIG_DIR", ".claude"),
        (Agent::Codex, "CODEX_HOME", ".codex"),
        (Agent::Grok, "GROK_HOME", ".grok"),
    ] {
        assert_eq!(crate::agents::layout::layout(agent).config_dir_var, var);
        let home = HashMap::from([("HOME".to_string(), "/home/user".to_string())]);
        assert_eq!(
            default_account_dir(agent, &home),
            PathBuf::from("/home/user").join(dir)
        );
        let mut exported = home.clone();
        exported.insert(var.to_string(), String::new());
        assert_eq!(
            default_account_dir(agent, &exported),
            PathBuf::from("/home/user").join(dir)
        );
        exported.insert(var.to_string(), "/elsewhere".to_string());
        assert_eq!(
            default_account_dir(agent, &exported),
            PathBuf::from("/elsewhere")
        );
    }
}

/// An account's own directory is what its sessions run in, whatever the shell exports; the
/// default account's follows the shell.
#[test]
fn a_pinned_account_dir_wins_over_the_default_one() {
    let shell = HashMap::from([
        ("HOME".to_string(), "/home/user".to_string()),
        ("CLAUDE_CONFIG_DIR".to_string(), "/exported".to_string()),
    ]);
    assert_eq!(
        account_dir(Agent::Claude, Some(std::path::Path::new("/pinned")), &shell),
        PathBuf::from("/pinned")
    );
    assert_eq!(
        account_dir(Agent::Claude, None, &shell),
        PathBuf::from("/exported")
    );
}

/// A shell environment whose `PATH` holds no agent binary.
fn no_agents(dir: &Path) -> HashMap<String, String> {
    HashMap::from([
        ("PATH".to_string(), dir.to_string_lossy().into_owned()),
        ("HOME".to_string(), dir.to_string_lossy().into_owned()),
    ])
}

/// A gone directory is refused before the binary is looked for, so the account's missing
/// directory is what a caller reports even where the CLI is not installed either.
#[test]
fn a_missing_directory_is_refused_before_the_binary_is_resolved() {
    let dir = ScratchDir::new("cli-run-missing");
    let gone = dir.join("gone");
    let refused = admitted_command(Agent::Codex, &no_agents(&dir), Some(&gone), &[])
        .err()
        .expect("refused");
    assert!(matches!(&refused, NotRun::DirMissing(missing) if *missing == gone));
    assert_eq!(
        refused.detail(),
        format!(
            "the account's configuration directory {} does not exist",
            gone.display()
        )
    );
}

#[test]
fn a_binary_off_the_path_is_named() {
    let dir = ScratchDir::new("cli-run-no-binary");
    let refused = admitted_command(Agent::Grok, &no_agents(&dir), Some(&dir), &[])
        .err()
        .expect("refused");
    assert!(matches!(refused, NotRun::BinaryNotFound("grok")));
    assert_eq!(
        refused.detail(),
        "`grok` was not found on PATH in the shell environment"
    );
}
