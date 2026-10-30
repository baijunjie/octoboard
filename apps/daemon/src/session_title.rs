//! The title a session shows beside its created name, put together from three sources in the
//! order of how well each says what the session is about: the title the agent's own records name
//! the conversation with, else the task in the agent's terminal title once the agent's
//! decorations are taken off it (each adapter knows its own), else a stand-in from the first
//! prompt, except the instructions opening a handoff. The terminal title is stored as the agent
//! sent it, so a row written before an adapter learned to read it is read correctly too.

use std::path::{Path, PathBuf};

use crate::adapter::adapter_for;
use crate::hostfs::lexically_normalise;
use crate::protocol::Agent;

/// What an agent's own records say a session is called. Stored with the session, never sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordTitles {
    /// A title the agent itself or the user named the conversation with.
    pub named: Option<String>,
    /// A stand-in taken from the conversation's first prompt, for one the agent left unnamed.
    pub prompt: Option<String>,
}

/// The title to show for a session, `None` when nothing says what it is about. `names` yields the
/// project's names (`project_names`), and is asked only when the agent's terminal title has to be
/// read against them.
pub fn resolve(
    agent: Agent,
    raw: Option<&str>,
    records: &RecordTitles,
    names: impl FnOnce() -> Vec<String>,
) -> Option<String> {
    if let Some(named) = &records.named {
        return Some(named.clone());
    }
    raw.and_then(|raw| {
        let adapter = adapter_for(agent);
        let names = if adapter.names_project() {
            names()
        } else {
            Vec::new()
        };
        adapter.task_title(raw, &names)
    })
    .or_else(|| {
        // A handoff's instructions do not name the task. A saved stand-in may already contain
        // them, so filter when resolving the title rather than only when reading the prompt.
        records
            .prompt
            .as_ref()
            .filter(|prompt| !prompt.starts_with(crate::handoff::operation::TAKEOVER_OPENING))
            .cloned()
    })
}

/// The names an agent may show the project under: the working directory's own name, that of the
/// git repository it lies in, and that of the main repository when it is a linked worktree. A
/// submodule's superproject is not the project.
pub fn project_names(cwd: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let mut push = |path: &Path| {
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            if !names.iter().any(|known| known == name) {
                names.push(name.to_owned());
            }
        }
    };
    push(cwd);
    for dir in cwd.ancestors() {
        let marker = dir.join(".git");
        if marker.is_dir() {
            push(dir);
            break;
        }
        if marker.is_file() {
            push(dir);
            if let Some(main) = main_repository(dir) {
                push(&main);
            }
            break;
        }
    }
    names
}

/// The main repository of the linked worktree at `dir`, whose `.git` file names
/// `<main repository>/.git/worktrees/<name>`, absolutely or relative to `dir`.
fn main_repository(dir: &Path) -> Option<PathBuf> {
    let gitdir = lexically_normalise(&crate::hostfs::git_dir::git_dir(dir)?);
    let worktrees = gitdir.parent()?;
    let git = worktrees.parent()?;
    (worktrees.file_name()? == "worktrees" && git.file_name()? == ".git")
        .then(|| git.parent().map(Path::to_path_buf))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records(named: Option<&str>, prompt: Option<&str>) -> RecordTitles {
        RecordTitles {
            named: named.map(str::to_owned),
            prompt: prompt.map(str::to_owned),
        }
    }

    #[test]
    fn a_named_title_wins_then_the_task_in_the_terminal_title_then_the_prompt() {
        let named = records(Some("Named"), Some("Prompt"));
        let none = records(None, Some("Prompt"));
        let resolve = |raw, records: &RecordTitles| resolve(Agent::Claude, raw, records, Vec::new);
        assert_eq!(resolve(Some("◐ Task"), &named).as_deref(), Some("Named"));
        assert_eq!(resolve(Some("◐ Task"), &none).as_deref(), Some("Task"));
        assert_eq!(
            resolve(Some("✳ Claude Code"), &none).as_deref(),
            Some("Prompt")
        );
        assert_eq!(resolve(None, &none).as_deref(), Some("Prompt"));
        assert_eq!(resolve(None, &RecordTitles::default()), None);
    }

    #[test]
    fn a_handoff_prompt_never_obscures_the_inherited_name_but_agent_titles_still_apply() {
        // Match a persisted, truncated prompt, independently of the current handoff wording.
        let prompt =
            "You are taking over the work of the project session `original` (\"Fix the build\"), …";
        for agent in [Agent::Claude, Agent::Codex, Agent::Grok] {
            let mut titles = records(None, Some(prompt));
            assert_eq!(resolve(agent, None, &titles, Vec::new), None);
            assert_eq!(
                resolve(agent, Some("Fix the build"), &titles, Vec::new).as_deref(),
                Some("Fix the build")
            );
            titles.named = Some("Repair the build pipeline".to_owned());
            assert_eq!(
                resolve(agent, None, &titles, Vec::new).as_deref(),
                Some("Repair the build pipeline")
            );
        }
    }

    #[test]
    fn the_working_directory_is_asked_for_only_when_the_agent_needs_the_project_name() {
        let asked = std::cell::Cell::new(false);
        let cwd = || {
            asked.set(true);
            vec!["proj".to_owned()]
        };
        let none = RecordTitles::default();
        resolve(Agent::Claude, Some("Task"), &none, cwd);
        resolve(
            Agent::Codex,
            Some("Task | other"),
            &records(Some("Named"), None),
            cwd,
        );
        assert!(!asked.get());
        let task = resolve(Agent::Codex, Some("Task | proj"), &none, cwd);
        assert_eq!(task.as_deref(), Some("Task"));
        assert!(asked.get());
    }

    #[test]
    fn project_names_cover_the_directory_its_repository_and_a_worktrees_main_repository() {
        let root = crate::test_support::ScratchDir::new("session-title-names");
        let main = root.join("main-repo");
        let linked = root.join("linked");
        std::fs::create_dir_all(main.join(".git/worktrees/linked")).unwrap();
        std::fs::create_dir_all(main.join(".git/modules/sub")).unwrap();
        std::fs::create_dir_all(linked.join("sub")).unwrap();
        std::fs::write(
            linked.join(".git"),
            format!("gitdir: {}\n", main.join(".git/worktrees/linked").display()),
        )
        .unwrap();
        std::fs::create_dir_all(main.join("pkg")).unwrap();
        assert_eq!(project_names(&main.join("pkg")), ["pkg", "main-repo"]);
        assert_eq!(
            project_names(&linked.join("sub")),
            ["sub", "linked", "main-repo"]
        );
        // A relative gitdir is relative to the directory holding the `.git` file.
        std::fs::write(
            linked.join(".git"),
            "gitdir: ../main-repo/.git/worktrees/linked\n",
        )
        .unwrap();
        assert_eq!(project_names(&linked), ["linked", "main-repo"]);
        // A submodule's `.git` file points into its superproject's modules directory.
        let submodule = main.join("sub");
        std::fs::create_dir_all(&submodule).unwrap();
        std::fs::write(submodule.join(".git"), "gitdir: ../.git/modules/sub\n").unwrap();
        assert_eq!(project_names(&submodule), ["sub"]);
    }
}
