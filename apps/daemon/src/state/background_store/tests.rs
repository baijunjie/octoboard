use std::sync::Arc;

use super::*;
use crate::hooks::BackgroundTasksChange;
use crate::protocol::{AgentId, SessionStatus};
use crate::state::background::EMPTY_TASKS_DELAY;
use crate::state::test_support::{insert_record, subagent};
use crate::test_support::{app_state, fake_live_session, runtime_settled, tasks_idle, StandIn};

fn task(id: &str) -> BackgroundTask {
    BackgroundTask {
        id: id.to_string(),
        kind: Some("shell".to_string()),
        description: None,
    }
}

fn described(id: &str, description: &str) -> BackgroundTask {
    BackgroundTask {
        description: Some(description.to_string()),
        ..subagent(id)
    }
}

fn live(state: &Arc<AppState>, id: &str) -> StandIn {
    let live = fake_live_session(id, AgentId::CLAUDE, 80, 24, "sleep 30");
    state.register_live(live.clone());
    live
}

/// The session's process ending, and what the daemon then drops for it, including the saved work
/// that goes off the runtime.
async fn end_process(state: &Arc<AppState>, id: &str) {
    let live = state.live_session(id).expect("the session is live");
    assert!(state.remove_live(&live));
    state.forget_session_bookkeeping(id);
    runtime_settled().await;
}

fn file_of(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// The ids of the tasks saved for `session`, `None` without an entry.
fn saved_ids(path: &Path, session: &str) -> Option<Vec<String>> {
    let file = file_of(path)?;
    let entry = file["sessions"].get(session)?;
    Some(
        entry["tasks"]
            .as_array()?
            .iter()
            .map(|task| task["id"].as_str().unwrap().to_string())
            .collect(),
    )
}

/// What a session adopted running `launch` is given back by the file at `path`, as a next daemon
/// that loads it.
fn restored_by_next(path: &Path, session: &str, launch: &str) -> Vec<BackgroundTask> {
    let (next, _dir) = app_state("background-store-next");
    next.load_saved_background(path);
    next.restore_background_work(session, launch);
    next.background_tasks_of(session)
}

/// A file that is missing, unreadable, of another version or of none gives nothing back and does
/// not stop the start; the next change of a list writes a file of this build's format over it.
#[tokio::test]
async fn a_file_that_cannot_be_used_gives_nothing_back_and_is_replaced_by_the_next_change() {
    let entry = r#""s":{"launch":"l1","tasks":[{"id":"a","type":"shell"}]}"#;
    let cases = [
        ("missing", None),
        ("empty", Some(String::new())),
        ("not json", Some("{ not json".to_string())),
        ("not an object", Some("[1, 2]".to_string())),
        (
            "another version",
            Some(format!(r#"{{"version":2,"sessions":{{{entry}}}}}"#)),
        ),
        ("no version", Some(format!(r#"{{"sessions":{{{entry}}}}}"#))),
        (
            "an entry of the wrong shape",
            Some(r#"{"version":1,"sessions":{"s":{"launch":7}}}"#.to_string()),
        ),
    ];
    for (name, content) in cases {
        let (state, dir) = app_state("background-store-unusable");
        let path = dir.join("background-tasks.json");
        if let Some(content) = &content {
            std::fs::write(&path, content).unwrap();
        }
        state.load_saved_background(&path);

        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let launch = live(&state, "s").launch_id.clone();
        state.restore_background_work("s", "l1");
        assert!(state.background_tasks_of("s").is_empty(), "{name}");

        state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("b")]));
        let file = file_of(&path).unwrap_or_else(|| panic!("{name}: the file is not rewritten"));
        assert_eq!(file["version"], 1, "{name}");
        assert_eq!(file["sessions"]["s"]["launch"], launch.as_str(), "{name}");
        assert_eq!(saved_ids(&path, "s"), Some(vec!["b".to_string()]), "{name}");
    }
}

/// What the file holds is the work of each session by its launch, and nothing else is left beside
/// it in the directory once it is written.
#[tokio::test]
async fn the_file_is_written_whole_in_the_versioned_format_with_nothing_left_beside_it() {
    let (state, dir) = app_state("background-store-format");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    live(&state, "s");
    let path = dir.join("background-tasks.json");
    state.load_saved_background(&path);

    state.apply_background_tasks_change(
        "s",
        BackgroundTasksChange::Replace(vec![described("a", "probe"), task("b")]),
    );
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("b")]));

    let file = file_of(&path).expect("the file reads");
    assert_eq!(file["version"], 1);
    assert_eq!(file["sessions"].as_object().unwrap().len(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "the file is open to others: {mode:o}");
    }
    let beside: Vec<_> = std::fs::read_dir(&*dir)
        .unwrap()
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().to_string_lossy().into_owned();
            name.contains("background-tasks").then_some(name)
        })
        .collect();
    assert_eq!(beside, ["background-tasks.json"]);
}

/// A subagent's description is kept with the work, so that a session adopted after a restart
/// shows the same task as before it.
#[tokio::test]
async fn a_subagents_description_survives_the_restart() {
    let (state, dir) = app_state("background-store-description");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let launch = live(&state, "s").launch_id.clone();
    let path = dir.join("background-tasks.json");
    state.load_saved_background(&path);

    let tasks = vec![described("a", "slow sleeper"), task("b")];
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(tasks.clone()));

    assert_eq!(restored_by_next(&path, "s", &launch), tasks);
}

/// Each session has its own entry: the list of one becoming empty, or one being forgotten, leaves
/// the other's as it was.
#[tokio::test]
async fn one_sessions_entry_going_leaves_the_others() {
    let (state, dir) = app_state("background-store-sessions");
    for id in ["s", "t"] {
        insert_record(&state, &dir, id, SessionStatus::Idle);
        live(&state, id);
    }
    let path = dir.join("background-tasks.json");
    state.load_saved_background(&path);
    for id in ["s", "t"] {
        state.apply_background_tasks_change(
            id,
            BackgroundTasksChange::Replace(vec![task("a"), task("b")]),
        );
    }

    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(Vec::new()));
    assert_eq!(saved_ids(&path, "s"), None);
    assert_eq!(saved_ids(&path, "t").map(|ids| ids.len()), Some(2));

    end_process(&state, "t").await;
    assert!(!path.exists());
}

/// A subagent's stop that leaves nothing is saved as empty from the moment it is waiting, so a
/// restart within [`EMPTY_TASKS_DELAY`] does not give back a subagent that has stopped; the list
/// itself empties once the delay has stood.
#[tokio::test(start_paused = true)]
async fn the_file_holds_the_delayed_emptying_from_the_stop_on() {
    let (state, dir) = app_state("background-store-delayed");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let launch = live(&state, "s").launch_id.clone();
    let path = dir.join("background-tasks.json");
    state.load_saved_background(&path);
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![subagent("a")]));
    assert_eq!(saved_ids(&path, "s"), Some(vec!["a".to_string()]));

    state.apply_background_tasks_change(
        "s",
        BackgroundTasksChange::SubagentStopped {
            id: "a".to_string(),
            tasks: vec![subagent("a")],
        },
    );
    // Still listed and not yet emptied, but already not saved.
    assert_eq!(state.background_tasks_of("s").len(), 1);
    assert!(!path.exists(), "the stopped subagent is still in the file");
    assert!(restored_by_next(&path, "s", &launch).is_empty());

    // What takes the emptying back saves the list again.
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![subagent("a")]));
    assert_eq!(saved_ids(&path, "s"), Some(vec!["a".to_string()]));
    state.apply_background_tasks_change(
        "s",
        BackgroundTasksChange::SubagentStopped {
            id: "a".to_string(),
            tasks: vec![subagent("a")],
        },
    );
    assert!(!path.exists());

    tokio::time::sleep(EMPTY_TASKS_DELAY).await;
    tasks_idle().await;
    assert!(
        state.background_tasks_of("s").is_empty(),
        "the list is not emptied"
    );
    assert!(!path.exists(), "the emptied list is still in the file");
}

/// The emptying that waits keeps a subagent's description saved with no tasks, so the file does
/// not give the stopped subagent back while it still remembers what it was.
#[tokio::test(start_paused = true)]
async fn a_waiting_emptying_saves_the_descriptions_with_no_tasks() {
    let (state, dir) = app_state("background-store-delayed-described");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let launch = live(&state, "s").launch_id.clone();
    let path = dir.join("background-tasks.json");
    state.load_saved_background(&path);
    state.apply_background_tasks_change(
        "s",
        BackgroundTasksChange::Replace(vec![described("a", "probe")]),
    );

    state.apply_background_tasks_change(
        "s",
        BackgroundTasksChange::SubagentStopped {
            id: "a".to_string(),
            tasks: vec![described("a", "probe")],
        },
    );

    assert_eq!(saved_ids(&path, "s"), Some(Vec::new()));
    assert!(restored_by_next(&path, "s", &launch).is_empty());
}

/// A list that has emptied is not brought back by a restart, even when a description of one of
/// its subagents is still remembered for the next start.
#[tokio::test]
async fn an_emptied_list_is_not_restored_even_with_descriptions_remembered() {
    let (state, dir) = app_state("background-store-emptied");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let launch = live(&state, "s").launch_id.clone();
    let path = dir.join("background-tasks.json");
    state.load_saved_background(&path);
    state.apply_background_tasks_change(
        "s",
        BackgroundTasksChange::Replace(vec![described("a", "probe")]),
    );
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(Vec::new()));

    assert_eq!(
        saved_ids(&path, "s").unwrap_or_default(),
        Vec::<String>::new()
    );
    assert!(restored_by_next(&path, "s", &launch).is_empty());
}

/// A session with no process has no launch to key the work by, so nothing is written for it.
#[tokio::test]
async fn work_of_a_session_with_no_process_is_not_written() {
    let (state, dir) = app_state("background-store-no-process");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let path = dir.join("background-tasks.json");
    state.load_saved_background(&path);

    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("a")]));

    assert_eq!(state.background_tasks_of("s").len(), 1);
    assert!(!path.exists());
}

/// Nothing is written before the start has named the file.
#[tokio::test]
async fn nothing_is_written_before_the_file_is_loaded() {
    let (state, dir) = app_state("background-store-unnamed");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    live(&state, "s");

    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("a")]));

    assert!(std::fs::read_dir(&*dir).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("background")));
}

/// A session with nothing saved for it is given nothing, and leaves the other entries in place.
#[tokio::test]
async fn restoring_a_session_with_no_entry_changes_nothing() {
    let (state, dir) = app_state("background-store-no-entry");
    let path = dir.join("background-tasks.json");
    std::fs::write(
        &path,
        r#"{"version":1,"sessions":{"s":{"launch":"l","tasks":[{"id":"a"}]}}}"#,
    )
    .unwrap();
    state.load_saved_background(&path);

    state.restore_background_work("other", "l");

    assert!(state.background_tasks_of("other").is_empty());
    assert_eq!(saved_ids(&path, "s"), Some(vec!["a".to_string()]));
    state.restore_background_work("s", "l");
    assert_eq!(state.background_tasks_of("s").len(), 1);
}

/// A claim released with a process adopted for the session keeps its saved work, which the file
/// goes on following; released with none, the work has nothing to be given back to and goes.
#[tokio::test]
async fn a_claim_released_without_a_process_drops_the_saved_work() {
    let (state, dir) = app_state("background-store-claims");
    for id in ["adopted", "gone"] {
        insert_record(&state, &dir, id, SessionStatus::Idle);
    }
    let path = dir.join("background-tasks.json");
    std::fs::write(
        &path,
        r#"{"version":1,"sessions":{
            "adopted":{"launch":"l","tasks":[{"id":"a"}]},
            "gone":{"launch":"l","tasks":[{"id":"b"}]}}}"#,
    )
    .unwrap();
    state.load_saved_background(&path);
    state.claim_unlinked("adopted", Some(("holder", "l")));
    state.claim_unlinked("gone", Some(("holder", "l")));
    live(&state, "adopted");

    state.release_unlinked_claim("adopted");
    assert_eq!(saved_ids(&path, "adopted"), Some(vec!["a".to_string()]));
    assert_eq!(saved_ids(&path, "gone"), Some(vec!["b".to_string()]));

    state.release_unlinked_claim("gone");
    assert_eq!(saved_ids(&path, "gone"), None);
    assert_eq!(saved_ids(&path, "adopted"), Some(vec!["a".to_string()]));
}

/// What is written is the work of the process that is current when the write is made: a change
/// after a relaunch saves the new launch, and one after the process left leaves the entry as it
/// is, which is what keeps the work of a session claimed and not adopted yet.
#[tokio::test]
async fn a_change_is_saved_for_the_launch_that_is_current_and_none_when_there_is_none() {
    let (state, dir) = app_state("background-store-current");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let first = live(&state, "s");
    let path = dir.join("background-tasks.json");
    state.load_saved_background(&path);
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("a")]));
    let file = file_of(&path).unwrap();
    assert_eq!(file["sessions"]["s"]["launch"], first.launch_id.as_str());

    // The process leaves; a late change finds none and does not touch the entry.
    assert!(state.remove_live(&first));
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("b")]));
    assert_eq!(saved_ids(&path, "s"), Some(vec!["a".to_string()]));

    let second = live(&state, "s");
    assert_ne!(first.launch_id, second.launch_id);
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("c")]));
    let file = file_of(&path).unwrap();
    assert_eq!(file["sessions"]["s"]["launch"], second.launch_id.as_str());
    assert_eq!(saved_ids(&path, "s"), Some(vec!["c".to_string()]));
}

/// The removal that goes off the runtime cannot be undone by a change persisted after the process
/// left, and does not take away the entry of a process that is live again by then.
#[tokio::test]
async fn the_removal_with_the_session_is_not_undone_nor_does_it_take_a_relaunchs_entry() {
    let (state, dir) = app_state("background-store-removal");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    live(&state, "s");
    let path = dir.join("background-tasks.json");
    state.load_saved_background(&path);
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("a")]));

    end_process(&state, "s").await;
    assert!(!path.exists());
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("b")]));
    assert!(!path.exists(), "a late change brought the entry back");

    // A relaunch saved its own work before the removal of the old one ran.
    let live = live(&state, "s");
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("c")]));
    state.forget_saved_background("s");
    runtime_settled().await;
    let file = file_of(&path).expect("the relaunch's entry is gone");
    assert_eq!(file["sessions"]["s"]["launch"], live.launch_id.as_str());
}

/// Work restored for a session takes the reports that follow on top, and a session whose list was
/// reported before the adoption is not overwritten by the older saved copy.
#[tokio::test]
async fn restored_work_takes_later_reports_and_is_not_overwritten_by_the_saved_copy() {
    let (state, dir) = app_state("background-store-overtaken");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let s = live(&state, "s");
    let path = dir.join("background-tasks.json");
    state.load_saved_background(&path);
    state.apply_background_tasks_change(
        "s",
        BackgroundTasksChange::Replace(vec![task("a"), task("b")]),
    );

    // The next daemon.
    let (next, next_dir) = app_state("background-store-overtaken-next");
    insert_record(&next, &next_dir, "s", SessionStatus::Idle);
    next.load_saved_background(&path);
    next.restore_background_work("s", &s.launch_id);
    assert_eq!(next.background_tasks_of("s"), vec![task("a"), task("b")]);

    // A notification that was held meanwhile is applied on top of what was restored.
    next.register_live(s.clone());
    next.apply_background_tasks_change("s", BackgroundTasksChange::Finished(vec!["a".to_string()]));
    assert_eq!(next.background_tasks_of("s"), vec![task("b")]);
    // As does a turn's end, which lists what is left.
    next.apply_background_tasks_change("s", BackgroundTasksChange::Replace(Vec::new()));
    assert!(next.background_tasks_of("s").is_empty());
    assert!(!path.exists());

    // Work that is there already is not replaced by a saved copy, whatever it holds.
    let (late, late_dir) = app_state("background-store-overtaken-late");
    insert_record(&late, &late_dir, "s", SessionStatus::Idle);
    std::fs::write(
        &path,
        format!(
            r#"{{"version":1,"sessions":{{"s":{{"launch":"{}","tasks":[{{"id":"old"}}]}}}}}}"#,
            s.launch_id
        ),
    )
    .unwrap();
    late.load_saved_background(&path);
    late.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("new")]));
    late.restore_background_work("s", &s.launch_id);
    assert_eq!(late.background_tasks_of("s"), vec![task("new")]);
}

/// What no process was adopted or claimed for is dropped, and the rest kept.
#[test]
fn saved_work_of_sessions_not_taken_over_is_dropped() {
    let (state, dir) = app_state("background-store-retained");
    let path = dir.join("background-tasks.json");
    std::fs::write(
        &path,
        r#"{"version":1,"sessions":{
            "kept":{"launch":"l","tasks":[{"id":"a"}]},
            "gone":{"launch":"l","tasks":[{"id":"b"}]}}}"#,
    )
    .unwrap();
    state.load_saved_background(&path);
    state.retain_saved_background(|id| id == "kept");
    assert_eq!(saved_ids(&path, "kept"), Some(vec!["a".to_string()]));
    assert_eq!(saved_ids(&path, "gone"), None);
}

/// A daemon that has loaded `tasks` saved for session `s` by `launch` and has not adopted it yet;
/// the file is in the returned directory.
fn before_adoption(
    name: &str,
    launch: &str,
    tasks: &[&str],
) -> (Arc<AppState>, crate::test_support::ScratchDir) {
    let (state, dir) = app_state(name);
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let path = dir.join("background-tasks.json");
    let tasks: Vec<_> = tasks
        .iter()
        .map(|id| format!(r#"{{"id":"{id}","type":"shell"}}"#))
        .collect();
    std::fs::write(
        &path,
        format!(
            r#"{{"version":1,"sessions":{{"s":{{"launch":"{launch}","tasks":[{}]}}}}}}"#,
            tasks.join(",")
        ),
    )
    .unwrap();
    state.load_saved_background(&path);
    (state, dir)
}

/// A report that empties the list before the adoption is not undone by the saved copy, and the
/// file follows it.
#[tokio::test]
async fn an_emptying_report_before_the_adoption_is_not_undone_by_the_saved_copy() {
    let (state, dir) = before_adoption("background-store-early-empty", "l", &["a", "b"]);
    let path = dir.join("background-tasks.json");
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(Vec::new()));
    assert!(state.background_tasks_of("s").is_empty());

    state.restore_background_work("s", "l");
    assert!(state.background_tasks_of("s").is_empty());
    assert_eq!(saved_ids(&path, "s"), None);
}

/// A completion before the adoption removes just that task from the saved ones.
#[tokio::test]
async fn a_completion_before_the_adoption_removes_only_its_task() {
    let (state, dir) = before_adoption("background-store-early-finished", "l", &["a", "b"]);
    let path = dir.join("background-tasks.json");
    state
        .apply_background_tasks_change("s", BackgroundTasksChange::Finished(vec!["a".to_string()]));
    state.restore_background_work("s", "l");
    assert_eq!(state.background_tasks_of("s"), vec![task("b")]);
    assert_eq!(saved_ids(&path, "s"), Some(vec!["b".to_string()]));
}

/// A subagent's start before the adoption joins the saved tasks instead of replacing them.
#[tokio::test]
async fn a_start_before_the_adoption_keeps_the_saved_tasks() {
    let (state, dir) = before_adoption("background-store-early-start", "l", &["a"]);
    let path = dir.join("background-tasks.json");
    state.apply_background_tasks_change("s", BackgroundTasksChange::SubagentStarted("x".into()));
    state.restore_background_work("s", "l");
    let ids: Vec<_> = state
        .background_tasks_of("s")
        .into_iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(ids, ["a", "x"]);
    assert_eq!(
        saved_ids(&path, "s"),
        Some(vec!["a".to_string(), "x".to_string()])
    );
}

/// Work loaded from another launch is cleared when the adoption finds a different process, and
/// the file loses it too.
#[tokio::test]
async fn work_of_another_launch_is_cleared_by_the_adoption() {
    let (state, dir) = before_adoption("background-store-early-mismatch", "old", &["a"]);
    let path = dir.join("background-tasks.json");
    state
        .apply_background_tasks_change("s", BackgroundTasksChange::Finished(vec!["z".to_string()]));

    state.restore_background_work("s", "new");
    assert!(state.background_tasks_of("s").is_empty());
    assert_eq!(saved_ids(&path, "s"), None);
}

/// What no process was adopted or claimed for loses its work in memory with its file entry, and
/// the work of a session claimed and not adopted yet is not shown as running.
#[tokio::test]
async fn work_of_sessions_not_adopted_is_pruned_and_not_shown_meanwhile() {
    let (state, dir) = before_adoption("background-store-pruned", "l", &["a"]);
    let path = dir.join("background-tasks.json");
    assert!(state.background_tasks_of("s").is_empty());
    let record = state.session_record("s").unwrap();
    assert!(state.wire_session(record).background_tasks.is_empty());

    state.retain_saved_background(|_| false);
    assert!(!path.exists());
    // Hidden work cannot show that memory was cleared: the adoption would give it back.
    state.restore_background_work("s", "l");
    assert!(state.background_tasks_of("s").is_empty());
}

/// A claim released with no process adopted clears the work in memory too.
#[tokio::test]
async fn a_released_claim_clears_the_work_in_memory() {
    let (state, dir) = before_adoption("background-store-released", "l", &["a"]);
    let path = dir.join("background-tasks.json");
    state.release_unlinked_claim("s");
    assert!(!path.exists());
    // Hidden work cannot show that memory was cleared: the adoption would give it back.
    state.restore_background_work("s", "l");
    assert!(state.background_tasks_of("s").is_empty());
}

/// A full list reported before the adoption makes the work wholly the reporting process's, so it is
/// kept when the adoption finds another launch than the one the loaded work was saved for, and not
/// shown until then.
#[tokio::test]
async fn a_list_reported_before_the_adoption_survives_a_launch_mismatch() {
    let (state, dir) = before_adoption("background-store-early-replace", "old", &["a"]);
    let path = dir.join("background-tasks.json");
    state.apply_background_tasks_change("s", BackgroundTasksChange::Replace(vec![task("x")]));
    assert!(state.background_tasks_of("s").is_empty());

    state.restore_background_work("s", "new");
    assert_eq!(state.background_tasks_of("s"), vec![task("x")]);
    let file = file_of(&path).unwrap();
    assert_eq!(file["sessions"]["s"]["launch"], "new");
    assert_eq!(saved_ids(&path, "s"), Some(vec!["x".to_string()]));
}
