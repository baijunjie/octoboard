use super::*;

use std::path::Path;

use crate::adapter::CliPackaging;
use crate::platform::shell::ResolvedBinary;
use crate::protocol::Code;
use latest::Failure;

/// A registry that answers a fixed way, so a test makes no request.
pub(super) enum Stub {
    Version(&'static str),
    Fails,
    Unreadable,
}

impl Registry for Stub {
    fn latest_version(
        &self,
        _package: &str,
        _shell_env: &HashMap<String, String>,
    ) -> std::result::Result<String, Failure> {
        match self {
            Stub::Version(version) => Ok((*version).to_owned()),
            Stub::Fails => Err(Failure::Failed("the registry is unreachable".to_owned())),
            Stub::Unreadable => Err(Failure::Unreadable),
        }
    }
}

/// What `registry()` is under test: nothing is asked of the network.
pub(super) struct NoRegistry;

impl Registry for NoRegistry {
    fn latest_version(
        &self,
        _package: &str,
        _shell_env: &HashMap<String, String>,
    ) -> std::result::Result<String, Failure> {
        Err(Failure::Failed("no network under test".to_owned()))
    }
}

static LATEST_0_170: Stub = Stub::Version("0.170.0");
static UNREACHABLE: Stub = Stub::Fails;
static UNREADABLE: Stub = Stub::Unreadable;

fn codex() -> &'static CliPackaging {
    adapter::provider(AgentId::CODEX).cli_packaging()
}

fn claude() -> &'static CliPackaging {
    adapter::provider(AgentId::CLAUDE).cli_packaging()
}

fn grok() -> &'static CliPackaging {
    adapter::provider(AgentId::GROK).cli_packaging()
}

fn method(packaging: &CliPackaging, path: &str) -> CliInstallMethod {
    Install::of_path(packaging, Path::new(path)).method
}

// -- install methods, told by a path ----------------------------------------------------------

#[test]
fn each_agents_own_installer_layout_is_native() {
    assert_eq!(
        method(claude(), "/Users/u/.local/share/claude/versions/2.1.319"),
        CliInstallMethod::Native
    );
    assert_eq!(
        method(claude(), "C:/Users/u/.local/bin/claude.exe"),
        CliInstallMethod::Native
    );
    assert_eq!(
        method(
            codex(),
            "/Users/u/.codex/packages/standalone/releases/0.166.0-aarch64-apple-darwin/bin/codex"
        ),
        CliInstallMethod::Native
    );
    assert_eq!(
        method(grok(), "/Users/u/.grok/downloads/grok-1.0.77-macos-aarch64"),
        CliInstallMethod::Native
    );
}

#[test]
fn a_layout_is_matched_whatever_the_case_of_its_directories() {
    assert_eq!(
        method(grok(), "C:/Users/U/.GROK/bin/grok.exe"),
        CliInstallMethod::Native
    );
}

#[test]
fn a_package_directory_of_the_agent_is_npm_wherever_the_prefix_is() {
    for path in [
        "/opt/homebrew/lib/node_modules/@openai/codex/bin/codex.js",
        "/Users/u/.local/state/fnm_multishells/60007/lib/node_modules/@openai/codex/bin/codex.js",
        "/Users/u/.nvm/versions/node/v22.1.0/lib/node_modules/@openai/codex/bin/codex.js",
        "C:/Users/u/AppData/Roaming/npm/node_modules/@openai/codex/bin/codex.js",
        "/usr/lib/node_modules/@openai/codex/node_modules/@openai/codex-linux-x64/vendor/codex",
    ] {
        assert_eq!(method(codex(), path), CliInstallMethod::Npm, "{path}");
    }
    assert_eq!(
        method(
            claude(),
            "/usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js"
        ),
        CliInstallMethod::Npm
    );
}

#[test]
fn the_platform_package_of_a_package_is_not_the_package() {
    assert_eq!(
        method(
            codex(),
            "/usr/lib/node_modules/@openai/codex-linux-x64/vendor/codex"
        ),
        CliInstallMethod::Unknown
    );
}

#[test]
fn an_install_of_another_package_manager_is_never_updated_with_npm() {
    for path in [
        "/Users/u/Library/pnpm/global/5/node_modules/.pnpm/@openai+codex@0.1.0/node_modules/@openai/codex/bin/codex.js",
        "/Users/u/.bun/install/global/node_modules/@openai/codex/bin/codex.js",
        "/Users/u/.config/yarn/global/node_modules/@openai/codex/bin/codex.js",
    ] {
        assert_eq!(method(codex(), path), CliInstallMethod::Unknown, "{path}");
    }
}

#[test]
fn a_homebrew_formula_or_cask_of_the_agent_is_homebrew() {
    assert_eq!(
        method(codex(), "/opt/homebrew/Cellar/codex/0.166.0/bin/codex"),
        CliInstallMethod::Homebrew
    );
    assert_eq!(
        method(
            claude(),
            "/opt/homebrew/Caskroom/claude-code/2.1.319/claude"
        ),
        CliInstallMethod::Homebrew
    );
    assert_eq!(
        method(
            codex(),
            "/home/linuxbrew/.linuxbrew/Cellar/codex/0.166.0/bin/codex"
        ),
        CliInstallMethod::Homebrew
    );
}

#[test]
fn another_package_of_homebrew_is_not_the_agent() {
    assert_eq!(
        method(codex(), "/opt/homebrew/Cellar/ripgrep/14.1.0/bin/rg"),
        CliInstallMethod::Unknown
    );
    // Claude Code has no formula, and Grok no package at all.
    assert_eq!(
        method(
            claude(),
            "/opt/homebrew/Cellar/claude-code/2.1.319/bin/claude"
        ),
        CliInstallMethod::Unknown
    );
    assert_eq!(
        method(grok(), "/opt/homebrew/Cellar/grok/1.0.77/bin/grok"),
        CliInstallMethod::Unknown
    );
}

#[test]
fn a_package_installed_under_homebrews_node_is_npm_not_homebrew() {
    assert_eq!(
        method(
            codex(),
            "/opt/homebrew/Cellar/node/22.1.0/lib/node_modules/@openai/codex/bin/codex.js"
        ),
        CliInstallMethod::Npm
    );
}

#[test]
fn a_path_that_says_nothing_is_unknown() {
    assert_eq!(
        method(codex(), "/usr/local/bin/codex"),
        CliInstallMethod::Unknown
    );
    assert_eq!(
        method(claude(), "/Users/u/work/claude-wrapper"),
        CliInstallMethod::Unknown
    );
}

#[test]
fn a_provider_that_says_nothing_has_no_install_recognised() {
    for path in [
        "/Users/u/.codex/packages/standalone/releases/1/bin/codex",
        "/opt/homebrew/lib/node_modules/@openai/codex/bin/codex.js",
    ] {
        assert_eq!(method(&CliPackaging::NONE, path), CliInstallMethod::Unknown);
    }
    assert!(CliPackaging::NONE.is_none());
    assert!(!codex().is_none());
}

// -- the command that updates an install --------------------------------------------------------

fn args_of(command: &ToolCommand) -> Vec<String> {
    command
        .command(&HashMap::new())
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

fn program_of(command: &ToolCommand) -> String {
    command
        .command(&HashMap::new())
        .get_program()
        .to_string_lossy()
        .into_owned()
}

#[cfg(unix)]
fn executable(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\n").unwrap();
    crate::test_support::make_executable(path);
}

#[cfg(unix)]
fn env_with_path(dirs: &[&Path]) -> HashMap<String, String> {
    let path: Vec<String> = dirs.iter().map(|dir| dir.display().to_string()).collect();
    HashMap::from([("PATH".to_string(), path.join(":"))])
}

#[test]
fn a_native_install_updates_itself_with_its_own_binary() {
    let install = Install::of_path(
        claude(),
        Path::new("/Users/u/.local/share/claude/versions/2.1.319"),
    );
    let binary = ResolvedBinary::new("/Users/u/.local/bin/claude");
    let command = install
        .update_command(claude(), &binary, &HashMap::new())
        .expect("a native install is updated");

    assert_eq!(program_of(&command), "/Users/u/.local/bin/claude");
    assert_eq!(args_of(&command), ["update"]);
    assert!(command.is_agent_cli());
}

#[cfg(unix)]
#[test]
fn an_npm_install_is_updated_in_its_own_prefix_with_the_npm_on_the_path() {
    let dir = crate::test_support::ScratchDir::new("cli-npm-command");
    let npm = dir.join("node/bin/npm");
    executable(&npm);
    let install = Install::of_path(
        codex(),
        Path::new("/opt/node/lib/node_modules/@openai/codex/bin/codex.js"),
    );
    let binary = ResolvedBinary::new("/opt/node/bin/codex");

    let command = install
        .update_command(codex(), &binary, &env_with_path(&[&dir.join("node/bin")]))
        .expect("an npm install is updated");

    assert_eq!(program_of(&command), npm.to_string_lossy());
    assert_eq!(
        args_of(&command),
        [
            "install",
            "-g",
            "--prefix",
            "/opt/node",
            "@openai/codex@latest"
        ]
    );
    assert!(!command.is_agent_cli());
}

#[test]
fn a_volta_install_is_updated_without_a_prefix_and_one_with_no_npm_not_at_all() {
    let install = Install::of_path(
        codex(),
        Path::new("/Users/u/.volta/tools/image/packages/@openai/codex/lib/node_modules/@openai/codex/bin/codex.js"),
    );
    assert_eq!(install.method, CliInstallMethod::Npm);
    let binary = ResolvedBinary::new("/Users/u/.volta/bin/codex");

    assert!(
        install
            .update_command(codex(), &binary, &HashMap::new())
            .is_none(),
        "npm is nowhere on this PATH"
    );
}

#[cfg(unix)]
#[test]
fn a_homebrew_install_is_upgraded_by_the_brew_of_its_own_prefix() {
    let dir = crate::test_support::ScratchDir::new("cli-brew-command");
    let brew = dir.join("prefix/bin/brew");
    executable(&brew);
    let formula = dir.join("prefix/Cellar/codex/0.166.0/bin/codex");
    let cask = dir.join("prefix/Caskroom/claude-code/2.1.319/claude");

    let upgrade = |packaging: &CliPackaging, installed: &Path| {
        let install = Install::of_path(packaging, installed);
        assert_eq!(install.method, CliInstallMethod::Homebrew);
        install
            .update_command(packaging, &ResolvedBinary::new(installed), &HashMap::new())
            .expect("a Homebrew install is updated")
    };

    let formula = upgrade(codex(), &formula);
    assert_eq!(program_of(&formula), brew.to_string_lossy());
    assert_eq!(args_of(&formula), ["upgrade", "codex"]);
    let cask = upgrade(claude(), &cask);
    assert_eq!(args_of(&cask), ["upgrade", "--cask", "claude-code"]);
}

#[cfg(unix)]
#[test]
fn a_homebrew_install_falls_back_to_the_brew_on_the_path() {
    let dir = crate::test_support::ScratchDir::new("cli-brew-path");
    executable(&dir.join("bin/brew"));
    let install = Install::of_path(
        codex(),
        Path::new("/nowhere/Cellar/codex/0.166.0/bin/codex"),
    );

    let command = install
        .update_command(
            codex(),
            &ResolvedBinary::new("/nowhere/bin/codex"),
            &env_with_path(&[&dir.join("bin")]),
        )
        .expect("brew is on the PATH");

    assert_eq!(program_of(&command), dir.join("bin/brew").to_string_lossy());
}

#[test]
fn an_unknown_install_has_no_update() {
    let install = Install::unknown();
    assert!(install
        .update_command(
            codex(),
            &ResolvedBinary::new("/usr/local/bin/codex"),
            &HashMap::new()
        )
        .is_none());
}

// -- installs on disk, links followed -----------------------------------------------------------

#[cfg(unix)]
#[test]
fn the_install_of_a_binary_is_told_after_following_every_link() {
    let dir = crate::test_support::ScratchDir::new("cli-links");
    let real = dir.join(".codex/packages/standalone/releases/0.166.0/bin/codex");
    executable(&real);
    let first = dir.join(".local/bin/codex");
    std::fs::create_dir_all(first.parent().unwrap()).unwrap();
    crate::test_support::symlink(&real, &first).unwrap();
    let second = dir.join("elsewhere/codex");
    std::fs::create_dir_all(second.parent().unwrap()).unwrap();
    crate::test_support::symlink(&first, &second).unwrap();

    for binary in [&real, &first, &second] {
        assert_eq!(
            Install::of_binary(codex(), &ResolvedBinary::new(binary)).method,
            CliInstallMethod::Native,
            "{}",
            binary.display()
        );
    }
    let lone = dir.join("usr/bin/codex");
    executable(&lone);
    assert_eq!(
        Install::of_binary(codex(), &ResolvedBinary::new(&lone)).method,
        CliInstallMethod::Unknown
    );
}

// -- checks and updates against stand-in CLIs ---------------------------------------------------

/// A fake CLI under a scratch directory, laid out where an install of `layout` puts its binary,
/// so that following its links ends in that layout: a hard link of the shared stand-in
/// executable (a link is followed no further, a symlink would end in the temporary directory) with
/// a symlink `bin/<binary>` to it, which is what the shell finds.
#[cfg(unix)]
struct Cli {
    state: Arc<AppState>,
    dir: crate::test_support::ScratchDir,
    env: HashMap<String, String>,
}

#[cfg(unix)]
fn cli(name: &str, agent: AgentId, layout: &str, body: &str) -> Cli {
    let (state, dir) = crate::test_support::app_state(name);
    let stand_in = dir.join("stand-in");
    std::fs::create_dir(&stand_in).unwrap();
    crate::test_support::fake_agent(&stand_in, agent.descriptor().binary, "");
    let dispatcher = std::fs::read_link(stand_in.join(agent.descriptor().binary)).unwrap();
    let real = dir.join(layout).join(agent.descriptor().binary);
    std::fs::create_dir_all(real.parent().unwrap()).unwrap();
    std::fs::hard_link(&dispatcher, &real).expect("a hard link of the stand-in");
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let link = bin.join(agent.descriptor().binary);
    crate::test_support::symlink(&real, &link).unwrap();
    std::fs::write(
        bin.join(format!("{}.body", agent.descriptor().binary)),
        body,
    )
    .unwrap();
    let env = HashMap::from([
        (
            "PATH".to_string(),
            format!("{}:/usr/bin:/bin", bin.display()),
        ),
        ("HOME".to_string(), dir.to_string_lossy().into_owned()),
    ]);
    Cli { state, dir, env }
}

/// A fake Codex that is at version 0.166.0 until its `update` runs, which moves it to 0.167.0.
#[cfg(unix)]
const CODEX_UPDATING: &str = r#"
    version_file="$HOME/codex.version"
    [ -f "$version_file" ] || echo 0.166.0 > "$version_file"
    case "$1" in
      --version) echo "codex-cli $(cat "$version_file")";;
      update) echo "downloading" >&2; echo 0.167.0 > "$version_file";;
    esac
"#;

#[cfg(unix)]
const NATIVE_CODEX: &str = ".codex/packages/standalone/releases/0.166.0/bin";

#[cfg(unix)]
async fn next_status(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
    until: impl Fn(&AgentCliStatus) -> bool,
) -> AgentCliStatus {
    tokio::time::timeout(crate::test_support::PATIENCE, async {
        loop {
            if let Ok(Event::AgentCliStatusUpdated { status }) = events.recv().await {
                if until(&status) {
                    return status;
                }
            }
        }
    })
    .await
    .expect("the status never came")
}

#[cfg(unix)]
fn ended(status: &AgentCliStatus) -> bool {
    status
        .update
        .as_ref()
        .is_some_and(|update| update.state != CliUpdateState::Running)
}

#[cfg(unix)]
#[test]
fn a_check_reads_the_version_the_method_and_the_latest_version() {
    let cli = cli("cli-check", AgentId::CODEX, NATIVE_CODEX, CODEX_UPDATING);

    let status = check(AgentId::CODEX, &cli.env, &LATEST_0_170);

    assert_eq!(status.installed_version.as_deref(), Some("0.166.0"));
    assert_eq!(status.install_method, CliInstallMethod::Native);
    assert_eq!(status.latest_version.as_deref(), Some("0.170.0"));
    assert_eq!(status.latest_error, None);
    assert_eq!(status.update, None);
}

#[cfg(unix)]
#[test]
fn a_lookup_that_fails_is_a_reason_beside_a_status_that_is_otherwise_whole() {
    let cli = cli(
        "cli-latest-fails",
        AgentId::CODEX,
        NATIVE_CODEX,
        CODEX_UPDATING,
    );

    let unreachable = check(AgentId::CODEX, &cli.env, &UNREACHABLE);
    assert_eq!(unreachable.installed_version.as_deref(), Some("0.166.0"));
    assert_eq!(unreachable.latest_version, None);
    assert_eq!(
        unreachable.latest_error,
        Some(agent_cli_code::LATEST_FAILED)
    );
    assert_eq!(
        unreachable.latest_detail.as_deref(),
        Some("the registry is unreachable")
    );

    let unreadable = check(AgentId::CODEX, &cli.env, &UNREADABLE);
    assert_eq!(
        unreadable.latest_error,
        Some(agent_cli_code::LATEST_UNREADABLE)
    );
    assert_eq!(unreadable.latest_detail, None);
}

#[cfg(unix)]
#[test]
fn an_install_that_is_not_recognised_shows_its_version_and_looks_up_nothing() {
    let cli = cli(
        "cli-unknown",
        AgentId::CODEX,
        "usr/local/bin-real",
        CODEX_UPDATING,
    );

    let status = check(AgentId::CODEX, &cli.env, &UNREACHABLE);

    assert_eq!(status.installed_version.as_deref(), Some("0.166.0"));
    assert_eq!(status.install_method, CliInstallMethod::Unknown);
    assert_eq!(status.latest_version, None);
    assert_eq!(
        status.latest_error, None,
        "nothing was asked, so nothing failed"
    );
}

#[test]
fn an_agent_that_does_not_resolve_has_nothing_to_show() {
    let status = check(AgentId::CODEX, &HashMap::new(), &LATEST_0_170);

    assert_eq!(status.installed_version, None);
    assert_eq!(status.install_method, CliInstallMethod::Unknown);
    assert_eq!(status.latest_version, None);
    assert_eq!(status.latest_error, None);
}

#[cfg(unix)]
#[test]
fn groks_own_check_gives_the_latest_version_of_a_native_install() {
    let body = r#"
        case "$*" in
          "update --check --json") echo '{"currentVersion":"1.0.77","latestVersion":"1.0.80","updateAvailable":true,"error":null}';;
          "--version") echo "grok 1.0.77";;
        esac
    "#;
    let cli = cli("cli-grok-check", AgentId::GROK, ".grok/bin", body);

    let status = check(AgentId::GROK, &cli.env, &UNREACHABLE);

    assert_eq!(status.install_method, CliInstallMethod::Native);
    assert_eq!(status.latest_version.as_deref(), Some("1.0.80"));
    assert_eq!(
        status.latest_error, None,
        "the registry is never asked for Grok"
    );
}

#[cfg(unix)]
#[test]
fn an_error_in_the_cli_s_own_check_is_the_failure_of_the_lookup() {
    let body = r#"
        case "$*" in
          "update --check --json") echo '{"latestVersion":null,"error":"offline"}';;
          "--version") echo "grok 1.0.77";;
        esac
    "#;
    let cli = cli("cli-grok-error", AgentId::GROK, ".grok/bin", body);

    let status = check(AgentId::GROK, &cli.env, &LATEST_0_170);

    assert_eq!(status.latest_version, None);
    assert_eq!(status.latest_error, Some(agent_cli_code::LATEST_FAILED));
    assert_eq!(status.latest_detail.as_deref(), Some("offline"));
}

#[cfg(unix)]
#[test]
fn a_failed_own_check_gives_its_error_field_never_a_line_of_its_json() {
    let body = r#"
        case "$*" in
          "update --check --json") echo '{"latestVersion":null,"error":"offline"}'; exit 1;;
          "--version") echo "grok 1.0.77";;
        esac
    "#;
    let first = cli("cli-grok-check-fails", AgentId::GROK, ".grok/bin", body);
    let status = check(AgentId::GROK, &first.env, &LATEST_0_170);
    assert_eq!(status.latest_detail.as_deref(), Some("offline"));

    let body = r#"
        case "$*" in
          "update --check --json") echo '{"latestVersion":null}'; echo "boom" >&2; exit 1;;
          "--version") echo "grok 1.0.77";;
        esac
    "#;
    let second = cli("cli-grok-check-stderr", AgentId::GROK, ".grok/bin", body);
    let status = check(AgentId::GROK, &second.env, &LATEST_0_170);
    assert_eq!(status.latest_detail.as_deref(), Some("boom"));

    let body = r#"
        case "$*" in
          "update --check --json") echo '{"latestVersion":null}'; exit 1;;
          "--version") echo "grok 1.0.77";;
        esac
    "#;
    let third = cli("cli-grok-check-bare", AgentId::GROK, ".grok/bin", body);
    let status = check(AgentId::GROK, &third.env, &LATEST_0_170);
    let detail = status.latest_detail.unwrap();
    assert!(detail.starts_with("the check ended with"), "{detail}");
}

#[cfg(unix)]
#[test]
fn homebrew_latest_is_the_stable_version_of_its_record() {
    let (state, dir) = crate::test_support::app_state("cli-brew-latest");
    drop(state);
    let brew_body = r#"
        if [ "$1 $2" = "info --json=v2" ] && [ "$HOMEBREW_NO_AUTO_UPDATE" = 1 ]; then
          echo '{"formulae":[{"versions":{"stable":"0.168.1"}}],"casks":[]}'
        fi
    "#;
    let prefix_bin = dir.join("prefix/bin");
    std::fs::create_dir_all(&prefix_bin).unwrap();
    crate::test_support::fake_agent(&prefix_bin, "brew", brew_body);
    let installed = dir.join("prefix/Cellar/codex/0.166.0/bin/codex");
    executable(&installed);
    let install = Install::of_path(codex(), &installed);
    let env = HashMap::from([("HOME".to_string(), dir.to_string_lossy().into_owned())]);

    let version = latest::latest_version(AgentId::CODEX, codex(), &install, &env, &UNREACHABLE);

    assert_eq!(version, Some(Ok("0.168.1".to_owned())));
}

#[cfg(unix)]
#[tokio::test]
async fn an_update_runs_the_native_command_and_announces_the_new_versions() {
    let cli = cli("cli-update", AgentId::CODEX, NATIVE_CODEX, CODEX_UPDATING);
    let mut events = cli.state.subscribe();
    let prepared = prepare(AgentId::CODEX, cli.env.clone()).unwrap();
    let env = cli.env.clone();

    start_update(
        &cli.state,
        prepared,
        UPDATE_TIMEOUT,
        &LATEST_0_170,
        Box::new(move || Ok(env)),
        &cli.dir,
    )
    .unwrap();

    let running = next_status(&mut events, |status| status.update.is_some()).await;
    assert_eq!(
        running.update.as_ref().map(|update| update.state),
        Some(CliUpdateState::Running)
    );
    assert_eq!(running.installed_version.as_deref(), Some("0.166.0"));
    let done = next_status(&mut events, ended).await;
    let update = done.update.as_ref().unwrap();
    assert_eq!(update.state, CliUpdateState::Succeeded);
    assert_eq!(update.error, None);
    assert!(update.finished_at.is_some());
    assert_eq!(update.started_at, running.update.unwrap().started_at);
    assert_eq!(done.installed_version.as_deref(), Some("0.167.0"));
    assert_eq!(done.latest_version.as_deref(), Some("0.170.0"));
    assert_eq!(cli.state.agent_clis.statuses(), vec![done]);
}

#[cfg(unix)]
#[tokio::test]
async fn a_failed_update_says_why_with_the_last_line_it_printed() {
    let body = r#"
        case "$1" in
          --version) echo "codex-cli 0.166.0";;
          update) echo "starting" >&2; echo "EACCES: permission denied" >&2; exit 1;;
        esac
    "#;
    let cli = cli("cli-update-fails", AgentId::CODEX, NATIVE_CODEX, body);
    let mut events = cli.state.subscribe();
    let env = cli.env.clone();

    start_update(
        &cli.state,
        prepare(AgentId::CODEX, cli.env.clone()).unwrap(),
        UPDATE_TIMEOUT,
        &LATEST_0_170,
        Box::new(move || Ok(env)),
        &cli.dir,
    )
    .unwrap();

    let done = next_status(&mut events, ended).await;
    let update = done.update.unwrap();
    assert_eq!(update.state, CliUpdateState::Failed);
    assert_eq!(update.error, Some(agent_cli_code::UPDATE_FAILED));
    assert_eq!(update.detail.as_deref(), Some("EACCES: permission denied"));
    assert_eq!(done.installed_version.as_deref(), Some("0.166.0"));
}

#[cfg(unix)]
#[tokio::test]
async fn an_update_that_ends_during_shutdown_records_its_outcome_and_checks_nothing() {
    let body = r#"
        case "$1" in
          --version) echo "codex-cli 0.166.0";;
          update) echo "npm error code EACCES" >&2; echo "see /x.log for the log" >&2; exit 1;;
        esac
    "#;
    let cli = cli("cli-update-shutdown", AgentId::CODEX, NATIVE_CODEX, body);
    let mut events = cli.state.subscribe();
    let refreshed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = refreshed.clone();
    let env = cli.env.clone();
    let prepared = prepare(AgentId::CODEX, cli.env.clone()).unwrap();

    // An update that began before the shutdown, as `start_update` refuses one after it.
    let claim = cli
        .state
        .agent_clis
        .begin_update(&cli.state, AgentId::CODEX)
        .unwrap();
    cli.state
        .agent_clis
        .record_update_started(&cli.state, prepared.found.clone());
    cli.state.stop_all_sessions().await;
    let state = cli.state.clone();
    let output = cli.dir.join("update.log");
    tokio::task::spawn_blocking(move || {
        update::run_update(
            &state,
            claim,
            prepared,
            UPDATE_TIMEOUT,
            &LATEST_0_170,
            Box::new(move || {
                seen.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(env)
            }),
            &output,
        )
    });

    let done = next_status(&mut events, ended).await;
    let update = done.update.unwrap();
    assert_eq!(update.state, CliUpdateState::Failed);
    assert_eq!(update.detail.as_deref(), Some("npm error code EACCES"));
    assert!(!refreshed.load(std::sync::atomic::Ordering::SeqCst));
}

#[cfg(unix)]
#[tokio::test]
async fn an_update_is_refused_while_the_daemon_shuts_down() {
    let cli = cli(
        "cli-update-refused-shutdown",
        AgentId::CODEX,
        NATIVE_CODEX,
        CODEX_UPDATING,
    );
    let mut events = cli.state.subscribe();
    let prepared = prepare(AgentId::CODEX, cli.env.clone()).unwrap();
    cli.state.stop_all_sessions().await;

    let err = start_update(
        &cli.state,
        prepared,
        UPDATE_TIMEOUT,
        &LATEST_0_170,
        Box::new(|| unreachable!("nothing runs")),
        &cli.dir,
    )
    .expect_err("shutting down");
    let asked = update_requested(&cli.state, AgentId::CODEX)
        .await
        .expect_err("shutting down");

    assert_eq!(
        crate::test_support::code_of(&err),
        error_code::DAEMON_SHUTTING_DOWN
    );
    assert_eq!(
        crate::test_support::code_of(&asked),
        error_code::DAEMON_SHUTTING_DOWN
    );
    assert!(events.try_recv().is_err(), "nothing was announced");
    assert!(cli.state.agent_clis.statuses().is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn an_update_that_outruns_its_time_is_stopped_and_says_so() {
    let body = r#"
        case "$1" in
          --version) echo "codex-cli 0.166.0";;
          update) sleep 30;;
        esac
    "#;
    let cli = cli("cli-update-timeout", AgentId::CODEX, NATIVE_CODEX, body);
    let mut events = cli.state.subscribe();
    let env = cli.env.clone();

    start_update(
        &cli.state,
        prepare(AgentId::CODEX, cli.env.clone()).unwrap(),
        Duration::from_millis(300),
        &LATEST_0_170,
        Box::new(move || Ok(env)),
        &cli.dir,
    )
    .unwrap();

    let done = next_status(&mut events, ended).await;
    let update = done.update.unwrap();
    assert_eq!(update.state, CliUpdateState::Failed);
    assert_eq!(update.error, Some(agent_cli_code::UPDATE_TIMED_OUT));
    assert_eq!(update.detail, None);
}

#[cfg(unix)]
#[tokio::test]
async fn an_update_writes_its_output_to_a_file_of_its_own_and_not_to_a_pipe() {
    let body = r#"
        case "$1" in
          --version) echo "codex-cli 0.166.0";;
          update)
            [ -f /dev/stdout ] && [ -f /dev/stderr ] || { echo "not a file" >&2; exit 3; }
            echo "to stdout"; echo "to stderr" >&2;;
        esac
    "#;
    let cli = cli("cli-update-file", AgentId::CODEX, NATIVE_CODEX, body);
    let mut events = cli.state.subscribe();
    let env = cli.env.clone();

    start_update(
        &cli.state,
        prepare(AgentId::CODEX, cli.env.clone()).unwrap(),
        UPDATE_TIMEOUT,
        &LATEST_0_170,
        Box::new(move || Ok(env)),
        &cli.dir,
    )
    .unwrap();

    let done = next_status(&mut events, ended).await;
    assert_eq!(done.update.unwrap().state, CliUpdateState::Succeeded);
    let written = std::fs::read_to_string(cli.dir.join("codex.log")).unwrap();
    assert!(written.contains("to stdout") && written.contains("to stderr"));
}

#[cfg(unix)]
#[tokio::test]
async fn a_second_update_of_an_agent_is_refused_until_the_first_ends() {
    let body = r#"
        case "$1" in
          --version) echo "codex-cli 0.166.0";;
          update) sleep 1;;
        esac
    "#;
    let cli = cli("cli-update-twice", AgentId::CODEX, NATIVE_CODEX, body);
    let mut events = cli.state.subscribe();
    let prepared = || prepare(AgentId::CODEX, cli.env.clone()).unwrap();
    let env = || {
        let env = cli.env.clone();
        Box::new(move || Ok(env)) as Refresh
    };

    start_update(
        &cli.state,
        prepared(),
        UPDATE_TIMEOUT,
        &LATEST_0_170,
        env(),
        &cli.dir,
    )
    .unwrap();
    let err = start_update(
        &cli.state,
        prepared(),
        UPDATE_TIMEOUT,
        &LATEST_0_170,
        env(),
        &cli.dir,
    )
    .expect_err("one update of the agent at a time");
    assert_eq!(
        crate::test_support::code_of(&err),
        error_code::AGENT_CLI_UPDATE_RUNNING
    );
    // Another agent is not held up by it.
    assert!(cli
        .state
        .agent_clis
        .begin_update(&cli.state, AgentId::CLAUDE)
        .is_ok());

    next_status(&mut events, ended).await;
    start_update(
        &cli.state,
        prepared(),
        UPDATE_TIMEOUT,
        &LATEST_0_170,
        env(),
        &cli.dir,
    )
    .expect("the first update is over");
    next_status(&mut events, ended).await;
}

#[cfg(unix)]
#[tokio::test]
async fn an_update_judges_the_agents_availability_again_on_the_refreshed_environment() {
    let cli = cli(
        "cli-update-availability",
        AgentId::CODEX,
        NATIVE_CODEX,
        CODEX_UPDATING,
    );
    let mut events = cli.state.subscribe();
    cli.state
        .set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent: AgentId::CODEX,
            availability: crate::protocol::Availability::Unavailable,
            default_account_dir: None,
        }]);
    let env = cli.env.clone();

    start_update(
        &cli.state,
        prepare(AgentId::CODEX, cli.env.clone()).unwrap(),
        UPDATE_TIMEOUT,
        &LATEST_0_170,
        Box::new(move || Ok(env)),
        &cli.dir,
    )
    .unwrap();

    let mut announced = None;
    tokio::time::timeout(crate::test_support::PATIENCE, async {
        loop {
            match events.recv().await {
                Ok(Event::AgentAvailabilityUpdated { agent_availability }) => {
                    announced = agent_availability
                        .into_iter()
                        .find(|entry| entry.agent == AgentId::CODEX)
                        .map(|entry| entry.availability);
                }
                Ok(Event::AgentCliStatusUpdated { status }) if ended(&status) => break,
                _ => {}
            }
        }
    })
    .await
    .expect("the update ended");
    assert_eq!(announced, Some(crate::protocol::Availability::Available));
    assert_eq!(
        cli.state.agent_availability_of(AgentId::CODEX).availability,
        crate::protocol::Availability::Available
    );
}

#[cfg(unix)]
#[tokio::test]
async fn an_update_whose_refresh_fails_reads_the_status_on_the_environment_it_had() {
    let cli = cli(
        "cli-update-refresh-fails",
        AgentId::CODEX,
        NATIVE_CODEX,
        CODEX_UPDATING,
    );
    let mut events = cli.state.subscribe();

    start_update(
        &cli.state,
        prepare(AgentId::CODEX, cli.env.clone()).unwrap(),
        UPDATE_TIMEOUT,
        &LATEST_0_170,
        Box::new(|| anyhow::bail!("the shell timed out")),
        &cli.dir,
    )
    .unwrap();

    let done = next_status(&mut events, ended).await;
    assert_eq!(done.update.unwrap().state, CliUpdateState::Succeeded);
    assert_eq!(done.installed_version.as_deref(), Some("0.167.0"));
}

#[cfg(unix)]
#[test]
fn an_update_through_npm_runs_npm_in_the_installs_prefix() {
    let body = r#"
        echo "$@" > "$HOME/npm.args"
    "#;
    let dir = crate::test_support::ScratchDir::new("cli-npm-update");
    let prefix = dir.join("prefix");
    let package_dir = prefix.join("lib/node_modules/@openai/codex/bin");
    let script = package_dir.join("codex.js");
    executable(&script);
    std::fs::create_dir_all(prefix.join("bin")).unwrap();
    crate::test_support::symlink(&script, prefix.join("bin/codex")).unwrap();
    let tools = dir.join("tools");
    std::fs::create_dir(&tools).unwrap();
    let mut env = crate::test_support::fake_agent(&tools, "npm", body);
    env.insert(
        "PATH".to_string(),
        format!(
            "{}:{}:/usr/bin:/bin",
            prefix.join("bin").display(),
            tools.display()
        ),
    );
    env.insert("HOME".to_string(), dir.to_string_lossy().into_owned());

    let prepared = prepare(AgentId::CODEX, env.clone()).expect("an npm install is updated");
    assert_eq!(prepared.found.install_method, CliInstallMethod::Npm);
    let status = run_update_tool(
        &prepared.command,
        &env,
        crate::test_support::PATIENCE,
        &dir.join("npm-update.log"),
    )
    .expect("npm ran");

    assert!(status.success());
    let canonical = crate::hostfs::canonical(&prefix).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("npm.args"))
            .unwrap()
            .trim(),
        format!(
            "install -g --prefix {} @openai/codex@latest",
            canonical.display()
        )
    );
}

// -- what an update is refused for ---------------------------------------------------------------

#[test]
fn a_provider_with_no_way_to_update_its_cli_is_refused_as_unsupported() {
    let err = prepare(AgentId::TEST, HashMap::new())
        .err()
        .expect("refused");

    assert_eq!(
        crate::test_support::code_of(&err),
        error_code::PROVIDER_UNSUPPORTED
    );
    assert_eq!(
        crate::test_support::param_of(&err, "operation"),
        "cli_update"
    );
}

#[tokio::test]
async fn an_unsupported_provider_is_refused_before_the_shell_environment_is_taken() {
    let (state, _dir) = crate::test_support::app_state("cli-refuse-unsupported-first");

    // The shell environment cannot be taken under test: were it asked for first, that would be
    // the refusal.
    let err = update_requested(&state, AgentId::TEST)
        .await
        .expect_err("refused");

    assert_eq!(
        crate::test_support::code_of(&err),
        error_code::PROVIDER_UNSUPPORTED
    );
}

#[test]
fn an_agent_that_is_not_installed_is_refused() {
    let err = prepare(AgentId::CODEX, HashMap::new())
        .err()
        .expect("refused");

    assert_eq!(
        crate::test_support::code_of(&err),
        error_code::AGENT_NOT_INSTALLED
    );
}

#[cfg(unix)]
#[test]
fn an_install_that_is_not_recognised_is_refused_and_nothing_is_run() {
    let body = r#"
        case "$1" in
          --version) echo "codex-cli 0.166.0";;
          *) echo "ran $@" >> "$HOME/ran";;
        esac
    "#;
    let cli = cli("cli-refuse-unknown", AgentId::CODEX, "usr/local/real", body);

    let err = prepare(AgentId::CODEX, cli.env.clone())
        .err()
        .expect("refused");

    assert_eq!(
        crate::test_support::code_of(&err),
        error_code::AGENT_CLI_UPDATE_UNSUPPORTED
    );
    assert!(!cli.dir.join("ran").exists());
}

#[cfg(unix)]
#[test]
fn an_npm_install_with_no_npm_to_run_is_refused() {
    let dir = crate::test_support::ScratchDir::new("cli-refuse-no-npm");
    let script = dir.join("prefix/lib/node_modules/@openai/codex/bin/codex.js");
    executable(&script);
    std::fs::create_dir_all(dir.join("prefix/bin")).unwrap();
    crate::test_support::symlink(&script, dir.join("prefix/bin/codex")).unwrap();
    let env = HashMap::from([
        (
            "PATH".to_string(),
            dir.join("prefix/bin").display().to_string(),
        ),
        ("HOME".to_string(), dir.to_string_lossy().into_owned()),
    ]);

    let err = prepare(AgentId::CODEX, env).err().expect("refused");

    assert_eq!(
        crate::test_support::code_of(&err),
        error_code::AGENT_CLI_UPDATE_UNSUPPORTED
    );
}

#[tokio::test]
async fn an_agent_that_is_not_added_is_refused_before_anything_is_run() {
    let (state, _dir) = crate::test_support::app_state("cli-refuse-not-added");
    state
        .store
        .set_added_agents_if_undecided(&[AgentId::CLAUDE])
        .unwrap();

    let err = update_requested(&state, AgentId::CODEX)
        .await
        .expect_err("not added");

    assert_eq!(
        crate::test_support::code_of(&err),
        error_code::AGENT_NOT_ADDED
    );
    assert!(state.agent_clis.statuses().is_empty());
}

// -- claims ----------------------------------------------------------------------------------------

#[tokio::test]
async fn an_agent_is_checked_by_one_request_at_a_time_and_not_again_within_the_floor() {
    let (state, _dir) = crate::test_support::app_state("cli-claims");
    let clis = &state.agent_clis;

    let first = clis.begin_check(&state, AgentId::CODEX).expect("free");
    assert!(
        clis.begin_check(&state, AgentId::CODEX).is_none(),
        "being checked"
    );
    assert!(
        clis.begin_check(&state, AgentId::CLAUDE).is_some(),
        "another agent"
    );
    drop(first);

    assert!(
        clis.begin_check(&state, AgentId::CODEX).is_none(),
        "checked just now"
    );
}

#[tokio::test]
async fn an_agent_being_updated_is_not_checked_and_a_claim_is_released_when_dropped() {
    let (state, _dir) = crate::test_support::app_state("cli-claims-update");
    let clis = &state.agent_clis;

    let update = clis.begin_update(&state, AgentId::CODEX).unwrap();
    assert!(clis.begin_check(&state, AgentId::CODEX).is_none());
    drop(update);

    let panicked = std::thread::spawn({
        let state = state.clone();
        move || {
            let _claim = state
                .agent_clis
                .begin_update(&state, AgentId::CODEX)
                .unwrap();
            panic!("the update task panicked");
        }
    })
    .join();
    assert!(panicked.is_err());
    assert!(
        clis.begin_update(&state, AgentId::CODEX).is_ok(),
        "a panic must not hold the agent for the rest of the daemon's life"
    );
}

#[tokio::test]
async fn a_check_begun_before_an_update_is_dropped_when_it_lands_after() {
    let (state, _dir) = crate::test_support::app_state("cli-stale-check");
    let status = |installed: &str| AgentCliStatus {
        agent: AgentId::CODEX,
        installed_version: Some(installed.to_owned()),
        install_method: CliInstallMethod::Native,
        latest_version: None,
        latest_error: None,
        latest_detail: None,
        checked_at: 1,
        update: None,
    };
    let clis = &state.agent_clis;
    let stale = clis.begin_check(&state, AgentId::CODEX).expect("free");
    let update = clis.begin_update(&state, AgentId::CODEX).unwrap();
    clis.record(&state, status("0.167.0"));
    drop(update);
    let mut events = state.subscribe();

    clis.record_checked(&state, &stale, status("0.166.0"));

    assert!(
        events.try_recv().is_err(),
        "a stale result is not announced"
    );
    assert_eq!(
        clis.statuses()[0].installed_version.as_deref(),
        Some("0.167.0")
    );
}

#[tokio::test]
async fn an_update_that_ends_without_a_word_is_recorded_as_failed() {
    let (state, _dir) = crate::test_support::app_state("cli-update-panics");
    let status = AgentCliStatus {
        agent: AgentId::CODEX,
        installed_version: Some("0.166.0".to_owned()),
        install_method: CliInstallMethod::Native,
        latest_version: None,
        latest_error: None,
        latest_detail: None,
        checked_at: 1,
        update: None,
    };
    let mut events = state.subscribe();

    let panicked = std::thread::spawn({
        let state = state.clone();
        move || {
            let _claim = state
                .agent_clis
                .begin_update(&state, AgentId::CODEX)
                .unwrap();
            state.agent_clis.record_update_started(&state, status);
            panic!("the update task panicked");
        }
    })
    .join();

    assert!(panicked.is_err());
    let announced = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            Event::AgentCliStatusUpdated { status } => Some(status),
            _ => None,
        })
        .last()
        .expect("the end is announced");
    let update = announced.update.clone().unwrap();
    assert_eq!(update.state, CliUpdateState::Failed);
    assert_eq!(update.error, Some(agent_cli_code::UPDATE_FAILED));
    assert_eq!(update.detail.as_deref(), Some("internal error"));
    assert_eq!(state.agent_clis.statuses(), vec![announced]);
}

#[test]
fn a_socks_proxy_that_refuses_is_a_failed_lookup_and_not_a_panic() {
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    let env = HashMap::from([("ALL_PROXY".to_owned(), format!("socks5://127.0.0.1:{port}"))]);
    let started = Instant::now();

    let result = latest::NpmRegistry.latest_version("@openai/codex", &env);

    assert!(matches!(result, Err(Failure::Failed(_))), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn an_updates_output_file_is_private_to_its_owner() {
    use std::os::unix::fs::PermissionsExt;

    let dir = crate::test_support::ScratchDir::new("cli-update-log-mode");
    let log = dir.join("codex.log");
    let mut command = std::process::Command::new("/bin/echo");
    command.arg("hello");

    crate::processes::subprocess::run_to_file(
        &mut command,
        &log,
        Duration::from_secs(30),
        crate::platform::proc::Reach::Group,
        Duration::from_secs(1),
    )
    .unwrap();

    assert_eq!(std::fs::read_to_string(&log).unwrap(), "hello\n");
    assert_eq!(
        std::fs::metadata(&log).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn the_registry_is_reached_through_the_proxy_the_shell_names() {
    let env = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    };
    let host = |env: &HashMap<String, String>| {
        latest::proxy_of(env)
            .unwrap()
            .map(|proxy| proxy.host().to_owned())
    };

    assert_eq!(host(&env(&[])), None);
    assert_eq!(
        host(&env(&[
            ("http_proxy", "http://plain:1"),
            ("HTTPS_PROXY", "http://secure:2")
        ])),
        Some("secure".to_owned())
    );
    assert_eq!(
        host(&env(&[
            ("ALL_PROXY", "http://all:3"),
            ("HTTP_PROXY", "http://plain:1")
        ])),
        Some("all".to_owned())
    );
    assert_eq!(
        host(&env(&[
            ("HTTPS_PROXY", "http://secure:2"),
            ("NO_PROXY", "localhost, .npmjs.org")
        ])),
        None,
        "exempted by a domain it is under"
    );
    assert_eq!(
        host(&env(&[
            ("HTTPS_PROXY", "http://secure:2"),
            ("no_proxy", "internal.example")
        ])),
        Some("secure".to_owned())
    );
    assert_eq!(
        host(&env(&[
            ("HTTPS_PROXY", "http://secure:2"),
            ("NO_PROXY", "*")
        ])),
        None
    );
    assert!(latest::proxy_of(&env(&[("HTTPS_PROXY", "://")])).is_err());
    assert_eq!(
        host(&env(&[("ALL_PROXY", "socks5://127.0.0.1:1080")])),
        Some("127.0.0.1".to_owned())
    );
    assert_eq!(
        host(&env(&[("HTTPS_PROXY", "socks5h://127.0.0.1:7891")])),
        Some("127.0.0.1".to_owned())
    );
}

#[tokio::test]
async fn a_check_that_lands_keeps_the_update_the_status_carries() {
    let (state, _dir) = crate::test_support::app_state("cli-keeps-update");
    let status = |installed: &str| AgentCliStatus {
        agent: AgentId::CODEX,
        installed_version: Some(installed.to_owned()),
        install_method: CliInstallMethod::Native,
        latest_version: None,
        latest_error: None,
        latest_detail: None,
        checked_at: 1,
        update: None,
    };
    state
        .agent_clis
        .record_update_started(&state, status("0.166.0"));
    let mut events = state.subscribe();
    let claim = state
        .agent_clis
        .begin_check(&state, AgentId::CODEX)
        .expect("free");

    state
        .agent_clis
        .record_checked(&state, &claim, status("0.166.0"));

    let Ok(Event::AgentCliStatusUpdated { status }) = events.try_recv() else {
        panic!("the check is announced");
    };
    assert_eq!(
        status.update.map(|update| update.state),
        Some(CliUpdateState::Running)
    );
}

#[test]
fn a_failure_line_is_the_last_one_trimmed_and_cut() {
    let output = |stdout: &str, stderr: &str| Output {
        status: Default::default(),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    };

    assert_eq!(failure_line(&output("out", "a\n  b  \n\n"), "none"), "b");
    assert_eq!(failure_line(&output("only out\n", ""), "none"), "only out");
    assert_eq!(failure_line(&output("", "\n"), "none"), "none");
    let long = "é".repeat(DETAIL_LIMIT + 50);
    let cut = failure_line(&output("", &long), "none");
    assert_eq!(cut.chars().count(), DETAIL_LIMIT + 1);
    assert!(cut.ends_with('…'));
}

/// What npm 10.9.7 printed for a global install into a prefix the user cannot write
/// (`npm install -g --prefix <read-only> is-odd`), with the paths shortened and the stack cut to
/// its first frames.
const NPM_EACCES: &str = "\
npm error code EACCES
npm error syscall mkdir
npm error path /scratch/ro/lib/node_modules/is-odd
npm error errno -13
npm error Error: EACCES: permission denied, mkdir '/scratch/ro/lib/node_modules/is-odd'
npm error     at async mkdir (node:internal/fs/promises:858:10)
npm error     at async /npm/node_modules/@npmcli/arborist/lib/arborist/reify.js:638:20
npm error {
npm error   errno: -13,
npm error   code: 'EACCES',
npm error   syscall: 'mkdir',
npm error   path: '/scratch/ro/lib/node_modules/is-odd'
npm error }
npm error
npm error The operation was rejected by your operating system.
npm error It is likely you do not have the permissions to access this file as the current user
npm error
npm error If you believe this might be a permissions issue, please double-check the
npm error permissions of the file and its containing directories, or try running
npm error the command again as root/Administrator.
npm error A complete log of this run can be found in: /scratch/cache/_logs/2026-11-08T11_33_24_295Z-debug-0.log
";

/// npm 10.9.7 installing a package that is not in the registry, verbatim but for the log path.
const NPM_E404: &str = "\
npm error code E404
npm error 404 Not Found - GET https://registry.npmjs.org/octoboard-surely-nonexistent-pkg-zz9 - Not found
npm error 404
npm error 404  'octoboard-surely-nonexistent-pkg-zz9@*' is not in this registry.
npm error 404
npm error 404 Note that you can also install from a
npm error 404 tarball, folder, http url, or git url.
npm error A complete log of this run can be found in: /scratch/cache/_logs/2026-11-08T11_31_40_577Z-debug-0.log
";

/// npm 10.9.7 against a registry that does not resolve (`--registry http://nonexistent.invalid`),
/// verbatim but for the log path.
const NPM_ENOTFOUND: &str = "\
npm error code ENOTFOUND
npm error syscall getaddrinfo
npm error errno ENOTFOUND
npm error network request to http://nonexistent.invalid/is-odd failed, reason: getaddrinfo ENOTFOUND nonexistent.invalid
npm error network This is a problem related to network connectivity.
npm error network In most cases you are behind a proxy or have bad network settings.
npm error network
npm error network If you are behind a proxy, please make sure that the
npm error network 'proxy' config is set properly.  See: 'npm help config'
npm error A complete log of this run can be found in: /scratch/cache/_logs/2026-11-08T11_31_41_494Z-debug-0.log
";

#[test]
fn the_error_line_of_npms_failure_block_is_its_code_line() {
    let line = |text: &str| error_line(text.as_bytes());

    assert_eq!(line(NPM_EACCES), Some("npm error code EACCES".to_owned()));
    assert_eq!(line(NPM_E404), Some("npm error code E404".to_owned()));
    assert_eq!(
        line(NPM_ENOTFOUND),
        Some("npm error code ENOTFOUND".to_owned())
    );
    assert_eq!(
        line(&format!(
            "added 1 package\nnpm warn deprecated x\n{NPM_E404}"
        )),
        Some("npm error code E404".to_owned()),
        "the block, whatever npm printed before it"
    );
}

#[test]
fn the_error_line_passes_over_the_log_pointer_of_npm_6_to_8_and_its_path() {
    let line = |text: &str| error_line(text.as_bytes());

    assert_eq!(
        line("npm ERR! code E404\nnpm ERR! 404 Not Found\n\nnpm ERR! A complete log of this run can be found in:\nnpm ERR!     /home/u/.npm/_logs/x-debug.log\n"),
        Some("npm ERR! code E404".to_owned())
    );
    assert_eq!(
        line(
            "npm ERR!\nnpm ERR! A complete log of this run can be found in:\nnpm ERR!     /x.log\n"
        ),
        Some("npm ERR!     /x.log".to_owned()),
        "a block that says nothing falls back to the last line"
    );
    assert_eq!(
        line("npm error Log files were not written due to an error writing to the folder: /x\n"),
        Some(
            "npm error Log files were not written due to an error writing to the folder: /x"
                .to_owned()
        ),
        "nothing else to say"
    );
}

#[test]
fn the_error_line_of_another_tool_is_the_last_that_reads_as_one() {
    let line = |text: &str| error_line(text.as_bytes());

    assert_eq!(
        line("Error: first\nerror: second\ndone, see above\n"),
        Some("error: second".to_owned())
    );
    assert_eq!(
        line("==> Upgrading codex\nError: Cask 'codex' is not upgradable\n"),
        Some("Error: Cask 'codex' is not upgradable".to_owned()),
        "Homebrew's single line"
    );
    assert_eq!(line("a\n  b  \n\n"), Some("b".to_owned()), "no error line");
    assert_eq!(line("\n \n"), None);
}

#[test]
fn the_codes_of_a_status_are_the_documented_ones() {
    let codes: Vec<Code> = vec![
        Failure::Failed(String::new()).code(),
        Failure::Unreadable.code(),
    ];
    assert_eq!(
        codes,
        [
            agent_cli_code::LATEST_FAILED,
            agent_cli_code::LATEST_UNREADABLE
        ]
    );
}

#[test]
fn the_registrys_reply_is_read_for_its_version_and_nothing_else_passes() {
    use latest::version_field;

    assert_eq!(
        version_field(
            r#"{"name":"@openai/codex","version":"0.170.0","dist":{}}"#,
            "version"
        ),
        Ok("0.170.0".to_owned())
    );
    for reply in [
        "",
        "not json",
        "{}",
        r#"{"version":7}"#,
        r#"{"version":"latest"}"#,
    ] {
        assert_eq!(
            version_field(reply, "version"),
            Err(Failure::Unreadable),
            "{reply:?}"
        );
    }
}
