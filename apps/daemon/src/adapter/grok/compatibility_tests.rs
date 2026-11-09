//! Tests of Grok as a consumer of compatibility sources: what a launch adds to the session's
//! `config.toml`, its environment and its rules.

use std::path::PathBuf;

use super::home_tests::FileLinks;
use super::tests::{plan_isolated, with_default_home};
use super::writeback_tests::{link_config, session_config, writeback_of, SHARED_CONFIG};
use super::*;
use crate::adapter::tests::{spec_fixture, SpecFixture};
use crate::providers::{ProviderSettings, SettingValue};

fn write(path: PathBuf, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A fixture whose user has a Grok home, a `~/.codex` with servers, a skill and a prompt, a
/// `~/.claude.json` of its own, and the named switches on.
fn world(switches: &[&str]) -> SpecFixture {
    let mut fixture = spec_fixture();
    let grok = with_default_home(&mut fixture);
    write(
        grok.join("config.toml"),
        "default_model = \"x\"\n\n[skills]\npaths = [\"/user/skills\"]\n\n[mcp_servers.own]\ncommand = \"grok-own\"\n",
    );
    let home = fixture.scratch.join("home");
    fixture
        .shell_env
        .insert("HOME".to_string(), home.to_string_lossy().into_owned());
    // The variables Codex's servers name, so that the servers naming them are kept.
    for var in ["FORWARDED", "TOKEN_VAR"] {
        fixture.shell_env.insert(var.to_string(), "set".to_string());
    }
    write(
        home.join(".codex/config.toml"),
        r#"
[mcp_servers.local]
command = "npx"
args = ["-y", "server"]
env = { SECRET = "s3cret-value" }
env_vars = ["FORWARDED"]

[mcp_servers.remote]
url = "https://example.com/mcp"
http_headers = { "X-Plain" = "plain-header-value" }
bearer_token_env_var = "TOKEN_VAR"

[mcp_servers.own]
command = "codex-own"

[mcp_servers.fromproject]
command = "codex-fromproject"

[mcp_servers.octoboard]
command = "codex-octoboard"

[mcp_servers."bad name"]
command = "x"
"#,
    );
    write(
        home.join(".codex/skills/review/SKILL.md"),
        "---\nname: review\n---\n",
    );
    write(home.join(".codex/prompts/fix.md"), "Fix $1");
    write(
        fixture.cwd.join(".grok/config.toml"),
        "[mcp_servers.fromproject]\ncommand = \"grok-project\"\n",
    );
    set_switches(&mut fixture, switches);
    fixture
}

fn set_switches(fixture: &mut SpecFixture, switches: &[&str]) {
    fixture.settings = ProviderSettings::new(
        switches
            .iter()
            .map(|id| (id.to_string(), Some(SettingValue::Switch(true)))),
    );
    fixture.compatibility_sources =
        crate::adapter::compatibility_sources_of(AgentId::GROK, &fixture.settings);
}

fn session_toml(plan: &LaunchPlan) -> toml::Table {
    std::fs::read_to_string(session_config(plan))
        .unwrap()
        .parse()
        .expect("the session's config.toml stays valid TOML")
}

fn server_names(config: &toml::Table) -> Vec<&str> {
    config["mcp_servers"]
        .as_table()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

fn env_of<'a>(plan: &'a LaunchPlan, prefix: &str) -> Vec<(&'a str, &'a str)> {
    plan.env
        .iter()
        .filter(|(key, _)| key.starts_with(prefix))
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect()
}

fn rules(plan: &LaunchPlan) -> &str {
    let at = plan.args.iter().position(|arg| arg == "--rules").unwrap();
    &plan.args[at + 1]
}

/// With the switch on, Codex's servers join Octoboard's own and Grok's as tables of the copy, a
/// name Grok defines itself (the user's copy, a project's `.grok/config.toml`) or Octoboard owns
/// is not taken, a literal value is in the process environment and not in the file or on argv, the
/// skills path joins the user's, and the guidance follows the role text.
#[test]
fn codex_compatibility_adds_servers_skills_and_instructions_to_the_copy() {
    let fixture = world(&["codex_compatibility"]);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let text = std::fs::read_to_string(session_config(&plan)).unwrap();
    let config = session_toml(&plan);
    assert_eq!(
        server_names(&config),
        ["local", "octoboard", "own", "remote"]
    );
    assert_eq!(
        config["mcp_servers"]["own"]["command"].as_str(),
        Some("grok-own")
    );
    assert_ne!(
        config["mcp_servers"]["octoboard"]["command"].as_str(),
        Some("codex-octoboard")
    );

    let carried = env_of(&plan, "OCTOBOARD_CODEX_MCP_");
    assert_eq!(carried.len(), 2, "{carried:?}");
    let value_of = |value: &str| carried.iter().find(|(_, v)| *v == value).unwrap().0;
    let local = &config["mcp_servers"]["local"];
    assert_eq!(local["command"].as_str(), Some("npx"));
    assert_eq!(local["args"].as_array().unwrap().len(), 2);
    assert_eq!(
        local["env"]["SECRET"].as_str(),
        Some(format!("${{{}}}", value_of("s3cret-value")).as_str())
    );
    assert_eq!(local["env"]["FORWARDED"].as_str(), Some("${FORWARDED}"));
    let remote = &config["mcp_servers"]["remote"];
    assert_eq!(remote["url"].as_str(), Some("https://example.com/mcp"));
    assert_eq!(
        remote["headers"]["X-Plain"].as_str(),
        Some(format!("${{{}}}", value_of("plain-header-value")).as_str())
    );
    assert_eq!(
        remote["headers"]["Authorization"].as_str(),
        Some("Bearer ${TOKEN_VAR}")
    );
    for secret in ["s3cret-value", "plain-header-value"] {
        assert!(!text.contains(secret), "{secret} is in the file");
        assert!(
            !plan.args.iter().any(|arg| arg.contains(secret)),
            "{secret} is on argv"
        );
    }

    let skills = fixture.scratch.join("codex-skills");
    let paths: Vec<_> = config["skills"]["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|path| path.as_str().unwrap())
        .collect();
    assert_eq!(paths, ["/user/skills", skills.to_str().unwrap()]);
    assert!(skills.join("review/SKILL.md").is_file());

    let role = session_tools::role::role_description(
        fixture.kind,
        fixture.owner.as_ref(),
        AgentId::GROK,
        &fixture.spec().role_options(),
    );
    assert!(rules(&plan).starts_with(&format!("{role}\n\n# Codex compatibility")));
    assert!(rules(&plan).contains("Honor Codex workflows in Grok Build"));
    assert!(text.contains("default_model = \"x\""));
}

/// A reference of Codex's to a variable the launch lacks is settled as Codex's agent would: an
/// `env` entry that is only the reference is dropped, and a server whose bearer variable is unset
/// is left out.
#[test]
fn a_codex_reference_to_a_variable_the_launch_lacks_is_settled_as_codex_would() {
    let mut fixture = world(&["codex_compatibility"]);
    fixture.shell_env.remove("FORWARDED");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let config = session_toml(&plan);
    let local = &config["mcp_servers"]["local"];
    assert!(local["env"].get("FORWARDED").is_none());
    assert!(local["env"].get("SECRET").is_some());
    assert!(config["mcp_servers"].get("remote").is_some());

    fixture.shell_env.remove("TOKEN_VAR");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let config = session_toml(&plan);
    assert_eq!(server_names(&config), ["local", "octoboard", "own"]);
}

#[test]
fn with_the_switch_off_none_of_codexs_configuration_is_added() {
    let fixture = world(&[]);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let config = session_toml(&plan);
    assert_eq!(server_names(&config), ["octoboard", "own"]);
    assert_eq!(config["skills"]["paths"].as_array().unwrap().len(), 1);
    assert!(!rules(&plan).contains("Codex"));
    assert!(env_of(&plan, "OCTOBOARD_").is_empty());
    assert!(!fixture.scratch.join("codex-skills").exists());
}

/// Every launch of a session that has the switch on gets it, a resume included.
#[test]
fn a_resume_gets_the_sources_too() {
    let fixture = world(&["codex_compatibility"]);
    let mut spec = fixture.spec();
    spec.resume_agent_session_id = Some("agent-side-id");
    let plan = plan_isolated(&spec).expect("plan");
    assert!(server_names(&session_toml(&plan)).contains(&"local"));
    assert!(rules(&plan).contains("# Codex compatibility"));
}

/// A copy that is not valid TOML is left as the user wrote it, with Octoboard's own block after
/// it, and the launch goes on with nothing of the source.
#[test]
fn a_copy_that_cannot_be_parsed_gets_nothing_and_the_launch_goes_on() {
    let fixture = world(&["codex_compatibility"]);
    let home = PathBuf::from(&fixture.shell_env["GROK_HOME"]);
    write(home.join("config.toml"), "not = [toml\n");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let text = std::fs::read_to_string(session_config(&plan)).unwrap();
    assert!(text.starts_with("not = [toml\n"));
    assert!(text.contains("[mcp_servers.octoboard]"));
    assert!(!text.contains("local") && !text.contains("codex-skills"));
    assert!(env_of(&plan, "OCTOBOARD_CODEX").is_empty());
}

/// A project's Grok config that exists but cannot be read leaves no server at all, while the
/// skills, which need no names, are still added.
#[test]
fn an_unreadable_project_grok_config_leaves_no_server() {
    let fixture = world(&["codex_compatibility"]);
    write(fixture.cwd.join(".grok/config.toml"), "not = [toml\n");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let config = session_toml(&plan);
    assert_eq!(server_names(&config), ["octoboard", "own"]);
    assert!(env_of(&plan, "OCTOBOARD_CODEX_MCP_").is_empty());
    assert_eq!(config["skills"]["paths"].as_array().unwrap().len(), 2);
}

/// A project's `.grok/config.toml` above the working directory, up to the repository root, counts
/// too.
#[test]
fn a_server_a_grok_config_up_to_the_repository_root_defines_is_not_taken() {
    let mut fixture = world(&["codex_compatibility"]);
    let repo = fixture.cwd.clone();
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let sub = repo.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    fixture.cwd = sub;
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    assert_eq!(
        server_names(&session_toml(&plan)),
        ["local", "octoboard", "own", "remote"]
    );
}

/// With no `[skills]` the table is made, and with no skill to offer none is.
#[test]
fn the_skills_table_is_made_when_absent_and_not_when_there_is_nothing_to_offer() {
    let fixture = world(&["codex_compatibility"]);
    let home = PathBuf::from(&fixture.shell_env["GROK_HOME"]);
    write(home.join("config.toml"), "default_model = \"x\"\n");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let config = session_toml(&plan);
    assert_eq!(config["skills"]["paths"].as_array().unwrap().len(), 1);
    assert_eq!(config["default_model"].as_str(), Some("x"));

    std::fs::remove_dir_all(fixture.scratch.join("home/.codex/skills")).unwrap();
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    assert!(session_toml(&plan).get("skills").is_none());
    assert!(!fixture.scratch.join("codex-skills").exists());
}

/// An inline `skills` table is edited as a table, and a `[skills]` that is not a table, or a
/// `paths` that is not an array, is left alone while the servers still go in.
#[test]
fn an_inline_skills_table_is_extended_and_a_malformed_one_is_left_alone() {
    let fixture = world(&["codex_compatibility"]);
    let home = PathBuf::from(&fixture.shell_env["GROK_HOME"]);
    write(
        home.join("config.toml"),
        "skills = { paths = [\"/user/skills\"] }\n",
    );
    let config = session_toml(&plan_isolated(&fixture.spec()).expect("plan"));
    assert_eq!(config["skills"]["paths"].as_array().unwrap().len(), 2);

    write(home.join("config.toml"), "skills = \"odd\"\n");
    let config = session_toml(&plan_isolated(&fixture.spec()).expect("plan"));
    assert_eq!(config["skills"].as_str(), Some("odd"));
    assert_eq!(
        server_names(&config),
        ["local", "octoboard", "own", "remote"]
    );

    write(home.join("config.toml"), "[skills]\npaths = \"odd\"\n");
    let config = session_toml(&plan_isolated(&fixture.spec()).expect("plan"));
    assert_eq!(config["skills"]["paths"].as_str(), Some("odd"));
}

/// Without file links the skills are still linked, as directories are.
#[test]
fn without_file_links_the_skills_directory_is_still_linked() {
    let _links = FileLinks::set(false);
    let fixture = world(&["codex_compatibility"]);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let skills = fixture.scratch.join("codex-skills");
    assert!(std::fs::symlink_metadata(skills.join("review"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(skills.join("review/SKILL.md").is_file());
    assert_eq!(
        session_toml(&plan)["skills"]["paths"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

/// The Claude Code global config in effect, and the one Grok reads.
fn claude_world(fixture: &mut SpecFixture) -> PathBuf {
    let home = fixture.scratch.join("home");
    write(
        home.join(".claude.json"),
        r#"{"mcpServers": {"from-home": {"command": "home-server"}}}"#,
    );
    let exported = fixture.scratch.join("claude-dir");
    write(
        exported.join(".claude.json"),
        r#"{"mcpServers": {
            "from-dir": {"command": "dir-server", "env": {"KEY": "claude-secret"}},
            "own": {"command": "claude-own"},
            "shared": {"command": "claude-shared"},
            "defaulted": {"command": "x", "env": {"A": "${A:-fallback}"}},
            "events": {"type": "sse", "url": "https://example.com/sse",
                       "headers": {"X-Key": "sse-secret"}}
        }}"#,
    );
    exported
}

fn claude_vars(plan: &LaunchPlan) -> Vec<(&str, &str)> {
    env_of(plan, "GROK_CLAUDE_")
}

/// Grok reads `$HOME/.claude.json` whatever `CLAUDE_CONFIG_DIR` says: where the user's Claude Code
/// reads another file, Grok's own MCP reading is turned off and the servers of that file come from
/// Octoboard, under the same rules as Codex's.
#[test]
fn a_claude_config_dir_turns_groks_own_mcp_reading_off_and_supplies_the_servers() {
    let mut fixture = world(&["claude_compatibility"]);
    let exported = claude_world(&mut fixture);
    fixture.shell_env.insert(
        "CLAUDE_CONFIG_DIR".to_string(),
        exported.to_string_lossy().into_owned(),
    );
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let vars = claude_vars(&plan);
    assert_eq!(vars.len(), 5);
    for (name, value) in vars {
        let expected = if name == "GROK_CLAUDE_MCPS_ENABLED" {
            "false"
        } else {
            "true"
        };
        assert_eq!(value, expected, "{name}");
    }
    let text = std::fs::read_to_string(session_config(&plan)).unwrap();
    let config = session_toml(&plan);
    assert_eq!(
        server_names(&config),
        [
            "defaulted",
            "events",
            "from-dir",
            "octoboard",
            "own",
            "shared"
        ]
    );
    // A server-sent-events server is given as a remote one, its header value out of the file.
    assert_eq!(
        config["mcp_servers"]["events"]["url"].as_str(),
        Some("https://example.com/sse")
    );
    assert!(config["mcp_servers"]["events"]["headers"]["X-Key"]
        .as_str()
        .is_some_and(|value| value.starts_with("${OCTOBOARD_CLAUDE_MCP_")));
    assert!(!text.contains("sse-secret"));
    assert_eq!(
        config["mcp_servers"]["own"]["command"].as_str(),
        Some("grok-own")
    );
    let carried = env_of(&plan, "OCTOBOARD_CLAUDE_MCP_");
    assert_eq!(carried.len(), 2, "{carried:?}");
    let secret = carried
        .iter()
        .find(|(_, value)| *value == "claude-secret")
        .unwrap();
    assert_eq!(
        config["mcp_servers"]["from-dir"]["env"]["KEY"].as_str(),
        Some(format!("${{{}}}", secret.0).as_str())
    );
    assert!(!text.contains("claude-secret"));
    assert_eq!(
        config["mcp_servers"]["defaulted"]["env"]["A"].as_str(),
        Some("${A:-fallback}"),
        "Grok expands a `${{A:-fallback}}` itself"
    );
    // Claude Code's own guidance is not added: Grok reads its files itself.
    assert!(!rules(&plan).contains("Claude Code compatibility"));
}

/// Where Claude Code and Grok read the same file, or the shell exports no directory, Grok's own
/// reading stays on and nothing of the file is copied.
#[test]
fn where_both_read_the_same_file_grok_reads_claude_codes_servers_itself() {
    let mut fixture = world(&["claude_compatibility"]);
    claude_world(&mut fixture);
    let home = fixture.scratch.join("home");
    for exported in [None, Some(&home)] {
        if let Some(dir) = exported {
            fixture.shell_env.insert(
                "CLAUDE_CONFIG_DIR".to_string(),
                dir.to_string_lossy().into_owned(),
            );
        }
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        assert_eq!(claude_vars(&plan).len(), 5);
        assert!(
            claude_vars(&plan).iter().all(|(_, value)| *value == "true"),
            "{exported:?}"
        );
        assert_eq!(server_names(&session_toml(&plan)), ["octoboard", "own"]);
        assert!(env_of(&plan, "OCTOBOARD_").is_empty());
    }
}

/// Both switches on: Claude Code's servers come first, so a name both give is Claude Code's, and
/// the two sets of values do not share variables.
#[test]
fn with_both_switches_on_claude_codes_servers_take_a_shared_name() {
    let mut fixture = world(&["claude_compatibility", "codex_compatibility"]);
    let exported = claude_world(&mut fixture);
    fixture.shell_env.insert(
        "CLAUDE_CONFIG_DIR".to_string(),
        exported.to_string_lossy().into_owned(),
    );
    let home = fixture.scratch.join("home");
    let mut codex = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap();
    codex.push_str("\n[mcp_servers.shared]\ncommand = \"codex-shared\"\n");
    write(home.join(".codex/config.toml"), &codex);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let config = session_toml(&plan);
    assert_eq!(
        server_names(&config),
        [
            "defaulted",
            "events",
            "from-dir",
            "local",
            "octoboard",
            "own",
            "remote",
            "shared"
        ]
    );
    assert_eq!(
        config["mcp_servers"]["shared"]["command"].as_str(),
        Some("claude-shared")
    );
    assert_eq!(env_of(&plan, "OCTOBOARD_CLAUDE_MCP_").len(), 2);
    assert_eq!(env_of(&plan, "OCTOBOARD_CODEX_MCP_").len(), 2);
    assert!(rules(&plan).contains("# Codex compatibility"));
}

/// A name Claude Code gives that Grok cannot take as it is stays Claude Code's: Codex's server of
/// that name is not added in its place.
#[test]
fn a_server_claude_code_gives_that_grok_cannot_take_still_holds_its_name_against_codex() {
    let mut fixture = world(&["claude_compatibility", "codex_compatibility"]);
    let exported = claude_world(&mut fixture);
    let file = exported.join(".claude.json");
    let text = std::fs::read_to_string(&file).unwrap();
    write(
        file,
        &text.replacen(
            "\"shared\"",
            "\"odd\": {\"command\": \"x\", \"env\": {\"A\": \"${A:+alt}\"}}, \"shared\"",
            1,
        ),
    );
    fixture.shell_env.insert(
        "CLAUDE_CONFIG_DIR".to_string(),
        exported.to_string_lossy().into_owned(),
    );
    let home = fixture.scratch.join("home");
    let mut codex = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap();
    codex.push_str("\n[mcp_servers.odd]\ncommand = \"codex-odd\"\n");
    write(home.join(".codex/config.toml"), &codex);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    assert!(!server_names(&session_toml(&plan)).contains(&"odd"));
}

/// Where Grok reads Claude Code's servers itself, a name both Claude Code and Codex give is still
/// Claude Code's: Codex's is not added beside it.
#[test]
fn a_name_grok_reads_from_claude_code_itself_is_not_taken_from_codex() {
    let mut fixture = world(&["claude_compatibility", "codex_compatibility"]);
    claude_world(&mut fixture);
    let home = fixture.scratch.join("home");
    let mut codex = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap();
    codex.push_str("\n[mcp_servers.from-home]\ncommand = \"codex-from-home\"\n");
    write(home.join(".codex/config.toml"), &codex);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    assert_eq!(
        server_names(&session_toml(&plan)),
        ["local", "octoboard", "own", "remote"]
    );
    assert!(claude_vars(&plan).iter().all(|(_, value)| *value == "true"));
}

/// Grok's own reading of Claude Code's servers is turned off only where they are in the copy: a
/// copy that cannot be edited leaves Grok to read them itself.
#[test]
fn a_copy_that_cannot_be_edited_leaves_groks_own_claude_reading_on() {
    let mut fixture = world(&["claude_compatibility"]);
    let exported = claude_world(&mut fixture);
    fixture.shell_env.insert(
        "CLAUDE_CONFIG_DIR".to_string(),
        exported.to_string_lossy().into_owned(),
    );
    let home = PathBuf::from(&fixture.shell_env["GROK_HOME"]);
    write(home.join("config.toml"), "= not toml\n");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    assert_eq!(claude_vars(&plan).len(), 5);
    assert!(claude_vars(&plan).iter().all(|(_, value)| *value == "true"));
    assert!(env_of(&plan, "OCTOBOARD_CLAUDE_MCP_").is_empty());
}

/// Grok reads the `.agents/skills` roots itself, so only the others are linked.
#[test]
fn the_agents_skills_roots_are_left_to_grok() {
    let fixture = world(&["codex_compatibility"]);
    let home = fixture.scratch.join("home");
    write(
        home.join(".agents/skills/shared/SKILL.md"),
        "---\nname: shared\n---\n",
    );
    plan_isolated(&fixture.spec()).expect("plan");
    let skills = fixture.scratch.join("codex-skills");
    assert!(skills.join("review/SKILL.md").is_file());
    assert!(!skills.join("shared").exists());
}

/// What a source adds to the session's copy is never written back to a linked source config: the
/// settings Grok changed in the session are, the servers, the skills path and the values are not.
#[test]
fn the_additions_are_not_written_back_to_the_shared_config() {
    let mut fixture = world(&["claude_compatibility", "codex_compatibility"]);
    let exported = claude_world(&mut fixture);
    fixture.shell_env.insert(
        "CLAUDE_CONFIG_DIR".to_string(),
        exported.to_string_lossy().into_owned(),
    );
    let shared = link_config(&mut fixture, SHARED_CONFIG);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let session = session_config(&plan);
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("local") && text.contains("from-dir") && text.contains("codex-skills"));
    std::fs::write(
        &session,
        text.replace("theme = \"dark\"", "theme = \"tokyonight\""),
    )
    .unwrap();
    writeback_of(&plan).publish();

    let updated = std::fs::read_to_string(&shared).unwrap();
    assert!(updated.contains("theme = \"tokyonight\""), "{updated}");
    for added in ["mcp_servers", "skills", "local", "from-dir", "OCTOBOARD"] {
        assert!(!updated.contains(added), "{added} in {updated}");
    }
}
