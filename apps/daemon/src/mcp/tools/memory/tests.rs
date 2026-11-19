use serde_json::{json, Value};

use crate::mcp::dispatch::call;
use crate::mcp::test_support::{arguments, console_session, shared_console_state};
use crate::protocol::{Session, MEMORY_ID_LENGTH, MEMORY_MAX_CONTENT_BYTES};

/// `shared_console_state` with Octoboard's memory turned on, as every test here but the one of
/// the setting needs.
fn memory_state(
    name: &str,
) -> (
    std::sync::Arc<crate::state::AppState>,
    crate::test_support::ScratchDir,
) {
    let (state, dir) = shared_console_state(name);
    state.store.set_memory_enabled(true).unwrap();
    (state, dir)
}

/// The arguments of a `save_memory` that creates an entry summarized `summary` holding
/// `content`.
fn entry(summary: &str, content: &str) -> Value {
    json!({
        "summary": summary,
        "content": content,
    })
}

/// The arguments of a `save_memory` that updates the entry `id` from `version`.
fn update(id: &Value, version: i64, content: &str) -> Value {
    let mut args = entry("updated", content);
    args["id"] = id.clone();
    args["version"] = json!(version);
    args
}

async fn ok(
    state: &std::sync::Arc<crate::state::AppState>,
    caller: &str,
    tool: &str,
    args: Value,
) -> Value {
    call(state, caller, tool, &arguments(args))
        .await
        .unwrap_or_else(|err| panic!("{tool}: {err:#}"))
}

async fn refused(
    state: &std::sync::Arc<crate::state::AppState>,
    caller: &str,
    tool: &str,
    args: Value,
) -> String {
    format!(
        "{:#}",
        call(state, caller, tool, &arguments(args))
            .await
            .expect_err(tool)
    )
}

/// One console session saves an entry and is told its id; another console session of the same
/// console lists it, reads it in full by id with its writer, updates it by id with its version,
/// and deletes it.
#[tokio::test]
async fn console_sessions_of_one_console_share_its_memory() {
    let (state, _dir) = memory_state("memory-shared");
    let saved = ok(
        &state,
        "a",
        "save_memory",
        entry("Web requests go to site", "web goes to `site`"),
    )
    .await;
    let id = saved["id"].clone();
    assert_eq!(id.as_str().unwrap().len(), MEMORY_ID_LENGTH);
    assert!(saved.get("name").is_none(), "{saved}");
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["local"], false);
    assert!(saved.get("content").is_none(), "{saved}");
    assert_eq!(saved["updated_by"], json!({ "kind": "agent" }));

    let listed = ok(&state, "b", "list_memories", json!({})).await;
    assert_eq!(
        listed["memories"],
        json!([{
            "id": id,
            "summary": "Web requests go to site",
            "local": false,
            "updated_at": saved["updated_at"],
            "updated_by": saved["updated_by"],
            "version": 1,
        }])
    );
    let read = ok(&state, "b", "read_memory", json!({ "id": id })).await;
    assert_eq!(read["content"], "web goes to `site`");
    assert_eq!(read["version"], 1);

    let updated = ok(
        &state,
        "b",
        "save_memory",
        update(&id, 1, "web goes to `site-v2`"),
    )
    .await;
    assert_eq!((&updated["id"], &updated["version"]), (&id, &json!(2)));
    assert_eq!(updated["summary"], "updated");
    assert!(updated.get("content").is_none(), "{updated}");
    assert_eq!(updated["created_at"], saved["created_at"]);
    assert_eq!(updated["updated_by"], json!({ "kind": "agent" }));

    assert_eq!(
        ok(&state, "a", "delete_memory", json!({ "id": id })).await,
        json!({ "deleted": id })
    );
    assert_eq!(
        ok(&state, "a", "list_memories", json!({})).await["memories"],
        json!([])
    );
}

/// Two entries may carry the same summary; each create is a new entry with an id of its own.
#[tokio::test]
async fn a_summary_is_no_key() {
    let (state, _dir) = memory_state("memory-same-summary");
    let first = ok(&state, "a", "save_memory", entry("same", "one")).await["id"].clone();
    let second = ok(&state, "a", "save_memory", entry("same", "two")).await["id"].clone();
    assert_ne!(first, second);
    assert_eq!(
        ok(&state, "a", "read_memory", json!({ "id": first })).await["content"],
        "one"
    );
    assert_eq!(
        ok(&state, "a", "list_memories", json!({})).await["memories"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

/// An update that names a version other than the current one is refused with the reason, what to
/// save next, and the current entry, content included, and leaves the entry as it was.
#[tokio::test]
async fn a_stale_update_is_refused_with_the_current_entry() {
    let (state, _dir) = memory_state("memory-stale");
    let id = ok(&state, "a", "save_memory", entry("prefs", "first")).await["id"].clone();
    ok(&state, "b", "save_memory", update(&id, 1, "second")).await;

    let error = refused(&state, "a", "save_memory", update(&id, 1, "third")).await;
    let id_text = id.as_str().unwrap();
    assert!(error.contains("is at version 2 now"), "{error}");
    assert!(
        error.contains(&format!("save again with `id` `{id_text}` and `version` 2")),
        "{error}"
    );
    assert!(error.contains("The current entry:"), "{error}");
    assert!(error.contains("\"content\": \"second\""), "{error}");

    let read = ok(&state, "a", "read_memory", json!({ "id": id })).await;
    assert_eq!(
        (read["content"].as_str(), read["version"].as_i64()),
        (Some("second"), Some(2))
    );
}

/// Each refusal of the rules, of an id and a version that do not go together, and of an id that
/// names no entry, comes back as a tool error in prose; nothing is saved.
#[tokio::test]
async fn a_broken_rule_or_a_missing_entry_is_refused_in_prose() {
    let (state, _dir) = memory_state("memory-refusals");
    let too_long = "x".repeat(MEMORY_MAX_CONTENT_BYTES + 1);
    let mut id_without_version = entry("d", "x");
    id_without_version["id"] = json!("abcd1234");
    let mut version_without_id = entry("d", "x");
    version_without_id["version"] = json!(1);
    for (tool, args, expected) in [
        ("save_memory", entry("n", " "), "`content` is blank"),
        (
            "save_memory",
            entry("n", &too_long),
            "an entry holds at most 16384",
        ),
        (
            "save_memory",
            json!({ "summary": "d", "content": "x", "local": "yes" }),
            "`local`",
        ),
        (
            "save_memory",
            entry("two\nlines", "x"),
            "`summary` must be one line",
        ),
        (
            "save_memory",
            id_without_version,
            "an update of the entry `abcd1234` needs its current `version`",
        ),
        (
            "save_memory",
            version_without_id,
            "`version` goes with the `id` of the entry to update",
        ),
        (
            "save_memory",
            update(&json!("nowhere0"), 1, "x"),
            "has no entry `nowhere0`",
        ),
        (
            "read_memory",
            json!({ "id": "nowhere0" }),
            "has no entry `nowhere0`",
        ),
        (
            "delete_memory",
            json!({ "id": "nowhere0" }),
            "has no entry `nowhere0`",
        ),
    ] {
        let error = refused(&state, "a", tool, args).await;
        assert!(error.contains(expected), "{tool}: {error}");
    }
    assert_eq!(
        ok(&state, "a", "list_memories", json!({})).await["memories"],
        json!([])
    );
}

/// The id of a deleted entry names nothing afterwards, however many entries are created since.
#[tokio::test]
async fn a_deleted_entrys_id_never_comes_back() {
    let (state, _dir) = memory_state("memory-id-gone");
    let id = ok(&state, "a", "save_memory", entry("d", "x")).await["id"].clone();
    ok(&state, "a", "delete_memory", json!({ "id": id })).await;
    for _ in 0..20 {
        let fresh = ok(&state, "a", "save_memory", entry("d", "y")).await["id"].clone();
        assert_ne!(fresh, id);
    }
    refused(&state, "b", "read_memory", json!({ "id": id })).await;
}

/// A console session reaches only its own console's memory: another console's session neither
/// lists, reads, updates nor deletes it.
#[tokio::test]
async fn a_console_session_reaches_only_its_own_consoles_memory() {
    let (state, _dir) = memory_state("memory-own-console");
    state
        .store
        .insert_session(&Session {
            console_id: "console-2".to_string(),
            ..console_session("other")
        })
        .unwrap();
    let id = ok(&state, "a", "save_memory", entry("prefs", "mine")).await["id"].clone();
    assert_eq!(
        ok(&state, "other", "list_memories", json!({})).await["memories"],
        json!([])
    );
    let error = refused(&state, "other", "read_memory", json!({ "id": id })).await;
    assert!(error.contains("has no entry"), "{error}");
    refused(&state, "other", "save_memory", update(&id, 1, "theirs")).await;
    refused(&state, "other", "delete_memory", json!({ "id": id })).await;
    let read = ok(&state, "a", "read_memory", json!({ "id": id })).await;
    assert_eq!(read["content"], "mine");
}

/// What a console session saves or deletes is told to every client, by id, with the session as
/// the writer, so a window showing the memory follows it.
#[tokio::test]
async fn a_console_sessions_change_is_told_to_every_client() {
    use crate::protocol::{Event, MemoryWriter};
    let (state, _dir) = memory_state("memory-announced");
    let mut events = state.subscribe();
    let id = ok(&state, "a", "save_memory", entry("routing", "x")).await["id"].clone();
    let Event::MemorySaved { entry: saved } = events.try_recv().unwrap() else {
        panic!("a save is announced");
    };
    assert_eq!(json!(saved.id), id);
    assert_eq!(saved.content, "x");
    assert_eq!(saved.updated_by, MemoryWriter::Agent);
    ok(&state, "b", "delete_memory", json!({ "id": id })).await;
    assert!(matches!(
        events.try_recv().unwrap(),
        Event::MemoriesDeleted { ref scope, ref memories }
            if scope == &saved.scope && memories == std::slice::from_ref(&saved.id)
    ));
    refused(&state, "b", "delete_memory", json!({ "id": id })).await;
    assert!(events.try_recv().is_err(), "a refusal is not announced");
}

/// The index lists the most recently updated first, an update moving its entry to the front.
#[tokio::test]
async fn the_index_is_newest_first() {
    use crate::test_support::clock::next_millisecond;
    let (state, _dir) = memory_state("memory-index-order");
    let summaries = |listed: &Value| -> Vec<String> {
        listed["memories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|memory| memory["summary"].as_str().unwrap().to_string())
            .collect()
    };
    let mut first = Value::Null;
    for summary in ["first", "second", "third"] {
        next_millisecond().await;
        let id = ok(&state, "a", "save_memory", entry(summary, "x")).await["id"].clone();
        if summary == "first" {
            first = id;
        }
    }
    assert_eq!(
        summaries(&ok(&state, "a", "list_memories", json!({})).await),
        ["third", "second", "first"]
    );
    next_millisecond().await;
    let mut moved = update(&first, 1, "y");
    moved["summary"] = json!("first, updated");
    ok(&state, "b", "save_memory", moved).await;
    assert_eq!(
        summaries(&ok(&state, "a", "list_memories", json!({})).await),
        ["first, updated", "third", "second"]
    );
}

/// A console session marks an entry that holds only on this machine with `local`: a new entry is
/// not local unless it says so, and an update that leaves `local` out keeps it.
#[tokio::test]
async fn a_console_entry_is_local_only_when_marked_and_an_update_keeps_it() {
    let (state, _dir) = memory_state("memory-local");
    let plain = ok(&state, "a", "save_memory", entry("plain", "x")).await;
    assert_eq!(plain["local"], false);
    let mut marked = entry("Docker needs a prompt here", "x");
    marked["local"] = json!(true);
    let marked = ok(&state, "a", "save_memory", marked).await;
    assert_eq!(marked["local"], true);
    let kept = ok(&state, "b", "save_memory", update(&marked["id"], 1, "y")).await;
    assert_eq!(kept["local"], true, "an update without `local` keeps it");
    let mut cleared = update(&marked["id"], 2, "z");
    cleared["local"] = json!(false);
    assert_eq!(
        ok(&state, "b", "save_memory", cleared).await["local"],
        false
    );
}

/// The project `project-3` of `console-2` at the directory of `project-1` of `console-1`, and a
/// coordinator `there` in it: two projects of two consoles at one place.
fn project_at_the_same_place(state: &std::sync::Arc<crate::state::AppState>) {
    use crate::mcp::test_support::project_session;
    let mut twin = state.store.require_project("project-1").unwrap();
    twin.id = "project-3".to_string();
    twin.console_id = "console-2".to_string();
    state.store.insert_project(&twin).unwrap();
    state
        .store
        .insert_session(&Session {
            project_id: Some("project-3".to_string()),
            ..project_session("there", "console-2", None)
        })
        .unwrap();
}

/// A project session, coordinator or worker, reaches its project's place's memory: a session of
/// another project at the same place, in another console, sees what it saved, while a console
/// session reaches only its console's and a project elsewhere sees none of it. A place's entry
/// carries no `local`, and one sent is ignored.
#[tokio::test]
async fn project_sessions_share_their_places_memory() {
    let (state, _dir) = memory_state("memory-place");
    project_at_the_same_place(&state);
    let mut args = entry("The test database runs on port 5433 here", "x");
    args["local"] = json!(true);
    let saved = ok(&state, "loose", "save_memory", args).await;
    let id = saved["id"].clone();
    assert!(saved.get("local").is_none(), "{saved}");

    for caller in ["of-a", "there"] {
        let listed = ok(&state, caller, "list_memories", json!({})).await;
        assert_eq!(listed["memories"].as_array().unwrap().len(), 1, "{caller}");
        assert_eq!(listed["memories"][0]["id"], id);
        assert!(listed["memories"][0].get("local").is_none());
    }
    let read = ok(&state, "there", "read_memory", json!({ "id": id })).await;
    assert_eq!(read["content"], "x");
    assert!(read.get("local").is_none());
    assert_eq!(
        ok(&state, "a", "list_memories", json!({})).await["memories"],
        json!([])
    );
    assert_eq!(
        ok(&state, "foreign", "list_memories", json!({})).await["memories"],
        json!([])
    );
    let error = refused(&state, "a", "read_memory", json!({ "id": id })).await;
    assert!(
        error.contains("this console's memory has no entry"),
        "{error}"
    );
    let error = refused(&state, "foreign", "read_memory", json!({ "id": id })).await;
    assert!(
        error.contains("this project's memory has no entry"),
        "{error}"
    );

    let updated = ok(&state, "of-a", "save_memory", update(&id, 1, "y")).await;
    assert_eq!(updated["version"], 2);
    ok(&state, "there", "delete_memory", json!({ "id": id })).await;
    refused(&state, "loose", "read_memory", json!({ "id": id })).await;
}

/// A project session is offered the memory tools worded for a project, with no `local`, and a
/// console session those worded for its console.
#[test]
fn each_kind_is_offered_the_memory_tools_worded_for_it() {
    use crate::protocol::SessionKind;
    use crate::session_tools::catalogue::tool_by_name;
    for worker in [false, true] {
        let tool = tool_by_name(SessionKind::Project, worker, "save_memory").unwrap();
        assert!(tool.description.contains("this project's memory"));
        assert!((tool.schema)()["properties"].get("local").is_none());
        for name in ["list_memories", "read_memory", "delete_memory"] {
            let tool = tool_by_name(SessionKind::Project, worker, name).unwrap();
            assert!(tool.description.contains("this project's memory"), "{name}");
        }
    }
    let tool = tool_by_name(SessionKind::Console, false, "save_memory").unwrap();
    assert!(tool.description.contains("this console's memory"));
    assert!((tool.schema)()["properties"].get("local").is_some());
}

/// A console holding as many entries as it may refuses a new one in prose, while its entries can
/// still be updated; a version that is not a positive whole number is refused too.
#[tokio::test]
async fn a_full_console_refuses_a_new_entry_and_a_bad_version_is_refused() {
    use crate::protocol::MEMORY_MAX_ENTRIES;
    let (state, _dir) = memory_state("memory-full");
    let mut ids = Vec::new();
    for index in 0..MEMORY_MAX_ENTRIES {
        ids.push(
            ok(&state, "a", "save_memory", entry(&format!("e{index}"), "x")).await["id"].clone(),
        );
    }
    let error = refused(&state, "a", "save_memory", entry("one-more", "x")).await;
    assert!(error.contains("200"), "{error}");
    assert!(error.contains("was not saved"), "{error}");
    assert_eq!(
        ok(&state, "b", "save_memory", update(&ids[0], 1, "y")).await["version"],
        2
    );
    assert_eq!(
        ok(&state, "a", "list_memories", json!({})).await["memories"]
            .as_array()
            .unwrap()
            .len(),
        MEMORY_MAX_ENTRIES
    );

    for version in [json!(0), json!(-1), json!("1"), json!(1.5)] {
        let mut args = update(&ids[1], 1, "z");
        args["version"] = version.clone();
        refused(&state, "a", "save_memory", args).await;
        let read = ok(&state, "a", "read_memory", json!({ "id": ids[1] })).await;
        assert_eq!(read["content"], "x", "{version}");
    }
}

/// The summary's and the content's limits hold at the edge: an entry of the longest of each is
/// held, one past either is refused.
#[tokio::test]
async fn limits_hold_at_the_edge() {
    let (state, _dir) = memory_state("memory-edges");
    ok(&state, "a", "save_memory", entry(&"d".repeat(200), "x")).await;
    let error = refused(&state, "a", "save_memory", entry(&"d".repeat(201), "x")).await;
    assert!(error.contains("`summary` must be"), "{error}");
    ok(
        &state,
        "a",
        "save_memory",
        entry("big", &"x".repeat(MEMORY_MAX_CONTENT_BYTES)),
    )
    .await;
}

/// While Octoboard's memory is off, every memory tool is refused at the call, for a console
/// session and a project session alike, saying the memory is turned off, and nothing is saved;
/// turned on, the same call goes through.
#[tokio::test]
async fn a_memory_tool_is_refused_while_memory_is_off() {
    let (state, _dir) = shared_console_state("memory-off");
    for caller in ["a", "loose"] {
        for (tool, args) in [
            ("save_memory", entry("s", "x")),
            ("list_memories", json!({})),
            ("read_memory", json!({ "id": "abcd1234" })),
            ("delete_memory", json!({ "id": "abcd1234" })),
        ] {
            let error = refused(&state, caller, tool, args).await;
            assert!(
                error.contains("Octoboard's memory is turned off"),
                "{caller} {tool}: {error}"
            );
        }
    }
    state.store.set_memory_enabled(true).unwrap();
    ok(&state, "a", "save_memory", entry("s", "x")).await;
    state.store.set_memory_enabled(false).unwrap();
    refused(&state, "a", "list_memories", json!({})).await;
}

/// The memory tools are offered only while the memory is on; every other tool is offered
/// either way.
#[test]
fn the_memory_tools_are_offered_only_while_memory_is_on() {
    use crate::protocol::SessionKind;
    use crate::session_tools::catalogue::{tools_for, tools_offered, MEMORY_TOOLS};
    for (kind, worker) in [
        (SessionKind::Console, false),
        (SessionKind::Project, false),
        (SessionKind::Project, true),
    ] {
        let names = |memory| -> Vec<&str> {
            tools_offered(kind, worker, memory)
                .into_iter()
                .map(|tool| tool.name)
                .collect()
        };
        let all: Vec<&str> = tools_for(kind, worker)
            .iter()
            .map(|tool| tool.name)
            .collect();
        assert_eq!(names(true), all);
        let off = names(false);
        assert!(MEMORY_TOOLS.iter().all(|tool| !off.contains(tool)));
        assert_eq!(off.len(), all.len() - MEMORY_TOOLS.len());
    }
}

/// Turning the memory off deletes nothing: the entries come back as they were once it is on again.
#[tokio::test]
async fn turning_memory_off_deletes_nothing() {
    let (state, _dir) = memory_state("memory-off-keeps");
    let id = ok(&state, "a", "save_memory", entry("prefs", "keep me")).await["id"].clone();
    state.store.set_memory_enabled(false).unwrap();
    refused(&state, "a", "read_memory", json!({ "id": id })).await;
    state.store.set_memory_enabled(true).unwrap();
    let read = ok(&state, "a", "read_memory", json!({ "id": id })).await;
    assert_eq!(read["content"], "keep me");
    assert_eq!(
        ok(&state, "a", "list_memories", json!({})).await["memories"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
