//! Reading a document: what it is refused for, how absent parts read, and how references and
//! paths are taken.

use super::*;

#[test]
fn a_document_is_refused_by_what_it_is_before_it_is_read_any_further() {
    let cases = [
        (json!([]), error_code::CONFIGURATION_UNREADABLE),
        (json!({}), error_code::CONFIGURATION_NOT_RECOGNISED),
        (
            json!({ "format": "something-else", "version": 1 }),
            error_code::CONFIGURATION_NOT_RECOGNISED,
        ),
        (
            json!({ "format": export::FORMAT }),
            error_code::CONFIGURATION_UNREADABLE,
        ),
        // Another version may be shaped in ways this one cannot read; it is told apart from
        // an unreadable body by its version.
        (
            json!({ "format": export::FORMAT, "version": export::VERSION + 1 }),
            error_code::CONFIGURATION_VERSION_UNSUPPORTED,
        ),
        (
            json!({ "format": export::FORMAT, "version": 0 }),
            error_code::CONFIGURATION_VERSION_UNSUPPORTED,
        ),
        (
            json!({ "format": export::FORMAT, "version": export::VERSION }),
            error_code::CONFIGURATION_UNREADABLE,
        ),
    ];
    for (value, code) in cases {
        let err = parse(&value).expect_err(&value.to_string());
        assert_eq!(code_of(&err), code, "{value}");
    }
    parse(&document(vec![console("Hub", Vec::new())])).expect("a valid document is read");
}

#[test]
fn a_document_without_exported_by_is_read_with_none() {
    let mut value = document(vec![console("Hub", Vec::new())]);
    value.as_object_mut().unwrap().remove("exported_by");

    let parsed = parse(&value).expect("read");

    assert_eq!(parsed.exported_by, None);
    assert_eq!(parsed.consoles.len(), 1);
}

/// Every export writes the whole settings region, so a document whose region lacks a part of
/// it is not one an export produced, and is refused rather than filled in.
#[test]
fn a_settings_region_missing_the_git_workflow_or_a_switch_is_refused() {
    let complete = with_settings(document(vec![console("Hub", Vec::new())]), true, false);
    parse(&complete).expect("a complete settings region is read");

    for key in ["git_workflow", "ask_questions_as_choices"] {
        let mut value = complete.clone();
        value["settings"].as_object_mut().unwrap().remove(key);
        let err = parse(&value).expect_err(key);
        assert_eq!(code_of(&err), error_code::CONFIGURATION_UNREADABLE, "{key}");
    }
    let mut value = complete;
    value["settings"]["git_workflow"]
        .as_object_mut()
        .unwrap()
        .remove("squash_before_delivery");
    let err = parse(&value).expect_err("squash_before_delivery");
    assert_eq!(code_of(&err), error_code::CONFIGURATION_UNREADABLE);
}

/// A document may leave an agent's entry out of `accounts` and `launch`: it reads as that
/// agent being unset, which an agent added to Octoboard after the document was exported is.
#[test]
fn an_agent_left_out_of_a_per_agent_entry_reads_as_unset() {
    let named = |name: &str| {
        Some(AccountName {
            name: name.to_string(),
        })
    };
    let launch = Launch {
        model: Some("grok-4".to_string()),
        effort: None,
    };
    let mut hub = console("Hub", vec![project("repo", "/work/repo")]);
    hub.accounts.codex = named("Work");
    hub.launch.grok = launch.clone();
    hub.projects[0].accounts.claude = ProjectAccountRef::Default;
    let mut value = document(vec![hub]);
    let parsed = parse(&value).expect("read");
    assert_eq!(parsed.consoles[0].accounts.codex, named("Work"));
    assert_eq!(parsed.consoles[0].launch.grok, launch);
    assert_eq!(
        parsed.consoles[0].projects[0].accounts.claude,
        ProjectAccountRef::Default
    );

    let hub = &mut value["consoles"][0];
    hub["accounts"].as_object_mut().unwrap().remove("codex");
    hub["launch"].as_object_mut().unwrap().remove("grok");
    hub["projects"][0]["accounts"]
        .as_object_mut()
        .unwrap()
        .remove("claude");
    let parsed = parse(&value).expect("read");

    assert_eq!(parsed.consoles[0].accounts.codex, None);
    assert_eq!(parsed.consoles[0].launch.grok, Launch::default());
    assert_eq!(
        parsed.consoles[0].projects[0].accounts.claude,
        ProjectAccountRef::Inherit
    );
}

#[test]
fn a_reference_is_rebound_by_agent_and_name_ignoring_case_and_falls_back_to_the_default() {
    let rebinder = Rebinder {
        accounts: vec![
            account("claude-work", Agent::Claude, "Work"),
            account("codex-work", Agent::Codex, "work"),
        ],
    };
    let named = |name: &str| {
        Some(AccountName {
            name: name.to_string(),
        })
    };
    let mut hub = console("Hub", Vec::new());
    // Matches by case-insensitive name in its own agent; the Grok one has no account at all;
    // the Codex one names an account that exists for Claude only.
    hub.accounts.claude = named("WORK");
    hub.accounts.codex = named("Work");
    hub.accounts.grok = named("Work");
    let bound = rebinder.console(&hub);
    assert_eq!(bound.accounts.claude.as_deref(), Some("claude-work"));
    assert_eq!(bound.accounts.codex.as_deref(), Some("codex-work"));
    assert_eq!(bound.accounts.grok, None);
    assert_eq!(
        bound.fallbacks,
        vec![AccountFallback {
            agent: Agent::Grok,
            name: "Work".to_string(),
        }]
    );

    let mut unmatched = project("p", "/p");
    unmatched.accounts.claude = ProjectAccountRef::Account {
        name: "missing".to_string(),
    };
    unmatched.accounts.codex = ProjectAccountRef::Default;
    unmatched.accounts.grok = ProjectAccountRef::Account {
        name: "work".to_string(),
    };
    let bound = rebinder.project(&unmatched);
    // The default account is pinned, not left to the console's reference.
    assert_eq!(bound.accounts.claude, ProjectAccount::Default);
    assert_eq!(bound.accounts.codex, ProjectAccount::Default);
    assert_eq!(bound.accounts.grok, ProjectAccount::Default);
    assert_eq!(
        bound.fallbacks.iter().map(|f| f.agent).collect::<Vec<_>>(),
        [Agent::Claude, Agent::Grok]
    );
    assert_eq!(
        rebinder.project(&project("q", "/q")).accounts.claude,
        ProjectAccount::Inherit
    );
}

#[test]
fn a_documents_paths_are_taken_only_as_an_absolute_path_and_its_last_segment() {
    assert_eq!(
        exported_directory("/work/./a/../b/").as_deref(),
        Some(Path::new("/work/b"))
    );
    assert_eq!(exported_directory("work/b"), None);
    assert_eq!(exported_directory("~/work/b"), None);
    assert_eq!(exported_directory(""), None);

    assert_eq!(
        derived_directory("/home/u/Projects", "/Users/x/code/app").as_deref(),
        Some(Path::new("/home/u/Projects/app"))
    );
    // Resolved before the last segment is taken, so a `..` cannot lead out of the directory.
    assert_eq!(
        derived_directory("/home/u/Projects", "/Users/x/code/app/../tool").as_deref(),
        Some(Path::new("/home/u/Projects/tool"))
    );
    assert_eq!(derived_directory("/home/u/Projects", "/"), None);
    // A document exported without paths records a directory's name alone.
    assert_eq!(
        derived_directory("/home/u/Projects", "app").as_deref(),
        Some(Path::new("/home/u/Projects/app"))
    );
    assert_eq!(derived_directory("/home/u/Projects", "work/app"), None);
    assert_eq!(derived_directory("/home/u/Projects", ""), None);
    assert_eq!(derived_directory("/home/u/Projects", "."), None);
    assert_eq!(derived_directory("/home/u/Projects", ".."), None);
    assert_eq!(derived_directory("Projects", "/Users/x/app"), None);

    assert_eq!(
        clone_directory("/home/u/Projects", "git@github.com:o/r.git").as_deref(),
        Some(Path::new("/home/u/Projects/r"))
    );
    assert_eq!(clone_directory("/home/u/Projects", "https://h/.."), None);
    assert_eq!(clone_directory("/home/u/Projects", "https://h/."), None);
    assert_eq!(clone_directory("/home/u/Projects", "/srv/git/r"), None);
    assert_eq!(clone_directory("Projects", "https://h/o/r"), None);
}
