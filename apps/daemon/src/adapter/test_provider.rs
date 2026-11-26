//! The behavior of the test-only provider (`crate::providers::test_provider`): the launch plan,
//! which every provider must have; a sign-in reading and an email reader for its accounts, which
//! the tests of a provider with no quota probe need; a record store that finds
//! `<agent session id>.jsonl` files under the records directory and declares a live feed reader and
//! companion, so the generic feed code is shown to follow what a provider declares; a second
//! record store whose record is a tree and whose conversations lie partly outside the records
//! directory ([`TreeRecords`]), so the generic record code is shown to follow a store of that
//! shape; a third whose conversations are rows of a database it reads itself ([`RowStore`]), so
//! the generic record code is shown to go through the provider for those; a prompt reader
//! of a made-up screen, so the generic interaction code is shown to follow what a provider reads;
//! and a screen reader of a made-up input box, so the generic composer requests (images and slash
//! commands) are shown to follow it; a server its messages go through, which its launch starts
//! through the launch's host ([`server`]), and an extension that opens the session's channel
//! ([`extension`]), so the generic delivery path is shown to follow either; and a sign-in a test
//! starts in a flow's place ([`AsksOnATerminal`]), whose CLI asks its question only on a terminal,
//! so the generic login question is shown to be reachable from one. Every other sub-interface is
//! the trait's default.

pub(crate) mod extension;
pub(crate) mod server;

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fs::FileType;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};

use super::{AgentProvider, LaunchPlan, LaunchSpec};
use crate::accounts::quotas::sign_in::SignInReading;
use crate::accounts::AccountOps;
use crate::hooks::{HookInterpreter, NoHooks};
use crate::hostfs::read_regular_text;
use crate::protocol::{ToolCategory, ToolStatus};
use crate::providers::test_provider::{
    CREDENTIAL_STORE, EMAIL_RECORD, SIGN_IN_ITEM, SIGN_IN_RECORD,
};
use crate::providers::{AgentId, ProviderDescriptor};
use crate::records::branch::{Branching, Link};
use crate::records::context::Context;
use crate::records::conversation::Entry;
use crate::records::feed::{FeedOut, FeedReader, Pending, ToolCall, ToolOutcome};
use crate::records::observed::Observed;
use crate::records::rows::{RowAccount, RowRecords};
use crate::records::RecordStore;
use crate::session::interaction::{Answer, CallSeen, InteractionReader, NextKey, Reading};
use crate::session::screen::{Interrupt, ScreenReader};
use crate::session::trust::{Screen, NO_SCREEN};

pub struct TestProvider;

impl AgentProvider for TestProvider {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        AgentId::TEST.descriptor()
    }

    /// [`TestRecords`], unless this thread has the provider keep no record files
    /// (`crate::providers::test_provider::without_records`): then a store that claims everything
    /// it is shown, so that only the layout's empty records directory keeps a walk, a removal, a
    /// relocation or a resumability check away from the account's own files. Or keep its records
    /// as trees (`crate::providers::test_provider::with_tree_records`): then [`TreeRecords`]. Or
    /// keep its conversations as rows (`crate::providers::test_provider::with_row_records`): then
    /// [`RowStore`].
    fn records(&self) -> &'static dyn RecordStore {
        if crate::providers::test_provider::declares_no_records() {
            &ClaimingRecords
        } else if crate::providers::test_provider::declares_tree_records() {
            &TreeRecords
        } else if crate::providers::test_provider::declares_row_records() {
            &RowStore
        } else {
            &TestRecords
        }
    }

    fn interactions(&self) -> Option<&'static dyn InteractionReader> {
        Some(&TestPrompts)
    }

    fn screen(&self) -> &'static dyn ScreenReader {
        crate::test_support::test_provider::declared_screen().unwrap_or(&TestScreen)
    }

    fn accounts(&self) -> Option<&'static dyn AccountOps> {
        Some(&TestAccounts)
    }

    /// [`NoHooks`], unless this thread has the provider read its hooks as a test says
    /// (`crate::test_support::test_provider::declaring_hooks`).
    fn hooks(&self) -> &'static dyn HookInterpreter {
        crate::test_support::test_provider::declared_hooks().unwrap_or(&NoHooks)
    }

    /// No status integration, so its status comes from its program's reports, unless this thread
    /// has it declare one with what its reports say beside it
    /// (`crate::test_support::test_provider::declaring_reported_signals`).
    fn has_status_integration(&self) -> bool {
        crate::test_support::test_provider::declared_reported_signals().is_some()
    }

    /// What the provider has, unless this thread has its launches go without
    /// (`crate::test_support::test_provider::declaring_launch_without_status_integration`).
    fn launch_has_status_integration(&self, _spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> bool {
        self.has_status_integration()
            && !crate::test_support::test_provider::declares_launch_without_status_integration()
    }

    /// What its screen reader declares, unless this thread has its launches go without its status
    /// integration (`crate::test_support::test_provider::declaring_launch_without_status_integration`),
    /// as a launch whose hooks the user switched off does: then nothing reports a held message the
    /// turn takes, and a cancel queues nothing again.
    fn launch_cancel_drops_held_messages(
        &self,
        _spec: &LaunchSpec<'_>,
        _plan: &LaunchPlan,
    ) -> bool {
        self.screen().cancel_drops_held_messages()
            && !crate::test_support::test_provider::declares_launch_without_status_integration()
    }

    /// Nothing, unless this thread has its launches wait
    /// (`crate::test_support::test_provider::declaring_tool_listing_wait`).
    fn awaits_tool_listing(
        &self,
        _spec: &LaunchSpec<'_>,
        _plan: &LaunchPlan,
    ) -> Option<std::time::Duration> {
        crate::test_support::test_provider::declared_tool_listing_wait()
    }

    /// Nothing, unless this thread has its launches tell more
    /// (`crate::test_support::test_provider::declaring_further_launch_notices`).
    fn further_launch_notices(
        &self,
        _spec: &LaunchSpec<'_>,
        _plan: &LaunchPlan,
    ) -> Vec<crate::protocol::Notice> {
        crate::test_support::test_provider::declared_further_launch_notices()
    }

    fn reported_signals(&self) -> crate::session::ReportedSignals {
        crate::test_support::test_provider::declared_reported_signals()
            .unwrap_or(crate::session::ReportedSignals::NONE)
    }

    /// None, unless this thread has it declare a gap
    /// (`crate::test_support::test_provider::declaring_reported_done_gap`).
    fn reported_done_gap(&self) -> Option<std::time::Duration> {
        crate::test_support::test_provider::declared_reported_done_gap()
    }

    /// The name a summary file gives the conversation, as [`TreeRecords`] reads one.
    fn summary_title(&self, summary: &Value) -> Option<String> {
        summary["name"].as_str().map(str::to_owned)
    }

    /// Nothing, unless this thread has the provider refuse every launch
    /// (`crate::test_support::test_provider::declaring_launch_refusal`).
    fn launch_refusal(&self, _spec: &LaunchSpec<'_>) -> Result<()> {
        if crate::test_support::test_provider::declares_launch_refusal() {
            return Err(crate::protocol::CodedError::provider_unsupported(
                AgentId::TEST,
                crate::protocol::ProviderOperation::OwnedAccounts,
            ));
        }
        Ok(())
    }

    /// [`Self::launch_refusal`], with what this thread has the check find to tell
    /// (`crate::test_support::test_provider::declaring_checked_notices`).
    fn check_launch(&self, spec: &LaunchSpec<'_>) -> Result<Vec<crate::protocol::Notice>> {
        self.launch_refusal(spec)?;
        Ok(crate::test_support::test_provider::declared_checked_notices())
    }

    /// `bare`, then `--resume <id>` for a resume, `--model <model>` and the task as the last
    /// argument, so a test reads back what the bare launch was given.
    fn bare_plan(&self, spec: &LaunchSpec<'_>) -> super::BarePlan {
        let mut args = vec!["bare".to_string()];
        if let Some(id) = spec.resume_agent_session_id {
            args.extend(["--resume".to_string(), id.to_string()]);
        }
        if let Some(model) = spec.model {
            args.extend(["--model".to_string(), model.to_string()]);
        }
        args.extend(spec.task.map(str::to_string));
        super::BarePlan {
            args,
            ..super::BarePlan::default()
        }
    }

    /// No arguments, and the account's directory through the layout: a pinned account's directory
    /// is exported as the layout's variable, as every provider whose CLI reads one does, and the
    /// default account leaves the snapshot's own value standing. A coded error instead where this
    /// thread has the plan fail (`crate::test_support::test_provider::declaring_failing_plan`).
    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        if crate::test_support::test_provider::declares_failing_plan() {
            return Err(crate::protocol::CodedError::raised(
                crate::test_support::test_provider::FAILING_PLAN_CODE,
                "the test plan failed",
                &[("path", "/nowhere")],
            ));
        }
        let env = spec
            .config_dir
            .and_then(|dir| self.descriptor().layout.config_dir_export(dir))
            .map(|(var, value)| (var.to_string(), value.to_string_lossy().into_owned()))
            .into_iter()
            .collect();
        let private_server = crate::test_support::test_provider::declares_private_server()
            .then(|| Box::new(server::TestServer) as Box<dyn crate::session::term::PrivateServer>);
        Ok(LaunchPlan {
            args: Vec::new(),
            private_server,
            env,
            resolves_approvals_itself: false,
            notice: None,
            first_message: None,
            shaped_prompt: None,
            carried_trust: None,
            after_exit: None,
            adoption: None,
        })
    }

    /// None, unless this thread has the provider give one
    /// (`crate::test_support::test_provider::declaring_reduced_plan`): `reduced` and what the CLI
    /// wrote as it exited, so a test reads back that the plan was handed the early exit.
    fn reduced_plan(&self, _spec: &LaunchSpec<'_>, exit: &super::EarlyExit) -> Option<LaunchPlan> {
        crate::test_support::test_provider::declares_reduced_plan().then(|| LaunchPlan {
            args: vec!["reduced".to_string(), exit.output.clone()],
            ..LaunchPlan::default()
        })
    }

    /// None, unless this thread has the provider's agent run an extension that opens the session's
    /// channel (`crate::test_support::test_provider::declaring_extension_channel`).
    fn extension_channel(&self, _spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> bool {
        crate::test_support::test_provider::declares_extension_channel()
    }

    /// Never, unless this thread has its launches expect a hook
    /// (`crate::test_support::test_provider::declaring_first_hook`).
    fn first_hook_due(
        &self,
        _spec: &LaunchSpec<'_>,
        _plan: &LaunchPlan,
    ) -> crate::adapter::FirstHook {
        crate::test_support::test_provider::declared_first_hook().unwrap_or_default()
    }
}

/// The accounts of the test-only provider: it has no plan quota and declares no login, and tells
/// whether an account is signed in from [`SIGN_IN_RECORD`], or, for an account that has a
/// [`SIGN_IN_ITEM`], from whether its stand-in credential store holds that item
/// ([`item_reading`]); its email reader reads [`EMAIL_RECORD`].
struct TestAccounts;

/// The sign-in reading of an account whose directory `dir` names an item ([`SIGN_IN_ITEM`]):
/// signed in when the stand-in store ([`CREDENTIAL_STORE`]) holds it, signed out when it does
/// not, asked through the generic bounded query; `None` for an account that names none.
fn item_reading(dir: &Path) -> Option<SignInReading> {
    use crate::platform::credential_store::{Presence, PresenceQuery};

    let named = read_regular_text(&dir.join(SIGN_IN_ITEM), 4096).ok()?;
    let (service, account) = named.lines().next().unwrap_or_default().split_once(' ')?;
    let store = dir.join(CREDENTIAL_STORE).to_string_lossy().into_owned();
    let query = PresenceQuery::stand_in(
        crate::test_support::bash(),
        &[
            "-c",
            "[ -e \"$0/broken\" ] && exit 3; [ -e \"$0/$1.$2\" ] || exit 44",
            store.as_str(),
            service,
            account.trim(),
        ],
        44,
    );
    Some(
        match crate::accounts::credential_presence::ask(Some(query)) {
            Presence::Present => SignInReading::SignedIn { email: None },
            Presence::Absent => SignInReading::SignedOut,
            Presence::CannotTell => SignInReading::CannotTell,
        },
    )
}

impl AccountOps for TestAccounts {
    fn run_home(
        &self,
        run_homes: &Path,
        _: crate::adapter::account_cli::BackgroundRun,
        _: &Path,
        shell_env: &HashMap<String, String>,
        _: &crate::adapter::account_cli::RunWait<'_>,
    ) -> anyhow::Result<Option<crate::adapter::account_cli::RunHome>> {
        if !shell_env.contains_key("OCTOBOARD_TEST_QUOTA_HOME") {
            return Ok(None);
        }
        let mut home =
            crate::adapter::account_cli::RunHome::make_private_run_dir(run_homes, "probe-")?;
        home.work_in_home = true;
        Ok(Some(home))
    }

    fn sign_in_state(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> SignInReading {
        let dir = super::account_dir(AgentId::TEST, config_dir, shell_env);
        if let Some(reading) = item_reading(&dir) {
            return reading;
        }
        let Ok(record) = read_regular_text(&dir.join(SIGN_IN_RECORD), 4096) else {
            return SignInReading::CannotTell;
        };
        let line = record.lines().next().unwrap_or_default().trim();
        match line.split_once(' ').unwrap_or((line, "")) {
            ("signed-out", _) => SignInReading::SignedOut,
            ("signed-in", email) => SignInReading::SignedIn {
                email: crate::accounts::quotas::email::tidy(email),
            },
            _ => SignInReading::CannotTell,
        }
    }

    fn email(
        &self,
        config_dir: Option<&Path>,
        shell_env: &HashMap<String, String>,
    ) -> Option<String> {
        let dir = super::account_dir(AgentId::TEST, config_dir, shell_env);
        let record = read_regular_text(&dir.join(EMAIL_RECORD), 4096).ok()?;
        crate::accounts::quotas::email::tidy(record.lines().next().unwrap_or_default())
    }
}

/// [`NO_SCREEN`] but for waiting for bracketed paste, as Claude Code's screen does, for a test to
/// have the test-only provider declare (`crate::test_support::test_provider::declaring_trust_screen`).
pub(crate) static PASTE_WAITING_SCREEN: Screen = Screen {
    waits_for_paste_mode: true,
    ..NO_SCREEN
};

/// A record store that takes every entry it is shown for a record of the conversation the entry's
/// name starts with, and `<id>.sidecar` in the account directory for what the agent keeps beside
/// the records: the strictest test of a layout that says it has no records directory.
struct ClaimingRecords;

impl RecordStore for ClaimingRecords {
    fn indexed(
        &self,
        path: &Path,
        name: &str,
        _kind: FileType,
        _leaf: bool,
    ) -> Option<(String, PathBuf)> {
        let id = name.strip_suffix(".jsonl").unwrap_or(name);
        Some((id.to_owned(), path.to_path_buf()))
    }

    fn own_paths(&self, _agent_id: &str, record: &Path, _levels_below_dir: usize) -> Vec<PathBuf> {
        vec![record.to_path_buf()]
    }

    fn names_record(&self, lowercase_name: &str, _kind: FileType, lowercase_id: &str) -> bool {
        lowercase_name.contains(lowercase_id)
    }

    fn recognises_records(&self) -> bool {
        true
    }

    fn beside_records(&self, root: &Path, agent_id: &str) -> Vec<PathBuf> {
        let sidecar = root.join(format!("{agent_id}.sidecar"));
        if sidecar.exists() {
            vec![sidecar]
        } else {
            Vec::new()
        }
    }
}

/// The test provider's records: a file `<agent session id>.jsonl` in the records directory, whose
/// lines are `{"user": text}`, `{"assistant": text}`, `{"call": id, "name": name}`,
/// `{"result": id, "output": text}`, `{"truncate": len}`, `{"panic": true}`, which panics, and
/// `{"gate": name}`, which holds the reader until the test opens that gate ([`gate`]);
/// beside it, `<id>.live` lines `{"pending": text}` (or none, which clears what was pending).
pub struct TestRecords;

impl RecordStore for TestRecords {
    fn indexed(
        &self,
        path: &Path,
        name: &str,
        kind: FileType,
        leaf: bool,
    ) -> Option<(String, PathBuf)> {
        let id = name.strip_suffix(".jsonl")?;
        (leaf && kind.is_file()).then(|| (id.to_owned(), path.to_path_buf()))
    }

    fn own_paths(&self, agent_id: &str, record: &Path, levels_below_dir: usize) -> Vec<PathBuf> {
        let named =
            record.file_name().and_then(|name| name.to_str()) == Some(&format!("{agent_id}.jsonl"));
        if levels_below_dir == 1 && named {
            vec![record.to_path_buf()]
        } else {
            Vec::new()
        }
    }

    fn feed_reader(&self) -> Option<Box<dyn FeedReader>> {
        Some(Box::new(TestReader))
    }

    fn feed_companion(&self, record: &Path) -> Option<PathBuf> {
        Some(record.with_extension("live"))
    }
}

/// The directory of an account, outside its records directory, in which the tree-shaped variant
/// keeps `<agent session id>.db` for each conversation.
pub(crate) const TREE_STATE_DIR: &str = "state";

/// The records of the variant whose record is a tree
/// (`crate::providers::test_provider::with_tree_records`): a file `<agent session id>.jsonl` in
/// the records directory whose lines are `{"id": id, "parent": id or null}` with one of
/// `"user": text`, `"assistant": text` and `"tokens": n` (the context size a request had); a line
/// with no `id` is outside the tree. `<agent session id>.title` beside the record is the
/// conversation's summary file, a line `name: <title>` and optionally `model: <model>`, and
/// `window.json` beside it gives the context window. Outside the records directory,
/// `state/<agent session id>.db` in the account directory is part of the conversation: it travels
/// with a carried record and is removed with the session. A missing record is a conversation
/// gone.
pub struct TreeRecords;

struct TreeLinks;

impl Branching for TreeLinks {
    fn link(&self, line: &Value) -> Option<Link> {
        Some(Link {
            id: line["id"].as_str()?.to_owned(),
            parent: line["parent"].as_str().map(str::to_owned),
        })
    }
}

impl RecordStore for TreeRecords {
    fn indexed(
        &self,
        path: &Path,
        name: &str,
        kind: FileType,
        leaf: bool,
    ) -> Option<(String, PathBuf)> {
        TestRecords.indexed(path, name, kind, leaf)
    }

    fn own_paths(&self, agent_id: &str, record: &Path, levels_below_dir: usize) -> Vec<PathBuf> {
        TestRecords.own_paths(agent_id, record, levels_below_dir)
    }

    fn names_record(&self, lowercase_name: &str, kind: FileType, lowercase_id: &str) -> bool {
        kind.is_file() && lowercase_name == format!("{lowercase_id}.jsonl")
    }

    fn recognises_records(&self) -> bool {
        true
    }

    fn absence_is_conclusive(&self) -> bool {
        true
    }

    fn beside_records(&self, root: &Path, agent_id: &str) -> Vec<PathBuf> {
        let state = root.join(TREE_STATE_DIR).join(format!("{agent_id}.db"));
        if std::fs::symlink_metadata(&state).is_ok() {
            vec![state]
        } else {
            Vec::new()
        }
    }

    fn removes_beside_records(&self) -> bool {
        true
    }

    fn carried_companions(&self, files: Vec<PathBuf>, _record: &Path) -> Vec<PathBuf> {
        files
            .into_iter()
            .filter(|relative| relative.starts_with(TREE_STATE_DIR))
            .collect()
    }

    fn branching(&self) -> Option<&dyn Branching> {
        Some(&TreeLinks)
    }

    /// `<agent session id>.title` beside the record: a name no one file name could say.
    fn summary_file(&self, record: &Path) -> Option<PathBuf> {
        Some(record.with_extension("title"))
    }

    /// Not JSON: a line `name: <title>` and, optionally, a line `model: <model>`, read as
    /// `{"name": <title>, "model": <model or null>}`.
    fn summary_in(&self, bytes: &[u8]) -> Option<Value> {
        let text = std::str::from_utf8(bytes).ok()?;
        let line = |key: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(key))
                .map(str::trim)
        };
        Some(json!({ "name": line("name: ")?, "model": line("model: ") }))
    }

    fn observe_summary(&self, summary: &Value, observed: &mut Observed) {
        observed.fold(summary["model"].as_str().map(str::to_owned), None);
    }

    /// `window.json` beside the record, `{"window": n}`.
    fn window_file(&self) -> Option<&'static str> {
        Some("window.json")
    }

    fn window_in(&self, file: &Value) -> Option<u64> {
        file["window"].as_u64()
    }

    fn fold_line(
        &self,
        line: &str,
        _past_boundary: bool,
        context: &mut Context,
        _observed: &mut Observed,
    ) {
        let tokens = serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|line| line["tokens"].as_u64());
        if let Some(tokens) = tokens {
            context.observe(tokens);
        }
    }

    fn entries(&self, line: &Value) -> Vec<Entry> {
        if let Some(text) = line["user"].as_str() {
            vec![Entry::User(text.to_owned())]
        } else if let Some(text) = line["assistant"].as_str() {
            vec![Entry::Assistant(text.to_owned())]
        } else {
            Vec::new()
        }
    }
}

/// The file in an account's directory that stands for the database of the row-shaped variant.
pub(crate) const ROW_DATABASE: &str = "rows.json";

thread_local! {
    static ROW_READS: Cell<usize> = const { Cell::new(0) };
    static ROW_REMOVALS: Cell<usize> = const { Cell::new(0) };
}

/// How many times this thread has asked [`RowStore`] for a conversation's rows, for a test to
/// show that a reader asked again, or did not.
pub(crate) fn row_reads() -> usize {
    ROW_READS.with(Cell::get)
}

/// How many times this thread has asked [`RowStore`] to remove a conversation, for a test to
/// show that a deletion asked once.
pub(crate) fn row_removals() -> usize {
    ROW_REMOVALS.with(Cell::get)
}

/// The records of the variant whose conversations are rows
/// (`crate::providers::test_provider::with_row_records`): one file [`ROW_DATABASE`] in the
/// account directory, a JSON object holding each conversation's rows under its id, which is
/// `row_` and lowercase letters and digits. A row is a line of [`TreeRecords`]'s shape, so a
/// conversation can hold rows the agent went back on. There is no file per conversation: a
/// conversation is listed, read and removed here. One missing from the database is gone.
pub struct RowStore;

impl RowStore {
    fn database(account: &RowAccount<'_>) -> Option<serde_json::Map<String, Value>> {
        let dir = super::account_dir(AgentId::TEST, account.config_dir, account.shell_env);
        let text = read_regular_text(&dir.join(ROW_DATABASE), 1 << 20).ok()?;
        match serde_json::from_str(&text).ok()? {
            Value::Object(conversations) => Some(conversations),
            _ => None,
        }
    }
}

impl RowRecords for RowStore {
    fn names_conversation(&self, id: &str) -> bool {
        id.strip_prefix("row_").is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
    }

    fn conversations(&self, account: &RowAccount<'_>) -> Option<HashSet<String>> {
        Some(Self::database(account)?.keys().cloned().collect())
    }

    fn lines(&self, account: &RowAccount<'_>, agent_id: &str) -> std::io::Result<Vec<Value>> {
        ROW_READS.with(|reads| reads.set(reads.get() + 1));
        let missing = || std::io::Error::from(std::io::ErrorKind::NotFound);
        match Self::database(account)
            .ok_or_else(missing)?
            .remove(agent_id)
        {
            Some(Value::Array(rows)) => Ok(rows),
            _ => Err(missing()),
        }
    }

    fn remove(&self, account: &RowAccount<'_>, agent_id: &str) -> Result<bool> {
        ROW_REMOVALS.with(|removals| removals.set(removals.get() + 1));
        let dir = super::account_dir(AgentId::TEST, account.config_dir, account.shell_env);
        let Some(mut conversations) = Self::database(account) else {
            anyhow::bail!("the test agent's database cannot be read");
        };
        conversations.remove(agent_id);
        std::fs::write(
            dir.join(ROW_DATABASE),
            Value::Object(conversations).to_string(),
        )?;
        Ok(true)
    }
}

impl RecordStore for RowStore {
    fn rows(&self) -> Option<&dyn RowRecords> {
        Some(self)
    }

    fn absence_is_conclusive(&self) -> bool {
        true
    }

    fn branching(&self) -> Option<&dyn Branching> {
        Some(&TreeLinks)
    }

    fn fold_line(
        &self,
        line: &str,
        past_boundary: bool,
        context: &mut Context,
        observed: &mut Observed,
    ) {
        TreeRecords.fold_line(line, past_boundary, context, observed);
        let model = serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|line| line["model"].as_str().map(str::to_owned));
        if past_boundary && model.is_some() {
            observed.fold(model, None);
        }
    }

    fn entries(&self, line: &Value) -> Vec<Entry> {
        TreeRecords.entries(line)
    }
}

struct TestReader;

impl FeedReader for TestReader {
    fn line(&mut self, line: &Value, out: &mut FeedOut) {
        if line["panic"].as_bool() == Some(true) {
            panic!("the test reader was told to panic");
        }
        if let Some(name) = line["gate"].as_str() {
            gate::pass(name);
        }
        if let Some(text) = line["user"].as_str() {
            out.user(text, None);
        } else if let Some(text) = line["assistant"].as_str() {
            out.assistant(text, None);
        } else if let Some(id) = line["call"].as_str() {
            out.tool_call(
                ToolCall {
                    call_id: id.to_owned(),
                    name: line["name"].as_str().unwrap_or("tool").to_owned(),
                    category: ToolCategory::Other,
                    title: None,
                    input: json!({}),
                    paths: Vec::new(),
                    status: ToolStatus::Running,
                    output: None,
                    exit_code: None,
                },
                None,
            );
        } else if let Some(id) = line["result"].as_str() {
            out.tool_result(
                id,
                ToolOutcome {
                    status: ToolStatus::Completed,
                    output: line["output"].as_str().map(str::to_owned),
                    exit_code: None,
                },
            );
        } else if let Some(len) = line["truncate"].as_u64() {
            out.truncate(len as usize);
        }
    }

    fn companion_line(&mut self, line: &Value, out: &mut FeedOut) {
        out.set_pending(
            line["pending"]
                .as_str()
                .map(|text| Pending::Assistant(text.to_owned()))
                .into_iter()
                .collect(),
        );
    }
}

/// Gates a test holds the test reader at, by name: a slow parse, for as long as the test likes.
pub mod gate {
    use std::collections::HashMap;
    use std::sync::{Condvar, Mutex, PoisonError};
    use std::time::Duration;

    /// Per gate: whether the reader has reached it, and whether it is open.
    static GATES: Mutex<Option<HashMap<String, (bool, bool)>>> = Mutex::new(None);
    static CHANGED: Condvar = Condvar::new();

    /// Waits until `name` is open (the tests' patience at most), having said it was reached.
    pub(super) fn pass(name: &str) {
        let mut gates = GATES.lock().unwrap_or_else(PoisonError::into_inner);
        gates
            .get_or_insert_with(HashMap::new)
            .entry(name.to_owned())
            .or_default()
            .0 = true;
        CHANGED.notify_all();
        let open = |gates: &mut Option<HashMap<String, (bool, bool)>>| {
            gates
                .as_ref()
                .and_then(|gates| gates.get(name))
                .is_some_and(|gate| gate.1)
        };
        let _ = CHANGED
            .wait_timeout_while(gates, crate::test_support::PATIENCE, |gates| !open(gates))
            .unwrap_or_else(PoisonError::into_inner);
    }

    /// Whether the reader has reached `name`, waiting for it at most `patience`.
    pub fn reached(name: &str, patience: Duration) -> bool {
        let gates = GATES.lock().unwrap_or_else(PoisonError::into_inner);
        let (gates, _) = CHANGED
            .wait_timeout_while(gates, patience, |gates| {
                !gates
                    .as_ref()
                    .and_then(|gates| gates.get(name))
                    .is_some_and(|gate| gate.0)
            })
            .unwrap_or_else(PoisonError::into_inner);
        gates
            .as_ref()
            .and_then(|gates| gates.get(name))
            .is_some_and(|gate| gate.0)
    }

    /// Lets the reader through `name`, now or when it gets there.
    pub fn open(name: &str) {
        let mut gates = GATES.lock().unwrap_or_else(PoisonError::into_inner);
        gates
            .get_or_insert_with(HashMap::new)
            .entry(name.to_owned())
            .or_default()
            .1 = true;
        CHANGED.notify_all();
    }
}

/// Reads a made-up screen: a line `PROMPT <tool> <command>` is a permission prompt for the call of
/// that tool and input `{"command": <command>}`, offering `1. Yes` and `2. No`, each chosen by its
/// digit, `2` only after [`UNSEEN_KEY`], a key that changes nothing on the screen, written together
/// with it (`NextKey::PressAll`); a line `EDITING` is that prompt with a text field open, which cannot be published; a line
/// `INPUT>` is the input box, and anything else is unknown. Calls start on `PreToolUse`, as
/// `{"tool_use_id", "tool_name", "tool_input"}`.
pub struct TestPrompts;

/// The key [`TestPrompts`] writes before `2`, standing for a move of a selection drawn in colour
/// alone.
pub(crate) const UNSEEN_KEY: &str = "x";

impl InteractionReader for TestPrompts {
    fn call_started(&self, event: &str, payload: &Value) -> Option<CallSeen> {
        (event == "PreToolUse").then(|| {
            CallSeen::new(
                payload
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                payload
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                payload.get("tool_input").unwrap_or(&Value::Null),
            )
        })
    }

    /// No, unless this thread has it say so
    /// (`crate::test_support::test_provider::declaring_prompt_unreadable_after_input`).
    fn prompt_unreadable_after_input(&self) -> bool {
        crate::test_support::test_provider::declares_prompt_unreadable_after_input()
    }

    fn read(&self, screen: &str, calls: &[CallSeen]) -> Reading {
        use crate::protocol::{
            ChoiceEffect, ChoiceText, InteractionPrompt, PermissionChoice, PermissionPrompt,
        };
        if screen.lines().any(|line| line.trim() == "EDITING") {
            return Reading::Up;
        }
        let prompt = screen
            .lines()
            .find_map(|line| line.trim().strip_prefix("PROMPT "));
        if let Some(prompt) = prompt {
            let (tool, command) = prompt.split_once(' ').unwrap_or((prompt, ""));
            let found = calls.iter().rposition(|call| {
                call.tool == tool
                    && call.input.get("command").and_then(Value::as_str) == Some(command)
            });
            let Some(call) = found else {
                return Reading::Up;
            };
            let choice = |id: &str, label: &str, effect| PermissionChoice {
                id: id.to_string(),
                label: label.to_string(),
                effect,
                text: (id == "2").then_some(ChoiceText::Optional),
            };
            return Reading::Shown {
                call,
                prompt: InteractionPrompt::Permission(PermissionPrompt {
                    tool: tool.to_string(),
                    category: Some(ToolCategory::Command),
                    title: Some(command.to_string()),
                    detail: None,
                    input: calls[call].input.clone(),
                    input_truncated: calls[call].input_truncated,
                    choices: vec![
                        choice("1", "Yes", ChoiceEffect::Allow),
                        choice("2", "No", ChoiceEffect::Deny),
                    ],
                }),
            };
        }
        if screen
            .lines()
            .any(|line| line.trim_start().starts_with("INPUT>"))
        {
            Reading::Gone
        } else {
            Reading::Unknown
        }
    }

    fn next_key(&self, screen: &str, _call: &CallSeen, answer: &Answer) -> NextKey {
        use crate::platform::term::Input;
        if !screen
            .lines()
            .any(|line| line.trim().starts_with("PROMPT "))
        {
            return NextKey::Stuck("no prompt on the screen");
        }
        match answer {
            Answer::Choice { id, text: None } if id == "2" => NextKey::PressAll(vec![
                Input::Text(UNSEEN_KEY.to_string()),
                Input::Text(id.clone()),
            ]),
            Answer::Choice { id, text: None } => NextKey::Press(Input::Text(id.clone())),
            _ => NextKey::Stuck("the test screen takes a plain choice"),
        }
    }
}

/// What the made-up title starts with while only the user can act.
pub(crate) const NEEDS_USER_TITLE: &str = "NEEDS USER";

/// Reads a made-up input box: the line beginning `INPUT>`, whose rest is the box's text, which
/// shows each attached image as `[Image #n]`; a line `COMPLETE <name>` is the slash-command
/// completion offering the command `<name>` and its only list, a line `STASH` shows the draft stashed, and
/// `[Pasted text #n +m lines]` is the form of a long paste. Its Ctrl-U clears the box. Its trust
/// screen is that of a provider nothing has been measured of, and it declares no interrupt, unless
/// a test gives it either on its own thread, or a whole reader of the test's own in its place
/// (`crate::test_support::test_provider`).
pub struct TestScreen;

impl ScreenReader for TestScreen {
    fn trust_screen(&self) -> &'static Screen {
        crate::test_support::test_provider::declared_trust_screen().unwrap_or(&NO_SCREEN)
    }

    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        screen.lines().find_map(|line| {
            line.trim_start()
                .strip_prefix("INPUT>")
                .map(|text| vec![text.trim().to_string()])
        })
    }

    fn image_placeholders(&self, text: &str) -> usize {
        crate::session::screen::count_image_placeholders(text)
    }

    fn without_image_placeholders(&self, text: &str) -> String {
        crate::session::screen::strip_image_placeholders(text)
    }

    fn command_completion(&self, screen: &str) -> Option<String> {
        screen.lines().find_map(|line| {
            line.trim()
                .strip_prefix("COMPLETE ")
                .map(|name| name.trim().to_string())
        })
    }

    /// Up while a `COMPLETE` line is drawn, its only list.
    fn list_shown(&self, screen: &str) -> Option<bool> {
        Some(self.command_completion(screen).is_some())
    }

    fn clears_input_box_with_ctrl_u(&self) -> bool {
        true
    }

    fn collapsed_paste(&self, text: &str) -> bool {
        text.starts_with("[Pasted text #") && text.ends_with(']')
    }

    fn stash_shown(&self, screen: &str) -> bool {
        screen.lines().any(|line| line.trim() == "STASH")
    }

    fn interrupt(&self) -> Option<&'static Interrupt> {
        crate::test_support::test_provider::declared_interrupt()
    }

    /// A title that starts with [`NEEDS_USER_TITLE`] says only the user can act.
    fn title_needs_user(&self, title: &str) -> bool {
        title.starts_with(NEEDS_USER_TITLE)
    }
}

/// The id of the question [`AsksOnATerminal`] puts to the user.
pub(crate) const TERMINAL_QUESTION: &str = "keep_token_in_a_file";

/// The phrase the prompt of [`AsksOnATerminal`]'s CLI is told by.
pub(crate) const TERMINAL_PROMPT: &str = "Keep the token in a file?";

/// A sign-in the test-only provider does not declare, for a test to start in a flow's place: its
/// CLI (`<binary> login`) asks [`TERMINAL_PROMPT`] only where it has a terminal, so the flow runs
/// it on one (`TerminalProcess`), puts the question to the user when the prompt appears, and
/// writes the user's answer back as `y` or `n`. The CLI's exit decides the login: 0 is signed
/// in, anything else fails with what it said last, also while the question is open, which is then
/// taken back. It asks once: a terminal that draws the prompt again does not ask again. The
/// generic login question is shown to be reachable from a prompt a CLI on pipes never prints.
pub(crate) struct AsksOnATerminal;

impl crate::accounts::login::flows::LoginFlow for AsksOnATerminal {
    fn run<'a>(
        &'a self,
        flow: &'a crate::accounts::login::flows::Flow,
        progress: &'a crate::accounts::login::flows::Progress,
        cancel: &'a tokio::sync::Notify,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = crate::accounts::login::flows::Outcome> + Send + 'a>,
    > {
        use crate::accounts::background_cli::TerminalProcess;
        use crate::accounts::login::flows::{Flow, Outcome};
        use crate::protocol::{LoginAnswer, LoginQuestion};

        /// How the login ends once its CLI has exited by itself.
        async fn ended(
            flow: &Flow,
            process: &mut TerminalProcess,
            mut last_words: String,
            success: Option<bool>,
        ) -> Outcome {
            process.last_words(&mut last_words).await;
            match success {
                Some(true) => Outcome::Succeeded,
                _ if last_words.is_empty() => flow.failed("it ended without signing in"),
                _ => flow.failed(&last_words),
            }
        }

        Box::pin(async move {
            let command = match flow.command(&["login"]) {
                Ok(command) => command,
                Err(outcome) => return outcome,
            };
            let mut process = match TerminalProcess::spawn(&command, flow.agent.label()) {
                Ok(process) => process,
                Err(err) => return flow.failed(&format!("it could not be started: {err}")),
            };
            let mut last_words = String::new();
            let mut asked = false;
            loop {
                let line = tokio::select! {
                    () = cancel.notified() => {
                        process.stop().await;
                        return Outcome::Cancelled;
                    }
                    success = process.child.exited() => {
                        return ended(flow, &mut process, last_words, success).await;
                    }
                    Some(line) = process.lines.recv() => line,
                };
                if asked || !line.text.contains(TERMINAL_PROMPT) {
                    if let Some(words) = line.into_words() {
                        last_words = words;
                    }
                    continue;
                }
                asked = true;
                let question = LoginQuestion::new(TERMINAL_QUESTION, TERMINAL_PROMPT, &[]);
                let answer = tokio::select! {
                    () = cancel.notified() => {
                        process.stop().await;
                        return Outcome::Cancelled;
                    }
                    // A CLI that gives up by itself leaves nothing to answer.
                    success = process.child.exited() => {
                        return ended(flow, &mut process, last_words, success).await;
                    }
                    answer = progress.ask(question) => answer,
                };
                let reply = match answer {
                    Ok(LoginAnswer::Yes) => "y",
                    Ok(LoginAnswer::No) => "n",
                    _ => {
                        process.stop().await;
                        return flow.failed("the login question did not receive a choice");
                    }
                };
                if let Err(err) = process.answer(reply).await {
                    process.stop().await;
                    return flow.failed(&format!("it could not be answered: {err}"));
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    fn sign_in_of(dir: &Path) -> SignInReading {
        TestAccounts.sign_in_state(Some(dir), &HashMap::new())
    }

    /// A sign-in read from an item of the credential store follows the item's presence, and a
    /// store that answers neither way leaves it untold.
    #[test]
    fn an_account_that_names_an_item_is_signed_in_exactly_while_the_store_holds_it() {
        let dir = ScratchDir::new("test-provider-sign-in-item");
        let store = dir.join(CREDENTIAL_STORE);
        std::fs::create_dir(&store).unwrap();
        std::fs::write(dir.join(SIGN_IN_ITEM), "test-agent me@example.com\n").unwrap();

        assert_eq!(sign_in_of(&dir), SignInReading::SignedOut);

        std::fs::write(store.join("test-agent.me@example.com"), "").unwrap();
        assert_eq!(sign_in_of(&dir), SignInReading::SignedIn { email: None });

        // Another account's item of the same service is not this one's.
        std::fs::remove_file(store.join("test-agent.me@example.com")).unwrap();
        std::fs::write(store.join("test-agent.you@example.com"), "").unwrap();
        assert_eq!(sign_in_of(&dir), SignInReading::SignedOut);

        std::fs::write(store.join("broken"), "").unwrap();
        assert_eq!(sign_in_of(&dir), SignInReading::CannotTell);
    }

    /// The item is asked about only by an account that names one: the sign-in record decides
    /// for every other, and is not looked at once an item is named.
    #[test]
    fn an_account_that_names_no_item_is_read_from_its_record() {
        let dir = ScratchDir::new("test-provider-sign-in-record");
        std::fs::write(dir.join(SIGN_IN_RECORD), "signed-in me@example.com\n").unwrap();
        assert_eq!(
            sign_in_of(&dir),
            SignInReading::SignedIn {
                email: Some("me@example.com".to_string())
            }
        );

        std::fs::create_dir(dir.join(CREDENTIAL_STORE)).unwrap();
        std::fs::write(dir.join(SIGN_IN_ITEM), "test-agent me@example.com\n").unwrap();
        assert_eq!(sign_in_of(&dir), SignInReading::SignedOut);
    }
}
