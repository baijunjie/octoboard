//! A console's memory in a document: what an export carries of it, what an import gives the
//! console it creates, and the entries an import refuses.

use super::*;
use crate::coordinator::memory::save_memory;
use crate::mcp::dispatch::call;
use crate::protocol::{
    DocumentMemoryEntry, MemoryDraft, MemoryScope, MemoryTarget, MemoryWriter, Session,
    SessionKind, MEMORY_MAX_CONTENT_BYTES, MEMORY_MAX_ENTRIES, MEMORY_MAX_SUMMARY_CHARS,
};
use crate::test_support::bare_session;

fn entry(subject: &str, content: &str) -> DocumentMemoryEntry {
    DocumentMemoryEntry {
        summary: format!("about {subject}"),
        content: content.to_string(),
    }
}

fn console_scope(console_id: &str) -> MemoryScope {
    MemoryScope::Console {
        console_id: console_id.to_string(),
    }
}

/// `apply` of the consoles alone, with no project placed.
async fn import_consoles(state: &Arc<AppState>, document: &serde_json::Value) -> Result<Imported> {
    apply(
        state,
        document,
        Vec::new(),
        ApplyOptions {
            settings: Vec::new(),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
}

/// The machine's console sessions read the console's memory through the tools.
async fn tool(
    state: &Arc<AppState>,
    caller: &str,
    name: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    call(state, caller, name, args.as_object().unwrap())
        .await
        .unwrap_or_else(|err| panic!("{name}: {err:#}"))
}

/// A console's entries travel without its local ones, the most recently updated first and without
/// their ids, who wrote them or when; a project's place's memory does not travel at all; the
/// console an import creates holds them in the same order, each with a new id, not local, as the
/// user's, created then, and its console sessions list and read them.
#[tokio::test]
async fn memory_travels_with_its_console_but_for_the_local_entries() {
    let (source, source_dir) = app_state("import-memory-source");
    let exported_console = crate::test_support::console(&source_dir);
    source.store.insert_console(&exported_console).unwrap();
    let writer = MemoryWriter::agent(crate::protocol::AgentId::CODEX);
    for (subject, local, content) in [
        ("web-routing", false, "web goes to `site`"),
        ("prefers-brevity", false, "Short replies.\n"),
        ("local-docker", true, "Docker needs a prompt"),
    ] {
        crate::test_support::clock::next_millisecond().await;
        save_memory(
            &source.store,
            &console_scope(&exported_console.id),
            &MemoryTarget::Create,
            MemoryDraft {
                summary: format!("about {subject}"),
                content: content.to_string(),
                local: Some(local),
            },
            &writer,
        )
        .unwrap()
        .unwrap();
    }
    let project = crate::test_support::insert_project(&source, &source_dir.join("repo"));
    save_memory(
        &source.store,
        &crate::coordinator::memory::place_of(&project),
        &MemoryTarget::Create,
        MemoryDraft {
            summary: "about the place".to_string(),
            content: "never exported".to_string(),
            local: None,
        },
        &writer,
    )
    .unwrap()
    .unwrap();

    let document = export::build(
        &source.store,
        &export::Selection::console(&exported_console.id, true),
    )
    .unwrap();
    assert_eq!(
        document.consoles[0].memories.as_deref(),
        Some(
            &[
                entry("prefers-brevity", "Short replies.\n"),
                entry("web-routing", "web goes to `site`"),
            ][..]
        )
    );
    let value = serde_json::to_value(&document).unwrap();
    // Carrying memory does not raise the document's version.
    assert_eq!(value["version"], 4);
    assert_eq!(
        value["consoles"][0]["memories"][0],
        json!({
            "summary": "about prefers-brevity",
            "content": "Short replies.\n",
        })
    );
    assert!(!value.to_string().contains("never exported"));

    let (state, _dir) = app_state("import-memory");
    state.store.set_memory_enabled(true).unwrap();
    let before = crate::time::now_millis();
    let imported = import_consoles(&state, &value).await.unwrap();
    let console_id = imported.consoles[0].console.clone().expect("created");
    let stored = state.store.memories(&console_scope(&console_id)).unwrap();
    assert_eq!(
        stored
            .iter()
            .map(|entry| &*entry.summary)
            .collect::<Vec<_>>(),
        ["about prefers-brevity", "about web-routing"]
    );
    for entry in &stored {
        assert_eq!(entry.id.len(), crate::protocol::MEMORY_ID_LENGTH);
    }
    for entry in &stored {
        assert!(!entry.local);
        assert_eq!(entry.updated_by, MemoryWriter::User);
        assert_eq!(entry.version, 1);
        assert!(entry.created_at >= before && entry.updated_at == entry.created_at);
    }

    state
        .store
        .insert_session(&Session {
            console_id: console_id.clone(),
            ..bare_session("console-session", SessionKind::Console)
        })
        .unwrap();
    let listed = tool(&state, "console-session", "list_memories", json!({})).await;
    let summaries: Vec<_> = listed["memories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["summary"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(summaries, ["about prefers-brevity", "about web-routing"]);
    let read = tool(
        &state,
        "console-session",
        "read_memory",
        json!({ "id": listed["memories"][1]["id"] }),
    )
    .await;
    assert_eq!(read["content"], "web goes to `site`");
    assert_eq!(read["local"], false);
    assert_eq!(read["updated_by"], json!({ "kind": "user" }));
}

/// A console without entries is exported with an empty list, and a document written before memory
/// travelled, with no `memories` at all, imports as before, giving the console none.
#[tokio::test]
async fn a_document_without_memory_gives_the_console_none() {
    let (source, source_dir) = app_state("import-memory-none-source");
    let exported_console = crate::test_support::console(&source_dir);
    source.store.insert_console(&exported_console).unwrap();
    let document = export::build(
        &source.store,
        &export::Selection::console(&exported_console.id, true),
    )
    .unwrap();
    assert_eq!(document.consoles[0].memories, Some(Vec::new()));

    let (state, _dir) = app_state("import-memory-none");
    let mut older = super::document(vec![console("Hub", Vec::new())]);
    older["consoles"][0]
        .as_object_mut()
        .unwrap()
        .remove("memories");
    inspect(&state, &older).await.expect("inspected");
    let imported = import_consoles(&state, &older).await.unwrap();
    let console_id = imported.consoles[0].console.clone().expect("created");
    assert_eq!(
        state.store.memories(&console_scope(&console_id)).unwrap(),
        []
    );
}

/// An entry the memory tools would not hold makes the document unreadable, refused by the
/// inspection and before an import creates anything.
#[tokio::test]
async fn an_entry_past_the_rules_makes_the_document_unreadable() {
    let (state, _dir) = app_state("import-memory-invalid");
    let fine = entry("fine", "c");
    let too_many: Vec<_> = (0..=MEMORY_MAX_ENTRIES)
        .map(|n| entry(&format!("e{n}"), "c"))
        .collect();
    let too_long = entry("huge", &"x".repeat(MEMORY_MAX_CONTENT_BYTES + 1));
    let at_limit = entry("huge", &"x".repeat(MEMORY_MAX_CONTENT_BYTES));
    // The entry each case breaks the rules with, by what the detail names it with: its place in
    // the console's list and its summary.
    let cases: Vec<(Vec<DocumentMemoryEntry>, &str, Option<&str>)> = vec![
        (
            vec![DocumentMemoryEntry {
                summary: "two\nlines".to_string(),
                ..fine.clone()
            }],
            "summary",
            Some("memory entry 0"),
        ),
        (
            vec![DocumentMemoryEntry {
                summary: " ".to_string(),
                ..fine.clone()
            }],
            "summary",
            Some("memory entry 0"),
        ),
        (
            vec![DocumentMemoryEntry {
                summary: "d".repeat(MEMORY_MAX_SUMMARY_CHARS + 1),
                ..fine.clone()
            }],
            "summary",
            Some("memory entry 0"),
        ),
        (vec![entry("blank", " \n")], "blank", Some("about blank")),
        (
            vec![fine.clone(), too_long],
            "bytes",
            Some("memory entry 1 (\"about huge\")"),
        ),
        (too_many, "more than", None),
    ];
    for (memories, says, entry_name) in cases {
        let mut hub = console("Hub", Vec::new());
        hub.memories = Some(memories);
        let document = super::document(vec![console("Other", Vec::new()), hub]);
        for err in [
            inspect(&state, &document).await.expect_err("inspected"),
            import_consoles(&state, &document)
                .await
                .expect_err("imported"),
        ] {
            assert_eq!(code_of(&err), error_code::CONFIGURATION_UNREADABLE);
            let detail = param_of(&err, "detail");
            assert!(detail.contains("console 1"), "{detail}");
            assert!(detail.contains(says), "{says}: {detail}");
            if let Some(entry_name) = entry_name {
                assert!(detail.contains(entry_name), "{entry_name}: {detail}");
            }
        }
        assert!(state.store.list_consoles().unwrap().is_empty());
    }

    // Exactly at each limit the entries are held, and two of one summary are two entries.
    let mut hub = console("Hub", Vec::new());
    hub.memories = Some(vec![at_limit.clone(), at_limit.clone()]);
    let mut other = console("Other", Vec::new());
    other.memories = Some(vec![at_limit]);
    let at_the_edge: Vec<_> = (0..MEMORY_MAX_ENTRIES)
        .map(|n| entry(&format!("e{n}"), "c"))
        .collect();
    let mut full = console("Full", Vec::new());
    full.memories = Some(at_the_edge);
    let imported = import_consoles(&state, &super::document(vec![hub, other, full]))
        .await
        .unwrap();
    let held: Vec<usize> = imported
        .consoles
        .iter()
        .map(|created| {
            let id = created.console.clone().expect("created");
            state.store.memories(&console_scope(&id)).unwrap().len()
        })
        .collect();
    assert_eq!(held, [2, 1, MEMORY_MAX_ENTRIES]);
}

/// Whether Octoboard's memory is on stays with the machine: a full export carries it nowhere, and
/// an import that applies every settings section the document carries leaves the importing
/// machine's own value.
#[tokio::test]
async fn the_memory_switch_does_not_travel_in_a_document() {
    let (source, source_dir) = app_state("import-memory-switch-source");
    source.store.set_memory_enabled(true).unwrap();
    source
        .store
        .insert_console(&crate::test_support::console(&source_dir))
        .unwrap();
    let document =
        serde_json::to_value(export::build(&source.store, &export::Selection::full(true)).unwrap())
            .unwrap();
    assert!(!document.to_string().contains("memory_enabled"));

    let (target, _target_dir) = app_state("import-memory-switch");
    apply(
        &target,
        &document,
        Vec::new(),
        ApplyOptions {
            settings: carried_sections(&document),
            accounts: None,
            consoles: true,
            clone_dir: true,
            agents: None,
        },
    )
    .await
    .unwrap();
    assert!(!target.store.memory_enabled().unwrap());
}
