use super::*;
use crate::mcp::dispatch::call;
use crate::mcp::test_support::{
    account, arguments, console_session, peer_state, project_session, shared_console_state,
    with_status, RepoState,
};
use crate::protocol::{Agent, Console, OrchestrationRole, Project, Session, SessionStatus};
use crate::test_support::{running, ScratchDir};
use anyhow::anyhow;
use serde_json::{json, Map, Value};

#[test]
fn escape_sequences_and_control_bytes_are_stripped_from_the_output_tail() {
    let raw = b"\x1b[2J\x1b[1;32mbuilding\x1b[0m\r\n\x1b]0;title\x07done\n";
    assert_eq!(readable_output(raw), "building\ndone\n");
}

#[test]
fn runs_of_blank_lines_collapse() {
    assert_eq!(readable_output(b"a\n\n\n\n b \n"), "a\n\n b\n");
}

/// Partial UTF-8 is normal: the tail starts at a byte offset into the ring buffer, not at a
/// character boundary.
#[test]
fn a_tail_cut_mid_character_still_reads() {
    let mut raw = "ok ".as_bytes().to_vec();
    raw.extend_from_slice(&"日".as_bytes()[..2]);
    assert!(readable_output(&raw).starts_with("ok "));
}

fn ids(listed: &Value) -> Vec<&str> {
    let mut ids: Vec<&str> = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|session| session["session"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    ids
}

/// `kind` and `branch` narrow both lists the same way, and `total` of the archive counts what
/// they keep.
#[tokio::test]
async fn sessions_are_filtered_by_kind_and_by_the_branch_they_are_responsible_for() {
    let fixture = RepoState::new("reads-kind-branch");
    let state = &fixture.state;
    fixture.branch("lead-work", "lead");
    fixture.branch("kid-work", "kid");
    let mut kid = state.session_record("kid").unwrap();
    kid.status = SessionStatus::Archived;
    state.save_session(&kid).unwrap();

    let live = |filters: Value| async move {
        call(state, "loose", "list_sessions", &arguments(filters)).await
    };
    let archived = |filters: Value| async move {
        call(
            state,
            "loose",
            "list_archived_sessions",
            &arguments(filters),
        )
        .await
    };
    assert_eq!(
        ids(&live(json!({ "branch": "lead-work" })).await.unwrap()),
        ["lead"]
    );
    assert!(ids(&live(json!({ "branch": "kid-work" })).await.unwrap()).is_empty());
    let consoles = live(json!({ "kind": "console" })).await.unwrap();
    assert!(!ids(&consoles).is_empty());
    assert!(consoles["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|session| session["role"] == "console"));
    let projects = live(json!({ "kind": "project" })).await.unwrap();
    assert!(ids(&projects).contains(&"lead"));
    assert!(projects["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|session| session["role"] == "project"));

    let found = archived(json!({ "branch": "kid-work" })).await.unwrap();
    assert_eq!(ids(&found), ["kid"]);
    assert_eq!(found["total"], 1);
    assert_eq!(found["sessions"][0]["branches"], json!(["kid-work"]));
    let none = archived(json!({ "branch": "lead-work" })).await.unwrap();
    assert!(ids(&none).is_empty());
    assert_eq!(none["total"], 0);

    for tool in ["list_sessions", "list_archived_sessions"] {
        let err = call(state, "loose", tool, &arguments(json!({ "kind": "hub" })))
            .await
            .expect_err(tool);
        assert!(err.to_string().contains("`kind` must be"), "{tool}: {err}");
    }
}

/// A session record carries `paused` only for an idle session whose last turn ended abnormally,
/// and a session over a view of its background agents is `dormant` with a note saying to leave it
/// be.
#[tokio::test]
async fn a_session_record_says_whether_it_is_paused_or_dormant() {
    let fixture = RepoState::new("reads-paused-dormant");
    let state = &fixture.state;
    let live = running(state, "lead");
    let get = || async {
        call(
            state,
            "loose",
            "get_session",
            &arguments(json!({ "session": "lead" })),
        )
        .await
        .unwrap()
    };

    with_status(state, "lead", SessionStatus::Idle);
    assert_eq!(get().await["paused"], false);
    state.set_paused("lead", true);
    let paused = get().await;
    assert_eq!(paused["status"], "idle");
    assert_eq!(paused["paused"], true);
    assert!(paused.get("note").is_none());
    let listed = call(state, "loose", "list_sessions", &arguments(json!({})))
        .await
        .unwrap();
    let entry = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["session"] == "lead")
        .unwrap();
    assert_eq!(entry["paused"], true);

    with_status(state, "lead", SessionStatus::Working);
    assert_eq!(get().await["paused"], false);

    with_status(state, "lead", SessionStatus::Idle);
    crate::test_support::show_agents_view(&live);
    let dormant = get().await;
    assert_eq!(dormant["status"], "dormant");
    assert_eq!(dormant["paused"], false);
    let note = dormant["note"].as_str().unwrap();
    assert!(note.contains("background-agents view"), "{note}");
}

/// An interrupted or archived session says whether its agent still holds the conversation:
/// proven gone is `false`, one that cannot be told `null`, one with no conversation `true`; a
/// running session has no such field, and `list_archived_sessions` adds it only when asked.
#[tokio::test]
async fn a_no_process_session_says_whether_it_can_be_resumed() {
    let fixture = RepoState::new("reads-resumable");
    let state = &fixture.state;
    let account = ScratchDir::new("reads-resumable-account");
    std::fs::create_dir_all(account.join("projects")).unwrap();
    let get = |id: &'static str| async move {
        call(
            state,
            "loose",
            "get_session",
            &arguments(json!({ "session": id })),
        )
        .await
    };

    let running = get("lead").await.unwrap();
    assert!(running.get("resumable").is_none());

    let mut kid = state.session_record("kid").unwrap();
    kid.status = SessionStatus::Interrupted;
    state.save_session(&kid).unwrap();
    assert_eq!(get("kid").await.unwrap()["resumable"], true);

    kid.has_conversation = true;
    kid.agent_session_id = None;
    state.save_session(&kid).unwrap();
    assert!(get("kid").await.unwrap()["resumable"].is_null());

    kid.agent_session_id = Some("019a0000-0000-7000-8000-000000000001".to_string());
    kid.status = SessionStatus::Archived;
    state.save_session(&kid).unwrap();
    state
        .set_session_account("kid", None, Some(&account.to_string_lossy()))
        .unwrap();
    assert_eq!(get("kid").await.unwrap()["resumable"], false);

    let asked = call(
        state,
        "loose",
        "list_archived_sessions",
        &arguments(json!({ "check_resumable": true })),
    )
    .await
    .unwrap();
    let entry_of = |listed: &Value| {
        listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|session| session["session"] == "kid")
            .unwrap()
            .clone()
    };
    assert_eq!(entry_of(&asked)["resumable"], false);
    let plain = call(state, "loose", "list_archived_sessions", &Map::new())
        .await
        .unwrap();
    assert!(entry_of(&plain).get("resumable").is_none());
}

/// Stores a Claude account `claude-a`, whose directory no reader may see, and a session
/// `rich` in `repo` with every field of the record set: on that account, pinned, locked, with
/// an observed model and effort and a default effort, and responsible for two branches.
fn rich_session(fixture: &RepoState) {
    let state = &fixture.state;
    state
        .store
        .insert_account(&account(
            "claude-a",
            Agent::Claude,
            "Work account",
            "/tmp/octoboard-secret-account-dir",
        ))
        .unwrap();
    state
        .store
        .insert_session(&Session {
            project_id: Some("repo".to_string()),
            account_id: Some("claude-a".to_string()),
            config_dir: Some("/tmp/octoboard-secret-account-dir".to_string()),
            pinned: true,
            locked: true,
            observed_model: Some("claude-opus-5-5".to_string()),
            observed_effort: Some("high".to_string()),
            ..project_session("rich", "console-1", Some("a"))
        })
        .unwrap();
    state.set_default_effort("rich", Some("medium".to_string()));
    fixture.branch("feature", "rich");
    fixture.branch("feat/nested", "rich");
}

/// What a record says of the session, as each kind of caller reads it: a console session and
/// an unbound project session through `get_session`, a bound project session through
/// `list_sessions`.
#[tokio::test]
async fn every_caller_reads_the_account_model_pin_lock_and_branches_of_a_session() {
    let fixture = RepoState::new("record-fields");
    rich_session(&fixture);
    let get = |caller: &'static str, session: &'static str| {
        let state = fixture.state.clone();
        async move {
            call(
                &state,
                caller,
                "get_session",
                &arguments(json!({ "session": session })),
            )
            .await
            .unwrap()
        }
    };
    let listed = call(&fixture.state, "lead-kid", "list_sessions", &Map::new())
        .await
        .unwrap();
    let in_list = |id: &str| {
        listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["session"] == id)
            .unwrap()
            .clone()
    };
    for (caller, record) in [
        ("a", get("a", "rich").await),
        ("lead", get("lead", "rich").await),
        ("lead-kid", in_list("rich")),
    ] {
        assert_eq!(record["account"], "claude-a", "{caller}");
        assert_eq!(record["account_name"], "Work account", "{caller}");
        assert_eq!(record["observed_model"], "claude-opus-5-5", "{caller}");
        assert_eq!(record["observed_effort"], "high", "{caller}");
        assert_eq!(record["default_effort"], "medium", "{caller}");
        assert_eq!(record["pinned"], true, "{caller}");
        assert_eq!(record["locked"], true, "{caller}");
        let mut branches: Vec<&str> = record["branches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|branch| branch.as_str().unwrap())
            .collect();
        branches.sort_unstable();
        assert_eq!(branches, ["feat/nested", "feature"], "{caller}");
        assert!(
            !record.to_string().contains("octoboard-secret-account-dir"),
            "{caller}: {record}"
        );
    }

    // A session on the default account that nothing was observed of.
    let plain = in_list("kid");
    assert_eq!(plain["account"], "default");
    assert_eq!(plain["account_name"], crate::protocol::DEFAULT_ACCOUNT_NAME);
    for field in ["observed_model", "observed_effort", "default_effort"] {
        assert!(plain[field].is_null(), "{field}");
    }
    assert_eq!(plain["pinned"], false);
    assert_eq!(plain["locked"], false);
    assert_eq!(plain["branches"], json!([]));

    // A console session is responsible for no branch.
    assert_eq!(get("lead", "b").await["branches"], json!([]));
}

/// The default account is shown under the name the user gave it.
#[tokio::test]
async fn the_default_account_is_shown_under_the_name_the_user_gave_it() {
    let fixture = RepoState::new("record-default-name");
    fixture
        .state
        .store
        .set_default_account_name(Agent::Claude, Some("Personal"))
        .unwrap();
    let record = call(
        &fixture.state,
        "a",
        "get_session",
        &arguments(json!({ "session": "kid" })),
    )
    .await
    .unwrap();
    assert_eq!(record["account"], "default");
    assert_eq!(record["account_name"], "Personal");
}

/// Reading a record runs no process: with no `git` to run, the branches are still listed.
#[tokio::test]
async fn reading_a_record_runs_no_git() {
    let fixture = RepoState::new("record-no-git");
    rich_session(&fixture);
    fixture.state.host_tools.replace(crate::host_tools::Tools {
        git: || Err(anyhow!("git must not run")),
        ..crate::test_support::host_tools()
    });
    let record = call(
        &fixture.state,
        "a",
        "get_session",
        &arguments(json!({ "session": "rich" })),
    )
    .await
    .unwrap();
    assert_eq!(record["branches"].as_array().unwrap().len(), 2);
}

/// Reads are console-wide: each console session sees the other's sessions and who owns them.
#[tokio::test]
async fn reads_cover_the_whole_console_with_each_sessions_owner() {
    let (state, _dir) = shared_console_state("tool-surface-reads");
    for (caller, other_owner_session, other_owner) in [("a", "of-b", "b"), ("b", "of-a", "a")] {
        let listed = call(&state, caller, "list_projects", &Map::new())
            .await
            .unwrap();
        // Only this console's project is listed, so the other console's session is not.
        assert_eq!(listed["projects"].as_array().unwrap().len(), 1);
        let sessions = listed["projects"][0]["sessions"].as_array().unwrap();
        let ids: Vec<&str> = sessions
            .iter()
            .map(|session| session["session"].as_str().unwrap())
            .collect();
        assert_eq!(ids.len(), 3, "{ids:?}");
        assert!(
            ids.contains(&"loose") && !ids.contains(&"foreign"),
            "{ids:?}"
        );
        let theirs = sessions
            .iter()
            .find(|session| session["session"] == other_owner_session)
            .unwrap();
        assert_eq!(theirs["owner"], other_owner);
        assert_eq!(theirs["owner_kind"], "console");
        assert_eq!(theirs["yours"], false);

        let fetched = call(
            &state,
            caller,
            "get_session",
            &arguments(json!({ "session": other_owner_session })),
        )
        .await
        .unwrap();
        assert_eq!(fetched["owner"], other_owner);
        assert_eq!(fetched["yours"], false);
    }
    let loose = call(
        &state,
        "a",
        "get_session",
        &arguments(json!({ "session": "loose" })),
    )
    .await
    .unwrap();
    assert!(loose["owner"].is_null());
}

/// A session whose process has ended still has its last output read, through the file the
/// terminal socket replays, capped and with escape sequences stripped. One with nothing kept
/// has no output to show.
#[tokio::test]
async fn a_session_whose_process_has_ended_still_shows_its_kept_output() {
    let (state, dir) = shared_console_state("tool-surface-kept-output");
    let output_dir = dir.join("output");
    std::fs::create_dir_all(&output_dir).unwrap();
    let mut kept = vec![b'a'; 10 * 1024];
    kept.extend_from_slice(b"\x1b[1mlast words\x1b[0m");
    std::fs::write(output_dir.join("archived-of-a"), kept).unwrap();

    let read = |session: &'static str| {
        let state = state.clone();
        async move {
            call(
                &state,
                "a",
                "get_session",
                &arguments(json!({ "session": session })),
            )
            .await
            .unwrap()
        }
    };
    let archived = read("archived-of-a").await;
    assert_eq!(archived["status"], "archived");
    let tail = archived["recent_output"].as_str().expect("kept output");
    assert!(tail.trim_end().ends_with("last words"), "{tail:?}");
    assert!(!tail.contains('\x1b'), "{tail:?}");
    assert!(tail.len() <= 8 * 1024, "{}", tail.len());
    assert!(read("archived-of-b").await["recent_output"].is_null());
}

/// `output_bytes` sets how much of the output is returned, from 1 to 64 KiB; outside that it is
/// refused, not clamped.
#[tokio::test]
async fn the_output_tail_length_is_the_callers_within_bounds() {
    let (state, dir) = shared_console_state("tool-surface-tail-length");
    let output_dir = dir.join("output");
    std::fs::create_dir_all(&output_dir).unwrap();
    let mut kept = b"first words ".to_vec();
    kept.extend(vec![b'a'; 20 * 1024]);
    kept.extend_from_slice(b" last words");
    std::fs::write(output_dir.join("archived-of-a"), kept).unwrap();
    let read = |extra: Value| {
        let state = state.clone();
        async move {
            let mut arguments = arguments(json!({ "session": "archived-of-a" }));
            arguments.extend(extra.as_object().unwrap().clone());
            call(&state, "a", "get_session", &arguments).await
        }
    };

    let short = read(json!({ "output_bytes": 16 })).await.unwrap();
    assert_eq!(
        short["recent_output"].as_str().unwrap().trim_end(),
        "aaaaa last words"
    );
    let default = read(json!({})).await.unwrap();
    assert_eq!(
        default["recent_output"].as_str().unwrap().trim_end().len(),
        8 * 1024
    );
    let long = read(json!({ "output_bytes": 64 * 1024 })).await.unwrap();
    assert!(long["recent_output"]
        .as_str()
        .unwrap()
        .contains("first words"));
    for bad in [json!(0), json!(64 * 1024 + 1), json!(-1), json!("8")] {
        let err = read(json!({ "output_bytes": bad }))
            .await
            .expect_err("out of bounds");
        assert!(err.to_string().contains("output_bytes"), "{err}");
    }
}

/// `list_sessions` narrows by console, project, owner and whether the session is the
/// caller's, every filter given having to hold; a name that matches nothing is refused.
#[tokio::test]
async fn list_sessions_filters_combine() {
    let (state, _dir) = peer_state("peer-list-filters");
    let ids = |caller: &'static str, filters: Value| {
        let state = state.clone();
        async move {
            let listed = call(&state, caller, "list_sessions", &arguments(filters))
                .await
                .unwrap();
            let mut ids: Vec<String> = listed["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|session| session["session"].as_str().unwrap().to_string())
                .collect();
            ids.sort();
            ids
        }
    };

    assert_eq!(
        ids("loose", json!({ "console": "console-2" })).await,
        ["foreign"]
    );
    assert_eq!(
        ids("loose", json!({ "project": "project-3" })).await,
        ["other-project"]
    );
    assert_eq!(ids("loose", json!({ "owner": "a" })).await, ["of-a"]);
    assert_eq!(ids("starter", json!({ "yours": true })).await, ["mine"]);
    let not_yours = ids("starter", json!({ "yours": false })).await;
    assert!(!not_yours.contains(&"mine".to_string()) && not_yours.len() > 3);
    assert_eq!(
        ids("starter", json!({ "project": "project-1", "yours": true })).await,
        ["mine"]
    );
    assert_eq!(
        ids("starter", json!({ "project": "project-3", "yours": true })).await,
        Vec::<String>::new()
    );
    assert_eq!(
        ids("a", json!({ "console": "console-1", "owner": "starter" })).await,
        ["mine"]
    );
    // A console session's own sessions, the same filter for any role.
    assert_eq!(ids("a", json!({ "yours": true })).await, ["of-a"]);

    for (filter, refusal) in [
        (
            json!({ "console": "nowhere" }),
            "there is no console `nowhere`",
        ),
        (
            json!({ "project": "nowhere" }),
            "there is no project `nowhere`",
        ),
        (
            json!({ "owner": "nowhere" }),
            "there is no session `nowhere`",
        ),
        (json!({ "yours": "yes" }), "`yours` must be true or false"),
    ] {
        let err = call(&state, "a", "list_sessions", &arguments(filter.clone()))
            .await
            .expect_err(&filter.to_string());
        assert_eq!(err.to_string(), refusal);
    }
}

/// `list_sessions` lists the sessions with a process by default, the interrupted ones with
/// `status: interrupted`, and both with `any`; an archived session is in none of them.
#[tokio::test]
async fn list_sessions_can_select_the_interrupted_sessions() {
    let (state, _dir) = peer_state("peer-list-status");
    with_status(&state, "of-a", SessionStatus::Interrupted);
    let ids = |filters: Value| {
        let state = state.clone();
        async move {
            let listed = call(&state, "loose", "list_sessions", &arguments(filters))
                .await
                .unwrap();
            let mut ids: Vec<String> = listed["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|session| session["session"].as_str().unwrap().to_string())
                .collect();
            ids.sort();
            ids
        }
    };

    let live = ids(json!({})).await;
    assert!(!live.contains(&"of-a".to_string()), "{live:?}");
    assert_eq!(ids(json!({ "status": "live" })).await, live);
    assert_eq!(ids(json!({ "status": "interrupted" })).await, ["of-a"]);
    let any = ids(json!({ "status": "any" })).await;
    let mut both = live.clone();
    both.push("of-a".to_string());
    both.sort();
    assert_eq!(any, both);
    assert!(!any.contains(&"archived-of-a".to_string()), "{any:?}");
    // It combines with the other filters, and shows the status the listing selected.
    assert_eq!(
        ids(json!({ "status": "interrupted", "owner": "b" })).await,
        Vec::<String>::new()
    );
    let listed = call(
        &state,
        "loose",
        "list_sessions",
        &arguments(json!({ "status": "interrupted" })),
    )
    .await
    .unwrap();
    assert_eq!(listed["sessions"][0]["status"], "interrupted");

    let err = call(
        &state,
        "loose",
        "list_sessions",
        &arguments(json!({ "status": "archived" })),
    )
    .await
    .expect_err("not a status");
    assert!(err.to_string().contains("`status` must be"), "{err}");
}

/// `list_archived_sessions` needs no project: with none it lists every console's archive, console
/// sessions included, and `console`, `project`, `owner` and `kind` narrow it, all having to
/// hold.
#[tokio::test]
async fn the_archive_can_be_listed_without_a_project_and_narrowed() {
    let (state, _dir) = shared_console_state("tool-surface-archive-filters");
    let mut console_archived = console_session("archived-console");
    console_archived.status = SessionStatus::Archived;
    console_archived.bound_to = Some("a".to_string());
    console_archived.orchestration_role = OrchestrationRole::Worker;
    let mut elsewhere = project_session("archived-elsewhere", "console-2", None);
    elsewhere.project_id = Some("project-2".to_string());
    elsewhere.status = SessionStatus::Archived;
    for session in [&console_archived, &elsewhere] {
        state.store.insert_session(session).unwrap();
    }
    let ids = |filters: Value| {
        let state = state.clone();
        async move {
            let listed = call(
                &state,
                "loose",
                "list_archived_sessions",
                &arguments(filters),
            )
            .await
            .unwrap();
            let mut ids: Vec<String> = listed["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|session| session["session"].as_str().unwrap().to_string())
                .collect();
            ids.sort();
            ids
        }
    };

    assert_eq!(
        ids(json!({})).await,
        [
            "archived-console",
            "archived-elsewhere",
            "archived-of-a",
            "archived-of-b"
        ]
    );
    assert_eq!(
        ids(json!({ "kind": "console" })).await,
        ["archived-console"]
    );
    assert_eq!(
        ids(json!({ "kind": "project" })).await,
        ["archived-elsewhere", "archived-of-a", "archived-of-b"]
    );
    assert_eq!(
        ids(json!({ "owner": "a" })).await,
        ["archived-console", "archived-of-a"]
    );
    assert_eq!(
        ids(json!({ "owner": "a", "kind": "project" })).await,
        ["archived-of-a"]
    );
    assert_eq!(
        ids(json!({ "console": "console-2" })).await,
        ["archived-elsewhere"]
    );
    assert_eq!(
        ids(json!({ "project": "project-1", "owner": "b" })).await,
        ["archived-of-b"]
    );

    for (filter, refusal) in [
        (
            json!({ "console": "nowhere" }),
            "there is no console `nowhere`",
        ),
        (
            json!({ "owner": "nowhere" }),
            "there is no session `nowhere`",
        ),
        (
            json!({ "kind": "hub" }),
            "`kind` must be `console` or `project`, not `hub`",
        ),
        (
            json!({ "kind": "console", "project": "project-1" }),
            "`kind: console` and `project` cannot both hold: a console session is in no \
             project",
        ),
    ] {
        let err = call(
            &state,
            "loose",
            "list_archived_sessions",
            &arguments(filter.clone()),
        )
        .await
        .expect_err(&filter.to_string());
        assert_eq!(err.to_string(), refusal);
    }
}

/// A `project` named beside a `console` is looked for in that console, not in the caller's
/// first: another console's project of the same name as the caller's own is reachable.
#[tokio::test]
async fn a_project_name_is_resolved_within_the_console_given() {
    let (state, _dir) = shared_console_state("tool-surface-archive-project-in-console");
    let template = state.store.get_project("project-2").unwrap().unwrap();
    state
        .store
        .insert_project(&Project {
            id: "twin".to_string(),
            name: "project-1".to_string(),
            path: "/tmp/twin".to_string(),
            ..template
        })
        .unwrap();
    let mut archived = project_session("archived-twin", "console-2", None);
    archived.project_id = Some("twin".to_string());
    archived.status = SessionStatus::Archived;
    state.store.insert_session(&archived).unwrap();

    for tool in ["list_archived_sessions", "list_sessions"] {
        let filters = if tool == "list_sessions" {
            json!({ "console": "console-2", "project": "project-1", "status": "any" })
        } else {
            json!({ "console": "console-2", "project": "project-1" })
        };
        let listed = call(&state, "loose", tool, &arguments(filters))
            .await
            .unwrap();
        let ids: Vec<&str> = listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|session| session["session"].as_str().unwrap())
            .collect();
        let expected: &[&str] = if tool == "list_archived_sessions" {
            &["archived-twin"]
        } else {
            &[]
        };
        assert_eq!(ids, expected, "{tool}");
    }
    // Without a console the caller's own console's `project-1` wins.
    let own = call(
        &state,
        "loose",
        "list_archived_sessions",
        &arguments(json!({ "project": "project-1" })),
    )
    .await
    .unwrap();
    assert_eq!(own["sessions"].as_array().unwrap().len(), 2);
    // Two projects of the console with one name are ambiguous there, and the refusal names it.
    let template = state.store.get_project("twin").unwrap().unwrap();
    state
        .store
        .insert_project(&Project {
            id: "twin-2".to_string(),
            path: "/tmp/twin-2".to_string(),
            ..template
        })
        .unwrap();
    let err = call(
        &state,
        "loose",
        "list_archived_sessions",
        &arguments(json!({ "console": "console-2", "project": "project-1" })),
    )
    .await
    .expect_err("two projects named project-1");
    assert!(err.to_string().contains("in console `console-2`"), "{err}");
    // A project that is in no such console is refused.
    let err = call(
        &state,
        "loose",
        "list_archived_sessions",
        &arguments(json!({ "console": "console-1", "project": "twin" })),
    )
    .await
    .expect_err("twin is console-2's");
    assert!(err.to_string().contains("no project `twin`"), "{err}");
}

/// The archive is listed newest archived first and at most `limit` long, with `total` saying
/// how many match.
#[tokio::test]
async fn the_archive_is_newest_first_and_capped_by_limit() {
    let (state, _dir) = shared_console_state("tool-surface-archive-limit");
    for (id, ended_at) in [
        ("old", Some(10)),
        ("new", Some(30)),
        ("mid", Some(20)),
        ("never", None),
    ] {
        let mut session = project_session(id, "console-1", None);
        session.status = SessionStatus::Archived;
        session.ended_at = ended_at;
        state.store.insert_session(&session).unwrap();
    }
    let list = |filters: Value| {
        let state = state.clone();
        async move {
            call(
                &state,
                "loose",
                "list_archived_sessions",
                &arguments(filters),
            )
            .await
        }
    };
    let listed = list(json!({ "kind": "project", "limit": 3 }))
        .await
        .unwrap();
    let ids: Vec<&str> = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|session| session["session"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["new", "mid", "old"]);
    assert_eq!(listed["total"], 6);

    let everything = list(json!({})).await.unwrap();
    assert_eq!(everything["sessions"].as_array().unwrap().len(), 6);
    assert_eq!(everything["total"], 6);
    for bad in [
        json!({ "limit": 0 }),
        json!({ "limit": 201 }),
        json!({ "limit": "5" }),
    ] {
        list(bad).await.expect_err("limit out of range");
    }
}

/// Both ways of reading a project give its git state as the daemon holds it: `null` before any
/// has been read, then the branch and counts with how long ago the last check completed, and a
/// credential in the error `git` reported removed.
#[tokio::test]
async fn a_projects_git_status_is_given_as_last_read() {
    let (state, _dir) = shared_console_state("tool-surface-git-status");
    let project_of_a = |listed: &Value| {
        listed["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|project| project["project"] == "project-1")
            .unwrap()["git_status"]
            .clone()
    };

    let listed = call(&state, "a", "list_projects", &Map::new())
        .await
        .unwrap();
    assert_eq!(project_of_a(&listed), Value::Null);
    let owned = call(&state, "of-a", "get_project", &Map::new())
        .await
        .unwrap();
    assert_eq!(owned["git_status"], Value::Null);

    state.publish_git_status(crate::protocol::GitStatus {
        project: "project-1".to_string(),
        repository: true,
        branch: Some("main".to_string()),
        detached: false,
        upstream: Some("origin/main".to_string()),
        ahead: 2,
        behind: 1,
        activity: crate::protocol::GitActivity::Idle,
        error: Some("unable to access 'https://user:token@example.com/r.git/'".to_string()),
    });
    let listed = call(&state, "a", "list_projects", &Map::new())
        .await
        .unwrap();
    let status = project_of_a(&listed);
    assert_eq!(status["branch"], "main");
    assert_eq!(status["upstream"], "origin/main");
    assert_eq!(
        (&status["ahead"], &status["behind"]),
        (&json!(2), &json!(1))
    );
    assert_eq!(status["activity"], "idle");
    assert_eq!(
        status["error"],
        "unable to access 'https://example.com/r.git/'"
    );
    // No remote check has completed in this daemon's run.
    assert_eq!(status["checked_seconds_ago"], Value::Null);
    let owned = call(&state, "of-a", "get_project", &Map::new())
        .await
        .unwrap();
    assert_eq!(owned["git_status"], status);

    state.record_git_check_completed("project-1");
    let owned = call(&state, "of-a", "get_project", &Map::new())
        .await
        .unwrap();
    assert_eq!(owned["git_status"]["checked_seconds_ago"], 0);

    // The age counts on the wall clock: the monotonic clock stands still while the machine
    // sleeps, so a check made an hour ago must read as an hour whatever the `Instant` says.
    state.set_git_check_wall_time_for_test("project-1", crate::time::now_millis() - 3_600_000);
    let owned = call(&state, "of-a", "get_project", &Map::new())
        .await
        .unwrap();
    assert_eq!(owned["git_status"]["checked_seconds_ago"], 3600);
}

/// A resumed session whose kept information is on its queue counts each message once, not
/// once in the store and once in the queue.
#[tokio::test]
async fn kept_information_that_is_queued_is_counted_once() {
    let (state, _dir) = shared_console_state("tool-surface-kept-count");
    let _live = running(&state, "of-a");
    let row = state.store.add_kept_message("of-a", "kept").unwrap();
    state.queue_kept_message("of-a", "kept", row);
    let described = call(
        &state,
        "a",
        "get_session",
        &arguments(json!({ "session": "of-a" })),
    )
    .await
    .unwrap();
    assert_eq!(described["queued_messages"], 1);
}

/// A running session that holds what is queued for it says so and why, in the listing and in
/// `get_session` alike, however idle its status reads; one with no process holds nothing.
#[tokio::test]
async fn a_session_holding_its_queue_says_why() {
    let (state, _dir) = shared_console_state("tool-surface-write-hold");
    let held = |listed: &Value| {
        let session = listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|session| session["session"] == "of-a")
            .unwrap()
            .clone();
        (
            session["writes_held"].clone(),
            session["queued_messages"].clone(),
        )
    };
    let fetched = || async {
        let fetched = call(
            &state,
            "a",
            "get_session",
            &arguments(json!({ "session": "of-a" })),
        )
        .await
        .unwrap();
        (
            fetched["writes_held"].clone(),
            fetched["queued_messages"].clone(),
        )
    };
    let listed = || async {
        call(&state, "a", "list_sessions", &Map::new())
            .await
            .unwrap()
    };

    assert_eq!(fetched().await, (Value::Null, json!(0)));

    // Freshly launched, before its agent's first hook.
    let live =
        crate::test_support::watched_live_session("of-a", Agent::Claude, 80, 24, "sleep 30", None);
    state.register_live((*live).clone());
    state.queue_message("of-a", "go on");
    let (hold, queued) = fetched().await;
    assert_eq!(hold["reason"], "starting");
    assert!(hold["explanation"].as_str().unwrap().contains("startup"));
    assert_eq!(queued, 1);
    assert_eq!(held(&listed().await), (hold, json!(1)));

    live.trust.end_watch();
    assert_eq!(fetched().await.0, Value::Null);

    live.mark_unsent(live.prompts_sent());
    assert_eq!(fetched().await.0["reason"], "unsent_message");
    assert_eq!(held(&listed().await).0["reason"], "unsent_message");

    // A dialog on its screen outranks the unsent message, and the session reads as waiting
    // for the user here as it does to a client, though its stored status is idle.
    live.on_output(b"Resume this conversation?\r\nEnter to confirm \xc2\xb7 Esc to resume\r\n");
    let fetched = call(
        &state,
        "a",
        "get_session",
        &arguments(json!({ "session": "of-a" })),
    )
    .await
    .unwrap();
    assert_eq!(fetched["status"], "waiting_user");
    assert_eq!(fetched["writes_held"]["reason"], "agent_dialog");
    assert_eq!(fetched["writes_held"]["dialog"], "claude_resume_at_launch");
    assert!(fetched["note"].is_string());
}

/// The reads are the same for every role: a bound project session, an unbound one and a console
/// session read any session, any project's archive in any console, and the projects of their
/// own console or another, and `yours` is worked out against whoever asks.
#[tokio::test]
async fn every_role_reads_the_same_things_with_ownership_relative_to_itself() {
    let (state, _dir) = peer_state("tool-surface-uniform-reads");
    // An archived session in a project of the other console.
    let mut elsewhere = project_session("archived-elsewhere", "console-2", None);
    elsewhere.project_id = Some("project-2".to_string());
    elsewhere.status = SessionStatus::Archived;
    state.store.insert_session(&elsewhere).unwrap();
    for caller in ["a", "starter", "mine"] {
        // A session of another owner, and a record of it.
        let theirs = call(
            &state,
            caller,
            "get_session",
            &arguments(json!({ "session": "of-b" })),
        )
        .await
        .unwrap();
        assert_eq!(theirs["yours"], false, "{caller}");
        let history = call(
            &state,
            caller,
            "read_session_history",
            &arguments(json!({ "session": "of-b" })),
        )
        .await
        .unwrap();
        assert_eq!(history["unavailable"], false, "{caller}");

        // The archive of a project of its own console and of another's.
        for project in ["project-1", "project-2"] {
            let archive = call(
                &state,
                caller,
                "list_archived_sessions",
                &arguments(json!({ "project": project })),
            )
            .await
            .unwrap();
            let ids: Vec<&str> = archive["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|session| session["session"].as_str().unwrap())
                .collect();
            let expected: &[&str] = if project == "project-1" {
                &["archived-of-a", "archived-of-b"]
            } else {
                &["archived-elsewhere"]
            };
            let mut sorted = ids.clone();
            sorted.sort();
            assert_eq!(sorted, expected, "{caller} {project}");
        }
        let err = call(
            &state,
            caller,
            "list_archived_sessions",
            &arguments(json!({ "project": "nowhere" })),
        )
        .await
        .expect_err("no such project");
        assert!(
            err.to_string().contains("there is no project"),
            "{caller}: {err}"
        );

        // Its own console in full, another's as the little that targets a project.
        let own = call(&state, caller, "list_projects", &Map::new())
            .await
            .unwrap();
        assert_eq!(own["console"], "console-1", "{caller}");
        let projects = own["projects"].as_array().unwrap();
        assert!(projects
            .iter()
            .all(|project| project["sessions"].is_array()));
        let sessions: Vec<&Value> = projects
            .iter()
            .flat_map(|project| project["sessions"].as_array().unwrap())
            .collect();
        let of_starter = sessions
            .iter()
            .find(|session| session["session"] == "mine")
            .unwrap();
        assert_eq!(of_starter["yours"], caller == "starter", "{caller}");
        let other = call(
            &state,
            caller,
            "list_projects",
            &arguments(json!({ "console": "console-2" })),
        )
        .await
        .unwrap();
        assert_eq!(other["console"], "console-2", "{caller}");
        assert_eq!(other["projects"][0]["project"], "project-2");
        assert!(other["projects"][0]["sessions"].is_null());

        let consoles = call(&state, caller, "list_consoles", &Map::new())
            .await
            .unwrap();
        let yours: Vec<(&str, bool)> = consoles["consoles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|console| {
                (
                    console["console"].as_str().unwrap(),
                    console["yours"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            yours,
            [("console-1", true), ("console-2", false)],
            "{caller}"
        );
    }
}

/// A project name that one project of the caller's console and one of another console both
/// carry is the caller's own, while an id reaches either.
#[tokio::test]
async fn a_project_name_in_the_callers_console_wins_for_the_archive() {
    let (state, dir) = shared_console_state("tool-surface-archive-precedence");
    for (id, console, archived) in [
        ("own-twin", "console-1", "archived-own"),
        ("foreign-twin", "console-2", "archived-foreign"),
    ] {
        state
            .store
            .insert_project(&Project {
                id: id.to_string(),
                console_id: console.to_string(),
                name: "twin".to_string(),
                path: dir.join(id).to_string_lossy().into_owned(),
                ..state.store.get_project("project-1").unwrap().unwrap()
            })
            .unwrap();
        let mut session = project_session(archived, console, None);
        session.project_id = Some(id.to_string());
        session.status = SessionStatus::Archived;
        state.store.insert_session(&session).unwrap();
    }
    for (wanted, expected) in [
        ("twin", "archived-own"),
        ("own-twin", "archived-own"),
        ("foreign-twin", "archived-foreign"),
    ] {
        let listed = call(
            &state,
            "loose",
            "list_archived_sessions",
            &arguments(json!({ "project": wanted })),
        )
        .await
        .unwrap();
        assert_eq!(listed["sessions"][0]["session"], expected, "{wanted}");
        assert_eq!(listed["sessions"].as_array().unwrap().len(), 1, "{wanted}");
    }
}

/// A project name two projects of another console share is refused rather than read as
/// either of them.
#[tokio::test]
async fn an_ambiguous_project_name_is_refused_for_the_archive() {
    let (state, dir) = shared_console_state("tool-surface-ambiguous-archive");
    for id in ["twin-1", "twin-2"] {
        state
            .store
            .insert_project(&Project {
                id: id.to_string(),
                name: "twin".to_string(),
                path: dir.join(id).to_string_lossy().into_owned(),
                ..state.store.get_project("project-2").unwrap().unwrap()
            })
            .unwrap();
    }
    let err = call(
        &state,
        "loose",
        "list_archived_sessions",
        &arguments(json!({ "project": "twin" })),
    )
    .await
    .expect_err("ambiguous");
    assert!(err.to_string().contains("names 2 projects"), "{err}");
    call(
        &state,
        "loose",
        "list_archived_sessions",
        &arguments(json!({ "project": "twin-1" })),
    )
    .await
    .unwrap();
}

/// `list_projects` with `console` shows another console's projects as the little an agent needs
/// to target one, while its own console, named or not, is listed whole. A console named
/// ambiguously or not at all is refused.
#[tokio::test]
async fn another_consoles_projects_are_listed_by_id_name_description_and_tags() {
    let (state, _dir) = shared_console_state("tool-surface-other-console-projects");
    let mut project = state.store.get_project("project-2").unwrap().unwrap();
    project.description = Some("Billing".to_string());
    project.tags = vec!["rust".to_string()];
    state.store.update_project(&project).unwrap();
    state
        .store
        .update_console(&Console {
            name: "Other".to_string(),
            ..state.store.get_console("console-2").unwrap().unwrap()
        })
        .unwrap();

    for wanted in ["console-2", "Other"] {
        let listed = call(
            &state,
            "a",
            "list_projects",
            &arguments(json!({ "console": wanted })),
        )
        .await
        .unwrap();
        assert_eq!(listed["console"], "console-2", "{wanted}");
        assert_eq!(
            listed["projects"],
            json!([{
                "project": "project-2",
                "name": "project-2",
                "description": "Billing",
                "tags": ["rust"],
            }]),
            "{wanted}"
        );
    }
    for wanted in ["console-1", "Console"] {
        let own = call(
            &state,
            "a",
            "list_projects",
            &arguments(json!({ "console": wanted })),
        )
        .await
        .unwrap();
        assert_eq!(own["console"], "console-1", "{wanted}");
        assert_eq!(own["projects"][0]["project"], "project-1", "{wanted}");
        assert!(own["projects"][0]["sessions"].is_array(), "{wanted}");
    }
    let plain = call(&state, "a", "list_projects", &Map::new())
        .await
        .unwrap();
    assert_eq!(plain["projects"].as_array().unwrap().len(), 1);

    state
        .store
        .update_console(&Console {
            name: "Console".to_string(),
            ..state.store.get_console("console-2").unwrap().unwrap()
        })
        .unwrap();
    for (wanted, reason) in [
        ("Console", "names 2 consoles"),
        ("nowhere", "there is no console `nowhere`"),
    ] {
        let err = call(
            &state,
            "a",
            "list_projects",
            &arguments(json!({ "console": wanted })),
        )
        .await
        .expect_err("refused");
        assert!(err.to_string().contains(reason), "{wanted}: {err}");
    }
}

/// The archive of a project is listed whole, owners marked, rather than filtered to the caller.
#[tokio::test]
async fn the_archive_lists_every_owners_sessions() {
    let (state, _dir) = shared_console_state("tool-surface-archive");
    let listed = call(
        &state,
        "a",
        "list_archived_sessions",
        &arguments(json!({ "project": "project-1" })),
    )
    .await
    .unwrap();
    let mut owners: Vec<(&str, &str)> = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|session| {
            (
                session["session"].as_str().unwrap(),
                session["owner"].as_str().unwrap(),
            )
        })
        .collect();
    owners.sort();
    assert_eq!(owners, [("archived-of-a", "a"), ("archived-of-b", "b")]);
}

/// Reads reach any session, of any project or console, console sessions included, and say who
/// owns what, including a project session as owner.
#[tokio::test]
async fn a_project_session_reads_any_session() {
    let (state, _dir) = peer_state("peer-reads");
    let read = |session: &'static str| {
        let state = state.clone();
        async move {
            call(
                &state,
                "starter",
                "get_session",
                &arguments(json!({ "session": session })),
            )
            .await
            .unwrap()
        }
    };
    let mine = read("mine").await;
    assert_eq!(mine["owner"], "starter");
    assert_eq!(mine["owner_kind"], "project");
    assert_eq!(mine["yours"], true);
    let theirs = read("of-a").await;
    assert_eq!(theirs["owner_kind"], "console");
    assert_eq!(theirs["yours"], false);
    assert!(read("loose").await["owner_kind"].is_null());
    for (outside, console) in [
        ("other-project", "console-1"),
        ("a", "console-1"),
        ("foreign", "console-2"),
    ] {
        let read = read(outside).await;
        assert_eq!(read["console"], console, "{outside}");
        assert_eq!(read["yours"], false, "{outside}");
    }
}

/// Every session sees every other running session, of every console and project, console
/// sessions included, with their owners, which one owns it and which are its own; archived
/// sessions are left out.
#[tokio::test]
async fn every_session_lists_the_running_sessions_everywhere() {
    let (state, _dir) = peer_state("peer-list");
    let listed = call(&state, "mine", "list_sessions", &Map::new())
        .await
        .unwrap();
    assert_eq!(listed["your_owner"], "starter");
    let mut ids: Vec<&str> = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|session| session["session"].as_str().unwrap())
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        [
            "a",
            "b",
            "foreign",
            "loose",
            "of-a",
            "of-b",
            "other-project",
            "starter"
        ]
    );
    for session in listed["sessions"].as_array().unwrap() {
        assert_eq!(
            session["your_owner"],
            session["session"] == "starter",
            "{session}"
        );
    }

    let from_console = call(&state, "a", "list_sessions", &Map::new())
        .await
        .unwrap();
    assert!(from_console["your_owner"].is_null());
    let of_a = from_console["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["session"] == "of-a")
        .unwrap();
    assert_eq!(
        (&of_a["owner_kind"], &of_a["yours"]),
        (&json!("console"), &json!(true))
    );
}

/// A session reads itself by leaving `session` out, which is how it learns its own id.
#[tokio::test]
async fn get_session_without_a_session_reads_the_caller() {
    let fixture = RepoState::new("get-session-self");
    for caller in ["a", "lead", "lead-kid"] {
        let record = call(&fixture.state, caller, "get_session", &Map::new())
            .await
            .unwrap();
        assert_eq!(record["session"], caller);
    }
    // Naming another session is unchanged.
    let record = call(
        &fixture.state,
        "lead",
        "get_session",
        &arguments(json!({ "session": "kid" })),
    )
    .await
    .unwrap();
    assert_eq!(record["session"], "kid");
}

/// A record carries what an owner needs to tell sessions apart and to decide on a transfer.
#[tokio::test]
async fn a_session_record_carries_its_identity_launch_choices_and_owned_count() {
    let fixture = RepoState::new("record-identity");
    let state = &fixture.state;
    state
        .store
        .insert_session(&Session {
            project_id: Some("repo".to_string()),
            agent_session_id: Some("0a1b2c3d-0000-4000-8000-000000000001".to_string()),
            has_conversation: true,
            model: Some(Some("opus".to_string())),
            effort: Some(None),
            orchestration_role: OrchestrationRole::Coordinator,
            ..project_session("lead-two", "console-1", Some("a"))
        })
        .unwrap();
    // One live and one archived session under it: both count.
    for (id, status) in [
        ("under-live", SessionStatus::Idle),
        ("under-archived", SessionStatus::Archived),
    ] {
        let mut under = project_session(id, "console-1", Some("lead-two"));
        under.status = status;
        state.store.insert_session(&under).unwrap();
    }
    let record = call(
        state,
        "a",
        "get_session",
        &arguments(json!({ "session": "lead-two" })),
    )
    .await
    .unwrap();
    assert_eq!(record["owned_sessions"], 2);
    assert_eq!(
        record["agent_session_id"],
        "0a1b2c3d-0000-4000-8000-000000000001"
    );
    assert_eq!(record["origin"], "user");
    assert_eq!(record["has_conversation"], true);
    assert_eq!(record["model"], "opus");
    // An explicit choice of the agent's own default is a `null`, not an absent key.
    assert!(record.get("effort").is_some_and(Value::is_null));
    assert_eq!(record["orchestration_role"], "coordinator");
    assert!(record.get("lead").is_none());

    // A session that inherits its launch settings has neither key, and owns nothing.
    let plain = call(
        state,
        "a",
        "get_session",
        &arguments(json!({ "session": "kid" })),
    )
    .await
    .unwrap();
    assert!(plain.get("model").is_none() && plain.get("effort").is_none());
    assert_eq!(plain["owned_sessions"], 0);
    assert_eq!(plain["orchestration_role"], "worker");
    assert_eq!(plain["agent_session_id"], Value::Null);
    assert_eq!(plain["display_title"], plain["title"]);
}

/// `list_archived_sessions` pages with `before`, and `yours` narrows it as it does `list_sessions`.
#[tokio::test]
async fn the_archive_is_paged_with_before_and_narrowed_to_yours() {
    let (state, _dir) = shared_console_state("archive-before");
    for (id, ended) in [("old", 10), ("mid", 20), ("new", 30), ("tie", 30)] {
        let mut archived = project_session(id, "console-1", Some("a"));
        archived.status = SessionStatus::Archived;
        archived.ended_at = Some(ended);
        state.store.insert_session(&archived).unwrap();
    }
    let list = |filters: Value| {
        let state = state.clone();
        async move {
            call(&state, "a", "list_archived_sessions", &arguments(filters))
                .await
                .unwrap()
        }
    };
    let ids = |listed: &Value| -> Vec<String> {
        listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|session| session["session"].as_str().unwrap().to_string())
            .collect()
    };
    // Newest first, the id breaking the tie; `archived-of-a` has no end time and comes last.
    let first = list(json!({ "owner": "a", "limit": 2 })).await;
    assert_eq!(ids(&first), ["new", "tie"]);
    assert_eq!(
        (first["total"].clone(), first["older"].clone()),
        (json!(5), json!(3))
    );
    // The cursor is a position, so the session it came from may leave the archive between
    // pages without moving the next one.
    let mut reopened = state.store.get_session("tie").unwrap().unwrap();
    reopened.status = SessionStatus::Idle;
    state.store.update_session(&reopened).unwrap();
    let second = list(json!({ "owner": "a", "limit": 2, "before": first["next"].clone() })).await;
    assert_eq!(ids(&second), ["mid", "old"]);
    assert_eq!(second["older"], 1);
    let third = list(json!({ "owner": "a", "limit": 2, "before": second["next"].clone() })).await;
    assert_eq!(ids(&third), ["archived-of-a"]);
    assert_eq!(third["older"], 0);
    assert_eq!(third["next"], Value::Null);

    let yours = ids(&list(json!({ "yours": true })).await);
    assert!(yours.contains(&"mid".to_string()) && !yours.contains(&"archived-of-b".to_string()));
    let others = ids(&list(json!({ "yours": false })).await);
    assert!(others.contains(&"archived-of-b".to_string()) && !others.contains(&"mid".to_string()));

    let err = call(
        &state,
        "a",
        "list_archived_sessions",
        &arguments(json!({ "before": "tie" })),
    )
    .await
    .expect_err("a session id is not a cursor");
    assert!(err.to_string().contains("is not a cursor"), "{err}");
}

/// A console session reads one project with `project`; `refresh` checks the git status first
/// and says it finished, and is refused for another console, whose entries have no status.
#[tokio::test]
async fn list_projects_names_one_project_and_refreshes_its_git_status() {
    let fixture = RepoState::new("list-projects-refresh");
    let state = &fixture.state;
    let listed = call(
        state,
        "a",
        "list_projects",
        &arguments(json!({ "project": "repo" })),
    )
    .await
    .unwrap();
    let projects = listed["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["project"], "repo");
    assert!(listed.get("git_refresh").is_none());

    let refreshed = call(
        state,
        "a",
        "list_projects",
        &arguments(json!({ "project": "repo", "refresh": true })),
    )
    .await
    .unwrap();
    assert_eq!(refreshed["git_refresh"], json!({ "finished": true }));
    let status = &refreshed["projects"][0]["git_status"];
    assert_eq!(status["repository"], true);
    assert!(status["checked_seconds_ago"].is_u64(), "{status}");

    // A project of another console is not among this one's.
    let err = call(
        state,
        "a",
        "list_projects",
        &arguments(json!({ "project": "project-2" })),
    )
    .await
    .expect_err("another console's project");
    assert!(err.to_string().contains("has no project"), "{err}");
    // Named beside its own console it is found, without the settings.
    let other = call(
        state,
        "a",
        "list_projects",
        &arguments(json!({ "console": "console-2", "project": "project-2" })),
    )
    .await
    .unwrap();
    assert_eq!(other["projects"][0]["project"], "project-2");
    let err = call(
        state,
        "a",
        "list_projects",
        &arguments(json!({ "console": "console-2", "refresh": true })),
    )
    .await
    .expect_err("no status to refresh");
    assert!(err.to_string().contains("`refresh`"), "{err}");
}

/// A session record says whether a session with a parent is idle after its last report reached
/// that parent, which a turn opening ends; a session with no parent never is.
#[tokio::test]
async fn a_session_record_says_whether_it_is_idle_after_its_report() {
    let fixture = RepoState::new("reads-idle-after-report");
    let state = &fixture.state;
    let _live = running(state, "lead-kid");
    let get = |id: &'static str| async move {
        call(
            state,
            "lead",
            "get_session",
            &arguments(json!({ "session": id })),
        )
        .await
        .unwrap()["idle_after_report"]
            .clone()
    };
    with_status(state, "lead-kid", SessionStatus::Idle);
    assert_eq!(get("lead-kid").await, false);

    state
        .note_report_accepted("lead-kid", false, state.turn_opened("lead-kid"))
        .unwrap();
    assert_eq!(get("lead-kid").await, true);
    state.turn_started("lead-kid", Some("next"));
    assert_eq!(get("lead-kid").await, false);

    state
        .note_report_accepted("lead", false, state.turn_opened("lead"))
        .unwrap();
    assert_eq!(get("lead").await, false, "lead has no parent");
}
