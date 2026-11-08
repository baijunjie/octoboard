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
            json!({ "format": CONFIGURATION_FORMAT }),
            error_code::CONFIGURATION_UNREADABLE,
        ),
        // Another version may be shaped in ways this one cannot read; it is told apart from
        // an unreadable body by its version.
        (
            json!({ "format": CONFIGURATION_FORMAT, "version": CONFIGURATION_VERSION + 1 }),
            error_code::CONFIGURATION_VERSION_UNSUPPORTED,
        ),
        (
            json!({ "format": CONFIGURATION_FORMAT, "version": 0 }),
            error_code::CONFIGURATION_VERSION_UNSUPPORTED,
        ),
        (
            json!({ "format": CONFIGURATION_FORMAT, "version": CONFIGURATION_VERSION }),
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
fn an_unsupported_version_is_refused_with_the_range_this_daemon_reads() {
    let value = json!({ "format": CONFIGURATION_FORMAT, "version": CONFIGURATION_VERSION + 1 });

    let err = parse(&value).expect_err("refused");

    let coded = err.downcast_ref::<CodedError>().expect("a coded error");
    assert_eq!(
        coded.params["version"],
        (CONFIGURATION_VERSION + 1).to_string()
    );
    assert_eq!(coded.params["oldest"], upgrade::OLDEST_VERSION.to_string());
    assert_eq!(coded.params["supported"], CONFIGURATION_VERSION.to_string());
}

#[test]
fn a_document_without_exported_by_is_read_with_none() {
    let mut value = document(vec![console("Hub", Vec::new())]);
    value.as_object_mut().unwrap().remove("exported_by");

    let parsed = parse(&value).expect("read");

    assert_eq!(parsed.exported_by, None);
    assert_eq!(parsed.consoles.len(), 1);
}

/// A document carries only the settings of the sections it was exported with, so a key may be
/// absent; the git workflow, once present, is whole.
#[test]
fn an_absent_setting_is_read_as_not_carried_but_a_partial_git_workflow_is_refused() {
    let complete = with_settings(document(vec![console("Hub", Vec::new())]), true, false);
    parse(&complete).expect("a complete settings region is read");

    for key in ["git_workflow", "ask_questions_as_choices"] {
        let mut value = complete.clone();
        value["settings"].as_object_mut().unwrap().remove(key);
        let parsed = parse(&value).expect(key);
        let settings = parsed.settings.unwrap();
        match key {
            "git_workflow" => assert!(settings.git_workflow.is_none()),
            _ => assert!(settings.ask_questions_as_choices.is_none()),
        }
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
    hub.accounts[AgentId::CODEX] = named("Work");
    hub.launch[AgentId::GROK] = launch.clone();
    hub.projects[0].accounts[AgentId::CLAUDE] = ProjectAccountRef::Default;
    let mut value = document(vec![hub]);
    let parsed = parse(&value).expect("read");
    assert_eq!(parsed.consoles[0].accounts[AgentId::CODEX], named("Work"));
    assert_eq!(parsed.consoles[0].launch[AgentId::GROK], launch);
    assert_eq!(
        parsed.consoles[0].projects[0].accounts[AgentId::CLAUDE],
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

    assert_eq!(parsed.consoles[0].accounts[AgentId::CODEX], None);
    assert_eq!(parsed.consoles[0].launch[AgentId::GROK], Launch::default());
    assert_eq!(
        parsed.consoles[0].projects[0].accounts[AgentId::CLAUDE],
        ProjectAccountRef::Inherit
    );
}

#[test]
fn a_reference_is_rebound_by_agent_and_name_ignoring_case_and_falls_back_to_the_default() {
    let rebinder = Rebinder {
        accounts: vec![
            account("claude-work", AgentId::CLAUDE, "Work"),
            account("codex-work", AgentId::CODEX, "work"),
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
    hub.accounts[AgentId::CLAUDE] = named("WORK");
    hub.accounts[AgentId::CODEX] = named("Work");
    hub.accounts[AgentId::GROK] = named("Work");
    let bound = rebinder.console(&hub);
    assert_eq!(
        bound.accounts[AgentId::CLAUDE].as_deref(),
        Some("claude-work")
    );
    assert_eq!(
        bound.accounts[AgentId::CODEX].as_deref(),
        Some("codex-work")
    );
    assert_eq!(bound.accounts[AgentId::GROK], None);
    assert_eq!(
        bound.fallbacks,
        vec![AccountFallback {
            agent: AgentId::GROK,
            name: "Work".to_string(),
        }]
    );

    let mut unmatched = project("p", "/p");
    unmatched.accounts[AgentId::CLAUDE] = ProjectAccountRef::Account {
        name: "missing".to_string(),
    };
    unmatched.accounts[AgentId::CODEX] = ProjectAccountRef::Default;
    unmatched.accounts[AgentId::GROK] = ProjectAccountRef::Account {
        name: "work".to_string(),
    };
    let bound = rebinder.project(&unmatched);
    // The default account is pinned, not left to the console's reference.
    assert_eq!(bound.accounts[AgentId::CLAUDE], ProjectAccount::Default);
    assert_eq!(bound.accounts[AgentId::CODEX], ProjectAccount::Default);
    assert_eq!(bound.accounts[AgentId::GROK], ProjectAccount::Default);
    assert_eq!(
        bound.fallbacks.iter().map(|f| f.agent).collect::<Vec<_>>(),
        [AgentId::CLAUDE, AgentId::GROK]
    );
    assert_eq!(
        rebinder.project(&project("q", "/q")).accounts[AgentId::CLAUDE],
        ProjectAccount::Inherit
    );
}

#[test]
fn a_documents_paths_are_taken_only_as_an_absolute_path_and_its_last_segment() {
    assert_eq!(
        exported_directory(&host_path("/work/./a/../b/")),
        Some(PathBuf::from(host_path("/work/b")))
    );
    assert_eq!(exported_directory("work/b"), None);
    assert_eq!(exported_directory("~/work/b"), None);
    assert_eq!(exported_directory(""), None);

    assert_eq!(
        derived_directory(
            &host_path("/home/u/Projects"),
            &host_path("/Users/x/code/app")
        ),
        Some(PathBuf::from(host_path("/home/u/Projects/app")))
    );
    // Resolved before the last segment is taken, so a `..` cannot lead out of the directory.
    assert_eq!(
        derived_directory(
            &host_path("/home/u/Projects"),
            &host_path("/Users/x/code/app/../tool")
        ),
        Some(PathBuf::from(host_path("/home/u/Projects/tool")))
    );
    assert_eq!(
        derived_directory(&host_path("/home/u/Projects"), &host_path("/")),
        None
    );
    // A document exported without paths records a directory's name alone.
    assert_eq!(
        derived_directory(&host_path("/home/u/Projects"), "app"),
        Some(PathBuf::from(host_path("/home/u/Projects/app")))
    );
    assert_eq!(
        derived_directory(&host_path("/home/u/Projects"), "work/app"),
        None
    );
    assert_eq!(derived_directory(&host_path("/home/u/Projects"), ""), None);
    assert_eq!(derived_directory(&host_path("/home/u/Projects"), "."), None);
    assert_eq!(
        derived_directory(&host_path("/home/u/Projects"), ".."),
        None
    );
    assert_eq!(derived_directory("Projects", "/Users/x/app"), None);
    // A path written on another operating system derives nothing, padded or not.
    let padded = format!(" {} ", foreign_path());
    assert!(is_foreign(&padded));
    assert_eq!(
        derived_directory(&host_path("/home/u/Projects"), &padded),
        None
    );

    assert_eq!(
        clone_directory(&host_path("/home/u/Projects"), "git@github.com:o/r.git"),
        Some(PathBuf::from(host_path("/home/u/Projects/r")))
    );
    assert_eq!(
        clone_directory(&host_path("/home/u/Projects"), "https://h/.."),
        None
    );
    assert_eq!(
        clone_directory(&host_path("/home/u/Projects"), "https://h/."),
        None
    );
    assert_eq!(
        clone_directory(&host_path("/home/u/Projects"), "/srv/git/r"),
        None
    );
    assert_eq!(clone_directory("Projects", "https://h/o/r"), None);
}

#[test]
fn a_path_written_on_another_operating_system_derives_no_directory() {
    let foreign = foreign_path();
    assert_eq!(exported_directory(foreign), None);
    assert_eq!(
        derived_directory(&host_path("/home/u/Projects"), foreign),
        None
    );
    // The same path in the style of this host, and a bare name, still derive.
    let own = host_path("/Users/x/code/app");
    assert!(exported_directory(&own).is_some());
    assert_eq!(
        derived_directory(&host_path("/home/u/Projects"), &own),
        Some(PathBuf::from(host_path("/home/u/Projects/app")))
    );
    assert_eq!(
        derived_directory(&host_path("/home/u/Projects"), "app"),
        Some(PathBuf::from(host_path("/home/u/Projects/app")))
    );
    assert!(!is_foreign(&own));
    assert!(!is_foreign("app"));
    assert!(is_foreign(foreign));
}
