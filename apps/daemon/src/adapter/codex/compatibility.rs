//! Codex as a compatibility source (`ProviderDescriptor::compatibility_source`): the MCP servers,
//! skills and custom prompts it would load for a session, and the guidance to read the rest of its
//! configuration, offered to the providers made compatible with it. Read, never written: none of
//! Codex's files is touched.
//!
//! The servers are the `[mcp_servers]` of the Codex home's `config.toml` (`CODEX_HOME` as the
//! launch's shell snapshot has it, else `~/.codex`), layered under the project's
//! `.codex/config.toml` files from the repository root down to the working directory, the nearest
//! winning and tables merging as Codex merges them. The project's files count only where Codex
//! trusts it, decided as Codex decides it, for each directory from the repository root down to the
//! working directory on its own: the home's `config.toml` entry `projects."<directory>"`, else the
//! entry of the repository's top (the nearest directory holding `.git`; a linked worktree's own
//! top), else the one of the main repository's root (the parent of the common git directory). The
//! first entry that has a `trust_level` string decides and only `"trusted"` trusts; an entry
//! without one is skipped and no entry is untrusted. So a directory's layer loads only where that
//! directory is trusted, whatever the layers around it are, and a trusted directory above the
//! repository covers nothing. Trust is decided before any project file is read, so a broken file
//! in an untrusted directory costs nothing. Any file in play that cannot be read or parsed leaves
//! no server at all, the safe side.
//!
//! The project's skill roots (`.codex/skills`, `.agents/skills`) of a directory count under that
//! directory's trust, so an untrusted repository injects no skill into a consumer; the user-level
//! roots do not depend on it. Codex itself loads project skills with or without trust (measured on
//! 0.166.0), so this is a deliberately narrower offer.
//!
//! Each server is given in the common MCP format. What that format cannot say is left out whole: a
//! server with a `cwd`, with `enabled_tools` or `disabled_tools`, with a transport other than
//! stdio or streamable HTTP, with a field of the wrong type, or with a literal value that holds
//! `${` (see [`CompatibilitySource::mcp_servers`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::config::{project_configs, read_config, ConfigFile};
use crate::adapter::account_cli::snapshot_home;
use crate::adapter::compatibility_offer::is_env_name;
use crate::adapter::files::{canonical_project, project_dirs};
use crate::adapter::CompatibilitySource;
use crate::protocol::AgentId;

/// The consumer's name goes where this has `{CONSUMER}`, and the Codex home where it has
/// `{CODEX_HOME}`.
const INSTRUCTIONS: &str = include_str!("compatibility.md");

/// What names a skill's instructions inside its directory.
const SKILL_FILE: &str = "SKILL.md";

pub(super) struct CodexCompatibility;

impl CompatibilitySource for CodexCompatibility {
    fn mcp_servers(
        &self,
        shell_env: &HashMap<String, String>,
        cwd: &Path,
    ) -> BTreeMap<String, Value> {
        Launch::read(shell_env, cwd).servers().unwrap_or_default()
    }

    /// Names the Codex home as the launch's shell snapshot resolves it (`~/.codex` when it names
    /// none).
    fn instructions(&self, consumer_label: &str, shell_env: &HashMap<String, String>) -> String {
        INSTRUCTIONS
            .replace("{CONSUMER}", consumer_label)
            .replace("{CODEX_HOME}", &codex_home(shell_env).to_string_lossy())
    }

    /// The skills Codex would offer in `cwd`, by directory name, each name once at its nearest
    /// place: the `.codex/skills` and `.agents/skills` of the working directory and every directory
    /// up to the repository root that Codex trusts (see the module's text), then the Codex home's
    /// `skills`, then `~/.agents/skills`. A skill the home's `config.toml` disables is left out
    /// without hiding a farther one of the same name; with that file unreadable none is offered,
    /// since what it disables is not known.
    ///
    /// Codex 0.166 differs in three ways that stay as they are: it finds a `SKILL.md` at any depth
    /// under a root, where only a root's direct children count here; it lists two skills of one
    /// name both; and it takes a skill's name from its front matter, where the directory's name
    /// stands here. The `[[skills.config]]` entries of a project's own `config.toml` are not read
    /// either.
    fn skill_dirs(&self, shell_env: &HashMap<String, String>, cwd: &Path) -> Vec<PathBuf> {
        let launch = Launch::read(shell_env, cwd);
        if matches!(launch.config, ConfigFile::Unreadable) {
            return Vec::new();
        }
        let disabled = launch.disabled_skills();
        let mut roots = Vec::new();
        // A project's skills are its own content, so like its configuration layers they count only
        // for a directory Codex trusts.
        for dir in project_dirs(&launch.project) {
            if launch.trusted.iter().any(|trusted| trusted == dir) {
                roots.push(dir.join(".codex/skills"));
                roots.push(dir.join(".agents/skills"));
            }
        }
        roots.push(launch.home.join("skills"));
        roots.push(snapshot_home(shell_env).join(".agents/skills"));

        let mut seen = BTreeSet::new();
        let mut found = Vec::new();
        for root in roots {
            for (name, dir) in children(&root) {
                let file = dir.join(SKILL_FILE);
                if !file.metadata().is_ok_and(|meta| meta.is_file())
                    || disabled.contains(&canonical_project(&file))
                {
                    continue;
                }
                if seen.insert(name) {
                    found.push(dir);
                }
            }
        }
        found
    }

    /// The custom prompts, the Markdown files of the Codex home's `prompts`.
    fn command_files(&self, shell_env: &HashMap<String, String>) -> Vec<PathBuf> {
        children(&codex_home(shell_env).join("prompts"))
            .into_iter()
            .filter(|(name, path)| {
                name.ends_with(".md")
                    && name.len() > ".md".len()
                    && path.metadata().is_ok_and(|meta| meta.is_file())
            })
            .map(|(_, path)| path)
            .collect()
    }
}

/// What one reading of the Codex configuration for a launch is made from: the home in effect, its
/// `config.toml` read once, and the directories Codex trusts, decided once from it.
struct Launch {
    home: PathBuf,
    /// The working directory, canonical.
    project: PathBuf,
    config: ConfigFile,
    /// The directories of [`project_dirs`] that Codex trusts.
    trusted: Vec<PathBuf>,
}

impl Launch {
    fn read(shell_env: &HashMap<String, String>, cwd: &Path) -> Self {
        let home = codex_home(shell_env);
        let project = canonical_project(cwd);
        let config = read_config(&home);
        let trusted = match &config {
            ConfigFile::Parsed(config) => trusted_dirs(config, &project),
            ConfigFile::Absent | ConfigFile::Unreadable => Vec::new(),
        };
        Self {
            home,
            project,
            config,
            trusted,
        }
    }

    /// The `SKILL.md` paths the `[[skills.config]]` entries of the home's `config.toml` turn off.
    fn disabled_skills(&self) -> BTreeSet<PathBuf> {
        let ConfigFile::Parsed(config) = &self.config else {
            return BTreeSet::new();
        };
        config
            .get("skills")
            .and_then(|skills| skills.get("config"))
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|entry| entry.get("enabled").and_then(toml::Value::as_bool) == Some(false))
            .filter_map(|entry| entry.get("path")?.as_str())
            .map(|path| canonical_project(Path::new(path)))
            .collect()
    }

    /// The servers Codex would load, by name; `None` while a file in play cannot be read.
    fn servers(self) -> Option<BTreeMap<String, Value>> {
        let home_config = match self.config {
            ConfigFile::Absent => toml::Table::new(),
            ConfigFile::Unreadable => return None,
            ConfigFile::Parsed(config) => config,
        };
        let mut layers = vec![home_config];
        // Trust first: an untrusted directory's file is not read at all.
        layers.extend(project_configs(&self.project, &self.home, |dir| {
            self.trusted.iter().any(|trusted| trusted == dir)
        })?);
        let mut merged = toml::Table::new();
        for layer in layers {
            merge(&mut merged, layer);
        }
        let servers = merged.get("mcp_servers")?.as_table()?;
        Some(
            servers
                .iter()
                .filter_map(|(name, server)| Some((name.clone(), translate(server)?)))
                .collect(),
        )
    }
}

fn codex_home(shell_env: &HashMap<String, String>) -> PathBuf {
    crate::adapter::default_account_dir(AgentId::CODEX, shell_env)
}

/// The entries of the directory `dir` whose names do not start with a dot (`.system` among them),
/// by name; none while it cannot be listed.
fn children(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (!name.starts_with('.')).then(|| (name, entry.path()))
        })
        .collect();
    found.sort();
    found
}

/// Merges `over` into `base`, a table at a time, the later value winning anywhere else.
fn merge(base: &mut toml::Table, over: toml::Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(base)), toml::Value::Table(over)) => merge(base, over),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// The directories of [`project_dirs`] for `project` (canonical) whose layer Codex loads, each
/// decided on its own: the `projects."<path>"` entry of the home's `config.toml` for the directory,
/// else for the repository's top, else for the main repository's root; the first entry holding a
/// `trust_level` string decides and only `"trusted"` trusts. The keys are looked up as Codex wrote
/// them (canonical), not resolved.
fn trusted_dirs(home_config: &toml::Table, project: &Path) -> Vec<PathBuf> {
    let Some(projects) = home_config.get("projects").and_then(toml::Value::as_table) else {
        return Vec::new();
    };
    let dirs = project_dirs(project);
    let top = dirs.last().copied().filter(|top| top.join(".git").exists());
    let main = top.and_then(main_root);
    dirs.iter()
        .filter(|dir| {
            let decision = [Some(**dir), top, main.as_deref()]
                .into_iter()
                .flatten()
                .find_map(|key| {
                    let level = projects.get(key.to_str()?)?.get("trust_level")?.as_str()?;
                    Some(level == "trusted")
                });
            decision.unwrap_or(false)
        })
        .map(|dir| dir.to_path_buf())
        .collect()
}

/// The main repository's root for the working tree `top`: the parent of the common git directory,
/// whatever that is named (a linked worktree's, a submodule's, a separate git directory's). `None`
/// when it cannot be found.
fn main_root(top: &Path) -> Option<PathBuf> {
    let admin = crate::git::layout::admin_dir(top)?;
    let common = crate::git::repo_files::common_dir_of(&admin)?;
    Some(common.parent()?.to_path_buf())
}

/// A server of `config.toml` in the common MCP format, or `None` where it is off or cannot be said
/// in it (see the module's text).
fn translate(server: &toml::Value) -> Option<Value> {
    let server = server.as_table()?;
    match server.get("enabled") {
        None => {}
        Some(toml::Value::Boolean(true)) => {}
        Some(_) => return None,
    }
    if ["cwd", "enabled_tools", "disabled_tools"]
        .iter()
        .any(|key| server.contains_key(*key))
    {
        return None;
    }
    match (server.get("command"), server.get("url")) {
        (Some(command), None) => {
            let mut env = literals(server.get("env"))?;
            for name in strings(server.get("env_vars"))? {
                env.push((name.clone(), reference(&name)?));
            }
            let mut out = json!({
                "command": command.as_str()?,
                "args": strings(server.get("args"))?,
            });
            if !env.is_empty() {
                out["env"] = Value::Object(env.into_iter().map(|(k, v)| (k, json!(v))).collect());
            }
            Some(out)
        }
        (None, Some(url)) => {
            let mut headers = literals(server.get("http_headers"))?;
            match server.get("env_http_headers") {
                None => {}
                Some(toml::Value::Table(table)) => {
                    for (header, name) in table {
                        headers.push((header.clone(), reference(name.as_str()?)?));
                    }
                }
                Some(_) => return None,
            }
            if let Some(name) = server.get("bearer_token_env_var") {
                headers.push((
                    "Authorization".to_owned(),
                    format!("Bearer {}", reference(name.as_str()?)?),
                ));
            }
            let mut out = json!({"type": "http", "url": url.as_str()?});
            if !headers.is_empty() {
                out["headers"] =
                    Value::Object(headers.into_iter().map(|(k, v)| (k, json!(v))).collect());
            }
            Some(out)
        }
        _ => None,
    }
}

/// A table of strings as its entries; empty when absent, `None` when it is not one or a value
/// holds `${`, which a consumer would expand where Codex does not.
fn literals(value: Option<&toml::Value>) -> Option<Vec<(String, String)>> {
    let Some(value) = value else {
        return Some(Vec::new());
    };
    value
        .as_table()?
        .iter()
        .map(|(key, value)| {
            let value = value.as_str().filter(|value| !value.contains("${"))?;
            Some((key.clone(), value.to_owned()))
        })
        .collect()
}

/// An array of strings; empty when absent, `None` when it is not one.
fn strings(value: Option<&toml::Value>) -> Option<Vec<String>> {
    let Some(value) = value else {
        return Some(Vec::new());
    };
    value
        .as_array()?
        .iter()
        .map(|item| item.as_str().map(str::to_owned))
        .collect()
}

/// `${NAME}`, for the name of an environment variable; `None` for a name that cannot be one.
fn reference(name: &str) -> Option<String> {
    is_env_name(name).then(|| format!("${{{name}}}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::codex::toml_string;
    use crate::test_support::ScratchDir;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A scratch home with a repository in it (`work/sub` the working directory) and a shell
    /// snapshot pointing at the home.
    fn world() -> (ScratchDir, PathBuf, HashMap<String, String>) {
        let root = ScratchDir::new("codex-source");
        let root_dir = canonical_project(&root);
        let repo = root_dir.join("work");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let cwd = repo.join("sub");
        std::fs::create_dir_all(&cwd).unwrap();
        let env = HashMap::from([("HOME".to_string(), root_dir.to_string_lossy().into_owned())]);
        (root, cwd, env)
    }

    fn trusted(project: &Path) -> String {
        format!(
            "[projects.{}]\ntrust_level = \"trusted\"\n",
            toml_string(&project.to_string_lossy())
        )
    }

    fn home(root: &ScratchDir) -> PathBuf {
        canonical_project(root).join(".codex")
    }

    fn load(env: &HashMap<String, String>, cwd: &Path) -> Option<BTreeMap<String, Value>> {
        Launch::read(env, cwd).servers()
    }

    #[test]
    fn the_home_servers_are_offered_in_the_common_format() {
        let (root, cwd, env) = world();
        write(
            &home(&root).join("config.toml"),
            r#"
[mcp_servers.local]
command = "npx"
args = ["-y", "server"]
env = { LITERAL = "1" }
env_vars = ["FORWARDED"]

[mcp_servers.remote]
url = "https://example.com/mcp"
http_headers = { "X-Plain" = "a" }
env_http_headers = { "X-Key" = "KEY_VAR" }
bearer_token_env_var = "TOKEN_VAR"

[mcp_servers.off]
command = "x"
enabled = false
"#,
        );
        let servers = load(&env, &cwd).unwrap();
        assert_eq!(servers.keys().collect::<Vec<_>>(), ["local", "remote"]);
        assert_eq!(
            servers["local"],
            json!({"command": "npx", "args": ["-y", "server"],
                   "env": {"LITERAL": "1", "FORWARDED": "${FORWARDED}"}})
        );
        assert_eq!(
            servers["remote"],
            json!({"type": "http", "url": "https://example.com/mcp",
                   "headers": {"X-Plain": "a", "X-Key": "${KEY_VAR}",
                               "Authorization": "Bearer ${TOKEN_VAR}"}})
        );
    }

    #[test]
    fn what_the_common_format_cannot_say_leaves_the_server_out() {
        let (root, cwd, env) = world();
        write(
            &home(&root).join("config.toml"),
            r#"
[mcp_servers.ok]
command = "a"
[mcp_servers.cwd]
command = "a"
cwd = "/x"
[mcp_servers.only]
command = "a"
enabled_tools = ["t"]
[mcp_servers.except]
command = "a"
disabled_tools = ["t"]
[mcp_servers.neither]
startup_timeout_sec = 3
[mcp_servers.both]
command = "a"
url = "https://example.com"
[mcp_servers.args]
command = "a"
args = "not an array"
[mcp_servers.env_type]
command = "a"
env = { A = 1 }
[mcp_servers.expands]
command = "a"
env = { A = "${HOME}" }
[mcp_servers.bad_var]
command = "a"
env_vars = ["NOT VALID"]
[mcp_servers.scalar]
"#,
        );
        let servers = load(&env, &cwd).unwrap();
        assert_eq!(servers.keys().collect::<Vec<_>>(), ["ok"]);
    }

    #[test]
    fn the_project_layers_count_only_where_codex_trusts_the_project_and_the_nearest_wins() {
        let (root, cwd, env) = world();
        let repo = cwd.parent().unwrap().to_path_buf();
        let config = home(&root).join("config.toml");
        write(
            &config,
            "[mcp_servers.a]\ncommand = \"home-a\"\nargs = [\"1\"]\n[mcp_servers.h]\ncommand = \"home-h\"\n",
        );
        write(
            &repo.join(".codex/config.toml"),
            "[mcp_servers.a]\ncommand = \"repo-a\"\n[mcp_servers.r]\ncommand = \"repo-r\"\n",
        );
        write(
            &cwd.join(".codex/config.toml"),
            "[mcp_servers.a]\ncommand = \"sub-a\"\n",
        );
        let commands = |servers: BTreeMap<String, Value>| -> Vec<(String, String)> {
            servers
                .into_iter()
                .map(|(name, server)| (name, server["command"].as_str().unwrap().to_owned()))
                .collect()
        };
        assert_eq!(
            commands(load(&env, &cwd).unwrap()),
            [("a".into(), "home-a".into()), ("h".into(), "home-h".into())]
        );

        let mut text = std::fs::read_to_string(&config).unwrap();
        text.push_str(&trusted(&repo));
        write(&config, &text);
        let servers = load(&env, &cwd).unwrap();
        // Tables merge, so the home's `args` stay under the nearer `command`.
        assert_eq!(servers["a"]["args"], json!(["1"]));
        assert_eq!(
            commands(servers),
            [
                ("a".into(), "sub-a".into()),
                ("h".into(), "home-h".into()),
                ("r".into(), "repo-r".into())
            ]
        );
    }

    #[test]
    fn a_file_in_play_that_cannot_be_read_leaves_no_server() {
        let (root, cwd, env) = world();
        let repo = cwd.parent().unwrap().to_path_buf();
        let config = home(&root).join("config.toml");
        write(&config, "[mcp_servers.a]\ncommand = \"a\"\n");
        assert_eq!(load(&env, &cwd).unwrap().len(), 1);
        // A project file counts only where Codex trusts the project, so a broken one in an
        // untrusted repository is never read and costs the home's servers nothing.
        write(&cwd.join(".codex/config.toml"), "not = [toml");
        assert_eq!(load(&env, &cwd).unwrap().len(), 1);
        write(
            &config,
            &format!("[mcp_servers.a]\ncommand = \"a\"\n{}", trusted(&repo)),
        );
        assert!(load(&env, &cwd).is_none());
        assert!(CodexCompatibility.mcp_servers(&env, &cwd).is_empty());
        std::fs::remove_file(cwd.join(".codex/config.toml")).unwrap();
        write(&config, "not = [toml");
        assert!(load(&env, &cwd).is_none());
    }

    /// `[projects."<dir>"]` with a `trust_level` of `level`, or without one for `None`.
    fn entry(dir: &Path, level: Option<&str>) -> String {
        let level = level.map_or(String::new(), |level| {
            format!("trust_level = \"{level}\"\n")
        });
        format!(
            "[projects.{}]\n{level}",
            toml_string(&dir.to_string_lossy())
        )
    }

    /// The trusted directories of `project` under the given entries, nearest first.
    fn decided(entries: &[String], project: &Path) -> Vec<PathBuf> {
        trusted_dirs(&entries.concat().parse::<toml::Table>().unwrap(), project)
    }

    #[test]
    fn trust_is_decided_per_directory_as_codex_decides_it() {
        let (root, cwd, _) = world();
        let repo = cwd.parent().unwrap().to_path_buf();
        let home_dir = canonical_project(&root);
        let both = vec![cwd.clone(), repo.clone()];
        assert!(trusted_dirs(&toml::Table::new(), &cwd).is_empty());
        // The repository's entry covers the directories below it.
        assert_eq!(decided(&[entry(&repo, Some("trusted"))], &cwd), both);
        // A directory with an entry of its own is decided by it: the repository trusted and the
        // subdirectory not loads the repository's layer only.
        assert_eq!(
            decided(
                &[
                    entry(&repo, Some("trusted")),
                    entry(&cwd, Some("untrusted"))
                ],
                &cwd
            ),
            vec![repo.clone()]
        );
        // An entry without a `trust_level` is skipped, so the repository's entry decides.
        assert_eq!(
            decided(&[entry(&repo, Some("trusted")), entry(&cwd, None)], &cwd),
            both
        );
        // The repository untrusted and the subdirectory trusted: the subdirectory's layer only.
        assert_eq!(
            decided(
                &[
                    entry(&repo, Some("untrusted")),
                    entry(&cwd, Some("trusted"))
                ],
                &cwd
            ),
            vec![cwd.clone()]
        );
        assert_eq!(
            decided(&[entry(&cwd, Some("trusted"))], &cwd),
            vec![cwd.clone()]
        );
        // Only `trusted` trusts, and nothing above the repository does, the home among them.
        assert!(decided(&[entry(&repo, Some("untrusted"))], &cwd).is_empty());
        assert!(decided(&[entry(&repo, None)], &cwd).is_empty());
        assert!(decided(&[entry(&home_dir, Some("trusted"))], &cwd).is_empty());
    }

    /// A linked worktree is decided by its own top, else by the main repository's root, where Codex
    /// records it.
    #[test]
    fn a_linked_worktree_is_decided_by_its_own_top_else_the_main_repositorys_root() {
        let (root, _, _) = world();
        let base = canonical_project(&root);
        let main = base.join("main");
        let admin = main.join(".git/worktrees/linked");
        std::fs::create_dir_all(&admin).unwrap();
        write(&admin.join("commondir"), "../..\n");
        let linked = base.join("linked");
        write(
            &linked.join(".git"),
            &format!("gitdir: {}\n", admin.display()),
        );
        let sub = linked.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let both = vec![sub.clone(), linked.clone()];
        assert_eq!(decided(&[entry(&main, Some("trusted"))], &sub), both);
        // Its own top's entry wins over the main repository's, either way round.
        assert_eq!(decided(&[entry(&linked, Some("trusted"))], &sub), both);
        assert!(decided(
            &[
                entry(&main, Some("trusted")),
                entry(&linked, Some("untrusted"))
            ],
            &sub
        )
        .is_empty());
        assert_eq!(
            decided(
                &[
                    entry(&main, Some("untrusted")),
                    entry(&linked, Some("trusted"))
                ],
                &sub
            ),
            both
        );
        // A top's entry without a `trust_level` falls through to the main repository's.
        assert_eq!(
            decided(&[entry(&main, Some("trusted")), entry(&linked, None)], &sub),
            both
        );
    }

    /// The main root is the parent of the common git directory whatever that is called, as for a
    /// submodule or a separate git directory.
    #[test]
    fn the_main_root_is_the_parent_of_the_common_git_directory_by_whatever_name() {
        let (root, _, _) = world();
        let base = canonical_project(&root);
        // Joined a component at a time, so that the key reads as Codex writes the canonical path
        // on every host: one joined with `/` keeps the slash on Windows.
        let modules = base.join("super").join(".git").join("modules");
        std::fs::create_dir_all(modules.join("child")).unwrap();
        let child = base.join("super").join("child");
        write(
            &child.join(".git"),
            &format!("gitdir: {}\n", modules.join("child").display()),
        );
        assert_eq!(
            decided(&[entry(&modules, Some("trusted"))], &child),
            vec![child.clone()]
        );
        assert!(decided(&[entry(&base.join("super"), Some("trusted"))], &child).is_empty());
    }

    #[test]
    fn each_directorys_layer_and_skills_load_only_where_that_directory_is_trusted() {
        let (root, cwd, env) = world();
        let repo = cwd.parent().unwrap().to_path_buf();
        let config = home(&root).join("config.toml");
        write(
            &repo.join(".codex/config.toml"),
            "[mcp_servers.r]\ncommand = \"repo\"\n",
        );
        write(
            &cwd.join(".codex/config.toml"),
            "[mcp_servers.s]\ncommand = \"sub\"\n",
        );
        let repo_skill = skill(&repo.join(".codex/skills"), "r");
        let sub_skill = skill(&cwd.join(".agents/skills"), "s");
        let state = |entries: &[String]| {
            write(&config, &entries.concat());
            let servers = load(&env, &cwd).unwrap_or_default();
            (
                servers.keys().cloned().collect::<Vec<_>>(),
                CodexCompatibility.skill_dirs(&env, &cwd),
            )
        };
        assert_eq!(state(&[]), (vec![], vec![]));
        let both = (
            vec!["r".to_owned(), "s".to_owned()],
            vec![sub_skill.clone(), repo_skill.clone()],
        );
        assert_eq!(state(&[entry(&repo, Some("trusted"))]), both);
        assert_eq!(
            state(&[entry(&repo, Some("trusted")), entry(&cwd, None)]),
            both
        );
        assert_eq!(
            state(&[
                entry(&repo, Some("trusted")),
                entry(&cwd, Some("untrusted"))
            ]),
            (vec!["r".to_owned()], vec![repo_skill])
        );
        assert_eq!(
            state(&[
                entry(&repo, Some("untrusted")),
                entry(&cwd, Some("trusted"))
            ]),
            (vec!["s".to_owned()], vec![sub_skill])
        );
        assert_eq!(
            state(&[entry(&canonical_project(&root), Some("trusted"))]),
            (vec![], vec![])
        );
    }

    #[test]
    fn the_codex_home_in_effect_is_the_one_read() {
        let (root, cwd, mut env) = world();
        let other = canonical_project(&root).join("elsewhere");
        env.insert("CODEX_HOME".into(), other.to_string_lossy().into_owned());
        write(
            &other.join("config.toml"),
            "[mcp_servers.x]\ncommand = \"x\"\n",
        );
        write(
            &home(&root).join("config.toml"),
            "[mcp_servers.y]\ncommand = \"y\"\n",
        );
        assert_eq!(load(&env, &cwd).unwrap().keys().collect::<Vec<_>>(), ["x"]);
        let text = CodexCompatibility.instructions("Some Agent", &env);
        assert!(text.contains(&format!("`{}`", other.display())), "{text}");
        assert!(text.contains("Honor Codex workflows in Some Agent"));
        assert!(!text.contains("{CONSUMER}") && !text.contains("{CODEX_HOME}"));
        assert!(!text.contains("Claude Code compatibility"));
    }

    fn skill(dir: &Path, name: &str) -> PathBuf {
        write(&dir.join(name).join(SKILL_FILE), "---\nname: x\n---\n");
        dir.join(name)
    }

    #[test]
    fn skills_are_found_nearest_first_once_per_name_and_never_from_dot_directories() {
        let (root, cwd, env) = world();
        let repo = cwd.parent().unwrap().to_path_buf();
        let home_dir = canonical_project(&root);
        write(&home(&root).join("config.toml"), &trusted(&repo));
        let near = skill(&cwd.join(".codex/skills"), "both");
        let agents = skill(&repo.join(".agents/skills"), "agents");
        skill(&repo.join(".codex/skills"), "both");
        let global = skill(&home(&root).join("skills"), "global");
        skill(&home(&root).join("skills"), "agents");
        skill(&home(&root).join("skills/.system"), "builtin");
        let user = skill(&home_dir.join(".agents/skills"), "user");
        // A directory without a `SKILL.md` is no skill.
        std::fs::create_dir_all(cwd.join(".codex/skills/empty")).unwrap();
        assert_eq!(
            CodexCompatibility.skill_dirs(&env, &cwd),
            [near, agents, global, user]
        );
    }

    #[test]
    fn project_skills_count_only_where_codex_trusts_the_project() {
        let (root, cwd, env) = world();
        let repo = cwd.parent().unwrap().to_path_buf();
        skill(&cwd.join(".codex/skills"), "project-codex");
        skill(&repo.join(".agents/skills"), "project-agents");
        let global = skill(&home(&root).join("skills"), "global");
        let user = skill(&canonical_project(&root).join(".agents/skills"), "user");
        // Without a home configuration, and with one that does not trust the project, only the
        // user-level roots are offered.
        assert_eq!(
            CodexCompatibility.skill_dirs(&env, &cwd),
            [global.clone(), user.clone()]
        );
        let config = home(&root).join("config.toml");
        write(
            &config,
            "[projects.\"/elsewhere\"]\ntrust_level = \"trusted\"\n",
        );
        assert_eq!(
            CodexCompatibility.skill_dirs(&env, &cwd),
            [global.clone(), user.clone()]
        );
        write(
            &config,
            &format!(
                "[projects.{}]\ntrust_level = \"untrusted\"\n",
                toml_string(&repo.to_string_lossy())
            ),
        );
        assert_eq!(CodexCompatibility.skill_dirs(&env, &cwd), [global, user]);
        write(&config, &trusted(&repo));
        assert_eq!(CodexCompatibility.skill_dirs(&env, &cwd).len(), 4);
    }

    #[test]
    fn a_skill_the_home_configuration_disables_is_left_out_and_an_unreadable_one_leaves_none() {
        let (root, cwd, env) = world();
        let off = skill(&home(&root).join("skills"), "off");
        skill(&home(&root).join("skills"), "on");
        let config = home(&root).join("config.toml");
        write(
            &config,
            &format!(
                "[[skills.config]]\npath = {}\nenabled = false\n",
                toml_string(&off.join(SKILL_FILE).to_string_lossy())
            ),
        );
        let names: Vec<_> = CodexCompatibility
            .skill_dirs(&env, &cwd)
            .iter()
            .map(|dir| dir.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["on"]);
        write(&config, "not = [toml");
        assert!(CodexCompatibility.skill_dirs(&env, &cwd).is_empty());
    }

    #[test]
    fn a_disabled_nearer_skill_does_not_hide_a_farther_one_of_the_same_name() {
        let (root, cwd, env) = world();
        let repo = cwd.parent().unwrap().to_path_buf();
        let near = skill(&cwd.join(".codex/skills"), "both");
        let far = skill(&home(&root).join("skills"), "both");
        write(
            &home(&root).join("config.toml"),
            &format!(
                "{}[[skills.config]]\npath = {}\nenabled = false\n",
                trusted(&repo),
                toml_string(&near.join(SKILL_FILE).to_string_lossy())
            ),
        );
        assert_eq!(CodexCompatibility.skill_dirs(&env, &cwd), [far]);
    }

    #[test]
    fn the_custom_prompts_are_the_markdown_files_of_the_homes_prompts_directory() {
        let (root, _, env) = world();
        let prompts = home(&root).join("prompts");
        write(&prompts.join("review.md"), "Review $1");
        write(&prompts.join("notes.txt"), "no");
        write(&prompts.join(".hidden.md"), "no");
        std::fs::create_dir_all(prompts.join("dir.md")).unwrap();
        assert_eq!(
            CodexCompatibility.command_files(&env),
            [prompts.join("review.md")]
        );
    }
}
