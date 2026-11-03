use super::*;

/// `owner`, an unbound project session, with `worker` bound to it; `worker` made `branch` in a
/// worktree of its own with uncommitted changes.
fn agent_fixture(label: &str) -> (Fixture, PathBuf) {
    let fixture = Fixture::new(label);
    fixture.store("owner", SessionStatus::Idle);
    let worker = Session {
        bound_to: Some("owner".to_string()),
        role: Role::Worker,
        ..session("worker", SessionStatus::Idle)
    };
    fixture.state.store.insert_session(&worker).unwrap();
    let root = fixture.worktree("work", "worker");
    std::fs::write(root.join("new.txt"), "untracked\n").unwrap();
    (fixture, root)
}

fn arguments(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[tokio::test]
async fn an_agents_archive_never_forces_and_names_what_it_left() {
    let (fixture, root) = agent_fixture("cleanup-agent-archive");
    let answer = crate::mcp::dispatch::call(
        &fixture.state,
        "owner",
        "archive_session",
        &arguments(json!({ "session": "worker" })),
    )
    .await
    .unwrap();
    assert!(root.exists() && fixture.has_branch("work"));
    let left = answer["branches_left"].as_array().unwrap();
    assert_eq!(left.len(), 2, "{answer}");
    assert_eq!(left[0]["reason"], "it has uncommitted changes");
    assert!(answer["note"]
        .as_str()
        .unwrap()
        .contains("git worktree remove"));
}

#[tokio::test]
async fn archiving_oneself_names_what_will_stay() {
    let (fixture, root) = agent_fixture("cleanup-agent-self");
    let owner_root = fixture.worktree("own", "owner");
    std::fs::write(owner_root.join("new.txt"), "untracked\n").unwrap();
    crate::coordinator::lifecycle::archive_session(&fixture.state, "worker").unwrap();
    let answer = crate::mcp::dispatch::call(&fixture.state, "owner", "archive_self", &Map::new())
        .await
        .unwrap();
    let left = answer["branches_left"].as_array().unwrap();
    assert!(left.iter().any(|item| item["branch"] == "own"), "{answer}");
    assert!(owner_root.exists() && root.exists());
}

#[tokio::test]
async fn a_done_report_names_what_will_stay() {
    let (fixture, root) = agent_fixture("cleanup-agent-report");
    let _owner = running(&fixture.state, "owner");
    let answer = crate::mcp::dispatch::call(
        &fixture.state,
        "worker",
        "report",
        &arguments(json!({ "summary": "Done.", "status": "done" })),
    )
    .await
    .unwrap();
    let note = answer["note"].as_str().unwrap();
    assert!(
        note.contains("will not be removed") && note.contains("uncommitted changes"),
        "{note}"
    );
    assert!(root.exists());
}
