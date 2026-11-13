//! The guard's decision: which file-edit tool calls are denied, what the denial says, and that the
//! hook stays silent on everything it cannot decide.
//!
//! The decision is read in process; the hook as an agent runs it (a payload on stdin, a decision
//! on stdout, an exit status) is run through a child, which the stand-in
//! [`guard_hook_child`] is: this test binary running only that test.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use super::*;
use crate::test_support::ScratchDir;

const TOOL: &str = "mcp__octoboard__request_session";

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn dir(parent: &Path, name: &str) -> PathBuf {
    let path = parent.join(name);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn guarded(name: &str, root: &Path) -> GuardedProject {
    GuardedProject {
        name: name.to_string(),
        root: text(root),
    }
}

fn list(own: &[&Path], projects: Vec<GuardedProject>, shared: &[&Path]) -> GuardList {
    GuardList {
        tool: TOOL.to_string(),
        own: own.iter().map(|root| text(root)).collect(),
        repository: None,
        projects,
        shared: shared.iter().map(|root| text(root)).collect(),
    }
}

/// A `PreToolUse` payload of Claude Code's `Write` for `target`, run in `cwd`.
fn write_payload(target: &Path, cwd: &Path) -> Value {
    json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Write",
        "cwd": text(cwd),
        "tool_input": { "file_path": text(target), "content": "x" },
    })
}

fn decide_for(list: &GuardList, payload: &Value) -> Option<Value> {
    decide(list, payload.to_string().as_bytes())
}

/// The reason of a denial, which the decision has to carry in the shape the agents read.
fn denial(decision: Option<Value>) -> Option<String> {
    let decision = decision?;
    assert_eq!(
        decision["hookSpecificOutput"]["hookEventName"], "PreToolUse",
        "{decision}"
    );
    assert_eq!(
        decision["hookSpecificOutput"]["permissionDecision"], "deny",
        "{decision}"
    );
    assert_eq!(
        decision.as_object().unwrap().len(),
        1,
        "only the agents' own field: {decision}"
    );
    Some(
        decision["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .expect("a reason")
            .to_string(),
    )
}

/// The refusal as the contract words it.
fn reason_for(path: &str, project: &str, tool: &str) -> String {
    format!(
        "Blocked: {path} is in project \"{project}\", not this session's. Don't write it another \
         way. Keep any changes there as they are and call {tool} for a \"{project}\" session, \
         listing the changed files and where they are, so it reviews them and takes over."
    )
}

/// The longest a reason may be, which Grok's display of it sets.
const LIMIT: usize = 270;

/// What the reason has to be for a refusal of `path`: the contract's text where it fits, and
/// where it does not (the scratch directory's path can be long) a path shown short with the
/// hand-over in full and the whole within the limit.
fn assert_reason(reason: &str, path: &str, project: &str, tool: &str) {
    let full = reason_for(path, project, tool);
    if full.chars().count() <= LIMIT {
        assert_eq!(reason, full);
        return;
    }
    assert!(reason.chars().count() <= LIMIT, "{reason}");
    assert!(reason.starts_with("Blocked: "), "{reason}");
    let wording = reason_for("", project, tool);
    let hand_over = wording.split_once(" is in project").unwrap().1;
    assert!(
        reason.contains(&format!(" is in project \"{project}\"")),
        "{reason}"
    );
    assert!(reason.ends_with(hand_over), "{reason}");
}

/// Two project directories side by side, the session's own and another's.
struct Pair {
    _scratch: ScratchDir,
    root: PathBuf,
    own: PathBuf,
    other: PathBuf,
}

fn pair() -> Pair {
    let scratch = ScratchDir::new("guard-hook");
    let root = dir(&scratch, "work");
    let own = dir(&root, "own");
    let other = dir(&root, "other");
    std::fs::write(other.join("file.txt"), "x").unwrap();
    std::fs::write(own.join("file.txt"), "x").unwrap();
    Pair {
        _scratch: scratch,
        root,
        own,
        other,
    }
}

impl Pair {
    fn list(&self) -> GuardList {
        list(&[&self.own], vec![guarded("Beta", &self.other)], &[])
    }
}

// -- what is denied -------------------------------------------------------------

#[test]
fn a_write_into_another_project_is_denied_with_the_hand_over() {
    let pair = pair();
    let target = pair.other.join("file.txt");

    let reason = denial(decide_for(&pair.list(), &write_payload(&target, &pair.own)));

    assert_reason(&reason.expect("denied"), &text(&target), "Beta", TOOL);
}

#[test]
fn a_write_into_the_sessions_own_project_or_outside_every_project_passes() {
    let pair = pair();
    let list = pair.list();
    let outside = dir(&pair.root, "outside");

    for target in [
        pair.own.join("file.txt"),
        pair.own.join("new/deep/file.txt"),
        outside.join("file.txt"),
        pair.root.join("file.txt"),
    ] {
        assert_eq!(
            decide_for(&list, &write_payload(&target, &pair.own)),
            None,
            "{}",
            target.display()
        );
    }
}

#[test]
fn the_target_is_read_from_each_field_the_agents_name_it_in() {
    let pair = pair();
    let target = text(&pair.other.join("file.txt"));
    let cwd = text(&pair.own);
    let shapes = [
        json!({ "cwd": cwd, "tool_input": { "file_path": target } }),
        json!({ "cwd": cwd, "tool_input": { "notebook_path": target } }),
        json!({ "cwd": cwd, "tool_input": { "path": target } }),
        json!({ "cwd": cwd, "tool_input": { "target_notebook": target } }),
        json!({ "cwd": cwd, "toolInput": { "file_path": target } }),
        json!({ "cwd": cwd, "toolInput": { "path": target } }),
        json!({ "cwd": cwd, "toolInput": { "target_notebook": target } }),
    ];

    for shape in shapes {
        let reason = denial(decide_for(&pair.list(), &shape)).unwrap_or_else(|| panic!("{shape}"));
        assert_reason(&reason, &target, "Beta", TOOL);
    }
}

#[test]
fn a_relative_target_is_resolved_against_the_payloads_directory() {
    let pair = pair();
    let payload = json!({
        "cwd": text(&pair.own),
        "tool_input": { "file_path": "../other/file.txt" },
    });

    let reason = denial(decide_for(&pair.list(), &payload)).expect("denied");

    assert!(reason.starts_with("Blocked: "), "{reason}");
    assert!(reason.contains("\"Beta\""), "{reason}");
    assert!(
        !reason.contains(".."),
        "the path is shown normalised: {reason}"
    );
    let payload = json!({
        "cwd": text(&pair.other),
        "tool_input": { "file_path": "../own/file.txt" },
    });
    assert_eq!(decide_for(&pair.list(), &payload), None);
}

/// A Codex `apply_patch` names the files it touches in its headers, and every one is judged.
#[test]
fn every_file_an_apply_patch_names_is_judged_against_the_payloads_directory() {
    let pair = pair();
    let other_file = text(&pair.other.join("file.txt"));
    let patch = |lines: &[String]| {
        json!({
            "cwd": text(&pair.own),
            "tool_input": {
                "command": format!("*** Begin Patch\n{}\n*** End Patch\n", lines.join("\n")),
            },
        })
    };
    let own_update = "*** Update File: file.txt".to_string();

    // Each header form names a file in the other project.
    for header in [
        format!("*** Update File: {other_file}"),
        format!("*** Add File: {}", text(&pair.other.join("added.txt"))),
        format!("*** Delete File: {other_file}"),
        // A relative name goes by the payload's directory.
        "*** Update File: ../other/file.txt".to_string(),
    ] {
        let payload = patch(&[own_update.clone(), header.clone()]);
        let reason = denial(decide_for(&pair.list(), &payload));
        assert!(
            reason.is_some_and(|reason| reason.contains("\"Beta\"")),
            "{header}"
        );
    }
    // A header may be indented.
    let indented = patch(&[
        own_update.clone(),
        format!(
            "  \t*** Add File: {}",
            text(&pair.other.join("indented.txt"))
        ),
    ]);
    assert!(denial(decide_for(&pair.list(), &indented)).is_some());
    // A move's destination counts, although the file moved is the session's own.
    let moved = patch(&[
        own_update.clone(),
        format!("*** Move to: {}", text(&pair.other.join("moved.txt"))),
    ]);
    assert!(denial(decide_for(&pair.list(), &moved)).is_some());
    // The other project's file named after the session's own ones is still found.
    let last = patch(&[
        own_update.clone(),
        "*** Add File: own-two.txt".to_string(),
        format!("*** Update File: {}", text(&pair.other.join("last.txt"))),
    ]);
    let reason = denial(decide_for(&pair.list(), &last)).expect("denied");
    assert!(reason.contains("last.txt"), "{reason}");

    // A patch inside the session's own project, and a shell command, pass.
    let own_only = patch(&[
        own_update,
        "*** Add File: new.txt".to_string(),
        "*** Move to: moved.txt".to_string(),
    ]);
    assert_eq!(decide_for(&pair.list(), &own_only), None);
    let shell = json!({ "cwd": text(&pair.own), "tool_input": { "command": "ls ../other" } });
    assert_eq!(decide_for(&pair.list(), &shell), None);
}

#[test]
fn a_file_that_does_not_exist_yet_is_judged_by_its_nearest_existing_directory() {
    let pair = pair();
    let list = pair.list();

    let new_in_other = pair.other.join("not/yet/there/new.txt");
    let reason = denial(decide_for(&list, &write_payload(&new_in_other, &pair.own)));
    assert_reason(&reason.expect("denied"), &text(&new_in_other), "Beta", TOOL);
    assert_eq!(
        decide_for(
            &list,
            &write_payload(&pair.own.join("not/yet/new.txt"), &pair.own)
        ),
        None
    );
}

#[cfg(unix)]
#[test]
fn a_link_into_another_project_is_followed() {
    let pair = pair();
    let list = pair.list();
    crate::test_support::symlink(&pair.other, pair.own.join("dir-link")).unwrap();
    crate::test_support::symlink(pair.other.join("file.txt"), pair.own.join("file-link")).unwrap();

    for target in [
        pair.own.join("dir-link/file.txt"),
        pair.own.join("dir-link/new.txt"),
        pair.own.join("file-link"),
    ] {
        assert!(
            denial(decide_for(&list, &write_payload(&target, &pair.own))).is_some(),
            "{}",
            target.display()
        );
    }
    // The other way round: a link in the other project that leads into the session's own.
    crate::test_support::symlink(&pair.own, pair.other.join("back")).unwrap();
    assert_eq!(
        decide_for(
            &list,
            &write_payload(&pair.other.join("back/file.txt"), &pair.own)
        ),
        None
    );
}

/// A path is told by what it leads to: through the second real path a firmlink or a bind mount
/// gives it, or in another letter case on a file system that ignores case.
#[test]
fn another_spelling_of_the_same_directory_is_the_same_project() {
    let pair = pair();
    let list = pair.list();
    let real = crate::hostfs::canonical(&pair.other).unwrap();
    match crate::test_support::second_real_path(&real) {
        Some(second) => assert!(
            denial(decide_for(
                &list,
                &write_payload(&second.join("file.txt"), &pair.own)
            ))
            .is_some(),
            "second real path"
        ),
        None => eprintln!("{} has no second real path", real.display()),
    }
    let respelled = pair.other.with_file_name("OTHER");
    let identity =
        |path: &Path| crate::platform::fs::identity(path, crate::platform::fs::Links::Follow).ok();
    if identity(&respelled) == identity(&pair.other) {
        assert!(
            denial(decide_for(
                &list,
                &write_payload(&respelled.join("file.txt"), &pair.own)
            ))
            .is_some(),
            "letter case"
        );
    } else {
        eprintln!("no case-only respelling: the file system tells case apart");
    }
}

// -- nested roots ---------------------------------------------------------------

#[test]
fn the_deepest_root_that_holds_a_path_decides() {
    let scratch = ScratchDir::new("guard-hook-nested");
    let outer = dir(&scratch, "outer");
    let inner = dir(&outer, "inner");

    // The session's own project inside another project: its own files pass, the rest of the
    // other project does not.
    let own_inside = list(&[&inner], vec![guarded("Outer", &outer)], &[]);
    assert_eq!(
        decide_for(&own_inside, &write_payload(&inner.join("a.txt"), &inner)),
        None
    );
    let reason = denial(decide_for(
        &own_inside,
        &write_payload(&outer.join("a.txt"), &inner),
    ));
    assert!(reason.is_some_and(|reason| reason.contains("\"Outer\"")));

    // Another project inside the session's own is refused, and the rest of the own project is not.
    let other_inside = list(&[&outer], vec![guarded("Inner", &inner)], &[]);
    let reason = denial(decide_for(
        &other_inside,
        &write_payload(&inner.join("a.txt"), &outer),
    ));
    assert!(reason.is_some_and(|reason| reason.contains("\"Inner\"")));
    assert_eq!(
        decide_for(&other_inside, &write_payload(&outer.join("a.txt"), &outer)),
        None
    );
}

#[test]
fn a_shared_directory_under_a_project_passes_and_the_rest_of_the_project_does_not() {
    let scratch = ScratchDir::new("guard-hook-shared");
    let home = dir(&scratch, "home");
    let agent_dir = dir(&home, ".agent");
    let work = dir(&scratch, "work");
    let list = list(&[&work], vec![guarded("Home", &home)], &[&agent_dir]);

    assert_eq!(
        decide_for(
            &list,
            &write_payload(&agent_dir.join("settings.json"), &work)
        ),
        None
    );
    assert_eq!(
        decide_for(
            &list,
            &write_payload(&agent_dir.join("new/dir/a.json"), &work)
        ),
        None
    );
    for target in [home.join("notes.txt"), home.join(".agent-other/a.json")] {
        let reason = denial(decide_for(&list, &write_payload(&target, &work)));
        assert!(
            reason.is_some_and(|reason| reason.contains("\"Home\"")),
            "{}",
            target.display()
        );
    }
}

#[test]
fn a_project_inside_a_shared_temporary_directory_is_still_guarded() {
    let scratch = ScratchDir::new("guard-hook-temp");
    let temp = dir(&scratch, "tmp");
    let project = dir(&temp, "project");
    let own = dir(&scratch, "own");
    let list = list(&[&own], vec![guarded("Temp", &project)], &[&temp]);

    let reason = denial(decide_for(
        &list,
        &write_payload(&project.join("a.txt"), &own),
    ));
    assert!(reason.is_some_and(|reason| reason.contains("\"Temp\"")));
    assert_eq!(
        decide_for(&list, &write_payload(&temp.join("scratch.txt"), &own)),
        None
    );
}

#[test]
fn a_root_that_cannot_be_resolved_holds_nothing() {
    let pair = pair();
    let gone = pair.root.join("gone");
    let list = list(
        &[&pair.own],
        vec![guarded("Gone", &gone), guarded("Beta", &pair.other)],
        &[&pair.root.join("missing-shared")],
    );

    assert_eq!(
        decide_for(&list, &write_payload(&gone.join("a.txt"), &pair.own)),
        None
    );
    let reason = denial(decide_for(
        &list,
        &write_payload(&pair.other.join("file.txt"), &pair.own),
    ));
    assert!(reason.is_some_and(|reason| reason.contains("\"Beta\"")));
}

/// A target in the session's own repository is its own wherever the working tree is: a linked
/// worktree made after the launch outside its project's directory, or a sibling project of a
/// monorepo, although another project's root holds it too.
#[test]
fn a_path_in_the_sessions_own_repository_passes_though_another_project_holds_it() {
    use crate::git::repo_files::repository_of;
    use crate::test_support::{git, repo_with};

    let scratch = ScratchDir::new("guard-hook-repository");
    let home = dir(&scratch, "home");
    let repository = repo_with(
        "guard-hook-own-repo",
        &[(b"a/one.txt", b"1"), (b"b/two.txt", b"2")],
    );
    let own = repository.join("a");
    let worktree = home.join("tree-made-later");
    git(
        &repository,
        &[
            "worktree",
            "add",
            "-q",
            worktree.to_str().unwrap(),
            "-b",
            "later",
        ],
    );
    let elsewhere = repo_with("guard-hook-home-repo", &[(b"c.txt", b"3")]);
    let inside_home = home.join("other-repo");
    git(
        &elsewhere,
        &[
            "worktree",
            "add",
            "-q",
            inside_home.to_str().unwrap(),
            "-b",
            "foreign",
        ],
    );
    let list = GuardList {
        repository: repository_of(&own).map(|common| text(&common)),
        ..list(&[&own], vec![guarded("Home", &home)], &[])
    };
    assert!(list.repository.is_some());

    // Held by the project at the home directory, and in the own repository.
    for target in [
        worktree.join("a/new.txt"),
        worktree.join("b/two.txt"),
        repository.join("b/two.txt"),
        repository.join("b/not/yet/there.txt"),
    ] {
        assert_eq!(
            decide_for(&list, &write_payload(&target, &own)),
            None,
            "{}",
            target.display()
        );
    }
    // Another repository inside the home directory is another project's, and so is a file there
    // that is in no repository.
    let loose = dir(&home, "loose");
    for target in [inside_home.join("c.txt"), loose.join("notes.txt")] {
        let reason = denial(decide_for(&list, &write_payload(&target, &own)));
        assert!(
            reason.is_some_and(|reason| reason.contains("\"Home\"")),
            "{}",
            target.display()
        );
    }
    // Without the repository in the list the same worktree is the other project's.
    let without = GuardList {
        repository: None,
        ..list
    };
    assert!(denial(decide_for(
        &without,
        &write_payload(&worktree.join("b/two.txt"), &own)
    ))
    .is_some());
}

#[test]
fn the_refusal_shows_the_path_normalised_and_in_the_case_the_payload_gave() {
    let pair = pair();
    let target = pair.other.join("MixedCase.TXT");
    let payload = json!({
        "cwd": text(&pair.own),
        "tool_input": { "file_path": format!("{}/../other/MixedCase.TXT", text(&pair.own)) },
    });

    let reason = denial(decide_for(&pair.list(), &payload)).expect("denied");

    assert_reason(&reason, &text(&target), "Beta", TOOL);
    assert!(!reason.contains(".."), "{reason}");
    // The case is the payload's: a long path is cut at its start, so the end is what shows.
    assert!(reason.contains("TXT is in project"), "{reason}");
}

// -- the reason -----------------------------------------------------------------

fn long_name(piece: &str, length: usize) -> String {
    piece.repeat(length / piece.len())
}

#[test]
fn a_long_path_is_shown_relative_to_the_project_and_the_hand_over_is_kept_whole() {
    let scratch = ScratchDir::new("guard-hook-reason-relative");
    let own = dir(&scratch, "own");
    // A root deep enough that its absolute path alone would overrun the limit.
    let deep = dir(&dir(&scratch, &long_name("p", 120)), &long_name("q", 120));
    let root = dir(&deep, "proj");
    let list = GuardList {
        tool: "t".to_string(),
        ..list(&[&own], vec![guarded("Beta", &root)], &[])
    };
    let target = root.join("src/lib.rs");
    assert!(reason_for(&text(&target), "Beta", "t").chars().count() > LIMIT);

    let reason = denial(decide_for(&list, &write_payload(&target, &own))).expect("denied");

    assert_eq!(reason, reason_for("src/lib.rs", "Beta", "t"));
    assert!(reason.chars().count() <= LIMIT);
}

#[test]
fn a_path_too_long_even_relative_is_cut_at_its_start_never_the_hand_over() {
    let scratch = ScratchDir::new("guard-hook-reason-cut");
    let own = dir(&scratch, "own");
    let root = dir(&scratch, "proj");
    let list = list(&[&own], vec![guarded("Beta", &root)], &[]);
    let nested = format!(
        "{}/{}/{}",
        long_name("a", 140),
        long_name("b", 140),
        "name.rs"
    );
    let target = root.join(&nested);

    let reason = denial(decide_for(&list, &write_payload(&target, &own))).expect("denied");

    assert!(
        reason.chars().count() <= LIMIT,
        "{}",
        reason.chars().count()
    );
    assert!(reason.starts_with("Blocked: \u{2026}"), "{reason}");
    assert!(
        reason.contains("name.rs is in project \"Beta\""),
        "{reason}"
    );
    let wording = reason_for("", "Beta", TOOL);
    let hand_over = wording.split_once(" is in project").unwrap().1;
    assert!(reason.ends_with(hand_over), "{reason}");
    assert!(reason.contains(TOOL), "{reason}");
}

#[test]
fn a_short_path_is_shown_as_it_is() {
    let pair = pair();
    let target = pair.other.join("file.txt");

    let list = GuardList {
        tool: "t".to_string(),
        ..pair.list()
    };
    if reason_for(&text(&target), "Beta", "t").chars().count() > LIMIT {
        eprintln!("the scratch directory's path is too long for a path to be shown whole");
        return;
    }

    let reason = denial(decide_for(&list, &write_payload(&target, &pair.own))).unwrap();

    assert_eq!(reason, reason_for(&text(&target), "Beta", "t"));
}

// -- failing open ---------------------------------------------------------------

#[test]
fn a_payload_that_cannot_be_read_is_no_decision() {
    let pair = pair();
    let list = pair.list();
    let target = text(&pair.other.join("file.txt"));

    for payload in [
        "".to_string(),
        "not json".to_string(),
        "[]".to_string(),
        "{}".to_string(),
        json!({ "cwd": text(&pair.own) }).to_string(),
        json!({ "cwd": text(&pair.own), "tool_input": {} }).to_string(),
        json!({ "cwd": text(&pair.own), "tool_input": "x" }).to_string(),
        json!({ "cwd": text(&pair.own), "tool_input": { "file_path": "" } }).to_string(),
        json!({ "cwd": text(&pair.own), "tool_input": { "file_path": 5 } }).to_string(),
        // A relative target with a relative directory leads nowhere.
        json!({ "cwd": "relative", "tool_input": { "file_path": "a.txt" } }).to_string(),
        json!({ "cwd": text(&pair.own), "tool_input": { "command": format!("cat {target}") } })
            .to_string(),
    ] {
        assert_eq!(decide(&list, payload.as_bytes()), None, "{payload}");
    }
}

#[test]
fn an_absolute_target_is_judged_without_a_payload_directory_and_a_relative_one_is_not() {
    let pair = pair();
    let list = pair.list();
    let target = text(&pair.other.join("file.txt"));

    let reason = denial(decide(
        &list,
        json!({ "tool_input": { "file_path": target } })
            .to_string()
            .as_bytes(),
    ))
    .expect("denied");
    assert_reason(&reason, &target, "Beta", TOOL);
    assert_eq!(
        decide(
            &list,
            json!({ "tool_input": { "file_path": text(&pair.own.join("file.txt")) } })
                .to_string()
                .as_bytes()
        ),
        None,
        "the session's own file passes"
    );
    // A relative target with no absolute directory to resolve it against gives no decision.
    let payload = json!({ "cwd": "relative", "tool_input": { "file_path": "../other/file.txt" } });
    assert_eq!(decide(&list, payload.to_string().as_bytes()), None);
}

#[test]
fn a_list_that_is_missing_or_unreadable_is_no_list() {
    let scratch = ScratchDir::new("guard-hook-list");
    let file = scratch.join("list.json");
    assert!(read_list(&file).is_none(), "missing");
    std::fs::write(&file, "not json").unwrap();
    assert!(read_list(&file).is_none(), "not a list");
    std::fs::write(&file, "{\"tool\":1}").unwrap();
    assert!(read_list(&file).is_none(), "wrong shape");
    assert!(read_list(&scratch).is_none(), "a directory");

    let wanted = list(
        &[Path::new("/a")],
        vec![guarded("B", Path::new("/b"))],
        &[Path::new("/c")],
    );
    std::fs::write(&file, serde_json::to_vec(&wanted).unwrap()).unwrap();
    assert_eq!(read_list(&file), Some(wanted));
}

// -- the hook as an agent runs it -----------------------------------------------

/// What the stand-in starts for `octoboardd guard-hook`: this test binary, running only this test,
/// with the list's file in the environment.
#[test]
#[ignore = "started by the tests below as the guard-hook process"]
fn guard_hook_child() {
    let list = std::env::var_os("OCTOBOARD_TEST_GUARD_LIST").unwrap();
    super::run(Path::new(&list));
    std::process::exit(0);
}

struct Ran {
    success: bool,
    stderr: String,
    /// The decision the hook printed, if it printed one.
    decision: Option<Value>,
    /// Anything of the hook's stdout that is not the test harness's own.
    printed: String,
}

fn run_child(list_file: &Path, payload: &str, env: &[(&str, &Path)]) -> Ran {
    use std::io::Write;

    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "cross_project_guard::hook::tests::guard_hook_child",
            "--ignored",
            "--nocapture",
        ])
        .env("OCTOBOARD_TEST_GUARD_LIST", list_file)
        .env_remove("GROK_WORKSPACE_ROOT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let payload = payload.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(payload.as_bytes());
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let marker = "{\"hookSpecificOutput\"";
    let printed = stdout.find(marker).map(|at| stdout[at..].to_string());
    Ran {
        success: output.status.success(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        decision: printed
            .as_deref()
            .and_then(|line| serde_json::from_str(line.lines().next()?).ok()),
        printed: printed.unwrap_or_default(),
    }
}

fn written_list(pair: &Pair) -> PathBuf {
    let file = pair.root.join("guard-list.json");
    std::fs::write(&file, serde_json::to_vec(&pair.list()).unwrap()).unwrap();
    file
}

#[test]
fn the_hook_prints_the_denial_on_one_line_and_exits_zero() {
    let pair = pair();
    let target = pair.other.join("file.txt");
    let payload = write_payload(&target, &pair.own).to_string();

    let ran = run_child(&written_list(&pair), &payload, &[]);

    assert!(ran.success, "{}", ran.stderr);
    assert_eq!(ran.stderr, "");
    let reason = denial(ran.decision.clone()).unwrap_or_else(|| panic!("{}", ran.printed));
    assert_reason(&reason, &text(&target), "Beta", TOOL);
    assert_eq!(ran.printed.lines().count(), 1, "{}", ran.printed);
}

/// A project name of CJK characters and an astral one (an emoji, which is a surrogate pair in
/// JSON's escapes), as a decision names it.
const UNICODE_NAME: &str = "\u{9879}\u{76EE} \u{1F680} caf\u{e9}";

#[test]
fn ascii_json_escapes_every_non_ascii_character_and_changes_nothing_else() {
    let decision = json!({
        "reason": format!("in project \"{UNICODE_NAME}\" at /a b/\\ \n"),
        "name\u{9879}": [1, true, null],
    });
    let text = decision.to_string();

    let escaped = ascii_json(&text);

    assert!(escaped.is_ascii(), "{escaped}");
    assert!(escaped.contains("\\u9879\\u76ee"), "{escaped}");
    // The emoji is one surrogate pair.
    assert!(escaped.contains("\\ud83d\\ude80"), "{escaped}");
    assert_eq!(serde_json::from_str::<Value>(&escaped).unwrap(), decision);
    // Text that is ASCII already is left as it is.
    assert_eq!(ascii_json("{\"a\":\"b\\n\"}"), "{\"a\":\"b\\n\"}");
}

#[test]
fn the_hook_prints_a_decision_naming_a_non_ascii_project_in_ascii_only() {
    let pair = pair();
    let list = GuardList {
        projects: vec![guarded(UNICODE_NAME, &pair.other)],
        ..pair.list()
    };
    let file = pair.root.join("unicode-list.json");
    std::fs::write(&file, serde_json::to_vec(&list).unwrap()).unwrap();
    let target = pair.other.join("file.txt");
    let payload = write_payload(&target, &pair.own).to_string();

    let ran = run_child(&file, &payload, &[]);

    assert!(ran.success, "{}", ran.stderr);
    assert!(ran.printed.is_ascii(), "{}", ran.printed);
    assert!(ran.printed.contains("\\u9879"), "{}", ran.printed);
    let reason = denial(ran.decision).expect("denied");
    assert!(
        reason.contains(&format!("project \"{UNICODE_NAME}\"")),
        "{reason}"
    );
}

#[test]
fn the_hook_is_silent_for_a_pass_and_for_everything_it_cannot_decide() {
    let pair = pair();
    let own_write = write_payload(&pair.own.join("file.txt"), &pair.own).to_string();
    let foreign_write = write_payload(&pair.other.join("file.txt"), &pair.own).to_string();
    let list = written_list(&pair);
    let missing = pair.root.join("no-such-list.json");
    let garbled = pair.root.join("garbled.json");
    std::fs::write(&garbled, "{{").unwrap();

    for (label, list_file, payload) in [
        ("a pass", &list, own_write.as_str()),
        ("a payload that is not one", &list, "not json"),
        ("no payload", &list, ""),
        ("a missing list", &missing, foreign_write.as_str()),
        ("an unreadable list", &garbled, foreign_write.as_str()),
    ] {
        let ran = run_child(list_file, payload, &[]);
        assert!(ran.success, "{label}: {}", ran.stderr);
        assert_eq!(ran.printed, "", "{label}");
        assert_eq!(ran.stderr, "", "{label}");
    }
}

#[test]
fn the_hook_judges_an_absolute_target_with_no_directory_and_stays_silent_for_a_relative_one() {
    let pair = pair();
    let list = written_list(&pair);
    let target = pair.other.join("file.txt");

    let absolute = json!({ "tool_input": { "file_path": text(&target) } }).to_string();
    let ran = run_child(&list, &absolute, &[]);
    assert!(ran.success, "{}", ran.stderr);
    let reason = denial(ran.decision).expect("denied");
    assert!(reason.contains("\"Beta\""), "{reason}");

    let relative = json!({ "tool_input": { "file_path": "../other/file.txt" } }).to_string();
    let ran = run_child(&list, &relative, &[]);
    assert!(ran.success, "{}", ran.stderr);
    assert_eq!(ran.printed, "");
    assert_eq!(ran.stderr, "");
}

/// Grok's payload may lack the directory, which it gives its hooks in the environment instead.
#[test]
fn a_payload_without_a_directory_goes_by_the_workspace_root_grok_gives() {
    let pair = pair();
    let payload = json!({ "toolInput": { "path": "../other/file.txt" } }).to_string();

    let ran = run_child(
        &written_list(&pair),
        &payload,
        &[("GROK_WORKSPACE_ROOT", &pair.own)],
    );

    assert!(ran.success, "{}", ran.stderr);
    let reason = denial(ran.decision).expect("denied");
    assert!(reason.contains("\"Beta\""), "{reason}");
    // From the other project's own root, the same relative name leads into the session's own.
    let payload = json!({ "toolInput": { "path": "../own/file.txt" } }).to_string();
    let ran = run_child(
        &written_list(&pair),
        &payload,
        &[("GROK_WORKSPACE_ROOT", &pair.other)],
    );
    assert!(ran.success, "{}", ran.stderr);
    assert_eq!(ran.printed, "");
}
