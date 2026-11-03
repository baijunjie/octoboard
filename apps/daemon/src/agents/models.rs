//! The models an agent currently offers, for the dialogs that let the user choose one. Nothing is
//! hard-coded per version: Claude Code resolves its own aliases, and the other two agents are
//! asked for their catalogues. What each agent offers is measured in the
//! "Model and reasoning effort" note of each agent's section ("Claude Code", "Codex",
//! "Grok Build") in `docs/agent-cli-reference.md`, and summarised here.
//!
//! - **Claude Code** has no list command, but its `fable`, `opus`, `sonnet` and `haiku` aliases
//!   always resolve to the newest model of their family, so those four are the answer, best
//!   first, and no subprocess is run. Claude Code caches a catalogue per account on disk
//!   (`cache/model-catalog/` in its configuration directory, found through the organization and
//!   account ids in the global config file), which says which model each alias resolves to and
//!   whether the account may use it; each entry carries that as `version` and `available`. A
//!   missing or unreadable catalogue leaves both unknown, and the four aliases still listed. The
//!   `best` alias is not offered: it falls back to Opus without checking that the account may use
//!   it, so the first available entry of the list is what a session with no model setting gets
//!   ([`Listing::default_model`]), unless Claude Code's own configuration names a model, which
//!   it then applies itself (see `launch_settings::launch_default_model`).
//! - **Codex** answers `codex debug models` with its catalogue as JSON: the models whose
//!   `visibility` is `list`, ordered by ascending `priority`.
//! - **Grok Build** answers `grok models` with the visible ids as text, which also refreshes
//!   `models_cache.json` in its home; that file, a map keyed by id, supplies each model's name,
//!   `hidden` flag and reasoning efforts, which the text does not carry. The text is the list and
//!   the file only decorates it, so a missing or unreadable file still leaves a list, of bare ids.
//!
//! A catalogue belongs to an account, so the command runs in the login-shell snapshot with the
//! agent's configuration-directory variable pointed at the account's directory. That is what a
//! Claude Code or Codex session gets, but not a Grok one: a session runs against a per-session
//! private home, precisely so that its settings cannot leak into the user's `config.toml`, while
//! the listing points `GROK_HOME` at the account directory itself, necessarily, since that is
//! what refreshes the `models_cache.json` read afterwards. The side effect is that opening the
//! Advanced section of a dialog writes into the user's Grok account directory.
//!
//! Every model carries effort levels, never "unknown": the model's own where its catalogue entry
//! gives them (an empty list meaning it takes none), else the agent's measured set. Levels are
//! not versioned the way models are, so the fallback is safe, and it is what Claude Code gets for
//! every alias that has no catalogue row. Failing to produce a list is never an error to the
//! caller: it is an empty list with a reason (`ModelsUnavailable`), because the form asking must
//! stay usable. Results are cached per agent and configuration directory ([`ModelCache`]), except
//! Claude Code's, which is two small file reads and is read afresh each time so that the default
//! model it names never goes stale.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::adapter;
use crate::adapter::account_cli::{self, NotRun, QueryFailure};
use crate::adapter::claude::settings::global_config_file;
use crate::agents::layout::{CLAUDE_MODEL_CATALOGUE_DIR, GROK_MODELS_CACHE};
use crate::env_shell;
use crate::processes::subprocess::BoundedFailure;
use crate::protocol::{models_unavailable_code as code, Agent, AgentModel, ModelsUnavailable};

/// How long a catalogue command may run. The commands answer in well under a second from a
/// warm cache and a few seconds when they refresh over the network; this leaves a wide margin
/// above that while keeping a form with no network from waiting longer than a person tolerates
/// (compare the ten seconds the login-shell snapshot gets).
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

/// The most a catalogue command may print or a file read for a listing may hold: Grok's models
/// cache, and Claude Code's global config file and cached catalogue. Codex's output runs to about
/// 600 KB because it carries every model's full instructions; this is far above that and far
/// below anything that would hurt.
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

/// How long a list that was read is reused (not Claude Code's, which is never cached; see the
/// module documentation). A catalogue changes when the agent is updated or the
/// account's plan changes, both rare against how often a dialog is opened, and a refresh touches
/// the network; ten minutes absorbs a session of opening and closing dialogs while still showing
/// a new model within the time it takes to notice one.
const FOUND_TTL: Duration = Duration::from_secs(10 * 60);

/// How long a failure is reused. Short, because the likely causes (a network that comes back, a
/// login completed in a terminal) are fixed by the user within seconds and the next dialog should
/// see it, but long enough that reopening a dialog does not wait out a timeout again each time.
const FAILED_TTL: Duration = Duration::from_secs(30);

/// Claude Code's aliases and how to show them, best first: each resolves to the newest model of
/// its family, which an account may or may not be allowed to use.
const CLAUDE_MODELS: [(&str, &str); 4] = [
    ("fable", "Fable"),
    ("opus", "Opus"),
    ("sonnet", "Sonnet"),
    ("haiku", "Haiku"),
];

/// The alias Claude Code accepted before the list stopped offering it. A stored value of it is
/// read as no model at all ([`legacy_claude_model`]).
const CLAUDE_LEGACY_BEST: &str = "best";

/// Whether `model` is the retired `best` alias, which is read as unset wherever it is stored.
pub fn legacy_claude_model(model: &str) -> bool {
    model == CLAUDE_LEGACY_BEST
}

/// The ids Claude Code's aliases go by, for a tool description that tells a model what it may pick.
pub fn claude_model_ids() -> Vec<&'static str> {
    CLAUDE_MODELS.iter().map(|(id, _)| *id).collect()
}

/// What `--effort` takes. Claude Code lowers a level the chosen model does not support to `high`
/// rather than refusing it, so it is offered for every alias whose catalogue row gives none.
/// Measured on 2.1.296.
const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// What Codex's `model_reasoning_effort` parser accepts, measured on 0.160.0: the levels offered
/// for a model whose catalogue entry gives none.
const CODEX_EFFORTS: [&str; 6] = ["low", "medium", "high", "xhigh", "max", "ultra"];

/// What Grok Build's `--reasoning-effort` parser accepts, measured on 1.0.50: the levels offered
/// for a model whose catalogue entry gives none.
const GROK_EFFORTS: [&str; 4] = ["low", "medium", "high", "xhigh"];

/// The agent's measured level set, for a model whose catalogue says nothing usable.
fn fallback_efforts(agent: Agent) -> Vec<String> {
    let levels: &[&str] = match agent {
        Agent::Claude => &CLAUDE_EFFORTS,
        Agent::Codex => &CODEX_EFFORTS,
        Agent::Grok => &GROK_EFFORTS,
    };
    levels.iter().map(|level| level.to_string()).collect()
}

/// The levels a catalogue entry lists under `field`, reading each element's `key`. `None` when
/// the entry says nothing usable (no array, or a non-empty one none of whose elements parse), so
/// the caller falls back; an empty array is `Some([])`, a model that takes none.
fn entry_efforts(entry: &Value, field: &str, key: &str) -> Option<Vec<String>> {
    let array = entry.get(field)?.as_array()?;
    let levels: Vec<String> = array
        .iter()
        .filter_map(|level| level.get(key).and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    (!levels.is_empty() || array.is_empty()).then_some(levels)
}

/// The answer to one `list_agent_models`: the models, or why there are none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub models: Vec<AgentModel>,
    pub unavailable: Option<ModelsUnavailable>,
}

impl Listing {
    /// The model a session of the agent gets when no setting names one: the first listed model the
    /// account may use. `None` when no model is known to be usable, which leaves the choice to
    /// the agent's own default.
    pub fn default_model(&self) -> Option<&str> {
        self.models
            .iter()
            .find(|model| model.available == Some(true))
            .map(|model| model.id.as_str())
    }

    fn found(models: Vec<AgentModel>) -> Self {
        Self {
            models,
            unavailable: None,
        }
    }

    fn unavailable(reason: ModelsUnavailable) -> Self {
        Self {
            models: Vec::new(),
            unavailable: Some(reason),
        }
    }
}

/// The listing for `agent` under `config_dir` (the pinned account's directory, `None` for the
/// default account), in the user's shell environment. Blocking: it may run a subprocess. Claude
/// Code's is read through [`claude_listing`] with the environment its caller holds instead, since
/// it is not cached.
pub fn load(agent: Agent, config_dir: Option<&Path>) -> Listing {
    // Human-paced, but the result is cached anyway, so the cached snapshot keeps a dialog from
    // forking a login shell on top of the command.
    match env_shell::cached_snapshot() {
        Ok(shell_env) => list(agent, &shell_env, config_dir, COMMAND_TIMEOUT),
        Err(err) => {
            let detail = format!("{err:#}");
            Listing::unavailable(ModelsUnavailable::new(
                code::SHELL_ENVIRONMENT_FAILED,
                format!("the shell environment could not be captured: {detail}"),
                &[("detail", &detail)],
            ))
        }
    }
}

/// The aliases of [`CLAUDE_MODELS`] with what the account's cached catalogue says about each (see
/// [`claude_catalogue`]); every alias is listed whatever it says, and one it cannot answer for has
/// no `version` and no `available`.
pub(crate) fn claude_listing(
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
) -> Listing {
    let (catalogue, denied) = claude_catalogue(shell_env, config_dir);
    Listing::found(
        CLAUDE_MODELS
            .iter()
            .map(|(id, name)| claude_model(id, name, catalogue.as_ref(), &denied))
            .collect(),
    )
}

/// The cached catalogue of the account whose global config file `shell_env` and `config_dir` lead
/// to, and the model ids that config file says the account is not entitled to. Claude Code names
/// the catalogue file `<organization id>-<first 12 hex digits of the SHA-256 of the account
/// id>-cc.json`, both ids lower-cased, because several accounts' catalogues can share a
/// directory. Either is `None` or empty when the file is not there or cannot be read. A catalogue
/// past its own `staleAt` is still the best answer there is, so it is read all the same.
fn claude_catalogue(
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
) -> (Option<Value>, Vec<String>) {
    let read = |file: &Path| -> Option<Value> {
        serde_json::from_str(&crate::hostfs::read_regular_text(file, OUTPUT_LIMIT).ok()?).ok()
    };
    let Some(global) = read(&global_config_file(shell_env, config_dir)) else {
        return (None, Vec::new());
    };
    let denied = global
        .get("modelAccessCache")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| entry.get("entitled").and_then(Value::as_bool) == Some(false))
        .filter_map(|entry| entry.get("apiName").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    let account = global.get("oauthAccount");
    let id_of = |key: &str| {
        account
            .and_then(|account| account.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_lowercase)
    };
    let catalogue = id_of("organizationUuid")
        .zip(id_of("accountUuid"))
        .and_then(|(organization, account)| {
            let file = claude_catalogue_file(
                &adapter::account_dir(Agent::Claude, config_dir, shell_env),
                &organization,
                &account,
            );
            read(&file)
        });
    (catalogue, denied)
}

/// Where Claude Code caches the catalogue of the account `account_id` of organization
/// `organization_id` (both lower-cased) under the configuration directory `dir`.
fn claude_catalogue_file(dir: &Path, organization_id: &str, account_id: &str) -> PathBuf {
    let digest = Sha256::digest(account_id.as_bytes());
    let hash: String = digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    dir.join(CLAUDE_MODEL_CATALOGUE_DIR)
        .join(format!("{organization_id}-{hash}-cc.json"))
}

/// One alias as the catalogue describes it: the row of the model the alias targets
/// (`provider_alias_targets`) gives its version name (`name`, "Opus 5.5"), its effort levels, and
/// whether the account can use it: not `disabled`, not in the `deprecated` section, and not
/// refused by `denied`. A catalogue with no such row leaves all three at their fallbacks.
fn claude_model(
    alias: &str,
    name: &str,
    catalogue: Option<&Value>,
    denied: &[String],
) -> AgentModel {
    let config = catalogue.and_then(|catalogue| catalogue.get("catalog")?.get("config"));
    let target = config
        .and_then(|config| {
            config
                .get("provider_alias_targets")?
                .get(alias)?
                .get("default")
        })
        .and_then(Value::as_str);
    let row = target.and_then(|target| {
        config?
            .get("models")?
            .as_array()?
            .iter()
            .find(|row| row.get("id").and_then(Value::as_str) == Some(target))
    });
    let version = row
        .and_then(|row| row.get("name")?.as_str())
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .map(str::to_string);
    let available = row.map(|row| {
        row.get("disabled").and_then(Value::as_bool) != Some(true)
            && row.get("section").and_then(Value::as_str) != Some("deprecated")
            && !target.is_some_and(|target| denied.iter().any(|api_name| api_name == target))
    });
    let efforts = row
        .and_then(|row| entry_levels(row.get("runtime")?.get("effort_levels")?))
        .unwrap_or_else(|| fallback_efforts(Agent::Claude));
    AgentModel {
        id: alias.to_string(),
        name: name.to_string(),
        efforts,
        version,
        available,
    }
}

/// The strings of a catalogue's level array, `None` when it has none to give.
fn entry_levels(levels: &Value) -> Option<Vec<String>> {
    let levels: Vec<String> = levels
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    (!levels.is_empty()).then_some(levels)
}

/// [`load`] over an already-captured environment and with its deadline given, which is what makes
/// the failure paths testable without waiting out [`COMMAND_TIMEOUT`].
pub(crate) fn list(
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
) -> Listing {
    match agent {
        Agent::Claude => claude_listing(shell_env, config_dir),
        Agent::Codex => run(agent, shell_env, config_dir, &["debug", "models"], timeout)
            .and_then(|stdout| parse_codex(&stdout).map_err(|()| unreadable(agent)))
            .map_or_else(Listing::unavailable, Listing::found),
        Agent::Grok => run(agent, shell_env, config_dir, &["models"], timeout)
            .and_then(|stdout| {
                let ids = parse_grok_list(&stdout);
                if ids.is_empty() {
                    return Err(unreadable(agent));
                }
                let home = adapter::account_dir(agent, config_dir, shell_env);
                // Read after the command, which is what refreshes it.
                let cache =
                    crate::hostfs::read_regular_text(&home.join(GROK_MODELS_CACHE), OUTPUT_LIMIT)
                        .ok();
                // The cache can drop every printed id as hidden; an empty list needs its reason.
                let models = decorate_grok(&ids, cache.as_deref());
                if models.is_empty() {
                    return Err(unreadable(agent));
                }
                Ok(models)
            })
            .map_or_else(Listing::unavailable, Listing::found),
    }
}

fn unreadable(agent: Agent) -> ModelsUnavailable {
    ModelsUnavailable::new(
        code::UNREADABLE_OUTPUT,
        format!("{} printed no model list that could be read", agent.label()),
        &[("agent", agent.label())],
    )
}

/// Runs the agent's catalogue command and returns its stdout. The environment is what a session
/// of the account would get: the shell snapshot with the account's directory exported under the
/// agent's own variable, and no more.
fn run(
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    args: &[&str],
    timeout: Duration,
) -> Result<String, ModelsUnavailable> {
    let output = account_cli::query(
        agent,
        shell_env,
        config_dir,
        args,
        timeout,
        OUTPUT_LIMIT,
        None,
    )
    .map_err(|failure| match failure {
        QueryFailure::NotRun(refused) => match refused {
            NotRun::DirMissing(_) => failed(agent, &refused.detail()),
            NotRun::BinaryNotFound(binary) => ModelsUnavailable::new(
                code::BINARY_NOT_FOUND,
                refused.detail(),
                &[("binary", binary)],
            ),
        },
        QueryFailure::Bounded(BoundedFailure::TimedOut(_)) => ModelsUnavailable::new(
            code::TIMED_OUT,
            format!("{} did not list its models in time", agent.label()),
            &[("agent", agent.label())],
        ),
        QueryFailure::Bounded(other) => failed(agent, &other.to_string()),
    })?;
    if !output.status.success() {
        let detail = account_cli::last_stderr_line(&output);
        let detail = if detail.is_empty() {
            output.status.to_string()
        } else {
            format!("{}: {detail}", output.status)
        };
        return Err(failed(agent, &detail));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn failed(agent: Agent, detail: &str) -> ModelsUnavailable {
    ModelsUnavailable::new(
        code::COMMAND_FAILED,
        format!("listing {}'s models failed: {detail}", agent.label()),
        &[("agent", agent.label()), ("detail", detail)],
    )
}

/// Codex's `debug models` JSON: the entries whose `visibility` is `list`, by ascending `priority`
/// (the server sorts them; the bundled fallback does not, so this does), as slug, display name
/// and the efforts of `supported_reasoning_levels`, or Codex's whole set where the entry gives
/// none that can be read. `Err` when there is no `models` array or no
/// listed model in it.
fn parse_codex(stdout: &str) -> Result<Vec<AgentModel>, ()> {
    let catalogue: Value = serde_json::from_str(stdout).map_err(|_| ())?;
    let entries = catalogue
        .get("models")
        .and_then(Value::as_array)
        .ok_or(())?;
    let mut listed: Vec<(i64, AgentModel)> = entries
        .iter()
        .filter(|entry| entry.get("visibility").and_then(Value::as_str) == Some("list"))
        .filter_map(|entry| {
            let slug = entry.get("slug").and_then(Value::as_str)?.trim();
            if slug.is_empty() {
                return None;
            }
            let name = entry
                .get("display_name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(slug);
            let efforts = entry_efforts(entry, "supported_reasoning_levels", "effort")
                .unwrap_or_else(|| fallback_efforts(Agent::Codex));
            let priority = entry
                .get("priority")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MAX);
            Some((
                priority,
                AgentModel {
                    id: slug.to_string(),
                    name: name.to_string(),
                    efforts,
                    version: None,
                    available: None,
                },
            ))
        })
        .collect();
    if listed.is_empty() {
        return Err(());
    }
    // Stable, so models of equal priority keep the order the catalogue gave them.
    listed.sort_by_key(|(priority, _)| *priority);
    Ok(listed.into_iter().map(|(_, model)| model).collect())
}

/// The ids in `grok models`' output: the run of bullet lines (`  * id (default)`, `  - id`) right
/// under its `Available models:` heading, in the order printed. The run ends at the first line
/// that is not a bullet, so whatever Grok prints after the block (tips, a rule) is never read as
/// a model. Empty when the heading or the lines are not there, which the caller reads as
/// unreadable output.
fn parse_grok_list(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("Available models"))
        .skip(1)
        .skip_while(|line| line.trim().is_empty())
        .map_while(grok_bullet_id)
        .map(str::to_string)
        .collect()
}

/// The id of one bullet line, or `None` for any other line. A bullet whose remainder is empty or
/// starts with another `-` is a rule (`-----`), not a model.
fn grok_bullet_id(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix(['*', '-'])?;
    let id = rest.split_whitespace().next()?;
    (!id.starts_with('-')).then_some(id)
}

/// Turns the ids `grok models` printed into models, using `models_cache.json` (`cache`, when it
/// could be read) for what the text lacks: an entry's `info.name`, its `hidden` flag, which drops
/// it, and its `reasoning_efforts` values. An id the file does not know, or no file at all, gives
/// the id as its own name; an entry with no usable efforts gets Grok's whole set.
fn decorate_grok(ids: &[String], cache: Option<&str>) -> Vec<AgentModel> {
    let cache: Option<Value> = cache.and_then(|text| serde_json::from_str(text).ok());
    ids.iter()
        .filter_map(|id| {
            let info = cache
                .as_ref()
                .and_then(|cache| cache.get("models"))
                .and_then(|models| models.get(id))
                .and_then(|entry| entry.get("info"));
            let Some(info) = info else {
                return Some(AgentModel {
                    id: id.clone(),
                    name: id.clone(),
                    efforts: fallback_efforts(Agent::Grok),
                    version: None,
                    available: None,
                });
            };
            if info.get("hidden").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            let name = info
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(id);
            let efforts = if info
                .get("supports_reasoning_effort")
                .and_then(Value::as_bool)
                == Some(false)
            {
                Vec::new()
            } else {
                entry_efforts(info, "reasoning_efforts", "value")
                    .unwrap_or_else(|| fallback_efforts(Agent::Grok))
            };
            Some(AgentModel {
                id: id.clone(),
                name: name.to_string(),
                efforts,
                version: None,
                available: None,
            })
        })
        .collect()
}

/// The listings read lately, keyed by agent and the configuration directory they were read under
/// (`None` for the default account), so two accounts of one agent never share an answer and an
/// account pointed at another directory starts afresh. Failures are kept too, for [`FAILED_TTL`].
#[derive(Default)]
pub struct ModelCache {
    entries: Mutex<HashMap<CacheKey, (Instant, Listing)>>,
    /// Claude Code listings a test put in place of reading its files; see [`Self::seed`].
    #[cfg(test)]
    seeded: Mutex<HashMap<CacheKey, Listing>>,
}

type CacheKey = (Agent, Option<PathBuf>);

impl ModelCache {
    /// Makes the Claude Code listing of `config_dir` this one, for a test that must not read the
    /// machine's own files. Claude Code's listing is never cached, so this is not `get_or_load`.
    #[cfg(test)]
    pub fn seed(&self, config_dir: Option<&Path>, listing: Listing) {
        let key = (Agent::Claude, config_dir.map(Path::to_path_buf));
        self.seeded
            .lock()
            .expect("model cache lock poisoned")
            .insert(key, listing);
    }

    /// The listing [`Self::seed`] put in place for `config_dir`, if any.
    #[cfg(test)]
    pub fn seeded(&self, config_dir: Option<&Path>) -> Option<Listing> {
        let key = (Agent::Claude, config_dir.map(Path::to_path_buf));
        self.seeded
            .lock()
            .expect("model cache lock poisoned")
            .get(&key)
            .cloned()
    }

    /// The cached listing for the key if it is still fresh, else `load()`'s, which is then kept.
    /// `load` runs outside the lock, so two requests arriving together both run it and the later
    /// answer wins; harmless, and it spares every other key from waiting on a slow command.
    pub fn get_or_load(
        &self,
        agent: Agent,
        config_dir: Option<&Path>,
        load: impl FnOnce() -> Listing,
    ) -> Listing {
        self.get_or_load_at(Instant::now(), agent, config_dir, load)
    }

    /// `now` is only the instant the freshness check is made at, so that tests can move it; an
    /// entry's own expiry is measured from when `load` returned, since `load` may run for as long
    /// as [`COMMAND_TIMEOUT`] and a failure that timed out must not start out partly used up.
    fn get_or_load_at(
        &self,
        now: Instant,
        agent: Agent,
        config_dir: Option<&Path>,
        load: impl FnOnce() -> Listing,
    ) -> Listing {
        let key = (agent, config_dir.map(Path::to_path_buf));
        if let Some((expires_at, listing)) = self
            .entries
            .lock()
            .expect("model cache lock poisoned")
            .get(&key)
        {
            if now < *expires_at {
                return listing.clone();
            }
        }
        let listing = load();
        let ttl = if listing.unavailable.is_some() {
            FAILED_TTL
        } else {
            FOUND_TTL
        };
        self.entries
            .lock()
            .expect("model cache lock poisoned")
            .insert(key, (Instant::now() + ttl, listing.clone()));
        listing
    }
}

/// A scratch Claude Code home and catalogue for tests that must not read the machine's own.
#[cfg(test)]
pub(crate) mod claude_fixtures {
    use super::*;
    use crate::test_support::ScratchDir;

    /// A Claude Code catalogue with the four aliases targeting rows that are usable (Opus),
    /// `disabled` with a notice (Fable), `deprecated` (Sonnet), and missing (Haiku).
    pub const CLAUDE_CATALOGUE: &str = r#"{"version":2,"fetchedAt":1,"staleAt":2,"catalog":{
        "surface":"cc","config":{
        "models":[
            {"id":"claude-opus-5-5","name":"Opus 5.5","section":"main",
             "runtime":{"effort_levels":["low","high"]}},
            {"id":"claude-fable-5-1","name":"Fable 5.1","section":"main","disabled":true,
             "notice":{"text":"Fable needs Max"}},
            {"id":"claude-sonnet-5-5","name":"Sonnet 5.5","section":"deprecated"}],
        "provider_alias_targets":{
            "fable":{"default":"claude-fable-5-1"},"opus":{"default":"claude-opus-5-5"},
            "sonnet":{"default":"claude-sonnet-5-5"},"haiku":{"default":"claude-haiku-5-5"}}}}}"#;

    /// Claude Code's global config file `config` for one account and, when `catalogue` is given,
    /// its cached catalogue where Claude Code keeps it under the configuration directory `dir`.
    pub fn write_claude_account(
        config: &Path,
        dir: &Path,
        access_cache: &str,
        catalogue: Option<&str>,
    ) {
        std::fs::write(
            config,
            format!(
                r#"{{"oauthAccount":{{"organizationUuid":"ORG-1","accountUuid":"Acct-1"}},
                    "modelAccessCache":{access_cache}}}"#
            ),
        )
        .unwrap();
        if let Some(catalogue) = catalogue {
            let file = claude_catalogue_file(dir, "org-1", "acct-1");
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, catalogue).unwrap();
        }
    }

    /// A home with the account of [`write_claude_account`] in it. Returns the shell environment
    /// pointing at it.
    pub fn claude_home(
        dir: &ScratchDir,
        access_cache: &str,
        catalogue: Option<&str>,
    ) -> HashMap<String, String> {
        let home: &Path = dir;
        write_claude_account(
            &home.join(".claude.json"),
            &home.join(".claude"),
            access_cache,
            catalogue,
        );
        HashMap::from([("HOME".to_string(), home.to_string_lossy().into_owned())])
    }
}

#[cfg(test)]
mod tests {
    use super::claude_fixtures::{claude_home, CLAUDE_CATALOGUE};
    use super::*;
    use crate::test_support::{fake_agent, ScratchDir};

    const CODEX_CATALOGUE: &str = include_str!("../../testdata/codex_debug_models.json");
    const GROK_LIST: &str = include_str!("../../testdata/grok_models.txt");
    const GROK_CACHE: &str = include_str!("../../testdata/grok_models_cache.json");

    fn ids(models: &[AgentModel]) -> Vec<&str> {
        models.iter().map(|model| model.id.as_str()).collect()
    }

    fn reason(listing: &Listing) -> crate::protocol::Code {
        listing.unavailable.as_ref().expect("a reason").code
    }

    #[test]
    fn the_catalogue_file_is_named_by_the_organization_and_a_hash_of_the_account() {
        // The digest of `acct-1`, first twelve hex digits.
        assert_eq!(
            claude_catalogue_file(Path::new("/c"), "org-1", "acct-1"),
            Path::new("/c/cache/model-catalog/org-1-ba36a4edd92d-cc.json")
        );
    }

    #[test]
    fn claude_lists_its_aliases_best_first_with_what_the_catalogue_says() {
        let dir = ScratchDir::new("agent-models-claude-catalogue");
        let env = claude_home(&dir, "[]", Some(CLAUDE_CATALOGUE));
        let listing = claude_listing(&env, None);
        assert_eq!(ids(&listing.models), ["fable", "opus", "sonnet", "haiku"]);
        assert!(listing.unavailable.is_none());
        let facts: Vec<_> = listing
            .models
            .iter()
            .map(|model| (model.version.as_deref(), model.available))
            .collect();
        assert_eq!(
            facts,
            [
                (Some("Fable 5.1"), Some(false)),
                (Some("Opus 5.5"), Some(true)),
                (Some("Sonnet 5.5"), Some(false)),
                (None, None),
            ]
        );
        // The row's own levels where it gives them, else the measured set.
        assert_eq!(listing.models[1].efforts, ["low", "high"].map(String::from));
        assert_eq!(listing.models[3].efforts, CLAUDE_EFFORTS.map(String::from));
        // The first usable model is the one a session without a setting gets.
        assert_eq!(listing.default_model(), Some("opus"));
    }

    #[test]
    fn an_account_the_config_file_says_is_not_entitled_is_unavailable() {
        let dir = ScratchDir::new("agent-models-claude-denied");
        let env = claude_home(
            &dir,
            r#"[{"apiName":"claude-opus-5-5","entitled":false},
                {"apiName":"claude-fable-5-1","entitled":true}]"#,
            Some(&CLAUDE_CATALOGUE.replace(r#","disabled":true"#, "")),
        );
        let listing = claude_listing(&env, None);
        let available: Vec<_> = listing.models.iter().map(|m| m.available).collect();
        assert_eq!(available, [Some(true), Some(false), Some(false), None]);
        assert_eq!(listing.default_model(), Some("fable"));
    }

    #[test]
    fn a_missing_or_unreadable_catalogue_leaves_the_aliases_listed_and_unknown() {
        let dir = ScratchDir::new("agent-models-claude-missing");
        let broken = claude_home(&dir, "[]", Some("not json"));
        let missing = HashMap::from([(
            "HOME".to_string(),
            dir.join("nowhere").to_string_lossy().into_owned(),
        )]);
        for env in [broken, missing] {
            let listing = claude_listing(&env, None);
            assert_eq!(ids(&listing.models), ["fable", "opus", "sonnet", "haiku"]);
            for model in &listing.models {
                assert_eq!((model.version.as_deref(), model.available), (None, None));
                assert_eq!(model.efforts, CLAUDE_EFFORTS.map(String::from));
            }
            // Nothing is known usable, so Claude Code is left to choose.
            assert_eq!(listing.default_model(), None);
        }
    }

    #[test]
    fn a_pinned_account_reads_the_catalogue_in_its_own_directory() {
        let dir = ScratchDir::new("agent-models-claude-pinned");
        let pinned = dir.join("pinned");
        let file = claude_catalogue_file(&pinned, "org-1", "acct-1");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, CLAUDE_CATALOGUE).unwrap();
        std::fs::write(
            pinned.join(".claude.json"),
            r#"{"oauthAccount":{"organizationUuid":"Org-1","accountUuid":"ACCT-1"}}"#,
        )
        .unwrap();
        let listing = claude_listing(&HashMap::new(), Some(&pinned));
        assert_eq!(listing.models[1].version.as_deref(), Some("Opus 5.5"));
    }

    #[test]
    fn the_retired_best_alias_is_recognised_but_never_offered() {
        let dir = ScratchDir::new("agent-models-retired-best");
        let env = HashMap::from([("HOME".to_string(), dir.to_string_lossy().into_owned())]);
        assert!(legacy_claude_model("best"));
        assert!(!legacy_claude_model("opus"));
        let listing = claude_listing(&env, None);
        assert!(!ids(&listing.models).contains(&"best"));
        // No binary on PATH and still an answer, because nothing is run.
        assert_eq!(list(Agent::Claude, &env, None, COMMAND_TIMEOUT), listing);
    }

    #[test]
    fn codex_keeps_the_listed_models_by_ascending_priority() {
        let models = parse_codex(CODEX_CATALOGUE).expect("parses");
        // The fixture is in the bundled catalogue's order, which is not priority order, and holds
        // two hidden entries.
        assert_eq!(
            ids(&models),
            ["gpt-6.1-sol", "gpt-6-astra", "gpt-6-luna", "gpt-5.5"]
        );
        assert_eq!(models[0].name, "GPT-6.1-Sol");
        assert_eq!(
            models[3].efforts,
            ["low", "medium", "high", "xhigh"].map(String::from)
        );
        assert_eq!(models[1].efforts.len(), 6);
    }

    #[test]
    fn codex_entries_without_a_name_degrade_and_without_levels_take_codexs_whole_set() {
        let models = parse_codex(
            r#"{"models":[{"slug":"a","visibility":"list"},{"visibility":"list"},
                {"slug":"b","display_name":" ","visibility":"list","priority":-1}]}"#,
        )
        .expect("parses");
        assert_eq!(ids(&models), ["b", "a"]);
        assert_eq!(models[0].name, "b");
        assert_eq!(models[0].efforts, fallback_efforts(Agent::Codex));
        assert_eq!(models[0].efforts.last().map(String::as_str), Some("ultra"));
    }

    #[test]
    fn codex_levels_are_the_models_own_none_or_the_agents_set() {
        let models = parse_codex(
            r#"{"models":[
                {"slug":"own","visibility":"list","priority":1,
                 "supported_reasoning_levels":[{"effort":"low"},{"effort":"high"}]},
                {"slug":"none","visibility":"list","priority":2,"supported_reasoning_levels":[]},
                {"slug":"junk","visibility":"list","priority":3,
                 "supported_reasoning_levels":[{"label":"x"},7]}]}"#,
        )
        .expect("parses");
        assert_eq!(models[0].efforts, ["low", "high"]);
        assert_eq!(models[1].efforts, Vec::<String>::new());
        assert_eq!(models[2].efforts, fallback_efforts(Agent::Codex));
    }

    #[test]
    fn codex_output_with_no_listed_model_or_no_json_is_unreadable() {
        assert!(parse_codex("not json").is_err());
        assert!(parse_codex(r#"{"data":[]}"#).is_err());
        assert!(parse_codex(r#"{"models":[{"slug":"x","visibility":"hide"}]}"#).is_err());
    }

    #[test]
    fn grok_ids_come_from_the_text_in_the_order_printed() {
        assert_eq!(
            parse_grok_list(GROK_LIST),
            ["grok-4.7", "grok-4.7-build-fast", "grok-4.6", "grok-4.5"]
        );
        assert!(parse_grok_list("You are logged in.\n\nDefault model: x\n").is_empty());
        assert!(parse_grok_list("").is_empty());
    }

    #[test]
    fn grok_ids_stop_at_the_end_of_the_block() {
        let tips = format!("{GROK_LIST}\nTips:\n  - run `grok --help` for more\n  * grok-fake\n");
        let rule = format!("{GROK_LIST}  -----\n  - grok-fake\n");
        let blank_then_bullet = format!("{GROK_LIST}\n  - run `grok --help` for more\n");
        for text in [tips, rule, blank_then_bullet] {
            assert_eq!(parse_grok_list(&text).len(), 4, "{text}");
        }
        assert_eq!(grok_bullet_id("  -----"), None);
        assert_eq!(grok_bullet_id("  -"), None);
        assert_eq!(grok_bullet_id("  * grok-4.7 (default)"), Some("grok-4.7"));
    }

    #[test]
    fn grok_names_and_efforts_come_from_the_cache_and_hidden_models_are_skipped() {
        let mut printed = parse_grok_list(GROK_LIST);
        printed.push("grok-internal".to_string());
        let models = decorate_grok(&printed, Some(GROK_CACHE));
        assert_eq!(
            ids(&models),
            ["grok-4.7", "grok-4.7-build-fast", "grok-4.6", "grok-4.5"]
        );
        assert_eq!(models[1].name, "Grok 4.7 Fast");
        assert_eq!(
            models[0].efforts,
            ["xhigh", "high", "medium", "low"].map(String::from)
        );
        assert_eq!(models[3].efforts.len(), 3);
    }

    #[test]
    fn grok_without_a_readable_cache_still_lists_bare_ids_with_groks_whole_set() {
        let printed = parse_grok_list(GROK_LIST);
        for cache in [None, Some("garbage")] {
            let models = decorate_grok(&printed, cache);
            assert_eq!(models.len(), 4);
            assert_eq!(models[2].name, "grok-4.6");
            assert_eq!(models[2].efforts, fallback_efforts(Agent::Grok));
        }
    }

    #[test]
    fn a_grok_model_that_takes_no_effort_reports_none_and_one_that_says_nothing_the_set() {
        let cache = r#"{"models":{
            "m":{"info":{"name":"M","hidden":false,
                "supports_reasoning_effort":false,"reasoning_efforts":[]}},
            "e":{"info":{"name":"E","hidden":false,"reasoning_efforts":[]}},
            "n":{"info":{"name":"N","hidden":false}},
            "j":{"info":{"name":"J","hidden":false,"reasoning_efforts":[{"id":"x"}]}}}}"#;
        let ids = ["m", "e", "n", "j"].map(String::from);
        let models = decorate_grok(&ids, Some(cache));
        assert_eq!(models[0].efforts, Vec::<String>::new());
        assert_eq!(models[1].efforts, Vec::<String>::new());
        assert_eq!(models[2].efforts, fallback_efforts(Agent::Grok));
        assert_eq!(models[3].efforts, fallback_efforts(Agent::Grok));
    }

    #[test]
    fn a_grok_cache_that_hides_every_printed_model_is_unreadable_not_empty() {
        let dir = ScratchDir::new("agent-models-grok-hidden");
        let home = dir.join("grok-home");
        std::fs::create_dir_all(&home).unwrap();
        let cache = ["grok-4.7", "grok-4.7-build-fast", "grok-4.6", "grok-4.5"]
            .map(|id| format!(r#""{id}":{{"info":{{"hidden":true}}}}"#))
            .join(",");
        std::fs::write(
            home.join(GROK_MODELS_CACHE),
            format!(r#"{{"models":{{{cache}}}}}"#),
        )
        .unwrap();
        let fixture = dir.join("models.txt");
        std::fs::write(&fixture, GROK_LIST).unwrap();
        let env = fake_agent(&dir, "grok", &format!("cat '{}'", fixture.display()));

        let listing = list(Agent::Grok, &env, Some(&home), COMMAND_TIMEOUT);
        assert!(listing.models.is_empty());
        assert_eq!(reason(&listing), code::UNREADABLE_OUTPUT);
    }

    /// A listing admitted after the account's directory was moved (a removal held it back) runs
    /// nothing, which would create the directory again.
    #[test]
    fn a_listing_admitted_after_the_directory_was_moved_runs_nothing() {
        let dir = ScratchDir::new("agent-models-moved");
        let ran = dir.join("ran");
        let env = fake_agent(&dir, "codex", &format!("touch '{}'", ran.display()));
        let account = dir.join("account");
        let listing = list(Agent::Codex, &env, Some(&account), COMMAND_TIMEOUT);
        assert_eq!(reason(&listing), code::COMMAND_FAILED);
        assert!(!ran.exists(), "the CLI was not run");
        assert!(!account.exists());
    }

    /// The command runs from the environment given, with the account's directory exported, and its
    /// stdout is parsed end to end. The fake agent prints the fixture only when `CODEX_HOME` is
    /// the directory the account names.
    #[test]
    fn codex_is_run_with_the_accounts_directory_and_its_output_parsed() {
        let dir = ScratchDir::new("agent-models-codex");
        let fixture = dir.join("catalogue.json");
        std::fs::write(&fixture, CODEX_CATALOGUE).unwrap();
        let account = dir.join("account");
        std::fs::create_dir(&account).unwrap();
        let env = fake_agent(
            &dir,
            "codex",
            &format!(
                "[ \"$1 $2\" = 'debug models' ] && [ \"$CODEX_HOME\" = '{}' ] && cat '{}'",
                account.display(),
                fixture.display()
            ),
        );

        let scoped = list(Agent::Codex, &env, Some(&account), COMMAND_TIMEOUT);
        assert_eq!(scoped.unavailable, None);
        assert_eq!(scoped.models.len(), 4);
        // Another directory is another account: the same command finds nothing to print.
        let other = list(Agent::Codex, &env, None, COMMAND_TIMEOUT);
        assert_eq!(reason(&other), code::COMMAND_FAILED);
    }

    #[test]
    fn grok_is_run_in_the_accounts_home_and_its_cache_read_from_there() {
        let dir = ScratchDir::new("agent-models-grok");
        let home = dir.join("grok-home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(GROK_MODELS_CACHE), GROK_CACHE).unwrap();
        let fixture = dir.join("models.txt");
        std::fs::write(&fixture, GROK_LIST).unwrap();
        let env = fake_agent(
            &dir,
            "grok",
            &format!(
                "[ \"$1\" = models ] && [ \"$GROK_HOME\" = '{}' ] && cat '{}'",
                home.display(),
                fixture.display()
            ),
        );

        let listing = list(Agent::Grok, &env, Some(&home), COMMAND_TIMEOUT);
        assert_eq!(listing.unavailable, None);
        assert_eq!(listing.models[1].name, "Grok 4.7 Fast");
    }

    #[test]
    fn a_missing_binary_is_a_reason_not_an_error() {
        let dir = ScratchDir::new("agent-models-missing");
        let env = HashMap::from([("PATH".to_string(), dir.to_string_lossy().into_owned())]);
        for agent in [Agent::Codex, Agent::Grok] {
            let listing = list(agent, &env, None, COMMAND_TIMEOUT);
            assert!(listing.models.is_empty());
            assert_eq!(reason(&listing), code::BINARY_NOT_FOUND);
        }
    }

    #[test]
    fn a_non_zero_exit_carries_the_last_line_of_stderr() {
        let dir = ScratchDir::new("agent-models-exit");
        let env = fake_agent(
            &dir,
            "codex",
            "echo noise >&2; echo not logged in >&2; exit 3",
        );
        let listing = list(Agent::Codex, &env, None, COMMAND_TIMEOUT);
        assert_eq!(reason(&listing), code::COMMAND_FAILED);
        let detail = &listing.unavailable.unwrap().params["detail"];
        assert!(
            detail.contains("not logged in") && !detail.contains("noise"),
            "{detail}"
        );
    }

    #[test]
    fn garbage_output_is_unreadable() {
        let dir = ScratchDir::new("agent-models-garbage");
        for (agent, name) in [(Agent::Codex, "codex"), (Agent::Grok, "grok")] {
            let env = fake_agent(&dir, name, "echo '<html>captive portal</html>'");
            let listing = list(agent, &env, None, COMMAND_TIMEOUT);
            assert!(listing.models.is_empty());
            assert_eq!(reason(&listing), code::UNREADABLE_OUTPUT);
        }
    }

    #[test]
    fn a_command_that_hangs_is_cut_off() {
        let dir = ScratchDir::new("agent-models-hang");
        let env = fake_agent(&dir, "codex", "sleep 30");
        let started = Instant::now();
        let listing = list(Agent::Codex, &env, None, Duration::from_millis(200));
        assert_eq!(reason(&listing), code::TIMED_OUT);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    fn some_listing(id: &str) -> Listing {
        Listing::found(vec![AgentModel {
            id: id.to_string(),
            name: id.to_string(),
            efforts: Vec::new(),
            version: None,
            available: None,
        }])
    }

    #[test]
    fn a_listing_is_reused_until_it_expires() {
        let cache = ModelCache::default();
        let start = Instant::now();
        let mut loads = 0;
        let mut ask = |at: Duration| {
            cache.get_or_load_at(start + at, Agent::Codex, None, || {
                loads += 1;
                some_listing(&format!("run-{loads}"))
            })
        };
        assert_eq!(ask(Duration::ZERO).models[0].id, "run-1");
        assert_eq!(
            ask(FOUND_TTL - Duration::from_secs(1)).models[0].id,
            "run-1"
        );
        // The expiry is measured from when the load returned, a moment after `start`.
        assert_eq!(
            ask(FOUND_TTL + Duration::from_secs(1)).models[0].id,
            "run-2"
        );
    }

    #[test]
    fn a_failure_is_reused_only_briefly() {
        let cache = ModelCache::default();
        let start = Instant::now();
        let failure = || Listing::unavailable(unreadable(Agent::Grok));
        cache.get_or_load_at(start, Agent::Grok, None, failure);
        let mut reran = false;
        cache.get_or_load_at(start + FAILED_TTL / 2, Agent::Grok, None, || {
            reran = true;
            failure()
        });
        assert!(!reran, "a fresh failure is served from the cache");
        cache.get_or_load_at(
            start + FAILED_TTL + Duration::from_secs(1),
            Agent::Grok,
            None,
            || {
                reran = true;
                some_listing("back")
            },
        );
        assert!(reran, "an expired failure is asked again");
    }

    #[test]
    fn a_failures_ttl_runs_from_after_the_command_not_before_it() {
        let cache = ModelCache::default();
        let start = Instant::now();
        // The slow load stands in for a command that ran into its timeout.
        cache.get_or_load_at(start, Agent::Grok, None, || {
            std::thread::sleep(Duration::from_millis(400));
            Listing::unavailable(unreadable(Agent::Grok))
        });
        // Past `FAILED_TTL` counted from the start, but inside it counted from the return.
        let mut reran = false;
        cache.get_or_load_at(
            start + FAILED_TTL + Duration::from_millis(200),
            Agent::Grok,
            None,
            || {
                reran = true;
                some_listing("late")
            },
        );
        assert!(
            !reran,
            "the time the command took does not count against the TTL"
        );
    }

    #[test]
    fn accounts_and_agents_never_share_an_entry() {
        let cache = ModelCache::default();
        let now = Instant::now();
        let a = Path::new("/accounts/a");
        let b = Path::new("/accounts/b");
        for (agent, dir, id) in [
            (Agent::Codex, Some(a), "a"),
            (Agent::Codex, Some(b), "b"),
            (Agent::Codex, None, "default"),
            (Agent::Grok, Some(a), "grok-a"),
        ] {
            cache.get_or_load_at(now, agent, dir, || some_listing(id));
        }
        for (agent, dir, id) in [
            (Agent::Codex, Some(a), "a"),
            (Agent::Codex, Some(b), "b"),
            (Agent::Codex, None, "default"),
            (Agent::Grok, Some(a), "grok-a"),
        ] {
            let hit = cache.get_or_load_at(now, agent, dir, || panic!("must be cached"));
            assert_eq!(hit.models[0].id, id);
        }
    }

    #[test]
    fn bounded_agent_files_leave_grok_models_available() {
        use crate::adapter::tests::{agent_file, bounded_file_cases};
        bounded_file_cases(
            "agents::models::tests::bounded_agent_files_leave_grok_models_available",
            &["regular", "linked", "fifo", "oversized"],
            |case| {
                let home = ScratchDir::new("bounded-model-cache");
                agent_file(
                    &home.join(GROK_MODELS_CACHE),
                    case,
                    GROK_CACHE,
                    OUTPUT_LIMIT,
                );
                let env = fake_agent(&home, "grok", &format!("printf '%s' '{}'", GROK_LIST));
                let listing = list(Agent::Grok, &env, Some(&home), COMMAND_TIMEOUT);
                let readable = matches!(case, "regular" | "linked");
                assert_eq!(listing.unavailable, None);
                assert_eq!(
                    listing.models,
                    decorate_grok(&parse_grok_list(GROK_LIST), readable.then_some(GROK_CACHE))
                );
            },
        );
    }
}
