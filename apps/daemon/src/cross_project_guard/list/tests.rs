//! The list a project session's launch writes for the guard hook.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use super::*;
use crate::protocol::{Account, AgentId, SessionKind};
use crate::session_git_hooks::fixture::{project, session_record, Fixture};
use crate::test_support::{git, repo_with, ScratchDir};

const TOOL: &str = "mcp__octoboard__request_session";

fn switch(fixture: &Fixture, on: bool) {
    fixture
        .state
        .store
        .set_keep_sessions_in_own_project(on)
        .unwrap();
}

async fn list_for(fixture: &Fixture, session: &str) -> Option<GuardList> {
    let session = fixture.state.store.get_session(session).unwrap().unwrap();
    for_session(&fixture.state, &session).await.unwrap()
}

fn names(list: &GuardList) -> BTreeSet<&str> {
    list.projects
        .iter()
        .map(|other| other.name.as_str())
        .collect()
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[tokio::test]
async fn a_list_is_made_only_for_a_project_session_while_the_switch_is_on() {
    let fixture = Fixture::new();
    let mut console = session_record("console-session", "project");
    console.project_id = None;
    console.kind = SessionKind::Console;
    fixture.state.store.insert_session(&console).unwrap();

    assert_eq!(list_for(&fixture, "a").await, None, "the switch is off");

    switch(&fixture, true);
    assert!(list_for(&fixture, "a").await.is_some());
    assert_eq!(
        list_for(&fixture, "console-session").await,
        None,
        "a console session"
    );

    switch(&fixture, false);
    assert_eq!(list_for(&fixture, "a").await, None, "off again");
}

#[tokio::test]
async fn the_list_names_the_tool_the_own_project_and_every_other_project_of_every_console() {
    let fixture = Fixture::new();
    switch(&fixture, true);
    let first = repo_with("guard-list-first", &[(b"a", b"a")]);
    let second = repo_with("guard-list-second", &[(b"b", b"b")]);
    let plain = ScratchDir::new("guard-list-plain");
    fixture
        .state
        .store
        .insert_project(&project("first", &first))
        .unwrap();
    fixture
        .state
        .store
        .insert_project(&project("plain", &plain))
        .unwrap();
    // Another console's project is listed as well.
    let mut console = crate::test_support::console(&fixture.dir);
    console.id = "console-two".to_string();
    console.workdir = text(&fixture.dir.join("workdir-two"));
    fixture.state.store.insert_console(&console).unwrap();
    let mut elsewhere = project("second", &second);
    elsewhere.console_id = "console-two".to_string();
    fixture.state.store.insert_project(&elsewhere).unwrap();

    let list = list_for(&fixture, "a").await.expect("a list");

    assert_eq!(list.tool, TOOL);
    assert_eq!(list.own, [text(&fixture.repo)]);
    assert_eq!(
        list.repository,
        crate::git::repo_files::repository_of(&fixture.repo).map(|common| text(&common)),
        "the own repository's common directory"
    );
    assert!(list.repository.is_some());
    assert_eq!(names(&list), BTreeSet::from(["first", "plain", "second"]));
    let root_of = |name: &str| {
        list.projects
            .iter()
            .find(|other| other.name == name)
            .unwrap()
            .root
            .clone()
    };
    assert_eq!(root_of("first"), text(&first));
    assert_eq!(root_of("second"), text(&second));
}

#[tokio::test]
async fn projects_in_the_sessions_own_repository_are_not_among_the_others() {
    let fixture = Fixture::new();
    switch(&fixture, true);
    let linked = fixture.dir.join("linked-worktree");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            linked.to_str().unwrap(),
            "-b",
            "wt",
        ],
    );
    fixture
        .state
        .store
        .insert_project(&project("sibling", &linked))
        .unwrap();
    // Two other projects in one repository of theirs are both listed.
    let shared = repo_with("guard-list-shared", &[(b"s", b"s")]);
    let second = fixture.dir.join("shared-second");
    git(
        &shared,
        &[
            "worktree",
            "add",
            "-q",
            second.to_str().unwrap(),
            "-b",
            "two",
        ],
    );
    fixture
        .state
        .store
        .insert_project(&project("shared-one", &shared))
        .unwrap();
    fixture
        .state
        .store
        .insert_project(&project("shared-two", &second))
        .unwrap();

    let list = list_for(&fixture, "a").await.expect("a list");

    assert_eq!(names(&list), BTreeSet::from(["shared-one", "shared-two"]));
    assert_eq!(
        list.own,
        [text(&fixture.repo)],
        "the sibling is not an own root here"
    );
}

#[tokio::test]
async fn a_project_that_is_no_repository_lists_every_other_project() {
    let fixture = Fixture::new();
    switch(&fixture, true);
    let plain = ScratchDir::new("guard-list-own-plain");
    fixture
        .state
        .store
        .insert_project(&project("plain", &plain))
        .unwrap();
    fixture
        .state
        .store
        .insert_session(&session_record("p", "plain"))
        .unwrap();

    let list = list_for(&fixture, "p").await.expect("a list");

    assert_eq!(names(&list), BTreeSet::from(["project"]));
    assert_eq!(list.own, [text(&plain)]);
    assert_eq!(
        list.repository, None,
        "no repository to be the session's own"
    );
}

#[tokio::test]
async fn the_shared_directories_are_the_data_directory_the_temp_roots_and_the_accounts() {
    let fixture = Fixture::new();
    switch(&fixture, true);
    let account = Account {
        id: "account-1".to_string(),
        agent: AgentId::CLAUDE,
        name: "Work".to_string(),
        config_dir: text(&fixture.dir.join("account-config")),
        share_default_config: false,
        share_default_config_incomplete: false,
        disabled: false,
        versions: Default::default(),
    };
    fixture.state.store.insert_account(&account).unwrap();

    let list = list_for(&fixture, "a").await.expect("a list");

    assert!(
        list.shared.contains(&text(&crate::paths::data_dir())),
        "{:?}",
        list.shared
    );
    assert!(
        list.shared.contains(&account.config_dir),
        "{:?}",
        list.shared
    );
    let temp = crate::path_links::temp_roots();
    assert!(!temp.lexical.is_empty() || !temp.canonical.is_empty());
    for root in temp.lexical.iter().chain(&temp.canonical) {
        // As a path, not as text: a root written with a trailing separator is the same directory.
        assert!(
            list.shared.iter().any(|shared| Path::new(shared) == root),
            "{} not in {:?}",
            root.display(),
            list.shared
        );
    }
}

#[test]
fn the_launch_adds_its_directory_and_each_agents_default_directory() {
    let home = ScratchDir::new("guard-list-home");
    let launch = ScratchDir::new("guard-list-launch");
    let custom = ScratchDir::new("guard-list-custom-config");
    let env = HashMap::from([
        ("HOME".to_string(), text(&home)),
        ("CLAUDE_CONFIG_DIR".to_string(), text(&custom)),
    ]);

    let list = GuardList::default().with_launch(&launch, &env);

    assert_eq!(list.own, [text(&launch)]);
    assert!(list.shared.contains(&text(&custom)), "{:?}", list.shared);
    for agent in AgentId::ALL {
        let usual = home.join(crate::providers::layout::layout(agent).default_dir);
        assert!(
            list.shared.contains(&text(&usual)),
            "{agent:?}: {}",
            usual.display()
        );
    }
}

/// An agent's default directory is listed once, whether the shell exports it or it is the usual one
/// under the home directory.
#[test]
fn the_launch_lists_no_directory_twice() {
    let home = ScratchDir::new("guard-list-dupes-home");
    let launch = ScratchDir::new("guard-list-dupes-launch");
    let env = HashMap::from([("HOME".to_string(), text(&home))]);

    let list = GuardList::default().with_launch(&launch, &env);

    let unique: BTreeSet<&String> = list.shared.iter().collect();
    assert_eq!(unique.len(), list.shared.len(), "{:?}", list.shared);
}

/// The temporary directories the session's shell names pass, unless one is relative, the root of a
/// disk, or holds the home directory, which would make everything there pass.
#[test]
fn the_launch_adds_the_shells_temporary_directories_that_are_safe_to_pass() {
    let launch = ScratchDir::new("guard-list-tmp-launch");
    let tmp = ScratchDir::new("guard-list-tmp-dir");
    let home = ScratchDir::new("guard-list-tmp-home");
    let mut env = HashMap::from([
        ("HOME".to_string(), text(&home)),
        ("TMPDIR".to_string(), text(&tmp)),
        ("TEMP".to_string(), "relative/temp".to_string()),
    ]);
    let root = home
        .ancestors()
        .last()
        .map(Path::to_path_buf)
        .expect("a root");
    env.insert("TMP".to_string(), text(&root));

    let list = GuardList::default().with_launch(&launch, &env);

    assert!(list.shared.contains(&text(&tmp)), "{:?}", list.shared);
    for refused in ["relative/temp".to_string(), text(&root)] {
        assert!(!list.shared.contains(&refused), "{refused}");
    }
    if let Some(known) = crate::paths::known_home_dir() {
        let env = HashMap::from([("TMPDIR".to_string(), text(&known))]);
        let list = GuardList::default().with_launch(&launch, &env);
        assert!(
            !list.shared.contains(&text(&known)),
            "a directory holding the home"
        );
    }
}

#[test]
fn the_list_is_written_into_the_scratch_directory_for_its_owner_alone() {
    let scratch = ScratchDir::new("guard-list-write");
    let list = GuardList {
        tool: TOOL.to_string(),
        own: vec!["/a".to_string()],
        repository: Some("/a/.git".to_string()),
        projects: vec![GuardedProject {
            name: "B".to_string(),
            root: "/b".to_string(),
        }],
        shared: vec!["/c".to_string()],
    };

    let file = write(&scratch, &list).unwrap();

    assert_eq!(file.parent().unwrap(), &*scratch);
    assert_eq!(
        serde_json::from_slice::<GuardList>(&std::fs::read(&file).unwrap()).unwrap(),
        list
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    // A second launch writes over the first.
    let other = GuardList {
        tool: "t".to_string(),
        ..GuardList::default()
    };
    assert_eq!(write(&scratch, &other).unwrap(), file);
    assert_eq!(
        serde_json::from_slice::<GuardList>(&std::fs::read(&file).unwrap()).unwrap(),
        other
    );
}
