//! Claude Code as a compatibility source (`ProviderDescriptor::compatibility_source`): the MCP
//! servers it would load for a session and the guidance to read the rest of its configuration,
//! offered to the providers made compatible with it. Read, never written: none of Claude Code's
//! files is touched.
//!
//! The servers are read from Claude Code's own scopes, merged the way it merges them (local over
//! project over user, by server name):
//!
//! - user: `mcpServers` at the top of the global config file (`.claude.json` inside the
//!   configuration directory in effect, else `~/.claude.json`);
//! - local: `projects.<working directory>.mcpServers` in the same file;
//! - project: the `.mcp.json` found from the working directory upwards, nearest first, and only
//!   the servers Claude Code would enable. A project server is used only once approved, so it needs
//!   `enableAllProjectMcpServers` or a place in `enabledMcpjsonServers`, and no place in
//!   `disabledMcpjsonServers`. Those come from the user's settings file, the project's
//!   `settings.local.json` and the project's entry in the global config file. The project's shared
//!   `settings.json` is not read for them, since a repository could otherwise approve its own
//!   servers; managed settings are not read either, so an approval given only there is missed (the
//!   server stays off, the safe side). Plugins' servers are not loaded.
//!
//! The files are read as the daemon reads anything in a project: inside the working directory
//! through `browse::live`, and for a directory above it or the home directory through a bounded
//! read of a regular file that is opened without blocking, so a FIFO cannot hold a launch.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use serde_json::Value;

use super::settings::{global_config_file, project_entry};
use crate::adapter::files::{
    canonical_project, is_agent_config_dir, read_bounded_text, read_regular_text, LARGE_FILE_LIMIT,
    SMALL_FILE_LIMIT,
};
use crate::adapter::CompatibilitySource;
use crate::protocol::AgentId;
use crate::providers::claude::CLAUDE_SETTINGS_FILE;

/// The consumer's name goes where this has `{CONSUMER}`, and Claude Code's user-level
/// configuration directory where it has `{CLAUDE_CONFIG_DIR}`.
const INSTRUCTIONS: &str = include_str!("compatibility.md");

pub(super) struct ClaudeCompatibility;

impl CompatibilitySource for ClaudeCompatibility {
    fn mcp_servers(
        &self,
        shell_env: &HashMap<String, String>,
        cwd: &Path,
    ) -> BTreeMap<String, Value> {
        load(shell_env, cwd)
    }

    /// Names the user-level configuration directory of Claude Code's default account as the
    /// launch's shell snapshot resolves it (`~/.claude` when it names none).
    fn instructions(&self, consumer_label: &str, shell_env: &HashMap<String, String>) -> String {
        INSTRUCTIONS.replace("{CONSUMER}", consumer_label).replace(
            "{CLAUDE_CONFIG_DIR}",
            &crate::adapter::default_account_dir(AgentId::CLAUDE, shell_env).to_string_lossy(),
        )
    }
}

/// The servers Claude Code would load, by name, the later scope replacing the earlier.
fn load(shell_env: &HashMap<String, String>, cwd: &Path) -> BTreeMap<String, Value> {
    let config_dir = crate::adapter::default_account_dir(AgentId::CLAUDE, shell_env);
    let project = canonical_project(cwd);
    let global = read_json(&global_config_file(shell_env, None), None, LARGE_FILE_LIMIT);
    let entry = global
        .as_ref()
        .and_then(|global| project_entry(global, cwd).or_else(|| project_entry(global, &project)));
    let mut servers = BTreeMap::new();
    let mut merge = |from: Option<&Value>, approved: &dyn Fn(&str) -> bool| {
        if let Some(map) = from.and_then(Value::as_object) {
            for (name, server) in map {
                if approved(name) {
                    servers.insert(name.clone(), server.clone());
                }
            }
        }
    };
    let disabled = entry
        .map(|entry| names(entry.get("disabledMcpServers")))
        .unwrap_or_default();
    let on = |name: &str| !disabled.contains(name);
    merge(
        global.as_ref().and_then(|global| global.get("mcpServers")),
        &on,
    );

    let approval = Approval::read(&config_dir, &project, entry);
    for dir in project.ancestors().collect::<Vec<_>>().into_iter().rev() {
        let file = read_json(&dir.join(".mcp.json"), Some(&project), SMALL_FILE_LIMIT);
        merge(
            file.as_ref().and_then(|file| file.get("mcpServers")),
            &|name| on(name) && approval.allows(name),
        );
    }
    merge(entry.and_then(|entry| entry.get("mcpServers")), &on);
    servers
}

/// Which servers of a `.mcp.json` Claude Code has been approved to start.
#[derive(Default)]
struct Approval {
    enable_all: bool,
    enabled: BTreeSet<String>,
    disabled: BTreeSet<String>,
}

impl Approval {
    fn read(config_dir: &Path, project: &Path, entry: Option<&Value>) -> Self {
        let mut approval = Self::default();
        // The project's `.claude` is Claude Code's own directory in a home opened as the project.
        let dir = project.join(".claude");
        let owner = (!is_agent_config_dir(&dir, config_dir)).then_some(project);
        let files = [
            read_json(
                &config_dir.join(CLAUDE_SETTINGS_FILE),
                None,
                SMALL_FILE_LIMIT,
            ),
            read_json(&dir.join("settings.local.json"), owner, SMALL_FILE_LIMIT),
            entry.cloned(),
        ];
        for file in files.iter().flatten() {
            if file
                .get("enableAllProjectMcpServers")
                .and_then(Value::as_bool)
                == Some(true)
            {
                approval.enable_all = true;
            }
            approval
                .enabled
                .extend(names(file.get("enabledMcpjsonServers")));
            approval
                .disabled
                .extend(names(file.get("disabledMcpjsonServers")));
        }
        approval
    }

    fn allows(&self, name: &str) -> bool {
        !self.disabled.contains(name) && (self.enable_all || self.enabled.contains(name))
    }
}

fn names(value: Option<&Value>) -> BTreeSet<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}

/// A JSON file's value; an absent file is `None` quietly, an unreadable or malformed one with a
/// log. A file in the tree of `project` (canonical) is read as the project's, one Claude Code's own
/// configuration holds (`None`) as a regular file wherever it lies.
fn read_json(path: &Path, project: Option<&Path>, limit: usize) -> Option<Value> {
    try_read_json(path, project, limit).ok().flatten()
}

/// [`read_json`], telling an absent file (`Ok(None)`) from one that cannot be read or parsed
/// (`Err`).
fn try_read_json(path: &Path, project: Option<&Path>, limit: usize) -> Result<Option<Value>, ()> {
    let read = match project {
        Some(project) => read_bounded_text(path, project, limit),
        None => read_regular_text(path, limit),
    };
    let text = match read {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "could not read a Claude Code file");
            return Err(());
        }
    };
    match serde_json::from_str(&text) {
        Ok(value) => Ok(Some(value)),
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "could not parse a Claude Code file");
            Err(())
        }
    }
}

/// The names of every MCP server Claude Code itself defines for a session in `cwd`, in its global
/// config file (user and local scopes) and in the `.mcp.json` files from the working directory
/// upwards (the config file of `pinned`, the session's own configuration directory, when there is
/// one), approved and enabled or not: a consumer must not put a server of the same name in
/// their place. `None` when one of those files exists and cannot be read or parsed, so that the
/// names are not known.
pub(super) fn defined_server_names(
    shell_env: &HashMap<String, String>,
    pinned: Option<&Path>,
    cwd: &Path,
) -> Option<BTreeSet<String>> {
    let project = canonical_project(cwd);
    let mut names = BTreeSet::new();
    let mut add = |from: Option<&Value>| {
        if let Some(map) = from.and_then(|from| from.get("mcpServers")?.as_object()) {
            names.extend(map.keys().cloned());
        }
    };
    let global = try_read_json(
        &global_config_file(shell_env, pinned),
        None,
        LARGE_FILE_LIMIT,
    )
    .ok()?;
    add(global.as_ref());
    if let Some(global) = &global {
        add(project_entry(global, cwd).or_else(|| project_entry(global, &project)));
    }
    for dir in project.ancestors() {
        let file = try_read_json(&dir.join(".mcp.json"), Some(&project), SMALL_FILE_LIMIT).ok()?;
        add(file.as_ref());
    }
    Some(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;
    use serde_json::json;
    use std::path::PathBuf;

    fn write(path: &Path, value: Value) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value.to_string()).unwrap();
    }

    /// A home with a project directory in it and a shell snapshot pointing at that home.
    fn world() -> (ScratchDir, PathBuf, HashMap<String, String>) {
        let root = ScratchDir::new("claude-mcp");
        let cwd = root.join("work");
        std::fs::create_dir_all(&cwd).unwrap();
        let env = HashMap::from([("HOME".to_string(), root.to_string_lossy().into_owned())]);
        (root, cwd, env)
    }

    #[test]
    fn scopes_merge_local_over_project_over_user() {
        let (root, cwd, env) = world();
        write(
            &root.join(".claude.json"),
            json!({
                "mcpServers": {"a": {"command": "user-a"}, "b": {"command": "user-b"}, "c": {"command": "user-c"}},
                "projects": {cwd.to_string_lossy(): {"mcpServers": {"a": {"command": "local-a"}}}}
            }),
        );
        write(
            &cwd.join(".mcp.json"),
            json!({"mcpServers": {"a": {"command": "proj-a"}, "b": {"command": "proj-b"}}}),
        );
        write(
            &cwd.join(".claude/settings.local.json"),
            json!({"enableAllProjectMcpServers": true}),
        );
        let servers = load(&env, &cwd);
        let command = |name: &str| servers[name]["command"].as_str().unwrap().to_owned();
        assert_eq!(command("a"), "local-a");
        assert_eq!(command("b"), "proj-b");
        assert_eq!(command("c"), "user-c");
    }

    #[test]
    fn the_config_dir_in_effect_holds_the_global_file() {
        let (root, cwd, mut env) = world();
        let dir = root.join("claude-home");
        env.insert(
            "CLAUDE_CONFIG_DIR".into(),
            dir.to_string_lossy().into_owned(),
        );
        write(
            &dir.join(".claude.json"),
            json!({"mcpServers": {"x": {"command": "in-dir"}}}),
        );
        write(
            &root.join(".claude.json"),
            json!({"mcpServers": {"y": {"command": "in-home"}}}),
        );
        let servers = load(&env, &cwd);
        assert!(servers.contains_key("x") && !servers.contains_key("y"));
    }

    #[test]
    fn project_servers_need_approval_and_a_disable_wins() {
        let (root, cwd, env) = world();
        write(
            &cwd.join(".mcp.json"),
            json!({"mcpServers": {
                "listed": {"command": "1"}, "unlisted": {"command": "2"},
                "off": {"command": "3"}, "entry": {"command": "4"}
            }}),
        );
        write(
            &root.join(".claude/settings.json"),
            json!({"enabledMcpjsonServers": ["listed", "off"], "disabledMcpjsonServers": ["off"]}),
        );
        write(
            &root.join(".claude.json"),
            json!({"projects": {cwd.to_string_lossy(): {"enabledMcpjsonServers": ["entry"]}}}),
        );
        let mut names: Vec<_> = load(&env, &cwd).into_keys().collect();
        names.sort();
        assert_eq!(names, ["entry", "listed"]);
    }

    #[test]
    fn the_projects_shared_settings_cannot_approve_its_own_servers() {
        let (_root, cwd, env) = world();
        write(
            &cwd.join(".mcp.json"),
            json!({"mcpServers": {"s": {"command": "1"}}}),
        );
        write(
            &cwd.join(".claude/settings.json"),
            json!({"enableAllProjectMcpServers": true}),
        );
        assert!(load(&env, &cwd).is_empty());
    }

    #[test]
    fn a_mcp_json_above_the_working_directory_is_found_and_the_nearer_one_wins() {
        let (_root, cwd, env) = world();
        let inner = cwd.join("sub");
        std::fs::create_dir_all(&inner).unwrap();
        write(
            &cwd.join(".mcp.json"),
            json!({"mcpServers": {"s": {"command": "outer"}, "t": {"command": "t"}}}),
        );
        write(
            &inner.join(".mcp.json"),
            json!({"mcpServers": {"s": {"command": "inner"}}}),
        );
        write(
            &inner.join(".claude/settings.local.json"),
            json!({"enableAllProjectMcpServers": true}),
        );
        let servers = load(&env, &inner);
        assert_eq!(servers["s"]["command"], "inner");
        assert_eq!(servers["t"]["command"], "t");
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_is_skipped_without_blocking() {
        let (root, cwd, env) = world();
        // Inside the project (through `browse::live`) and above it (a bounded read).
        crate::test_support::mkfifo(&cwd.join(".mcp.json"));
        crate::test_support::mkfifo(&root.join(".mcp.json"));
        assert!(load(&env, &cwd).is_empty());
    }

    #[test]
    fn an_oversized_file_is_skipped_without_blocking() {
        let (root, _, _) = world();
        std::fs::write(root.join(".claude.json"), vec![b' '; SMALL_FILE_LIMIT + 1]).unwrap();
        assert!(read_json(&root.join(".claude.json"), None, SMALL_FILE_LIMIT).is_none());
        // The global config file may be larger.
        std::fs::write(
            root.join(".claude.json"),
            format!("{{\"pad\":\"{}\"}}", "x".repeat(SMALL_FILE_LIMIT)),
        )
        .unwrap();
        assert!(read_json(&root.join(".claude.json"), None, SMALL_FILE_LIMIT).is_none());
        assert!(read_json(&root.join(".claude.json"), None, LARGE_FILE_LIMIT).is_some());
    }

    /// A home directory opened as the project holds Claude Code's own configuration, which a
    /// dotfiles manager may link elsewhere: it is read as Claude Code reads it, not refused as a
    /// link out of the project.
    #[test]
    fn claude_codes_own_files_under_a_home_opened_as_the_project_are_read_through_their_links() {
        let (root, _, _) = world();
        // Canonical, as the project is, so that the files lie inside it.
        let home = canonical_project(&root);
        let env = HashMap::from([("HOME".to_string(), home.to_string_lossy().into_owned())]);
        let dotfiles = ScratchDir::new("claude-mcp-dotfiles");
        std::fs::create_dir(dotfiles.join("claude")).unwrap();
        write(
            &dotfiles.join(".claude.json"),
            json!({"mcpServers": {"s": {"command": "linked"}}}),
        );
        write(
            &dotfiles.join("claude/settings.local.json"),
            json!({"enableAllProjectMcpServers": true}),
        );
        write(
            &home.join(".mcp.json"),
            json!({"mcpServers": {"p": {"command": "approved"}}}),
        );
        crate::test_support::symlink(dotfiles.join(".claude.json"), home.join(".claude.json"))
            .unwrap();
        crate::test_support::symlink(dotfiles.join("claude"), home.join(".claude")).unwrap();
        let servers = load(&env, &home);
        assert_eq!(servers["s"]["command"], "linked");
        assert_eq!(servers["p"]["command"], "approved");
    }

    #[test]
    fn the_defined_names_include_every_scope_approved_or_not_and_none_while_a_file_is_unreadable() {
        let (root, cwd, env) = world();
        write(
            &root.join(".claude.json"),
            json!({
                "mcpServers": {"user": {"command": "u"}},
                "projects": {cwd.to_string_lossy(): {"mcpServers": {"local": {"command": "l"}}}}
            }),
        );
        write(
            &cwd.join(".mcp.json"),
            json!({"mcpServers": {"unapproved": {"command": "p"}}}),
        );
        let names: Vec<_> = defined_server_names(&env, None, &cwd)
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(names, ["local", "unapproved", "user"]);
        std::fs::write(cwd.join(".mcp.json"), "{ not json").unwrap();
        assert!(defined_server_names(&env, None, &cwd).is_none());
        std::fs::remove_file(cwd.join(".mcp.json")).unwrap();
        std::fs::write(root.join(".claude.json"), "{ not json").unwrap();
        assert!(defined_server_names(&env, None, &cwd).is_none());
    }

    #[test]
    fn a_malformed_file_is_skipped() {
        let (root, cwd, env) = world();
        std::fs::write(root.join(".claude.json"), "{ not json").unwrap();
        assert!(load(&env, &cwd).is_empty());
    }

    /// The instructions name the consumer, wherever the template does, and Claude Code's
    /// configuration directory as the shell snapshot resolves it.
    #[test]
    fn the_instructions_name_the_consumer_and_the_configuration_directory() {
        let env = HashMap::from([("CLAUDE_CONFIG_DIR".to_string(), "/work/claude".to_string())]);
        let text = ClaudeCompatibility.instructions("Some Agent", &env);
        assert!(text.contains("`/work/claude`"), "{text}");
        assert!(
            text.contains("Honor Claude workflows in Some Agent"),
            "{text}"
        );
        assert!(!text.contains("{CONSUMER}") && !text.contains("{CLAUDE_CONFIG_DIR}"));
        assert!(!text.contains("Codex"));
    }
}
