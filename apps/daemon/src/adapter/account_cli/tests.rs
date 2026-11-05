use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{account_dir, admitted_command, default_account_dir, NotRun};
use crate::protocol::AgentId;
use crate::test_support::ScratchDir;

/// Each agent's configuration directory: the variable it reads it from, set in the shell
/// snapshot, else its own directory under the snapshot's home; an empty variable counts as
/// unset.
#[test]
fn each_agents_default_account_is_its_variable_or_its_directory_under_home() {
    for (agent, var, dir) in [
        (AgentId::CLAUDE, "CLAUDE_CONFIG_DIR", ".claude"),
        (AgentId::CODEX, "CODEX_HOME", ".codex"),
        (AgentId::GROK, "GROK_HOME", ".grok"),
    ] {
        assert_eq!(crate::providers::layout::layout(agent).config_dir_var, var);
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
        account_dir(
            AgentId::CLAUDE,
            Some(std::path::Path::new("/pinned")),
            &shell
        ),
        PathBuf::from("/pinned")
    );
    assert_eq!(
        account_dir(AgentId::CLAUDE, None, &shell),
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
    let refused = admitted_command(AgentId::CODEX, &no_agents(&dir), Some(&gone), &[])
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
    let refused = admitted_command(AgentId::GROK, &no_agents(&dir), Some(&dir), &[])
        .err()
        .expect("refused");
    assert!(matches!(refused, NotRun::BinaryNotFound("grok")));
    assert_eq!(
        refused.detail(),
        "`grok` was not found on PATH in the shell environment"
    );
}

/// A launcher whose native program is missing is refused as a launch refuses it, so an account's
/// queries and sign-in report the provider's code rather than a binary that was not found.
#[cfg(windows)]
#[test]
fn a_launcher_without_its_native_program_is_refused_with_the_providers_code() {
    use crate::protocol::Code;
    use crate::providers::codex::NATIVE_PROGRAM_MISSING;

    let dir = ScratchDir::new("cli-run-shim");
    let env = crate::test_support::codex_shim_without_native_program(&dir);
    let refused = admitted_command(AgentId::CODEX, &env, Some(&dir), &[])
        .err()
        .expect("refused");
    let NotRun::BinaryRefused(refusal) = &refused else {
        panic!("refused for another reason: {refused:?}");
    };
    assert_eq!(refusal.code, Code::of_provider(NATIVE_PROGRAM_MISSING));
    assert!(matches!(
        super::resolve_account_binary(AgentId::CODEX, &env),
        Err(super::Unstartable::Refused(_))
    ));
    let listing = crate::agents::models::list(
        AgentId::CODEX,
        &env,
        Some(&dir),
        std::time::Duration::from_secs(5),
    );
    assert_eq!(
        listing.unavailable.map(|reason| reason.code),
        Some(Code::of_provider(NATIVE_PROGRAM_MISSING))
    );
}
