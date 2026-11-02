use std::collections::HashMap;
use std::path::PathBuf;

use super::{account_dir, default_account_dir};

/// Each agent's configuration directory: the variable it reads it from, set in the shell
/// snapshot, else its own directory under the snapshot's home; an empty variable counts as
/// unset.
#[test]
fn each_agents_default_account_is_its_variable_or_its_directory_under_home() {
    use crate::protocol::Agent;
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
    use crate::protocol::Agent;
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
