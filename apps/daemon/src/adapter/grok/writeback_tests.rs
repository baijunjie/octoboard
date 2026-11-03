//! Tests of the shared-config writeback.

use std::path::PathBuf;

use super::tests::{plan_isolated, with_default_home};
use super::writeback::SharedConfigWriteback;
use super::*;
use crate::adapter::files::SMALL_FILE_LIMIT;
use crate::adapter::tests::{spec_fixture, SpecFixture};

/// The write-back the plan leaves for after the exit.
fn writeback_of(plan: &LaunchPlan) -> &SharedConfigWriteback {
    plan.after_exit
        .as_ref()
        .expect("writeback")
        .as_any()
        .downcast_ref()
        .expect("the work after the exit is the write-back")
}

const SHARED_CONFIG: &str = "\
# kept comment
[models]
default = \"kept-model\" # kept
default_reasoning_effort = \"high\"

[ui]
theme = \"dark\"

[plugins]
enabled = [\"kept-plugin\"]

[notes]
mark = 'stay'
";

/// Replaces the fixture's source `config.toml` with a link to a file holding `text`.
fn link_config(fixture: &mut SpecFixture, text: &str) -> PathBuf {
    let account = with_default_home(fixture);
    let shared = fixture.scratch.join("default-home");
    std::fs::create_dir_all(&shared).expect("default directory");
    std::fs::write(shared.join("config.toml"), text).expect("shared config");
    std::fs::remove_file(account.join("config.toml")).expect("replace config");
    std::os::unix::fs::symlink(shared.join("config.toml"), account.join("config.toml"))
        .expect("link");
    shared.join("config.toml")
}

fn session_config(plan: &LaunchPlan) -> PathBuf {
    PathBuf::from(&plan.env[0].1).join("config.toml")
}

/// A linked config above its size bound is not read back for a writeback, and its copy keeps the
/// user's text whole with the server appended on a line of its own.
#[test]
fn an_oversized_linked_config_is_kept_whole_and_not_written_back() {
    let mut fixture = spec_fixture();
    let text = format!("default_model = \"x\"\n# {}", "x".repeat(SMALL_FILE_LIMIT));
    link_config(&mut fixture, &text);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    assert!(plan.after_exit.is_none());
    let copy = std::fs::read_to_string(session_config(&plan)).expect("copy");
    let appended = copy.strip_prefix(&text).expect("the user's text first");
    assert!(appended.starts_with("\n\n[mcp_servers."), "{appended}");
}

#[test]
fn a_plain_config_is_not_written_back() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    assert!(plan.after_exit.is_none());
}

#[test]
fn a_dangling_config_link_is_not_written_back() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    std::fs::remove_file(home.join("config.toml")).expect("replace config");
    std::os::unix::fs::symlink(
        fixture.scratch.join("missing.toml"),
        home.join("config.toml"),
    )
    .expect("dangling link");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    assert!(plan.after_exit.is_none());
    assert!(std::fs::symlink_metadata(home.join("config.toml"))
        .expect("link")
        .file_type()
        .is_symlink());
}

#[test]
fn a_linked_config_gains_the_theme_the_plugin_switches_and_the_chosen_model() {
    use std::os::unix::fs::MetadataExt;

    let mut fixture = spec_fixture();
    fixture.model = Some("launch-model".to_string());
    let shared = link_config(&mut fixture, SHARED_CONFIG);
    let link = PathBuf::from(fixture.shell_env["GROK_HOME"].clone()).join("config.toml");
    let link_ino = std::fs::symlink_metadata(&link).expect("link").ino();
    let link_target = std::fs::read_link(&link).expect("target");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let session = session_config(&plan);
    assert!(!std::fs::symlink_metadata(&session)
        .expect("copy")
        .file_type()
        .is_symlink());
    assert!(std::fs::read_to_string(&session)
        .expect("copy")
        .contains("[mcp_servers.octoboard]"));
    assert_eq!(
        std::fs::read_to_string(&shared).expect("shared"),
        SHARED_CONFIG
    );
    let writeback = writeback_of(&plan);

    std::fs::write(
        &session,
        "\
[models]
default = \"other-model\"
default_reasoning_effort = \"low\"

[ui]
theme = \"tokyonight\"
max_thoughts_width = 120
yolo = false
compact_mode = false

[plugins]
enabled = []
disabled = [\"kept-plugin\"]

[marketplace]
default_skills_installs_purged = true

[mcp_servers.octoboard]
command = \"/usr/bin/true\"
args = []
tool_timeout_sec = 660
",
    )
    .expect("session config");
    writeback.publish();

    let updated = std::fs::read_to_string(&shared).expect("shared");
    let parsed: toml::Table = updated.parse().expect("shared config");
    assert_eq!(parsed["ui"]["theme"].as_str(), Some("tokyonight"));
    assert_eq!(parsed["models"]["default"].as_str(), Some("other-model"));
    assert_eq!(
        parsed["models"]["default_reasoning_effort"].as_str(),
        Some("high")
    );
    assert_eq!(
        parsed["plugins"]["enabled"]
            .as_array()
            .expect("enabled")
            .len(),
        0
    );
    assert_eq!(
        parsed["plugins"]["disabled"][0].as_str(),
        Some("kept-plugin")
    );
    assert!(parsed.get("marketplace").is_none(), "{updated}");
    assert!(
        parsed["ui"].get("max_thoughts_width").is_none(),
        "{updated}"
    );
    assert!(!updated.contains("mcp_servers"), "{updated}");
    assert!(!updated.contains("launch-model"), "{updated}");
    assert!(updated.contains("# kept comment"), "{updated}");
    assert!(updated.contains("# kept"), "{updated}");
    assert!(updated.contains("mark = 'stay'"), "{updated}");
    let after = std::fs::symlink_metadata(&link).expect("link");
    assert!(after.file_type().is_symlink());
    assert_eq!(after.ino(), link_ino);
    assert_eq!(std::fs::read_link(&link).expect("target"), link_target);
}

#[test]
fn the_launch_model_and_startup_writes_stay_in_the_session_copy() {
    use std::os::unix::fs::MetadataExt;

    let mut fixture = spec_fixture();
    fixture.model = Some("launch-model".to_string());
    let shared = link_config(&mut fixture, SHARED_CONFIG);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let before_ino = std::fs::metadata(&shared).expect("shared").ino();
    std::fs::write(
        session_config(&plan),
        "\
[models]
default = \"launch-model\"
default_reasoning_effort = \"low\"

[ui]
theme = \"dark\"
max_thoughts_width = 120
yolo = false
compact_mode = false

[plugins]
enabled = [\"kept-plugin\"]

[marketplace]
default_skills_installs_purged = true

[mcp_servers.octoboard]
command = \"/usr/bin/true\"
",
    )
    .expect("session config");
    writeback_of(&plan).publish();
    assert_eq!(
        std::fs::read_to_string(&shared).expect("shared"),
        SHARED_CONFIG
    );
    assert_eq!(
        std::fs::metadata(&shared).expect("shared").ino(),
        before_ino
    );
}

#[test]
fn a_model_chosen_without_a_launch_model_is_written_back() {
    let mut fixture = spec_fixture();
    let shared = link_config(&mut fixture, SHARED_CONFIG);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let mut session = std::fs::read_to_string(session_config(&plan)).expect("copy");
    session = session.replace("kept-model", "other-model");
    std::fs::write(session_config(&plan), session).expect("session config");
    writeback_of(&plan).publish();
    let parsed: toml::Table = std::fs::read_to_string(&shared)
        .expect("shared")
        .parse()
        .expect("shared config");
    assert_eq!(parsed["models"]["default"].as_str(), Some("other-model"));
    assert_eq!(
        parsed["models"]["default_reasoning_effort"].as_str(),
        Some("high")
    );
}

#[test]
fn a_chain_of_links_is_written_at_its_end_and_both_links_stay() {
    use std::os::unix::fs::MetadataExt;

    let mut fixture = spec_fixture();
    let account = with_default_home(&mut fixture);
    let real_dir = fixture.scratch.join("dotfiles");
    std::fs::create_dir_all(&real_dir).expect("dotfiles");
    let real = real_dir.join("config.toml");
    std::fs::write(&real, SHARED_CONFIG).expect("real config");
    let middle = fixture.scratch.join("middle.toml");
    std::os::unix::fs::symlink(&real, &middle).expect("middle link");
    std::fs::remove_file(account.join("config.toml")).expect("replace config");
    std::os::unix::fs::symlink(&middle, account.join("config.toml")).expect("account link");
    let account_ino = std::fs::symlink_metadata(account.join("config.toml"))
        .expect("link")
        .ino();
    let middle_ino = std::fs::symlink_metadata(&middle).expect("link").ino();

    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let mut session = std::fs::read_to_string(session_config(&plan)).expect("copy");
    session = session.replace("theme = \"dark\"", "theme = \"tokyonight\"");
    std::fs::write(session_config(&plan), session).expect("session config");
    writeback_of(&plan).publish();

    let parsed: toml::Table = std::fs::read_to_string(&real)
        .expect("real")
        .parse()
        .expect("real config");
    assert_eq!(parsed["ui"]["theme"].as_str(), Some("tokyonight"));
    assert_eq!(
        std::fs::read_link(account.join("config.toml")).expect("account link"),
        middle
    );
    assert_eq!(std::fs::read_link(&middle).expect("middle link"), real);
    assert_eq!(
        std::fs::symlink_metadata(account.join("config.toml"))
            .expect("link")
            .ino(),
        account_ino
    );
    assert_eq!(
        std::fs::symlink_metadata(&middle).expect("link").ino(),
        middle_ino
    );
}

#[test]
fn a_link_replaced_before_exit_is_left_alone() {
    let mut fixture = spec_fixture();
    let shared = link_config(&mut fixture, SHARED_CONFIG);
    let link = PathBuf::from(fixture.shell_env["GROK_HOME"].clone()).join("config.toml");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    std::fs::remove_file(&link).expect("unlink");
    std::fs::write(&link, "own = true\n").expect("plain file");
    let mut session = std::fs::read_to_string(session_config(&plan)).expect("copy");
    session = session.replace("theme = \"dark\"", "theme = \"tokyonight\"");
    std::fs::write(session_config(&plan), session).expect("session config");
    writeback_of(&plan).publish();
    assert_eq!(
        std::fs::read_to_string(&link).expect("account file"),
        "own = true\n"
    );
    assert_eq!(
        std::fs::read_to_string(&shared).expect("shared"),
        SHARED_CONFIG
    );
}

#[test]
fn a_link_redirected_or_broken_before_exit_is_not_written_back() {
    for dangling in [false, true] {
        let mut fixture = spec_fixture();
        let shared = link_config(&mut fixture, SHARED_CONFIG);
        let link = PathBuf::from(&fixture.shell_env["GROK_HOME"]).join("config.toml");
        let replacement = fixture.scratch.join("replacement.toml");
        if !dangling {
            std::fs::write(&replacement, "other = true\n").expect("other config");
        }
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        std::fs::write(session_config(&plan), "[ui]\ntheme = \"tokyonight\"\n")
            .expect("session change");
        std::fs::remove_file(&link).expect("unlink");
        std::os::unix::fs::symlink(&replacement, &link).expect("redirect link");
        writeback_of(&plan).publish();
        assert_eq!(std::fs::read_to_string(&shared).unwrap(), SHARED_CONFIG);
        assert_eq!(std::fs::read_link(&link).unwrap(), replacement);
        if dangling {
            assert!(!replacement.exists());
        } else {
            assert_eq!(
                std::fs::read_to_string(replacement).unwrap(),
                "other = true\n"
            );
        }
    }
}

#[test]
fn concurrent_exits_merge_different_fields_into_the_latest_shared_file() {
    use std::sync::{Arc, Barrier};

    let mut first = spec_fixture();
    let shared = link_config(&mut first, SHARED_CONFIG);
    let mut second = spec_fixture();
    second.shell_env = first.shell_env.clone();
    let theme = plan_isolated(&first.spec()).expect("theme session");
    let model = plan_isolated(&second.spec()).expect("model session");
    let theme_text = std::fs::read_to_string(session_config(&theme)).unwrap();
    let model_text = std::fs::read_to_string(session_config(&model)).unwrap();
    std::fs::write(
        session_config(&theme),
        theme_text.replace("theme = \"dark\"", "theme = \"tokyonight\""),
    )
    .unwrap();
    std::fs::write(
        session_config(&model),
        model_text.replace("kept-model", "other-model"),
    )
    .unwrap();
    // An editor's already-saved change is also retained by the read at publication time.
    std::fs::write(
        &shared,
        format!("{SHARED_CONFIG}\n[external]\nkeep = true\n"),
    )
    .unwrap();
    let ready = Arc::new(Barrier::new(3));
    std::thread::scope(|scope| {
        for plan in [theme, model] {
            let ready = ready.clone();
            scope.spawn(move || {
                ready.wait();
                writeback_of(&plan).publish();
            });
        }
        ready.wait();
    });
    let current: toml::Table = std::fs::read_to_string(shared).unwrap().parse().unwrap();
    assert_eq!(current["ui"]["theme"].as_str(), Some("tokyonight"));
    assert_eq!(current["models"]["default"].as_str(), Some("other-model"));
    assert_eq!(current["external"]["keep"].as_bool(), Some(true));
    assert!(current.get("mcp_servers").is_none());
}

#[test]
fn the_last_exit_that_changed_a_field_wins_but_an_unchanged_session_does_not() {
    let mut first = spec_fixture();
    let shared = link_config(&mut first, SHARED_CONFIG);
    let mut second = spec_fixture();
    second.shell_env = first.shell_env.clone();
    let mut unchanged = spec_fixture();
    unchanged.shell_env = first.shell_env.clone();
    let plans = [
        plan_isolated(&first.spec()).unwrap(),
        plan_isolated(&second.spec()).unwrap(),
        plan_isolated(&unchanged.spec()).unwrap(),
    ];
    for (plan, theme) in plans.into_iter().zip(["tokyonight", "grokday", "dark"]) {
        let session = session_config(&plan);
        let text = std::fs::read_to_string(&session).unwrap();
        std::fs::write(
            &session,
            text.replace("theme = \"dark\"", &format!("theme = \"{theme}\"")),
        )
        .unwrap();
        writeback_of(&plan).publish();
    }
    let current: toml::Table = std::fs::read_to_string(shared).unwrap().parse().unwrap();
    assert_eq!(current["ui"]["theme"].as_str(), Some("grokday"));
}

#[test]
fn an_invalid_session_config_leaves_the_shared_file_untouched() {
    let mut fixture = spec_fixture();
    let shared = link_config(&mut fixture, SHARED_CONFIG);
    let plan = plan_isolated(&fixture.spec()).unwrap();
    std::fs::write(session_config(&plan), "[ui]\ntheme = \"").unwrap();
    assert!(writeback_of(&plan).publish_locked().is_err());
    assert_eq!(std::fs::read_to_string(shared).unwrap(), SHARED_CONFIG);
}

#[test]
fn a_section_that_is_not_a_table_is_left_unchanged() {
    let mut fixture = spec_fixture();
    let shared = link_config(&mut fixture, "ui = \"dark\"\n");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    std::fs::write(session_config(&plan), "[ui]\ntheme = \"tokyonight\"\n")
        .expect("session config");
    writeback_of(&plan).publish();
    assert_eq!(
        std::fs::read_to_string(&shared).expect("shared"),
        "ui = \"dark\"\n"
    );
}

#[test]
fn bounded_agent_files_do_not_hold_grok_writeback() {
    use crate::adapter::tests::{agent_file, bounded_file_cases};
    bounded_file_cases(
        "adapter::grok::writeback_tests::bounded_agent_files_do_not_hold_grok_writeback",
        &[
            "shared-regular",
            "session-linked",
            "shared-fifo",
            "session-fifo",
            "shared-oversized",
            "session-oversized",
        ],
        |case| {
            let (which, kind) = case.split_once('-').unwrap();
            let mut fixture = spec_fixture();
            let shared = link_config(&mut fixture, SHARED_CONFIG);
            let plan = plan_isolated(&fixture.spec()).unwrap();
            let session = session_config(&plan);
            let changed = SHARED_CONFIG.replace("dark", "tokyonight");
            std::fs::write(&session, &changed).unwrap();
            let path = if which == "shared" { &shared } else { &session };
            let text = if which == "shared" {
                SHARED_CONFIG
            } else {
                &changed
            };
            agent_file(path, kind, text, SMALL_FILE_LIMIT);
            let writeback = writeback_of(&plan);
            let readable = matches!(kind, "regular" | "linked");
            if !readable {
                let error = writeback.publish_locked().unwrap_err();
                assert!(
                    format!("{error:#}").contains(if kind == "fifo" {
                        "not a regular file"
                    } else {
                        "file too large"
                    }),
                    "{error:#}"
                );
            }
            writeback.publish();
            if readable {
                assert!(std::fs::read_to_string(&shared)
                    .unwrap()
                    .contains("tokyonight"));
            } else if which == "session" {
                assert_eq!(std::fs::read_to_string(&shared).unwrap(), SHARED_CONFIG);
            } else if kind == "oversized" {
                assert_eq!(
                    std::fs::metadata(&shared).unwrap().len(),
                    (SHARED_CONFIG.len() + SMALL_FILE_LIMIT) as u64
                );
            } else {
                use std::os::unix::fs::FileTypeExt;
                assert!(std::fs::symlink_metadata(&shared)
                    .unwrap()
                    .file_type()
                    .is_fifo());
            }
            std::fs::remove_dir_all(session.parent().unwrap()).unwrap();
            assert!(!session.parent().unwrap().exists());
        },
    );
}
