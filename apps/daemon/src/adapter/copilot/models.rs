//! GitHub Copilot CLI's model catalogue, asked of the CLI's SDK server (`copilot --headless
//! --no-auto-update --stdio`, JSON-RPC 2.0 with each message under a `Content-Length` header):
//! `models.list`, which needs no session, answers `{models, auto, imageModels}`, each model with
//! its id, its name, its policy and, for a model that takes one, its reasoning efforts. Auto, which
//! `--model auto` picks and which takes no effort, is offered first while its default tier is
//! enabled. The ACP mode lists no models (1.0.114; `docs/agent-cli-reference.md`, "GitHub Copilot
//! CLI", "Model and reasoning effort").
//!
//! The server is never run in the account's own directory: a start rewrites a `config.json` that
//! is not as the running version writes it. It is run with `COPILOT_HOME` naming a directory of
//! Octoboard's own, private to the user, whose `config.json` holds the account's sign-in keys only
//! (`super::accounts::sign_in_keys`, which names the login and holds no credential), which signs
//! it in as the same login through the credential store, and with its log there too
//! (`--log-dir`); the directory is removed after the run. Where the account keeps its token in its
//! own `config.json` (the user's choice of plain text), the token is handed to the server through
//! `COPILOT_GITHUB_TOKEN`, which it takes before any stored login (measured on 1.0.114:
//! `auth.getStatus` answers `authType: "env"` and the list is the same), and is never written:
//! Octoboard uses the credential but does not copy it.
//!
//! The list is asked for only once `auth.getStatus` names the login the account's directory
//! names: a directory naming a login whose credential is gone signs the server in as the GitHub
//! CLI's login instead, without saying so, and would list that login's models. A directory that
//! names no login lists whatever the server signs in as, as a session there would.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde_json::{json, Value};

use crate::adapter::account_cli::{self, NotRun, RunHome};
use crate::agents::json_rpc;
use crate::agents::models::{failed, not_run, timed_out, unreadable, Listing, ModelCatalogue};
use crate::processes::subprocess::{
    run_conversation_traced, Bounds, Conversation, ExchangeFailure, ExchangeTrace, Framing, Heard,
    AGENT_REPLY_GRACE,
};
use crate::protocol::{AgentId, AgentModel, ModelsUnavailable};
use crate::providers::copilot::COPILOT_STATE_FILE;

/// The arguments that start the SDK server, before `--log-dir`.
pub(super) const SERVER_ARGS: &[&str] = &["--headless", "--no-auto-update", "--stdio"];

/// The id of the first request, the sign-in status.
const STATUS_ID: i64 = 1;

/// The id of the second request, the list, written once the status is answered.
const LIST_ID: i64 = 2;

/// The variables Copilot takes a token from, in the order it takes them, before any stored login.
/// A listing hands the account's own token through the first only where the environment gives
/// none of them, so that the run signs in as a session in that environment would.
const TOKEN_VARS: &[&str] = &["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"];

/// The model id that has Copilot pick the model itself (`--help`, `--model`).
pub(super) const AUTO_MODEL: &str = "auto";

/// The most the server may print before it has answered. A free plan's answer is about 2 KB;
/// a paid plan's, with a few dozen models and their capabilities, stays in the tens of kilobytes.
const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

/// The name prefix for Copilot's private background queries.
const QUERY_DIR_PREFIX: &str = "copilot-query-";

/// The first-launch date Copilot itself writes into a new home's `config.json`, so that the run
/// has nothing of its own first launch to write.
const FIRST_LAUNCH_AT: &str = "2026-03-11T00:00:00.000Z";

pub(super) struct CopilotCatalogue;

impl ModelCatalogue for CopilotCatalogue {
    fn list(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _run_homes: &Path,
        timeout: Duration,
    ) -> Listing {
        list_in(&crate::paths::run_dir(), shell_env, config_dir, timeout)
    }

    /// The server answered in 2 to 4.5 seconds from its start (it loads its whole runtime first),
    /// so its deadline leaves room for startup.
    fn deadline(&self) -> Duration {
        Duration::from_secs(30)
    }
}

/// The listing, its run given a directory of its own made under `parent` and removed after.
pub(super) fn list_in(
    parent: &Path,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
) -> Listing {
    let agent = AgentId::COPILOT;
    let account = account_cli::account_dir(agent, config_dir, shell_env);
    // Read once: the sign-in keys, the token and the login named all come from this one value.
    let state = super::accounts::state_of(&account);
    let own = match private_query_home(parent, state.as_ref()) {
        Ok(own) => own,
        Err(err) => return Listing::unavailable(failed(agent, &format!("{err:#}"))),
    };
    let logs = own.path().join("logs").to_string_lossy().into_owned();
    let mut args = SERVER_ARGS.to_vec();
    args.extend(["--log-dir", logs.as_str()]);
    let mut run = match account_cli::admitted_command_in_home(
        agent,
        shell_env,
        config_dir,
        Some(&own),
        &args,
    ) {
        Ok(run) => run,
        Err(refused) => return Listing::unavailable(session_unavailable(Failure::NotRun(refused))),
    };
    if let Some(token) = token_to_hand(state.as_ref(), shell_env) {
        run.command.env(TOKEN_VARS[0], token);
    }
    let named = state.as_ref().and_then(super::accounts::named_login);
    let answer = RefCell::new(None);
    let never = AtomicBool::new(false);
    let steps = [
        vec![request(STATUS_ID, "auth.getStatus").to_string()],
        vec![request(LIST_ID, "models.list").to_string()],
    ];
    let outcome = run_conversation_traced(
        &mut run.command,
        Conversation {
            framing: Framing::ContentLength,
            steps: &steps,
        },
        &Bounds::agent_query(timeout, OUTPUT_LIMIT),
        &never,
        AGENT_REPLY_GRACE,
        &mut ExchangeTrace::default(),
        |message| hear(message, named.as_deref(), &mut answer.borrow_mut()),
    );
    let failure = match (outcome, answer.into_inner()) {
        (Ok(_), Some(Ok(result))) => {
            return match parse(&result) {
                Some(models) => Listing::found(models),
                None => Listing::unavailable(unreadable(agent)),
            }
        }
        (Ok(_), Some(Err(failure))) => failure,
        (Ok(_), None) => Failure::Unreadable,
        (Err(ExchangeFailure::TimedOut(_)), _) => Failure::TimedOut,
        (Err(other), _) => Failure::Failed(other.to_string()),
    };
    Listing::unavailable(session_unavailable(failure))
}

enum Failure {
    NotRun(NotRun),
    TimedOut,
    Failed(String),
    Unreadable,
}

fn session_unavailable(failure: Failure) -> ModelsUnavailable {
    let agent = AgentId::COPILOT;
    match failure {
        Failure::NotRun(refused) => not_run(agent, refused),
        Failure::TimedOut => timed_out(agent),
        Failure::Failed(detail) => failed(agent, &detail),
        Failure::Unreadable => unreadable(agent),
    }
}

/// A request of `method` with no parameter: `auth.getStatus`, the login the server is signed in
/// as; `models.list`, that login's models.
fn request(id: i64, method: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {}})
}

/// The token the account's own `config.json` (parsed as `state`) keeps, to be handed to the run
/// through its environment, where the environment gives the run no token of its own.
pub(super) fn token_to_hand(
    state: Option<&Value>,
    shell_env: &HashMap<String, String>,
) -> Option<String> {
    let given = TOKEN_VARS
        .iter()
        .any(|var| shell_env.get(*var).is_some_and(|value| !value.is_empty()));
    if given {
        return None;
    }
    state.and_then(super::accounts::token_in_file)
}

/// Whether the status answer `message` lets the list be asked for: always where the account's
/// directory names no login, else only where the server is signed in as that login. Where a login
/// is named, a refusal of the status request is the reason in its own words, not a sign-in as
/// someone else.
fn signed_in_as(message: &Value, named: Option<&str>) -> Result<(), Failure> {
    let Some(named) = named else {
        return Ok(());
    };
    if let Some(detail) = json_rpc::error_detail(message) {
        return Err(Failure::Failed(detail));
    }
    let authenticated = message
        .pointer("/result/isAuthenticated")
        .and_then(Value::as_bool)
        == Some(true);
    let login = message.pointer("/result/login").and_then(Value::as_str);
    if authenticated && login.is_some_and(|login| login.eq_ignore_ascii_case(named)) {
        Ok(())
    } else {
        Err(Failure::Failed(format!(
            "Copilot is not signed in as {named}, the login this account names"
        )))
    }
}

/// What the listing makes of one message: the status answer lets the list be written or ends the
/// run with the reason; the answer to the list, a result or a refusal, is the reply; anything
/// else the server sends is passed over.
fn hear(message: &str, named: Option<&str>, answer: &mut Option<Result<Value, Failure>>) -> Heard {
    let Ok(message) = serde_json::from_str::<Value>(message) else {
        return Heard::Nothing;
    };
    if json_rpc::is_reply_to(&message, STATUS_ID) {
        return match signed_in_as(&message, named) {
            Ok(()) => Heard::Next,
            Err(failure) => {
                *answer = Some(Err(failure));
                Heard::Reply
            }
        };
    }
    if !json_rpc::is_reply_to(&message, LIST_ID) {
        return Heard::Nothing;
    }
    *answer = Some(match json_rpc::error_detail(&message) {
        Some(detail) => Err(Failure::Failed(detail)),
        None => match message.get("result") {
            Some(result) if result.is_object() => Ok(result.clone()),
            _ => Err(Failure::Unreadable),
        },
    });
    Heard::Reply
}

/// The models of a `models.list` result: Auto first while the default of its tiers is enabled,
/// then every listed model in the server's order. `None` for a result in no shape this knows
/// (no `models` array); an account with no model of its own and no Auto gives an empty list.
pub(super) fn parse(result: &Value) -> Option<Vec<AgentModel>> {
    let listed = result.get("models")?.as_array()?;
    let mut models: Vec<AgentModel> = auto_entry(result.get("auto")).into_iter().collect();
    models.extend(listed.iter().filter_map(model_entry));
    Some(models)
}

/// Auto, offered while its default tier is enabled; the name Copilot shows for that tier
/// follows it. It takes no effort.
fn auto_entry(auto: Option<&Value>) -> Option<AgentModel> {
    let auto = auto?;
    let default = auto.get("defaultTier")?.as_str()?;
    let tier = auto
        .get("tiers")?
        .as_array()?
        .iter()
        .find(|tier| tier.get("id").and_then(Value::as_str) == Some(default))?;
    let enabled = tier
        .pointer("/status/enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !enabled {
        return None;
    }
    let name = match tier.get("displayName").and_then(Value::as_str) {
        Some(tier) if !tier.trim().is_empty() => format!("Auto ({})", tier.trim()),
        _ => "Auto".to_string(),
    };
    Some(AgentModel {
        id: AUTO_MODEL.to_string(),
        name,
        efforts: Vec::new(),
        version: None,
        available: Some(true),
    })
}

/// One listed model: its id, its name (the id where it has none), the efforts it lists (none where
/// it lists none, which the SDK's types say of a model that takes no effort), and whether the
/// account may use it by its policy (`enabled` yes, `disabled` no, anything else not known).
fn model_entry(entry: &Value) -> Option<AgentModel> {
    let id = entry.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    let name = entry
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(id);
    let efforts = entry
        .get("supportedReasoningEfforts")
        .and_then(crate::agents::models::entry_levels)
        .unwrap_or_default();
    let available = match entry.pointer("/policy/state").and_then(Value::as_str) {
        Some("enabled") => Some(true),
        Some("disabled") => Some(false),
        _ => None,
    };
    Some(AgentModel {
        id: id.to_string(),
        name: name.to_string(),
        efforts,
        version: None,
        available,
    })
}

/// Makes a background query's `COPILOT_HOME` under `parent`, with a
/// `config.json` holding the first-launch date and the sign-in keys of the account's own
/// `config.json`, parsed as `state`; removed when dropped, however the run ended.
pub(super) fn private_query_home(parent: &Path, state: Option<&Value>) -> anyhow::Result<RunHome> {
    use anyhow::Context;

    let own = RunHome::make_private_run_dir(parent, QUERY_DIR_PREFIX)?;
    let mut state = state.map(super::accounts::sign_in_keys).unwrap_or_default();
    state.insert("firstLaunchAt".to_string(), json!(FIRST_LAUNCH_AT));
    let file = own.path().join(COPILOT_STATE_FILE);
    // Private from its first byte: it holds the account's sign-in keys.
    crate::platform::fs::create_private(
        &file,
        std::fs::OpenOptions::new().write(true).create_new(true),
    )
    .and_then(|mut created| {
        std::io::Write::write_all(&mut created, Value::Object(state).to_string().as_bytes())
    })
    .with_context(|| format!("writing {}", file.display()))?;
    Ok(own)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::models::COMMAND_TIMEOUT;
    use crate::protocol::models_unavailable_code as code;
    use crate::test_support::{fake_agent, ScratchDir};
    use std::path::PathBuf;

    /// What `models.list` answered on 1.0.114 for the user's free-plan account, signed in through
    /// the GitHub CLI's token (`testdata/copilot_models_list_free_1.0.114.json`).
    const FREE: &str = include_str!("../../../testdata/copilot_models_list_free_1.0.114.json");

    fn ids(models: &[AgentModel]) -> Vec<&str> {
        models.iter().map(|model| model.id.as_str()).collect()
    }

    /// The free plan lists no model of its own: Auto alone is offered, under its default tier's
    /// name, usable and taking no effort.
    #[test]
    fn the_free_plan_offers_auto_alone() {
        let result: Value = serde_json::from_str(FREE).expect("capture");
        let models = parse(&result).expect("a known shape");
        assert_eq!(
            models,
            [AgentModel {
                id: "auto".to_string(),
                name: "Auto (Standard)".to_string(),
                efforts: Vec::new(),
                version: None,
                available: Some(true),
            }]
        );
    }

    /// A listed model follows Auto with its own name, the efforts it lists in their order (none
    /// where it lists none) and its policy as whether it may be used; an entry with no id is no
    /// model, and one with no name is shown by its id.
    #[test]
    fn a_listed_model_carries_its_name_efforts_and_policy() {
        let result = json!({
            "models": [
                {"id": "gpt-x", "name": "GPT X", "policy": {"state": "enabled"},
                 "supportedReasoningEfforts": ["low", "medium", "high"],
                 "defaultReasoningEffort": "medium"},
                {"id": "claude-y", "name": "Claude Y", "policy": {"state": "disabled"}},
                {"id": "z-model", "policy": {"state": "unconfigured"}},
                {"name": "No id"},
                {"id": "  "},
            ],
            "auto": {"defaultTier": "balance", "tiers": [
                {"id": "balance", "displayName": "Standard", "status": {"enabled": true}}
            ]},
        });
        let models = parse(&result).expect("a known shape");
        assert_eq!(ids(&models), ["auto", "gpt-x", "claude-y", "z-model"]);
        assert_eq!(models[1].name, "GPT X");
        assert_eq!(models[1].efforts, ["low", "medium", "high"]);
        assert_eq!(models[1].available, Some(true));
        assert!(models[2].efforts.is_empty());
        assert_eq!(models[2].available, Some(false));
        assert_eq!(models[3].name, "z-model");
        assert_eq!(models[3].available, None);
    }

    /// Auto is offered only while its default tier is enabled; with no tier to go by, or a
    /// default that is disabled or not among the tiers, it is not.
    #[test]
    fn auto_is_offered_only_while_its_default_tier_is_enabled() {
        let with_auto = |auto: Value| {
            ids(&parse(&json!({"models": [], "auto": auto})).unwrap())
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        let tier = |enabled: bool| json!({"id": "balance", "status": {"enabled": enabled}});
        assert_eq!(
            with_auto(json!({"defaultTier": "balance", "tiers": [tier(true)]})),
            ["auto"]
        );
        assert!(with_auto(json!({"defaultTier": "balance", "tiers": [tier(false)]})).is_empty());
        assert!(with_auto(json!({"defaultTier": "other", "tiers": [tier(true)]})).is_empty());
        assert!(with_auto(json!({"tiers": [tier(true)]})).is_empty());
        assert!(with_auto(Value::Null).is_empty());
        let unnamed = parse(
            &json!({"models": [], "auto": {"defaultTier": "balance", "tiers": [tier(true)]}}),
        )
        .unwrap();
        assert_eq!(unnamed[0].name, "Auto");
    }

    /// A result with no `models` array is in no shape this knows.
    #[test]
    fn a_result_without_a_models_array_is_unreadable() {
        assert_eq!(parse(&json!({})), None);
        assert_eq!(parse(&json!({"models": "gpt-x"})), None);
        assert_eq!(parse(&json!({"models": []})), Some(Vec::new()));
    }

    /// A fake server that keeps its arguments, its `COPILOT_HOME`'s `config.json`, the modes of
    /// that directory and file, and the token it was handed in `dir`, then answers each framed
    /// request it reads, in order, with the next of `replies` framed by length (keeping the
    /// request as `request_<n>.json`), and waits for its input to end.
    fn server(dir: &ScratchDir, replies: &[&str]) -> HashMap<String, String> {
        let mut answers = String::new();
        for (n, reply) in replies.iter().enumerate() {
            let body = dir.join(format!("reply_{}.json", n + 1));
            std::fs::write(&body, reply).expect("reply");
            answers.push_str(&format!(
                "IFS= read -r header || exit 0\n\
                 IFS= read -r blank\n\
                 n=$(printf '%s' \"$header\" | tr -dc 0-9)\n\
                 dd bs=1 count=\"$n\" 2>/dev/null > '{request}'\n\
                 printf 'Content-Length: %d\\r\\n\\r\\n' $(wc -c < '{body}')\n\
                 cat '{body}'\n",
                request = dir.join(format!("request_{}.json", n + 1)).display(),
                body = body.display(),
            ));
        }
        for n in 1..=replies.len() {
            let _ = std::fs::remove_file(dir.join(format!("request_{n}.json")));
        }
        let script = format!(
            "echo \"$@\" > '{args}'\n\
             cp \"$COPILOT_HOME/config.json\" '{state}'\n\
             ls -ld \"$COPILOT_HOME\" | cut -c1-10 > '{modes}'\n\
             printf '%s' \"${{COPILOT_GITHUB_TOKEN-}}\" > '{token}'\n\
             {answers}\
             cat > /dev/null\n",
            args = dir.join("args").display(),
            state = dir.join("state.json").display(),
            modes = dir.join("modes").display(),
            token = dir.join("token").display(),
        );
        fake_agent(dir, "copilot", &script)
    }

    /// The status answer of a server signed in as `login`.
    fn status(login: &str) -> String {
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"isAuthenticated\":true,\
             \"authType\":\"user\",\"host\":\"https://github.com\",\"login\":\"{login}\"}}}}"
        )
    }

    /// The status answer of a server signed in as no one.
    const SIGNED_OUT: &str = "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"isAuthenticated\":false,\
        \"statusMessage\":\"Not authenticated\"}}";

    /// The list answer of the free plan.
    fn free_list() -> String {
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}}",
            FREE.trim()
        )
    }

    fn requested(dir: &ScratchDir, n: usize) -> Option<Value> {
        let text = std::fs::read_to_string(dir.join(format!("request_{n}.json"))).ok()?;
        Some(serde_json::from_str(&text).expect("a framed request"))
    }

    /// An account directory under `dir` whose `config.json` is `state`.
    fn account_with(dir: &ScratchDir, state: &str) -> PathBuf {
        let account = dir.join("account");
        std::fs::create_dir_all(&account).expect("account");
        std::fs::write(account.join("config.json"), state).expect("state");
        account
    }

    fn listed(dir: &ScratchDir, env: &HashMap<String, String>, account: Option<&Path>) -> Listing {
        list_in(&dir.join("listings"), env, account, COMMAND_TIMEOUT)
    }

    fn left_behind(dir: &ScratchDir) -> Vec<std::ffi::OsString> {
        std::fs::read_dir(dir.join("listings")).map_or_else(
            |_| Vec::new(),
            |entries| entries.flatten().map(|entry| entry.file_name()).collect(),
        )
    }

    /// The listing starts the SDK server with its log in a directory of its own, private to the
    /// user, asks the sign-in status, then `models.list` once the status names the account's
    /// login, reads the framed answers, and leaves nothing behind: the account's directory is not
    /// the one the server is given, and is not written to.
    #[test]
    fn the_listing_asks_the_sdk_server_in_a_directory_of_its_own() {
        let dir = ScratchDir::new("copilot-models-server");
        let env = server(&dir, &[&status("octocat"), &free_list()]);
        let state =
            "{\"lastLoggedInUser\":{\"host\":\"https://github.com\",\"login\":\"octocat\"},\
                     \"loggedInUsers\":[{\"host\":\"https://github.com\",\"login\":\"octocat\"}],\
                     \"trustedFolders\":[\"/somewhere\"]}";
        let account = account_with(&dir, state);

        let listing = listed(&dir, &env, Some(&account));
        assert_eq!(listing.unavailable, None);
        assert_eq!(ids(&listing.models), ["auto"]);

        let args = std::fs::read_to_string(dir.join("args")).expect("args");
        let own = dir.join("listings").join(QUERY_DIR_PREFIX);
        assert!(
            args.starts_with(&format!(
                "--headless --no-auto-update --stdio --log-dir {}",
                own.display()
            )),
            "{args}"
        );
        let first = requested(&dir, 1).expect("the status asked");
        assert_eq!(first["method"], "auth.getStatus");
        assert_eq!(first["id"], STATUS_ID);
        let second = requested(&dir, 2).expect("the list asked");
        assert_eq!(second["method"], "models.list");
        assert_eq!(second["id"], LIST_ID);
        // The sign-in keys are copied, nothing else of the account's file.
        let given: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("state.json")).unwrap())
                .expect("state given");
        assert_eq!(given["lastLoggedInUser"]["login"], "octocat");
        assert_eq!(given["loggedInUsers"][0]["login"], "octocat");
        assert_eq!(given["firstLaunchAt"], FIRST_LAUNCH_AT);
        assert!(given.get("trustedFolders").is_none());
        if cfg!(unix) {
            assert_eq!(
                std::fs::read_to_string(dir.join("modes")).unwrap().trim(),
                "drwx------"
            );
        }
        assert_eq!(left_behind(&dir), Vec::<std::ffi::OsString>::new());
        assert_eq!(
            std::fs::read_to_string(account.join("config.json")).unwrap(),
            state
        );
    }

    /// The account's file with its token kept in plain text (made up).
    const PLAINTEXT: &str =
        "{\"authTokens\":{\"https://github.com:octocat\":\"FAKE-TOKEN-NOT-A-REAL-ONE\"},\
         \"lastLoggedInUser\":{\"host\":\"https://github.com\",\"login\":\"octocat\"},\
         \"loggedInUsers\":[{\"host\":\"https://github.com\",\"login\":\"octocat\"}]}";

    /// A token the account keeps in its own file is handed to the server through
    /// `COPILOT_GITHUB_TOKEN` and never written into the listing's file; where the environment
    /// gives a token of its own, none is handed, so the run signs in as a session there would.
    #[test]
    fn a_token_kept_in_the_file_is_handed_through_the_environment_never_written() {
        let dir = ScratchDir::new("copilot-models-token");
        let account = account_with(&dir, PLAINTEXT);
        let env = server(&dir, &[&status("octocat"), &free_list()]);
        let listing = listed(&dir, &env, Some(&account));
        assert_eq!(listing.unavailable, None);
        assert_eq!(
            std::fs::read_to_string(dir.join("token")).unwrap(),
            "FAKE-TOKEN-NOT-A-REAL-ONE"
        );
        let given = std::fs::read_to_string(dir.join("state.json")).unwrap();
        assert!(!given.contains("FAKE-TOKEN"), "{given}");
        assert!(!given.contains("authTokens"), "{given}");
        assert_eq!(
            given.parse::<Value>().unwrap()["lastLoggedInUser"]["login"],
            "octocat"
        );

        for var in TOKEN_VARS {
            let mut own = server(&dir, &[&status("octocat"), &free_list()]);
            own.insert(var.to_string(), "the environment's own".to_string());
            let listing = listed(&dir, &own, Some(&account));
            assert_eq!(listing.unavailable, None);
            let handed = std::fs::read_to_string(dir.join("token")).unwrap();
            let expected = if *var == TOKEN_VARS[0] {
                "the environment's own"
            } else {
                ""
            };
            assert_eq!(handed, expected, "{var}");
        }
        assert_eq!(left_behind(&dir), Vec::<std::ffi::OsString>::new());
    }

    /// Where the account names a login and the server is signed in as another (the GitHub CLI's,
    /// where the login's credential is gone) or as no one, the list is never asked for and the
    /// reason says so; an account that names no login lists whatever the server signs in as.
    #[test]
    fn the_list_is_asked_only_while_signed_in_as_the_login_the_account_names() {
        let dir = ScratchDir::new("copilot-models-login");
        let account = account_with(
            &dir,
            "{\"lastLoggedInUser\":{\"host\":\"https://github.com\",\"login\":\"octocat\"}}",
        );
        for answer in [status("someone-else"), SIGNED_OUT.to_string()] {
            let env = server(&dir, &[&answer, &free_list()]);
            let listing = listed(&dir, &env, Some(&account));
            let reason = listing.unavailable.expect("a reason");
            assert_eq!(reason.code, code::COMMAND_FAILED);
            let detail = reason.params.get("detail").map(String::as_str);
            assert!(
                detail.is_some_and(|detail| detail.contains("not signed in as octocat")),
                "{answer}: {detail:?}"
            );
            assert!(listing.models.is_empty());
            assert_eq!(requested(&dir, 2), None, "{answer}");
        }
        // A refusal of the status request is that refusal, in its own words.
        let refused = "{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32603,\"message\":\"status lookup failed\"}}";
        let env = server(&dir, &[refused, &free_list()]);
        let listing = listed(&dir, &env, Some(&account));
        let reason = listing.unavailable.expect("a reason");
        assert_eq!(reason.code, code::COMMAND_FAILED);
        let detail = reason.params.get("detail").cloned().unwrap_or_default();
        assert!(detail.contains("status lookup failed"), "{detail}");
        assert!(!detail.contains("not signed in"), "{detail}");
        assert_eq!(requested(&dir, 2), None);
        // The login is compared as GitHub compares logins, whatever its case.
        let env = server(&dir, &[&status("OctoCat"), &free_list()]);
        assert_eq!(listed(&dir, &env, Some(&account)).unavailable, None);

        let nameless = ScratchDir::new("copilot-models-nameless");
        let account = account_with(&nameless, "{}");
        let env = server(&nameless, &[&status("whoever"), &free_list()]);
        let listing = listed(&nameless, &env, Some(&account));
        assert_eq!(listing.unavailable, None);
        assert_eq!(ids(&listing.models), ["auto"]);
        assert_eq!(left_behind(&dir), Vec::<std::ffi::OsString>::new());
    }

    /// A refusal of the request, an answer in no known shape, and a server that ends without
    /// answering are each the list's reason, and leave nothing behind.
    #[test]
    fn a_refused_unreadable_or_missing_answer_is_the_reason() {
        let dir = ScratchDir::new("copilot-models-failures");
        // An account that names no login, so the status lets any answer through.
        let account = account_with(&dir, "{}");
        let account = Some(account.as_path());
        let refused = "{\"jsonrpc\":\"2.0\",\"id\":2,\"error\":{\"code\":-32000,\"message\":\"Not authenticated\"}}";
        let listing = listed(&dir, &server(&dir, &[SIGNED_OUT, refused]), account);
        assert_eq!(
            listing.unavailable.expect("a reason").code,
            code::COMMAND_FAILED
        );

        let unreadable = "{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"auto\":{}}}";
        let listing = listed(
            &dir,
            &server(&dir, &[&status("octocat"), unreadable]),
            account,
        );
        assert_eq!(
            listing.unavailable.expect("a reason").code,
            code::UNREADABLE_OUTPUT
        );

        let env = fake_agent(&dir, "copilot", "exit 1");
        let listing = listed(&dir, &env, None);
        assert_eq!(
            listing.unavailable.expect("a reason").code,
            code::COMMAND_FAILED
        );
        assert_eq!(left_behind(&dir), Vec::<std::ffi::OsString>::new());
    }
}
