//! Tests of Antigravity CLI's launch plan: its arguments, its environment, the task going in as the
//! first prompt or as a first message, and the refusal of a CLI that does not take the flag naming
//! the launch's directory. What the launch's directory holds is tested with `home`.
//!
//! No test reads or writes the developer's own `~/.gemini` or runs the real `agy`: every launch
//! gets a scratch home through its shell environment, and the probe of the flag runs a stand-in
//! (`test_support::fake_agent`) or, with no `PATH` in the environment, finds no program at all.

use std::path::PathBuf;

use serde_json::{json, Value};

use super::*;
use crate::adapter::tests::{spec_fixture, SpecFixture};
use crate::protocol::{Code, CodedError};
use crate::providers::antigravity::{
    LAUNCH_DIRECTORY_FLAG_NOT_TAKEN, MEASURED_VERSION, SETTINGS_FILE,
};
use crate::providers::{ProviderSettings, SettingValue};
use crate::test_support::fake_agent;

const RESUMED: &str = "d7372c8f-aef5-4628-b74c-10a183d622bc";

/// A launch fixture whose home directory, the one the CLI reads `.gemini` under, is a directory of
/// its own in the scratch directory, and whose working directory is spelled canonically (the
/// CLI's trust is only read for a spelling with no link in it). It has no `PATH`, so the probe of
/// the directory flag finds no program to run.
pub(super) fn isolated() -> SpecFixture {
    let mut fixture = spec_fixture();
    let home = fixture.scratch.join("home");
    std::fs::create_dir_all(&home).expect("home directory");
    fixture
        .shell_env
        .insert("HOME".to_string(), home.to_string_lossy().into_owned());
    fixture.cwd = crate::hostfs::canonical(&fixture.cwd).expect("canonical working directory");
    fixture
}

/// The directory the CLI keeps under the fixture's home.
pub(super) fn user_gemini(fixture: &SpecFixture) -> PathBuf {
    fixture.scratch.join("home").join(".gemini")
}

/// The CLI's settings file under the fixture's home, with the folders the user trusted.
fn trust(fixture: &SpecFixture, folders: &[&std::path::Path]) {
    let dir = user_gemini(fixture).join(crate::providers::antigravity::APP_DATA_DIR);
    std::fs::create_dir_all(&dir).expect("data directory");
    let listed: Vec<String> = folders
        .iter()
        .map(|folder| folder.to_string_lossy().into_owned())
        .collect();
    std::fs::write(
        dir.join(SETTINGS_FILE),
        json!({ "trustedWorkspaces": listed }).to_string(),
    )
    .expect("settings");
}

fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let at = args.iter().position(|arg| arg == flag)?;
    args.get(at + 1).map(String::as_str)
}

fn choose(fixture: &mut SpecFixture, mode: &str) {
    fixture.settings = ProviderSettings::new([(
        "approval_mode".to_string(),
        Some(SettingValue::Choice(mode.to_string())),
    )]);
}

/// The launch's settings: what the file named after `--settings` holds.
fn settings_of(plan: &LaunchPlan) -> Value {
    let path = value_after(&plan.args, "--settings").expect("a settings file");
    serde_json::from_str(&std::fs::read_to_string(path).expect("the settings file"))
        .expect("the settings are JSON")
}

fn launch_dir(fixture: &SpecFixture) -> String {
    fixture
        .scratch
        .join("antigravity")
        .to_string_lossy()
        .into_owned()
}

/// The flag that names the launch's directory comes first, joined to its value with `=` (the
/// spelling the CLI takes, measured), and `--settings` follows with a file of that directory, which
/// holds the rule that lets Octoboard's server run with no prompt, for that server only, and the
/// survey off. No argument holds the settings themselves.
#[test]
fn the_launch_names_its_directory_first_and_gives_its_settings_for_the_launch_alone() {
    let fixture = isolated();
    let plan = AntigravityProvider.plan(&fixture.spec()).expect("plan");
    assert_eq!(
        plan.args[0],
        format!("--gemini_dir={}", launch_dir(&fixture))
    );
    assert_eq!(plan.args[1], "--settings");
    assert_eq!(
        std::path::Path::new(&plan.args[2]).parent(),
        Some(std::path::Path::new(&launch_dir(&fixture)))
    );
    assert!(
        plan.args.iter().all(|arg| !arg.contains("permissions")),
        "{:?}",
        plan.args
    );
    // Beside the title command, which `every_launch_gives_its_own_title_command` checks.
    let settings = without_title(settings_of(&plan));
    assert_eq!(
        settings,
        json!({
            "permissions": { "allow": ["mcp(octoboard/*)"] },
            "showFeedbackSurvey": false,
        })
    );
    // Nothing else is chosen where nothing was asked: no conversation, model, effort or mode.
    assert_eq!(plan.args.len(), 3, "{:?}", plan.args);
}

/// The status reports are switched on by one variable and nothing else is set.
#[test]
fn the_environment_switches_the_status_reports_on_and_sets_nothing_else() {
    let fixture = isolated();
    let plan = AntigravityProvider.plan(&fixture.spec()).expect("plan");
    assert_eq!(
        plan.env,
        [("AGY_CLI_OSC7501".to_string(), "on".to_string())]
    );
}

/// A resume names the conversation; a new launch names none, since the CLI gives it its id.
#[test]
fn a_resume_names_the_conversation_and_a_new_launch_names_none() {
    let fixture = isolated();
    let plan = AntigravityProvider.plan(&fixture.spec()).expect("plan");
    assert!(!plan.args.iter().any(|arg| arg.contains("conversation")));

    let mut spec = fixture.spec();
    spec.resume_agent_session_id = Some(RESUMED);
    let plan = AntigravityProvider.plan(&spec).expect("plan");
    assert_eq!(value_after(&plan.args, "--conversation"), Some(RESUMED));
}

/// The effort is passed beside a model the catalogue lists by its base id, and alone where no
/// model is chosen (the CLI applies it to its default model, 1.5.14); an id that carries a level of
/// its own takes none (the CLI refuses another level beside it).
#[test]
fn the_effort_goes_with_a_base_model_or_alone_and_never_with_a_level_in_the_id() {
    for (model, effort, expected) in [
        (Some("gemini-3.8-flash"), Some("high"), Some("high")),
        (Some("gemini-4-neon"), Some("xhigh"), Some("xhigh")),
        (Some("gemini-3.8-flash"), None, None),
        (Some("gemini-3.8-flash-low"), Some("high"), None),
        (Some("claude-sonnet-4-6"), None, None),
        (None, Some("high"), Some("high")),
    ] {
        let mut fixture = isolated();
        fixture.model = model.map(str::to_string);
        fixture.effort = effort.map(str::to_string);
        let plan = AntigravityProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(value_after(&plan.args, "--model"), model, "{model:?}");
        assert_eq!(
            value_after(&plan.args, "--effort"),
            expected,
            "{model:?} {effort:?}"
        );
    }

    let fixture = isolated();
    let plan = AntigravityProvider.plan(&fixture.spec()).expect("plan");
    assert!(!plan
        .args
        .iter()
        .any(|arg| arg == "--model" || arg == "--effort"));
}

/// The approval setting becomes the CLI's own flag: accept edits is a mode, skipping permissions
/// is the flag of its own, and no choice, or a value the setting does not take, passes neither so
/// the CLI's own configuration decides. The same on a resume.
#[test]
fn the_approval_mode_becomes_the_clis_own_flag() {
    for resume in [None, Some(RESUMED)] {
        let plan_for = |mode: Option<&str>| {
            let mut fixture = isolated();
            if let Some(mode) = mode {
                choose(&mut fixture, mode);
            }
            let mut spec = fixture.spec();
            spec.resume_agent_session_id = resume;
            AntigravityProvider.plan(&spec).expect("plan").args
        };
        let accepted = plan_for(Some("accept_edits"));
        assert_eq!(value_after(&accepted, "--mode"), Some("accept-edits"));
        assert!(!accepted.iter().any(|arg| arg.contains("dangerously")));

        let auto = plan_for(Some("auto"));
        assert_eq!(value_after(&auto, "--mode"), Some("auto"));
        assert!(!auto.iter().any(|arg| arg.contains("dangerously")));

        let skipped = plan_for(Some("skip_permissions"));
        assert!(skipped.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(!skipped.iter().any(|arg| arg == "--mode"));

        // Plan mode is not offered: it was measured to let an edit through to its permission
        // prompt on the first model call (1.5.14).
        for none in [plan_for(None), plan_for(Some("plan"))] {
            assert!(!none.iter().any(|arg| arg == "--mode"), "{none:?}");
            assert!(
                !none.iter().any(|arg| arg.contains("dangerously")),
                "{none:?}"
            );
        }
    }
}

/// The task is the session's first message, never the CLI's own first prompt, in a folder the CLI
/// trusts as in one it does not: a prompt given at launch was measured to run before the CLI
/// offers the tools of its MCP servers (the first model request of `--prompt-interactive` holds
/// no `call_mcp_tool`, 1.5.14), and behind the trust screen where there is one. Whatever the task
/// starts with, no argument carries it.
#[test]
fn the_task_is_the_first_message_in_a_trusted_folder_as_in_any_other() {
    for trusted in [true, false] {
        for task in [
            "fix the build",
            "- fix the build\n- add the tests",
            "--model looks like a flag",
            "first line\n\nsecond line\n",
        ] {
            let fixture = isolated();
            if trusted {
                trust(&fixture, &[&fixture.cwd]);
            }
            let mut spec = fixture.spec();
            spec.task = Some(task);
            let plan = AntigravityProvider.plan(&spec).expect("plan");
            assert!(
                !plan
                    .args
                    .iter()
                    .any(|arg| arg.starts_with("--prompt") || arg == "-i" || arg.contains(task)),
                "trusted={trusted} {task:?}: {:?}",
                plan.args
            );
            assert_eq!(
                plan.first_message.as_deref(),
                Some(task),
                "trusted={trusted} {task:?}"
            );
        }
    }
}

/// A resume never repeats the first task, whether the folder is trusted or not.
#[test]
fn a_resume_does_not_repeat_the_task() {
    for trusted in [true, false] {
        let fixture = isolated();
        if trusted {
            trust(&fixture, &[&fixture.cwd]);
        }
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        spec.task = Some("first task");
        let plan = AntigravityProvider.plan(&spec).expect("plan");
        assert_eq!(plan.first_message, None, "trusted={trusted}");
        assert!(
            !plan
                .args
                .iter()
                .any(|arg| arg.starts_with("--prompt") || arg.contains("first task")),
            "trusted={trusted}: {:?}",
            plan.args
        );
    }
}

/// A launch with no task passes no prompt and has no first message, trusted folder or not.
#[test]
fn a_launch_without_a_task_has_no_prompt() {
    let fixture = isolated();
    trust(&fixture, &[&fixture.cwd]);
    let plan = AntigravityProvider.plan(&fixture.spec()).expect("plan");
    assert!(!plan.args.iter().any(|arg| arg.starts_with("--prompt")));
    assert_eq!(plan.first_message, None);
}

/// The line an `agy` command writes on its standard error for a flag it does not define.
const UNDEFINED_FLAG_LINE: &str = "flags provided but not defined: -gemini_dir";

/// Makes the fixture's `agy` a stand-in that ends with `status` after writing `stderr` (when it is
/// not empty) on its standard error, and records, in files of the home directory it returns the
/// paths of: the last run's arguments, one a line, then `built` when the launch's directory
/// already held its hooks file and `update=<value>` of the variable that turns the CLI's own
/// update off; and one line a run, to count them.
fn probing(fixture: &mut SpecFixture, status: i32, stderr: &str) -> (PathBuf, PathBuf) {
    let home = fixture.scratch.join("home");
    let record = home.join("probe-record");
    let runs = home.join("probe-runs");
    let say = if stderr.is_empty() {
        String::new()
    } else {
        format!("printf '%s\\n' '{stderr}' >&2\n")
    };
    let body = format!(
        "echo run >> '{runs}'\n\
         printf '%s\\n' \"$@\" > '{record}'\n\
         dir=\"${{1#--gemini_dir=}}\"\n\
         [ -f \"$dir/config/hooks.json\" ] && echo built >> '{record}'\n\
         echo \"update=$AGY_CLI_DISABLE_AUTO_UPDATE\" >> '{record}'\n\
         {say}exit {status}\n",
        record = record.display(),
        runs = runs.display()
    );
    fixture.shell_env = fake_agent(&home, "agy", &body);
    (record, runs)
}

/// How many times the stand-in ran.
fn runs_of(runs: &std::path::Path) -> usize {
    std::fs::read_to_string(runs).map_or(0, |text| text.lines().count())
}

/// A CLI that does not define the flag ends any command with status 2 and a line naming the flag
/// on its standard error before it does anything else: the launch is refused with the provider's
/// code, naming the version that takes the flag, before a terminal is up. The probe is
/// `agy --gemini_dir=<dir> mcp list` over the directory the launch built, run with the CLI's own
/// update turned off, while the session's own environment is given nothing of the kind.
#[test]
fn a_cli_that_does_not_take_the_directory_flag_refuses_the_launch() {
    let mut fixture = isolated();
    let (record, _) = probing(&mut fixture, 2, UNDEFINED_FLAG_LINE);
    let err = AntigravityProvider
        .plan(&fixture.spec())
        .err()
        .expect("refused");
    let coded = err.downcast_ref::<CodedError>().expect("coded");
    assert_eq!(
        coded.code,
        Code::of_provider(LAUNCH_DIRECTORY_FLAG_NOT_TAKEN)
    );
    assert_eq!(
        coded.params.get("version").map(String::as_str),
        Some(MEASURED_VERSION)
    );
    assert!(coded.message.contains("--gemini_dir"), "{}", coded.message);
    assert!(
        coded.message.contains(MEASURED_VERSION),
        "{}",
        coded.message
    );

    let recorded = std::fs::read_to_string(record).expect("the stand-in ran");
    assert_eq!(
        recorded.lines().collect::<Vec<_>>(),
        [
            format!("--gemini_dir={}", launch_dir(&fixture)).as_str(),
            "mcp",
            "list",
            "built",
            "update=true"
        ]
    );
}

/// The status and the line together say the flag is not defined: status 2 is what every error of
/// usage gives, so it alone refuses nothing, and the line with another status refuses nothing
/// either. A CLI that takes the flag and ends 0, one that fails some other way, and a probe that
/// cannot run at all (no program on the `PATH`) say nothing about the flag: the launch is its own to
/// fail or not.
#[test]
fn only_the_status_and_the_line_together_refuse_the_launch() {
    let cases = [
        (0, ""),
        (1, ""),
        (127, ""),
        (2, ""),
        (2, "Error: some other usage error"),
        (1, UNDEFINED_FLAG_LINE),
        (0, UNDEFINED_FLAG_LINE),
        (3, UNDEFINED_FLAG_LINE),
    ];
    for (status, stderr) in cases {
        let mut fixture = isolated();
        probing(&mut fixture, status, stderr);
        AntigravityProvider
            .plan(&fixture.spec())
            .unwrap_or_else(|err| panic!("status {status} {stderr:?} refused: {err}"));
    }
    // `isolated` has no `PATH`: the program is found nowhere.
    let fixture = isolated();
    AntigravityProvider
        .plan(&fixture.spec())
        .expect("a probe that cannot run does not refuse");
}

/// The outcome of a probe is kept for the binary and its version, whatever it was: the same
/// binary at the same version is run once however many launches follow, and a new version, which
/// an update of the CLI makes, is probed again. A probe that says nothing is kept as well as a
/// refusal, and a kept refusal still refuses.
#[test]
fn a_probe_is_run_once_for_a_binary_and_a_version() {
    for status in [0, 1] {
        let mut fixture = isolated();
        let (_, runs) = probing(&mut fixture, status, "");
        for _ in 0..2 {
            AntigravityProvider
                .plan(&fixture.spec())
                .expect("a probe that says nothing does not refuse");
        }
        assert_eq!(runs_of(&runs), 1, "status {status}");

        for (version, probed) in [((1, 5, 14), 2), ((1, 6, 0), 3)] {
            fixture.agent_version = Some(version);
            for _ in 0..2 {
                AntigravityProvider.plan(&fixture.spec()).expect("plan");
            }
            assert_eq!(runs_of(&runs), probed, "status {status} {version:?}");
        }
    }

    let mut fixture = isolated();
    let (_, runs) = probing(&mut fixture, 2, UNDEFINED_FLAG_LINE);
    for _ in 0..2 {
        let err = AntigravityProvider
            .plan(&fixture.spec())
            .err()
            .expect("refused again from what was kept");
        assert_eq!(
            err.downcast_ref::<CodedError>().expect("coded").code,
            Code::of_provider(LAUNCH_DIRECTORY_FLAG_NOT_TAKEN)
        );
    }
    assert_eq!(runs_of(&runs), 1);

    // Another binary is probed on its own, whatever the version.
    let mut other = isolated();
    let (_, other_runs) = probing(&mut other, 0, "");
    AntigravityProvider.plan(&other.spec()).expect("plan");
    assert_eq!(runs_of(&other_runs), 1);
}

/// A refusal of a user's file comes before the probe: the plan fails on the file whatever the CLI
/// would have said, and the launch's directory is not left behind.
#[test]
fn an_unusable_user_file_refuses_before_the_probe() {
    let mut fixture = isolated();
    let (_, runs) = probing(&mut fixture, 2, UNDEFINED_FLAG_LINE);
    let config = user_gemini(&fixture).join("config");
    std::fs::create_dir_all(&config).expect("config directory");
    std::fs::write(config.join("hooks.json"), "{ // not strict\n}").expect("file");
    let err = AntigravityProvider
        .plan(&fixture.spec())
        .err()
        .expect("refused");
    let coded = err.downcast_ref::<CodedError>().expect("coded");
    assert_eq!(
        coded.code,
        Code::of_provider(crate::providers::antigravity::USER_CONFIG_UNUSABLE)
    );
    assert_eq!(runs_of(&runs), 0, "the probe ran");
}

/// What the launch's directory found to tell the user is the plan's notice: the settings file the
/// CLI does not read, with which the launch still starts, its settings holding Octoboard's rule
/// alone. A launch with nothing to tell has none, and no further notice either.
#[test]
fn the_plan_carries_the_notice_of_a_settings_file_the_cli_does_not_read() {
    let fixture = isolated();
    let plan = AntigravityProvider.plan(&fixture.spec()).expect("plan");
    assert!(plan.notice.is_none());
    assert!(AntigravityProvider
        .further_launch_notices(&fixture.spec(), &plan)
        .is_empty());

    let fixture = isolated();
    let data = user_gemini(&fixture).join(crate::providers::antigravity::APP_DATA_DIR);
    std::fs::create_dir_all(&data).expect("data directory");
    std::fs::write(
        data.join(SETTINGS_FILE),
        r#"{"permissions":{"allow":["Bash(ls:*)"]}, // mine
        }"#,
    )
    .expect("settings");
    let plan = AntigravityProvider.plan(&fixture.spec()).expect("plan");
    let notice = plan.notice.as_ref().expect("a notice");
    assert_eq!(
        notice.code,
        Code::of_provider(crate::providers::antigravity::SETTINGS_NOT_READ)
    );
    assert_eq!(
        settings_of(&plan)["permissions"]["allow"],
        json!(["mcp(octoboard/*)"])
    );
}

/// The codes of what a launch of `fixture` tells the user: the plan's notice, then the further
/// ones, as the launch shows them.
fn told_at_launch(fixture: &SpecFixture) -> Vec<Code> {
    let spec = fixture.spec();
    let plan = AntigravityProvider.plan(&spec).expect("plan");
    let further = AntigravityProvider.further_launch_notices(&spec, &plan);
    plan.notice
        .into_iter()
        .chain(further)
        .map(|notice| notice.code)
        .collect()
}

/// Rules an earlier version left behind are a further notice of the launch, beside the plan's
/// own: alone where the settings file is read, and second, after the settings notice, where it is
/// not, so that neither hides the other.
#[test]
fn leftovers_are_a_further_notice_beside_the_settings_one() {
    use crate::providers::antigravity::{EARLIER_LAYOUT_LEFTOVERS_NOT_READ, SETTINGS_NOT_READ};

    let with_leftovers = || {
        let fixture = isolated();
        let gemini = user_gemini(&fixture);
        let rules = gemini
            .join(crate::providers::antigravity::APP_DATA_DIR)
            .join("rules");
        std::fs::create_dir_all(&rules).expect("rules left behind");
        std::fs::write(rules.join("mine.md"), "mine").expect("a rule");
        std::fs::create_dir_all(gemini.join("config")).expect("configuration directory");
        std::fs::write(gemini.join("config").join(".migrated"), "").expect("marker");
        fixture
    };

    let fixture = with_leftovers();
    assert_eq!(
        told_at_launch(&fixture),
        [Code::of_provider(EARLIER_LAYOUT_LEFTOVERS_NOT_READ)]
    );

    let fixture = with_leftovers();
    std::fs::write(
        user_gemini(&fixture)
            .join(crate::providers::antigravity::APP_DATA_DIR)
            .join(SETTINGS_FILE),
        "{ not json",
    )
    .expect("settings");
    assert_eq!(
        told_at_launch(&fixture),
        [
            Code::of_provider(SETTINGS_NOT_READ),
            Code::of_provider(EARLIER_LAYOUT_LEFTOVERS_NOT_READ)
        ]
    );
}

/// The launch holds what is written into the session until Octoboard's server has answered the
/// CLI's listing of its tools, for the bound measured, whatever the launch: the CLI offers the
/// model `call_mcp_tool` only from then on.
#[test]
fn every_launch_waits_for_the_tool_listing_up_to_the_bound() {
    let fixture = isolated();
    for task in [None, Some("a first task")] {
        let mut spec = fixture.spec();
        spec.task = task;
        let plan = AntigravityProvider.plan(&spec).expect("plan");
        assert_eq!(
            AntigravityProvider.awaits_tool_listing(&spec, &plan),
            Some(TOOL_LISTING_WAIT),
            "{task:?}"
        );
    }
    // Several times the slowest listing measured (3.1 s), and short enough that a server the
    // CLI never asks holds a first message for seconds, not minutes.
    assert!(TOOL_LISTING_WAIT >= std::time::Duration::from_secs(10));
    assert!(TOOL_LISTING_WAIT <= std::time::Duration::from_secs(30));
}

/// An account with a directory of its own is refused before the probe: the CLI has the default
/// account only.
#[test]
fn an_account_with_a_directory_of_its_own_refuses_before_the_probe() {
    let mut fixture = isolated();
    let (_, runs) = probing(&mut fixture, 0, "");
    let account = fixture.scratch.join("account").join(".gemini");
    std::fs::create_dir_all(&account).expect("account directory");
    fixture.config_dir = Some(account);
    let err = AntigravityProvider
        .plan(&fixture.spec())
        .err()
        .expect("refused");
    let coded = err.downcast_ref::<CodedError>().expect("coded");
    assert_eq!(
        coded.code,
        crate::protocol::error_code::PROVIDER_UNSUPPORTED
    );
    assert_eq!(runs_of(&runs), 0, "the probe ran");
}

/// A home whose earlier layout the CLI has not yet migrated refuses before the probe as well, and
/// leaves no launch directory.
#[test]
fn an_earlier_layout_not_yet_migrated_refuses_before_the_probe() {
    let mut fixture = isolated();
    let (_, runs) = probing(&mut fixture, 2, UNDEFINED_FLAG_LINE);
    let data = user_gemini(&fixture).join("antigravity-cli");
    std::fs::create_dir_all(data.join("skills")).expect("an earlier layout's directory");
    let err = AntigravityProvider
        .plan(&fixture.spec())
        .err()
        .expect("refused");
    let coded = err.downcast_ref::<CodedError>().expect("coded");
    assert_eq!(
        coded.code,
        Code::of_provider(crate::providers::antigravity::EARLIER_LAYOUT_NOT_MIGRATED)
    );
    assert_eq!(runs_of(&runs), 0, "the probe ran");
    assert!(!std::path::Path::new(&launch_dir(&fixture)).exists());
}

/// The user's own `permissions.allow` rules go first in the launch's settings, which stand in
/// place of the user's list, and Octoboard's rule once after them, whether or not the user had
/// it; their `deny` and `ask` lists are not passed (the CLI keeps them), nor anything else of
/// their settings. The rules are in the settings file only, never in an argument.
#[test]
fn the_users_allow_rules_come_first_and_octoboards_rule_once_after_them() {
    let ours = "mcp(octoboard/*)";
    for (user, expected) in [
        (vec![], vec![ours]),
        (
            vec!["Bash(git status:*)", "mcp(other/*)"],
            vec!["Bash(git status:*)", "mcp(other/*)", ours],
        ),
        (vec![ours, "Bash(ls:*)"], vec!["Bash(ls:*)", ours]),
        (vec!["Bash(ls:*)", ours, ours], vec!["Bash(ls:*)", ours]),
    ] {
        let fixture = isolated();
        let data = user_gemini(&fixture).join("antigravity-cli");
        std::fs::create_dir_all(&data).expect("data directory");
        std::fs::write(
            data.join("settings.json"),
            json!({
                "permissions": {
                    "allow": user,
                    "deny": ["Bash(rm:*)"],
                    "ask": ["Bash(curl:*)"],
                },
                "theme": "dark",
            })
            .to_string(),
        )
        .expect("settings");
        let plan = AntigravityProvider.plan(&fixture.spec()).expect("plan");
        assert!(
            plan.args.iter().all(|arg| !arg.contains("Bash(")),
            "{:?}",
            plan.args
        );
        assert_eq!(
            without_title(settings_of(&plan)),
            json!({ "permissions": { "allow": expected }, "showFeedbackSurvey": false }),
            "{user:?}"
        );
    }

    // The same from the settings alone.
    assert_eq!(
        serde_json::from_str::<Value>(&AntigravityProvider::launch_settings(
            &[json!("a")],
            json!("t"),
            None,
        ))
        .expect("JSON"),
        json!({ "permissions": { "allow": ["a", ours] }, "showFeedbackSurvey": false, "title": "t" })
    );
    // The title command goes in under its key.
    assert_eq!(
        serde_json::from_str::<Value>(&AntigravityProvider::launch_settings(
            &[],
            json!({ "type": "command", "command": "x" }),
            None,
        ))
        .expect("JSON")["title"],
        json!({ "type": "command", "command": "x" })
    );
}

#[test]
fn trusts_the_console_directory_for_one_launch() {
    let mut fixture = isolated();
    let project = AntigravityProvider
        .plan(&fixture.spec())
        .expect("project plan");
    assert!(settings_of(&project).get("trustedWorkspaces").is_none());

    fixture.kind = crate::protocol::SessionKind::Console;
    let console = AntigravityProvider
        .plan(&fixture.spec())
        .expect("console plan");
    assert_eq!(
        settings_of(&console)["trustedWorkspaces"],
        json!([fixture.cwd])
    );
}

/// The launch's settings without the title command, which [`every_launch_gives_its_own_title_command`] checks.
fn without_title(mut settings: Value) -> Value {
    if let Some(settings) = settings.as_object_mut() {
        settings.remove("title");
    }
    settings
}

/// Every launch gives the CLI a title command of its own, on every host: the daemon's title
/// helper on the annotations under the launch's data directory. The settings file is the only
/// place that names it, and no script is written for it.
#[test]
fn every_launch_gives_its_own_title_command() {
    let fixture = isolated();
    let spec = fixture.spec();
    let plan = AntigravityProvider.plan(&spec).expect("plan");
    let settings = settings_of(&plan);
    assert_eq!(settings["title"]["type"], "command");
    let command = settings["title"]["command"].as_str().expect("a command");
    let annotations = std::path::Path::new(&launch_dir(&fixture))
        .join("antigravity-cli")
        .join("annotations");
    assert_eq!(
        command,
        crate::adapter::helper_command(
            spec.self_exe,
            AgentId::ANTIGRAVITY,
            "title",
            &[&annotations.to_string_lossy()],
        )
    );
    assert!(!std::path::Path::new(&launch_dir(&fixture))
        .join("octoboard-title.sh")
        .exists());
    assert!(plan.args.iter().all(|arg| !arg.contains("agent-helper")));
}

/// The task title is what the title command printed, unescaped; an empty title carries none.
#[test]
fn the_task_title_is_the_title_the_command_printed() {
    assert_eq!(
        AntigravityProvider
            .task_title(r#"Fix \"login\" bug"#, &[])
            .as_deref(),
        Some(r#"Fix "login" bug"#)
    );
    assert_eq!(AntigravityProvider.task_title("", &[]), None);
    assert!(!AntigravityProvider.names_project());
}

/// The CLI is installed by its maker's script as `~/.local/bin/agy`, which is told as its own
/// installer's install and updated with `agy update`; it is on no registry Octoboard knows, and
/// has no command that only reports the latest version.
#[test]
fn its_own_installer_s_binary_is_told_and_updated_with_its_update_command() {
    use crate::agents::cli::install::Install;
    use crate::protocol::CliInstallMethod;

    let packaging = AntigravityProvider.cli_packaging();
    assert_eq!(packaging.native_update, ["update"]);
    for (path, method) in [
        ("/Users/someone/.local/bin/agy", CliInstallMethod::Native),
        ("/home/someone/.local/bin/agy", CliInstallMethod::Native),
        (
            "C:/Users/someone/AppData/Local/agy/bin/agy.exe",
            CliInstallMethod::Native,
        ),
        ("/Users/someone/bin/agy", CliInstallMethod::Unknown),
        ("/opt/homebrew/bin/agy", CliInstallMethod::Unknown),
    ] {
        assert_eq!(
            Install::of_path(packaging, std::path::Path::new(path)).method,
            method,
            "{path}"
        );
    }
    assert!(packaging.native_latest.is_none());
    assert!(packaging.npm_packages.is_empty());
    assert!(packaging.homebrew_formula.is_none());
    assert!(packaging.homebrew_cask.is_none());
}

/// The latest version is read from the manifest of Google's updater for the platform Rust names,
/// one manifest per platform the installer offers (`<os>_<arch>`, `darwin` for macOS and `amd64`
/// for x86-64), all at the updater's own host; a platform the installer offers none for has none.
#[test]
fn each_platform_reads_the_updaters_manifest_of_its_own() {
    let packaging = AntigravityProvider.cli_packaging();
    let manifest = packaging
        .native_manifest
        .as_ref()
        .expect("a manifest of Google's updater");
    assert_eq!(manifest.version_field, "version");
    let base = "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/";
    for (os, arch, platform) in [
        ("macos", "aarch64", "darwin_arm64"),
        ("macos", "x86_64", "darwin_amd64"),
        ("linux", "aarch64", "linux_arm64"),
        ("linux", "x86_64", "linux_amd64"),
        ("windows", "aarch64", "windows_arm64"),
        ("windows", "x86_64", "windows_amd64"),
    ] {
        assert_eq!(
            manifest.url_for(os, arch),
            Some(format!("{base}{platform}.json").as_str()),
            "{os} {arch}"
        );
    }
    assert_eq!(manifest.urls.len(), 6);
    assert_eq!(manifest.url_for("freebsd", "x86_64"), None);
    assert_eq!(manifest.url_for("linux", "riscv64"), None);
}
