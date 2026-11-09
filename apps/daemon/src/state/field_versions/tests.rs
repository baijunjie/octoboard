use super::*;
use crate::protocol::AgentId;

fn settings() -> Settings {
    Settings::default()
}

#[test]
fn a_record_always_carries_every_path() {
    let versions = FieldVersions::starting_at(1_000);
    let mut stamped = settings();
    versions.stamp_settings(&mut stamped, Stamp::Snapshot);
    let paths = settings_paths(&settings());
    assert_eq!(
        stamped.versions.keys().collect::<Vec<_>>(),
        paths.keys().collect::<Vec<_>>()
    );
    let name = AgentId::ALL[0].wire_name();
    assert!(stamped.versions.contains_key("auto_pull_repositories"));
    assert!(stamped.versions.contains_key("git_workflow.delivery"));
    assert!(stamped.versions.contains_key(&format!("agent_tags.{name}")));
    assert!(stamped.versions.values().all(|version| *version == 1_000));
}

#[test]
fn a_publication_advances_only_the_fields_that_changed() {
    let versions = FieldVersions::starting_at(1_000);
    let mut first = settings();
    versions.stamp_settings(&mut first, Stamp::Snapshot);

    let mut same = settings();
    versions.stamp_settings(&mut same, Stamp::Publish);
    assert_eq!(same.versions, first.versions);

    let mut changed = settings();
    changed.auto_pull_repositories = true;
    versions.stamp_settings(&mut changed, Stamp::Publish);
    assert_eq!(changed.versions["auto_pull_repositories"], 1_001);
    assert_eq!(changed.versions["default_clone_dir"], 1_000);
}

#[test]
fn a_record_never_seen_has_all_its_fields_advanced_when_published() {
    let versions = FieldVersions::starting_at(1_000);
    let mut published = settings();
    versions.stamp_settings(&mut published, Stamp::Publish);
    assert!(published.versions.values().all(|version| *version > 1_000));
}

#[test]
fn a_snapshot_keeps_the_baseline_a_publication_compares_with() {
    let versions = FieldVersions::starting_at(1_000);
    let mut read = settings();
    versions.stamp_settings(&mut read, Stamp::Snapshot);
    let mut stored = settings();
    stored.ask_questions_as_choices = !stored.ask_questions_as_choices;
    // A snapshot of a value that has changed, not yet published, does not move its version.
    versions.stamp_settings(&mut stored, Stamp::Snapshot);
    assert_eq!(stored.versions["ask_questions_as_choices"], 1_000);
    versions.stamp_settings(&mut stored, Stamp::Publish);
    assert_eq!(stored.versions["ask_questions_as_choices"], 1_001);
}

#[test]
fn a_later_run_starts_above_every_version_an_earlier_one_gave() {
    let earlier = FieldVersions::starting_at(1_000);
    let mut stamped = settings();
    for step in 0..50 {
        stamped.default_clone_dir = format!("/dir/{step}");
        earlier.stamp_settings(&mut stamped, Stamp::Publish);
    }
    let later = FieldVersions::starting_at(2_000);
    let mut fresh = settings();
    later.stamp_settings(&mut fresh, Stamp::Snapshot);
    let newest_before = stamped.versions.values().max().unwrap();
    assert!(fresh
        .versions
        .values()
        .all(|version| version > newest_before));
}

#[test]
fn accounts_are_stamped_with_the_settings() {
    let versions = FieldVersions::starting_at(1_000);
    let account = crate::protocol::Account {
        id: "a".to_string(),
        agent: AgentId::ALL[0],
        name: "Work".to_string(),
        config_dir: "/work".to_string(),
        share_default_config: false,
        share_default_config_incomplete: false,
        disabled: false,
        versions: Default::default(),
    };
    let mut first = settings();
    first.accounts.push(account.clone());
    versions.stamp_settings(&mut first, Stamp::Snapshot);
    assert_eq!(first.accounts[0].versions["name"], 1_000);

    let mut renamed = settings();
    renamed.accounts.push(crate::protocol::Account {
        name: "Home".to_string(),
        ..account
    });
    versions.stamp_settings(&mut renamed, Stamp::Publish);
    assert_eq!(renamed.accounts[0].versions["name"], 1_001);
    assert_eq!(renamed.accounts[0].versions["config_dir"], 1_000);
}

#[test]
fn a_check_names_every_stale_path_with_its_current_version() {
    let versions = FieldVersions::starting_at(1_000);
    let mut stamped = settings();
    versions.stamp_settings(&mut stamped, Stamp::Snapshot);
    stamped.auto_pull_repositories = true;
    versions.stamp_settings(&mut stamped, Stamp::Publish);
    let record = settings_paths(&settings());

    let current: BTreeMap<String, u64> = [
        ("auto_pull_repositories", 1_001),
        ("default_clone_dir", 1_000),
    ]
    .into_iter()
    .map(|(path, version)| (path.to_string(), version))
    .collect();
    versions
        .check(&RecordKey::Settings, &record, &current)
        .unwrap();

    let stale: BTreeMap<String, u64> =
        [("auto_pull_repositories", 1_000), ("default_clone_dir", 7)]
            .into_iter()
            .map(|(path, version)| (path.to_string(), version))
            .collect();
    let err = versions
        .check(&RecordKey::Settings, &record, &stale)
        .unwrap_err();
    let coded = err.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.code, error_code::WRITE_CONFLICT);
    assert_eq!(
        coded.params["fields"],
        "auto_pull_repositories,default_clone_dir"
    );
    assert_eq!(
        coded.params["versions"],
        r#"{"auto_pull_repositories":1001,"default_clone_dir":1000}"#
    );
}

#[test]
fn a_check_refuses_a_path_that_has_no_version() {
    let versions = FieldVersions::starting_at(1_000);
    let record = settings_paths(&settings());
    for path in ["no_such_field", "agent_tags.no-such-agent"] {
        let base = BTreeMap::from([(path.to_string(), 1_000)]);
        let err = versions
            .check(&RecordKey::Settings, &record, &base)
            .unwrap_err();
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::UNREADABLE_REQUEST, "{path}");
    }
}
