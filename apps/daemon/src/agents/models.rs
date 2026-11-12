//! The models a provider currently offers, for the dialogs that let the user choose one, and the
//! cache and checks around the lists. Nothing is hard-coded per version: how a provider's list is
//! read is the provider's own ([`ModelCatalogue`], returned by `AgentProvider::catalogue`; the
//! implementations are `crate::adapter::<provider>::models`, which also say what each provider
//! offers and how it was measured, in the "Model and reasoning effort" note of the provider's
//! section in `docs/agent-cli-reference.md`).
//!
//! A catalogue belongs to an account, so a command that lists models runs in the login-shell
//! snapshot with the provider's configuration-directory variable pointed at the account's
//! directory, which is what a session of the account gets ([`run_catalogue_command`]).
//!
//! Every model carries effort levels, never "unknown": the model's own where its catalogue entry
//! gives them (an empty list meaning it takes none), else the provider's measured set. Levels are
//! not versioned the way models are, so the fallback is safe. Failing to produce a list is never an
//! error to the caller: it is an empty list with a reason (`ModelsUnavailable`), because the form
//! asking must stay usable. Results are cached per provider and configuration directory
//! ([`ModelCache`]), except a provider's that names a default model
//! ([`ModelCatalogue::names_default_model`]), which is read afresh each time so that the default
//! it names never goes stale.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::adapter::account_cli::{self, NotRun, QueryFailure};
use crate::adapter::provider;
use crate::env_shell;
use crate::processes::subprocess::BoundedFailure;
use crate::protocol::{models_unavailable_code as code, AgentId, AgentModel, ModelsUnavailable};

/// How long a catalogue command may run. The commands answer in well under a second from a
/// warm cache and a few seconds when they refresh over the network; this leaves a wide margin
/// above that while keeping a form with no network from waiting longer than a person tolerates
/// (compare the ten seconds the login-shell snapshot gets).
pub(crate) const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

/// The most a catalogue command may print or a file read for a listing may hold (a provider's
/// models cache, global config file or cached catalogue). The largest known output runs to about
/// 600 KB, because it carries every model's full instructions; this is far above that and far
/// below anything that would hurt.
pub(crate) const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

/// How long a list that was read is reused (not a provider's that names a default model; see the
/// module documentation). A catalogue changes when the agent is updated or the
/// account's plan changes, both rare against how often a dialog is opened, and a refresh touches
/// the network; ten minutes absorbs a session of opening and closing dialogs while still showing
/// a new model within the time it takes to notice one.
const FOUND_TTL: Duration = Duration::from_secs(10 * 60);

/// How long a failure is reused. Short, because the likely causes (a network that comes back, a
/// login completed in a terminal) are fixed by the user within seconds and the next dialog should
/// see it, but long enough that reopening a dialog does not wait out a timeout again each time.
const FAILED_TTL: Duration = Duration::from_secs(30);

/// How a provider's models are listed, beside the generic cache and the launch-setting checks that
/// use the list. Only `list` has no default.
pub trait ModelCatalogue: Send + Sync {
    /// The models an account of the provider offers, read in `shell_env` under `config_dir` (the
    /// pinned account's directory, `None` for the default account). Blocking: it may run the
    /// provider's CLI, held to `timeout`. Never an error: an empty list with a reason
    /// ([`Listing::unavailable`]).
    fn list(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        timeout: Duration,
    ) -> Listing;

    /// Whether Octoboard names a default model for a session with no model setting (the first
    /// listed model the account may use, [`Listing::default_model`]), because the provider's own
    /// default is not checked against what the account may use. A provider that does is listed
    /// afresh on every read rather than through [`ModelCache`], so the default it names never goes
    /// stale; its `list` must then be cheap (file reads, no process).
    fn names_default_model(&self) -> bool {
        false
    }

    /// The model the provider's own configuration names for a session under `config_dir` in `cwd`
    /// (a settings file, an environment variable, a managed policy), which a `--model` would
    /// override; `None` when nothing names one or the provider's configuration is not read.
    fn configured_model(
        &self,
        _shell_env: &HashMap<String, String>,
        _config_dir: Option<&Path>,
        _cwd: Option<&Path>,
    ) -> Option<String> {
        None
    }

    /// Model ids that never change, for a tool description that can tell a model what it may pick;
    /// empty for a provider whose list is only known by asking it.
    fn fixed_model_ids(&self) -> &'static [&'static str] {
        &[]
    }
}

/// The catalogue of a provider that has none: nothing is known, so the list is empty with no
/// reason, a launch setting is let through as it is for a list that could not be read, and the
/// pickers show no models.
pub struct NoCatalogue;

impl ModelCatalogue for NoCatalogue {
    fn list(&self, _: &HashMap<String, String>, _: Option<&Path>, _: Duration) -> Listing {
        Listing {
            models: Vec::new(),
            unavailable: None,
        }
    }
}

/// `levels` as the owned strings a model carries.
pub(crate) fn levels(levels: &[&str]) -> Vec<String> {
    levels.iter().map(|level| level.to_string()).collect()
}

/// The levels a catalogue entry lists under `field`, reading each element's `key`. `None` when
/// the entry says nothing usable (no array, or a non-empty one none of whose elements parse), so
/// the caller falls back; an empty array is `Some([])`, a model that takes none.
pub(crate) fn entry_efforts(entry: &Value, field: &str, key: &str) -> Option<Vec<String>> {
    let array = entry.get(field)?.as_array()?;
    let levels: Vec<String> = array
        .iter()
        .filter_map(|level| level.get(key).and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    (!levels.is_empty() || array.is_empty()).then_some(levels)
}

/// The strings of a catalogue's level array, `None` when it has none to give.
pub(crate) fn entry_levels(levels: &Value) -> Option<Vec<String>> {
    let levels: Vec<String> = levels
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    (!levels.is_empty()).then_some(levels)
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

    pub(crate) fn found(models: Vec<AgentModel>) -> Self {
        Self {
            models,
            unavailable: None,
        }
    }

    pub(crate) fn unavailable(reason: ModelsUnavailable) -> Self {
        Self {
            models: Vec::new(),
            unavailable: Some(reason),
        }
    }
}

/// The listing for `agent` under `config_dir` (the pinned account's directory, `None` for the
/// default account), in the user's shell environment. Blocking: it may run a subprocess.
pub fn load(agent: AgentId, config_dir: Option<&Path>) -> Listing {
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

/// [`load`] over an already-captured environment and with its deadline given, which is what makes
/// the failure paths testable without waiting out [`COMMAND_TIMEOUT`].
pub(crate) fn list(
    agent: AgentId,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
) -> Listing {
    provider(agent)
        .catalogue()
        .list(shell_env, config_dir, timeout)
}

pub(crate) fn unreadable(agent: AgentId) -> ModelsUnavailable {
    ModelsUnavailable::new(
        code::UNREADABLE_OUTPUT,
        format!("{} printed no model list that could be read", agent.label()),
        &[("agent", agent.label())],
    )
}

/// Runs the agent's catalogue command and returns its stdout. The environment is what a session
/// of the account would get: the shell snapshot with the account's directory exported under the
/// agent's own variable, and no more.
pub(crate) fn run_catalogue_command(
    agent: AgentId,
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
            NotRun::BinaryRefused(refusal) => ModelsUnavailable {
                code: refusal.code,
                params: refusal.params,
                message: refusal.message,
            },
        },
        QueryFailure::Bounded(BoundedFailure::TimedOut(_)) => ModelsUnavailable::new(
            code::TIMED_OUT,
            format!("{} did not list its models in time", agent.label()),
            &[("agent", agent.label())],
        ),
        QueryFailure::Bounded(other) => failed(agent, &other.to_string()),
    })?;
    if !output.status.success() {
        let detail = account_cli::failure_detail(&output, |line| line);
        let detail = if detail.is_empty() {
            output.status.to_string()
        } else {
            format!("{}: {detail}", output.status)
        };
        return Err(failed(agent, &detail));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn failed(agent: AgentId, detail: &str) -> ModelsUnavailable {
    ModelsUnavailable::new(
        code::COMMAND_FAILED,
        format!("listing {}'s models failed: {detail}", agent.label()),
        &[("agent", agent.label()), ("detail", detail)],
    )
}

/// The listings read lately, keyed by agent and the configuration directory they were read under
/// (`None` for the default account), so two accounts of one agent never share an answer and an
/// account pointed at another directory starts afresh. Failures are kept too, for [`FAILED_TTL`].
#[derive(Default)]
pub struct ModelCache {
    entries: Mutex<HashMap<CacheKey, (Instant, Listing)>>,
    /// Listings of a provider that names a default model, which a test put in place of reading its
    /// files; see [`Self::seed`].
    #[cfg(test)]
    seeded: Mutex<HashMap<CacheKey, Listing>>,
}

type CacheKey = (AgentId, Option<PathBuf>);

impl ModelCache {
    /// Makes the listing of `agent` under `config_dir` this one, for a test that must not read the
    /// machine's own files. The listing of a provider that names a default model is never cached,
    /// so this is not `get_or_load`.
    #[cfg(test)]
    pub fn seed(&self, agent: AgentId, config_dir: Option<&Path>, listing: Listing) {
        let key = (agent, config_dir.map(Path::to_path_buf));
        self.seeded
            .lock()
            .expect("model cache lock poisoned")
            .insert(key, listing);
    }

    /// The listing [`Self::seed`] put in place for `agent` under `config_dir`, if any.
    #[cfg(test)]
    pub fn seeded(&self, agent: AgentId, config_dir: Option<&Path>) -> Option<Listing> {
        let key = (agent, config_dir.map(Path::to_path_buf));
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
        agent: AgentId,
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
        agent: AgentId,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fake_agent, ScratchDir};

    fn reason(listing: &Listing) -> crate::protocol::Code {
        listing.unavailable.as_ref().expect("a reason").code
    }

    #[test]
    fn a_provider_without_a_catalogue_lists_nothing_and_says_nothing_of_why() {
        let listing = NoCatalogue.list(&HashMap::new(), None, COMMAND_TIMEOUT);
        assert!(listing.models.is_empty());
        assert_eq!(listing.unavailable, None);
        assert!(!NoCatalogue.names_default_model());
        assert!(NoCatalogue.fixed_model_ids().is_empty());
    }

    /// The test-only provider has no catalogue of its own, so the registry gives it the default:
    /// nothing listed, no reason, no default model named and no fixed model id.
    #[test]
    fn the_test_provider_gets_the_default_catalogue() {
        let listing = list(AgentId::TEST, &HashMap::new(), None, COMMAND_TIMEOUT);
        assert!(listing.models.is_empty());
        assert_eq!(listing.unavailable, None);
        assert_eq!(listing.default_model(), None);
        let catalogue = provider(AgentId::TEST).catalogue();
        assert!(!catalogue.names_default_model());
        assert!(catalogue.fixed_model_ids().is_empty());
        assert_eq!(
            catalogue.configured_model(&HashMap::new(), None, None),
            None
        );
    }

    /// A listing admitted after the account's directory was moved (a removal held it back) runs
    /// nothing, which would create the directory again.
    #[test]
    fn a_listing_admitted_after_the_directory_was_moved_runs_nothing() {
        let dir = ScratchDir::new("agent-models-moved");
        let ran = dir.join("ran");
        let env = fake_agent(&dir, "codex", &format!("touch '{}'", ran.display()));
        let account = dir.join("account");
        let listing = list(AgentId::CODEX, &env, Some(&account), COMMAND_TIMEOUT);
        assert_eq!(reason(&listing), code::COMMAND_FAILED);
        assert!(!ran.exists(), "the CLI was not run");
        assert!(!account.exists());
    }

    #[test]
    fn a_missing_binary_is_a_reason_not_an_error() {
        let dir = ScratchDir::new("agent-models-missing");
        let env = HashMap::from([("PATH".to_string(), dir.to_string_lossy().into_owned())]);
        for agent in [AgentId::CODEX, AgentId::GROK] {
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
        let listing = list(AgentId::CODEX, &env, None, COMMAND_TIMEOUT);
        assert_eq!(reason(&listing), code::COMMAND_FAILED);
        let detail = &listing.unavailable.unwrap().params["detail"];
        assert!(
            detail.contains("not logged in") && !detail.contains("noise"),
            "{detail}"
        );
    }

    /// A reason printed on stdout reaches the detail, beside the stderr line that ends the
    /// stream, whichever the CLI printed it on; the status leads.
    #[test]
    fn a_non_zero_exit_carries_a_reason_printed_on_stdout() {
        let dir = ScratchDir::new("agent-models-exit-stdout");
        for (agent, name) in [(AgentId::CODEX, "codex"), (AgentId::GROK, "grok")] {
            let env = fake_agent(&dir, name, "echo 'not logged in'; exit 3");
            let listing = list(agent, &env, None, COMMAND_TIMEOUT);
            assert_eq!(reason(&listing), code::COMMAND_FAILED);
            let detail = &listing.unavailable.unwrap().params["detail"];
            assert!(detail.ends_with(": not logged in"), "{detail}");
            assert!(detail.contains('3'), "{detail}");

            let env = fake_agent(&dir, name, "echo 'not logged in'; echo noise >&2; exit 3");
            let listing = list(agent, &env, None, COMMAND_TIMEOUT);
            let detail = &listing.unavailable.unwrap().params["detail"];
            assert!(
                detail.ends_with(": not logged in (stderr: noise)"),
                "{detail}"
            );
        }
    }

    /// With nothing printed the detail is the exit status alone.
    #[test]
    fn a_non_zero_exit_that_printed_nothing_carries_only_the_status() {
        let dir = ScratchDir::new("agent-models-exit-silent");
        let env = fake_agent(&dir, "grok", "exit 4");
        let listing = list(AgentId::GROK, &env, None, COMMAND_TIMEOUT);
        let detail = &listing.unavailable.unwrap().params["detail"];
        let status = std::process::Command::new("sh")
            .args(["-c", "exit 4"])
            .status()
            .unwrap();
        assert_eq!(detail, &status.to_string());
    }

    #[test]
    fn garbage_output_is_unreadable() {
        let dir = ScratchDir::new("agent-models-garbage");
        for (agent, name) in [(AgentId::CODEX, "codex"), (AgentId::GROK, "grok")] {
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
        let listing = list(AgentId::CODEX, &env, None, Duration::from_millis(200));
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
            cache.get_or_load_at(start + at, AgentId::CODEX, None, || {
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
        let failure = || Listing::unavailable(unreadable(AgentId::GROK));
        cache.get_or_load_at(start, AgentId::GROK, None, failure);
        let mut reran = false;
        cache.get_or_load_at(start + FAILED_TTL / 2, AgentId::GROK, None, || {
            reran = true;
            failure()
        });
        assert!(!reran, "a fresh failure is served from the cache");
        cache.get_or_load_at(
            start + FAILED_TTL + Duration::from_secs(1),
            AgentId::GROK,
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
        cache.get_or_load_at(start, AgentId::GROK, None, || {
            std::thread::sleep(Duration::from_millis(400));
            Listing::unavailable(unreadable(AgentId::GROK))
        });
        // Past `FAILED_TTL` counted from the start, but inside it counted from the return.
        let mut reran = false;
        cache.get_or_load_at(
            start + FAILED_TTL + Duration::from_millis(200),
            AgentId::GROK,
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
            (AgentId::CODEX, Some(a), "a"),
            (AgentId::CODEX, Some(b), "b"),
            (AgentId::CODEX, None, "default"),
            (AgentId::GROK, Some(a), "grok-a"),
        ] {
            cache.get_or_load_at(now, agent, dir, || some_listing(id));
        }
        for (agent, dir, id) in [
            (AgentId::CODEX, Some(a), "a"),
            (AgentId::CODEX, Some(b), "b"),
            (AgentId::CODEX, None, "default"),
            (AgentId::GROK, Some(a), "grok-a"),
        ] {
            let hit = cache.get_or_load_at(now, agent, dir, || panic!("must be cached"));
            assert_eq!(hit.models[0].id, id);
        }
    }
}
