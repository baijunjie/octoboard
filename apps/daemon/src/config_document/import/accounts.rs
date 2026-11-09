//! The accounts region of a configuration document: what an inspection says importing each account
//! would do, and the creation of the accounts as the Add account form would, with the added agents,
//! the default accounts' names and flags and the settings of the Agent accounts section around
//! them. The account helpers it calls are the coordinator's, so an imported account passes the
//! checks a hand-added one does.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;

use super::progress::Progress;
use super::{based_on, exported_in_effect, known_agent_tags, Rebinder};
use crate::accounts::dirs::normalize_account_dir;
use crate::agents::availability;
use crate::config_document::export;
use crate::coordinator::{self, settings::SettingsChanges};
use crate::hostfs::dir_key;
use crate::platform::names;
use crate::protocol::{
    error_code, AccountOutcome, AccountResolution, AccountStatus, AgentId, CodedError,
    DocumentAccount, DocumentAccounts, DocumentAgentSettings, ImportFailure, ImportItem,
    ImportItemState, ImportedAccount, ImportedAccounts, ImportedAgentAccounts, InspectedAccount,
    InspectedAccounts, InspectedAgentAccounts, InspectedAgentSettings,
};
use crate::state::AppState;

/// One agent's part of a document's accounts. A hand-edited document may list an agent twice:
/// the parts are read as one, the accounts in the order they are listed.
struct AgentAccounts<'a> {
    agent: AgentId,
    default_name: Option<&'a str>,
    default_disabled: Option<bool>,
    accounts: Vec<&'a DocumentAccount>,
}

fn group_accounts(document: &DocumentAccounts) -> Vec<AgentAccounts<'_>> {
    let mut groups: Vec<AgentAccounts<'_>> = Vec::new();
    for entry in &document.agents {
        let named = entry
            .default_account
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let at = match groups.iter().position(|group| group.agent == entry.agent) {
            Some(at) => at,
            None => {
                groups.push(AgentAccounts {
                    agent: entry.agent,
                    default_name: None,
                    default_disabled: None,
                    accounts: Vec::new(),
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[at];
        group.default_name = group.default_name.or(named);
        group.default_disabled = group.default_disabled.or(entry.default_account.disabled);
        group.accounts.extend(&entry.accounts);
    }
    groups
}

/// An account of the document that an inspection says is new, kept to catch a later account of the
/// document that takes its name or directory.
struct Earlier {
    name: String,
    name_key: String,
    dir_key: String,
}

/// What importing one account that no account of the machine has the name of comes to, by the
/// checks the Add account form makes.
struct Judged {
    proposed: Option<String>,
    status: AccountStatus,
    in_use_by: Option<String>,
    error: Option<ImportFailure>,
}

fn judge_account(
    state: &Arc<AppState>,
    agent: AgentId,
    account: &DocumentAccount,
    earlier: &[Earlier],
) -> Result<Judged> {
    let proposed = normalize_account_dir(agent, &account.directory);
    let invalid = |proposed: Option<String>, err: anyhow::Error| Judged {
        proposed,
        status: AccountStatus::Invalid,
        in_use_by: None,
        error: Some(ImportFailure::of(&err)),
    };
    let name = match coordinator::accounts::normalize_account_name(&account.name) {
        Ok(name) => name,
        Err(err) => return Ok(invalid(proposed.ok(), err)),
    };
    let key = crate::accounts::account_name_key(&name);
    let taken_by = match coordinator::accounts::colliding_account_name(state, agent, &name, None)? {
        Some(existing) => Some(existing),
        None => earlier
            .iter()
            .find(|earlier| earlier.name_key == key)
            .map(|earlier| earlier.name.clone()),
    };
    if let Some(existing) = taken_by {
        return Ok(invalid(
            proposed.ok(),
            coordinator::accounts::account_name_taken(agent, &name, &existing),
        ));
    }
    let dir = match proposed {
        Ok(dir) => dir,
        Err(err) => return Ok(invalid(None, err)),
    };
    let in_use_by = match coordinator::accounts::colliding_account_dir(state, agent, &dir, None)? {
        Some(existing) => Some(existing),
        None => {
            let key = dir_key(&dir);
            earlier
                .iter()
                .find(|earlier| earlier.dir_key == key)
                .map(|earlier| earlier.name.clone())
        }
    };
    let status = if in_use_by.is_some() {
        AccountStatus::DirectoryInUse
    } else {
        AccountStatus::New
    };
    Ok(Judged {
        proposed: Some(dir),
        status,
        in_use_by,
        error: None,
    })
}

/// Whether the document's name for `agent`'s default account is to be applied: this machine's
/// default account has no name of its own, and the name is neither blank, the word the daemon
/// shows for an unnamed default account, nor the name of an account of the agent (or one in
/// `created`, an account the import is about to create).
fn default_name_applies(
    state: &Arc<AppState>,
    agent: AgentId,
    name: &str,
    created: &HashSet<(AgentId, String)>,
) -> Result<bool> {
    let key = crate::accounts::account_name_key(name);
    if crate::accounts::clears_default_account_name(name)
        || state
            .store
            .default_account_names()?
            .contains_key(agent.wire_name())
        || created.contains(&(agent, key))
    {
        return Ok(false);
    }
    Ok(coordinator::accounts::colliding_owned_account_name(state, agent, name, None)?.is_none())
}

/// What importing the document's accounts would do on this machine.
pub(super) fn inspect_accounts(
    state: &Arc<AppState>,
    document: &DocumentAccounts,
    rebinder: &Rebinder,
) -> Result<InspectedAccounts> {
    let decided = state.store.added_agents()?;
    let agents_to_add = match &decided {
        Some(added) => {
            let mut missing = Vec::new();
            for agent in document.added_agents.iter().flatten() {
                if !added.contains(agent) && !missing.contains(agent) {
                    missing.push(*agent);
                }
            }
            missing
        }
        None => Vec::new(),
    };
    let mut created = HashSet::new();
    let mut agents = Vec::new();
    for group in group_accounts(document) {
        let mut earlier: Vec<Earlier> = Vec::new();
        let mut accounts = Vec::new();
        for account in &group.accounts {
            let key = crate::accounts::account_name_key(&account.name);
            let inspected = if rebinder.find(group.agent, &account.name).is_some() {
                let proposed = normalize_account_dir(group.agent, &account.directory);
                (proposed.ok(), AccountStatus::Existing, None, None)
            } else {
                let judged = judge_account(state, group.agent, account, &earlier)?;
                if judged.status == AccountStatus::New {
                    let dir = judged.proposed.clone().unwrap_or_default();
                    earlier.push(Earlier {
                        name: account.name.trim().to_string(),
                        name_key: key.clone(),
                        dir_key: dir_key(&dir),
                    });
                    created.insert((group.agent, key));
                }
                (
                    judged.proposed,
                    judged.status,
                    judged.in_use_by,
                    judged.error,
                )
            };
            let (proposed_directory, status, in_use_by, error) = inspected;
            accounts.push(InspectedAccount {
                name: account.name.clone(),
                directory: account.directory.clone(),
                foreign_directory: names::is_foreign_absolute(account.directory.trim()),
                proposed_directory,
                status,
                in_use_by,
                error,
                disabled: account.disabled,
            });
        }
        let default_name_applies = match group.default_name {
            Some(name) => default_name_applies(state, group.agent, name, &created)?,
            None => false,
        };
        let default_disabled_applies = match group.default_disabled {
            Some(disabled) => disabled != default_account_disabled(state, group.agent)?,
            None => false,
        };
        agents.push(InspectedAgentAccounts {
            agent: group.agent,
            accounts,
            default_name: group.default_name.map(str::to_string),
            default_name_applies,
            default_disabled: group.default_disabled,
            default_disabled_applies,
        });
    }
    let agent_settings = match &document.agent_settings {
        Some(settings) => Some(inspect_agent_settings(state, settings)?),
        None => None,
    };
    Ok(InspectedAccounts {
        agents_to_add,
        agents,
        agent_settings,
    })
}

/// Whether this machine's default account of `agent` is turned off.
fn default_account_disabled(state: &Arc<AppState>, agent: AgentId) -> Result<bool> {
    Ok(state
        .store
        .disabled_default_accounts()?
        .contains(agent.wire_name()))
}

/// The settings of the Agent accounts section a document carries, beside this machine's own.
fn inspect_agent_settings(
    state: &Arc<AppState>,
    settings: &DocumentAgentSettings,
) -> Result<InspectedAgentSettings> {
    Ok(InspectedAgentSettings {
        auto_select_account: settings.auto_select_account,
        current_auto_select_account: state.store.auto_select_account()?,
        auto_switch_on_usage_limit: settings.auto_switch_on_usage_limit,
        current_auto_switch_on_usage_limit: state.store.auto_switch_on_usage_limit()?,
        auto_start_quota_windows: settings.auto_start_quota_windows,
        current_auto_start_quota_windows: state.store.auto_start_quota_windows()?,
        provider_settings: crate::config_document::exported_provider_settings(
            &settings.provider_settings,
        ),
        current_provider_settings: exported_in_effect(state.store.provider_settings()?),
        agent_tags: known_agent_tags(&settings.agent_tags).collect(),
        current_agent_tags: state.store.agent_tags()?,
    })
}

/// Whether the document's accounts have something for `agent`: one of its agent entries, a place
/// in the added agents, or provider settings or tags of its own. A key of the settings that names
/// no agent this build knows counts for none.
fn carries_agent(document: &DocumentAccounts, agent: AgentId) -> bool {
    let names_agent = |name: &String| AgentId::from_wire_name(name) == Some(agent);
    document
        .added_agents
        .iter()
        .flatten()
        .any(|added| *added == agent)
        || document.agents.iter().any(|entry| entry.agent == agent)
        || document.agent_settings.iter().any(|settings| {
            settings.provider_settings.keys().any(names_agent)
                || settings.agent_tags.keys().any(names_agent)
        })
}

/// The document's accounts reduced to what concerns `agents` (one listed twice counts once): their
/// entries, their places in the added agents and their provider settings and tags. The three
/// switches stay whole, since they are no agent's. An agent the document carries nothing for is
/// refused with `import_agent_missing`, before anything is done.
pub(super) fn only_agents(
    document: &DocumentAccounts,
    agents: &[AgentId],
) -> Result<DocumentAccounts> {
    if let Some(agent) = agents
        .iter()
        .find(|agent| !carries_agent(document, **agent))
    {
        return Err(CodedError::raised(
            error_code::IMPORT_AGENT_MISSING,
            format!(
                "the document carries nothing for the {} agent to apply",
                agent.label()
            ),
            &[("agent", agent.label())],
        ));
    }
    let kept =
        |name: &str| AgentId::from_wire_name(name).is_some_and(|agent| agents.contains(&agent));
    let mut only = document.clone();
    only.added_agents = only.added_agents.map(|added| {
        added
            .into_iter()
            .filter(|agent| agents.contains(agent))
            .collect()
    });
    only.agents.retain(|entry| agents.contains(&entry.agent));
    if let Some(settings) = &mut only.agent_settings {
        settings.provider_settings.retain(|name, _| kept(name));
        settings.agent_tags.retain(|name, _| kept(name));
    }
    Ok(only)
}

/// The choice made for one account of the document.
pub(super) enum AccountChoice {
    /// Another directory, as typed (an absolute path or one starting with `~/`).
    Directory(String),
    LeaveOut,
}

/// The account resolutions keyed by agent and index, refused as a whole when one cannot be right.
pub(super) fn index_account_resolutions(
    document: &DocumentAccounts,
    resolutions: Vec<AccountResolution>,
) -> Result<HashMap<(AgentId, usize), AccountChoice>> {
    let groups = group_accounts(document);
    let mut chosen = HashMap::new();
    for resolution in resolutions {
        let AccountResolution {
            agent,
            account,
            directory,
            leave_out,
        } = resolution;
        let invalid = |reason: &str| {
            CodedError::raised(
                error_code::IMPORT_RESOLUTION_INVALID,
                format!(
                    "resolution for the {} account {account}: {reason}",
                    agent.label()
                ),
                &[("agent", agent.label()), ("account", &account.to_string())],
            )
        };
        let exists = groups
            .iter()
            .find(|group| group.agent == agent)
            .is_some_and(|group| account < group.accounts.len());
        if !exists {
            return Err(invalid("the document has no such account"));
        }
        let choice = match (directory, leave_out) {
            (Some(directory), false) => {
                coordinator::validate::absolute_path(&directory)
                    .map_err(|_| invalid("the directory is not absolute"))?;
                AccountChoice::Directory(directory)
            }
            (None, true) => AccountChoice::LeaveOut,
            _ => return Err(invalid("give either `directory` or `leave_out`")),
        };
        if chosen.insert((agent, account), choice).is_some() {
            return Err(invalid("the account is resolved twice"));
        }
    }
    Ok(chosen)
}

/// What an import of the document's accounts settles before it announces anything, so that a
/// failure of it leaves no announced item waiting.
pub(super) struct PreparedAccounts {
    /// The accounts this machine had before the import, which an account of the document is
    /// matched against by name.
    before: Rebinder,
    agents_added: Vec<AgentId>,
}

/// Appends every agent of the document missing from the added agents (a list not yet decided is
/// first filled, and where it stays undecided nothing is appended, since every agent counts as
/// added until it is) and reads the accounts the import is matched against.
pub(super) fn prepare_accounts(
    state: &Arc<AppState>,
    document: &DocumentAccounts,
) -> Result<PreparedAccounts> {
    let host = state.store.list_accounts()?;
    let before = Rebinder { accounts: host };

    availability::fill_added_agents(state)?;
    let mut agents_added = Vec::new();
    if state.store.added_agents()?.is_some() {
        for agent in document.added_agents.iter().flatten() {
            if state.store.add_added_agent(*agent)? {
                agents_added.push(*agent);
            }
        }
        if !agents_added.is_empty() {
            crate::state::broadcast_settings(state)?;
        }
    }
    Ok(PreparedAccounts {
        before,
        agents_added,
    })
}

/// Announces as waiting each account of the document that [`apply_accounts`] handles: all but the
/// left-out ones, an existing account included whatever was chosen for it, since one is used as it
/// is and the choice is never read. The settings of the Agent accounts section follow, where the
/// document carries them.
pub(super) fn announce_accounts(
    document: &DocumentAccounts,
    prepared: &PreparedAccounts,
    chosen: &HashMap<(AgentId, usize), AccountChoice>,
    progress: &Progress,
) {
    for group in group_accounts(document) {
        for (account, document_account) in group.accounts.iter().enumerate() {
            let existing = prepared
                .before
                .find(group.agent, &document_account.name)
                .is_some();
            let left_out = matches!(
                chosen.get(&(group.agent, account)),
                Some(AccountChoice::LeaveOut)
            );
            if existing || !left_out {
                progress.waiting(ImportItem::Account {
                    agent: group.agent,
                    account,
                });
            }
        }
    }
    if document.agent_settings.is_some() {
        progress.waiting(ImportItem::AgentSettings);
    }
}

/// Creates the document's accounts as the Add account form would, with the added agents and the
/// default accounts' names around them.
///
/// The agents were appended by [`prepare_accounts`]. Then each account in the document's order: one
/// whose name an account of the agent had before the import is used as it is, a left-out one is
/// skipped, and any other goes through the form's checks, a failure of which is recorded for that
/// account alone. A created account takes the document's disabled flag and comes after the agent's
/// other accounts in its display order; it never shares the agent's default configuration,
/// whatever the document says. The default account's name is applied under the rule an inspection
/// reports, and its disabled flag as the document has it. The settings of the Agent accounts
/// section come last, through the path a settings update takes; a failure of those is reported on
/// the result and leaves the accounts as they are. Each account, and those settings, is reported
/// to `progress` as it is handled (see [`announce_accounts`]).
pub(super) async fn apply_accounts(
    state: &Arc<AppState>,
    document: &DocumentAccounts,
    prepared: PreparedAccounts,
    mut chosen: HashMap<(AgentId, usize), AccountChoice>,
    progress: &Progress,
    settings_versions: Option<&BTreeMap<String, u64>>,
) -> ImportedAccounts {
    let PreparedAccounts {
        before,
        agents_added,
    } = prepared;

    let mut agents = Vec::new();
    for group in group_accounts(document) {
        let mut accounts = Vec::new();
        let mut created_ids = Vec::new();
        for (index, account) in group.accounts.iter().enumerate() {
            let outcome = |status, id: Option<String>| ImportedAccount {
                name: account.name.clone(),
                status,
                account: id,
                error: None,
            };
            let item = ImportItem::Account {
                agent: group.agent,
                account: index,
            };
            if let Some(id) = before.find(group.agent, &account.name) {
                accounts.push(outcome(AccountOutcome::Existing, Some(id)));
                progress.report(item, ImportItemState::Done);
                continue;
            }
            let directory = match chosen.remove(&(group.agent, index)) {
                Some(AccountChoice::LeaveOut) => {
                    accounts.push(outcome(AccountOutcome::LeftOut, None));
                    continue;
                }
                Some(AccountChoice::Directory(directory)) => directory,
                None => account.directory.clone(),
            };
            progress.report(item.clone(), ImportItemState::Creating);
            match coordinator::accounts::insert_new_account(
                state,
                group.agent,
                account.name.clone(),
                directory,
                account.disabled,
            ) {
                Ok(created) => {
                    accounts.push(outcome(AccountOutcome::Created, Some(created.id.clone())));
                    created_ids.push(created.id);
                    progress.report(item, ImportItemState::Done);
                }
                Err(err) => {
                    let failure = ImportFailure::of(&err);
                    progress.failed(item, &failure);
                    let mut failed = outcome(AccountOutcome::Failed, None);
                    failed.error = Some(failure);
                    accounts.push(failed);
                }
            }
        }
        if !created_ids.is_empty() {
            place_after_existing(state, group.agent, &created_ids);
        }
        agents.push((
            group.agent,
            group.default_name,
            group.default_disabled,
            accounts,
        ));
    }

    let mut imported = Vec::new();
    for (agent, default_name, default_disabled, accounts) in agents {
        let mut default_name_applied = false;
        if let Some(name) = default_name {
            // The rule an inspection reports, checked under the lock the write takes: a name
            // given to the default account since is kept, and a name an account has taken since
            // is refused, which is expected here and not reported.
            match coordinator::accounts::name_unnamed_default_account(state, agent, name) {
                Ok(written) => default_name_applied = written,
                Err(err) if is_name_taken(&err) => {}
                Err(err) => {
                    tracing::warn!(%err, "naming the default account after an import failed")
                }
            }
        }
        let mut default_disabled_applied = None;
        if let Some(disabled) = default_disabled {
            match coordinator::accounts::set_account_disabled(state, agent, None, disabled) {
                Ok(changed) => default_disabled_applied = changed.then_some(disabled),
                Err(err) => {
                    tracing::warn!(%err, "setting the default account's flag after an import failed")
                }
            }
        }
        imported.push(ImportedAgentAccounts {
            agent,
            accounts,
            default_name_applied,
            default_disabled_applied,
        });
    }
    let (mut agent_settings_applied, mut agent_settings_error) = (None, None);
    if let Some(settings) = &document.agent_settings {
        progress.report(ImportItem::AgentSettings, ImportItemState::Creating);
        let changes = agent_settings_changes(settings);
        let base = based_on(&changes, settings_versions);
        match coordinator::settings::update_settings_checked(state, changes, &base).await {
            Ok(()) => {
                agent_settings_applied = Some(settings.clone());
                progress.report(ImportItem::AgentSettings, ImportItemState::Done);
            }
            Err(err) => {
                let failure = ImportFailure::of(&err);
                progress.failed(ImportItem::AgentSettings, &failure);
                agent_settings_error = Some(failure);
            }
        }
    }
    ImportedAccounts {
        agents_added,
        agents: imported,
        agent_settings_applied,
        agent_settings_error,
    }
}

/// The write of the Agent accounts settings the document carries.
pub(super) fn agent_settings_changes(settings: &DocumentAgentSettings) -> SettingsChanges {
    SettingsChanges {
        auto_select_account: Some(settings.auto_select_account),
        auto_switch_on_usage_limit: Some(settings.auto_switch_on_usage_limit),
        auto_start_quota_windows: Some(settings.auto_start_quota_windows),
        // A setting this build does not carry is skipped, so a document from a build that knows
        // more providers still applies the rest of its settings.
        provider_settings: crate::config_document::exported_provider_settings(
            &settings.provider_settings,
        ),
        agent_tags: known_agent_tags(&settings.agent_tags).collect(),
        ..Default::default()
    }
}

/// Whether `err` is the refusal of a name an account already has.
fn is_name_taken(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>()
        .is_some_and(|coded| coded.code == error_code::ACCOUNT_NAME_TAKEN)
}

/// Puts the accounts `created` last in `agent`'s display order, in the order given: saved at once,
/// since the order of accounts created in one instant is otherwise left to the query. Another
/// change of the accounts that gets in between makes the write refuse, which leaves the order as
/// the client derives it.
fn place_after_existing(state: &Arc<AppState>, agent: AgentId, created: &[String]) {
    let placed = || -> Result<bool> {
        let owned: Vec<String> = state
            .store
            .list_accounts()?
            .into_iter()
            .filter(|account| account.agent == agent)
            .map(|account| account.id)
            .collect();
        let mut order = export::display_order(&state.store.account_order(agent)?, &owned);
        order.retain(|id| id.as_ref().is_none_or(|id| !created.contains(id)));
        order.extend(created.iter().cloned().map(Some));
        Ok(state.store.reorder_accounts(agent, &order)?)
    };
    match placed() {
        Ok(true) => {
            if let Err(err) = crate::state::broadcast_settings(state) {
                tracing::warn!(%err, "broadcasting the account order after an import failed");
            }
        }
        Ok(false) => {}
        Err(err) => {
            tracing::warn!(%err, "placing the imported accounts in the display order failed")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{
        account, at, console, directory, document, foreign_path, project, stored_projects,
    };
    use super::super::{apply, apply_reporting, inspect, ApplyOptions, Inspection};
    use super::*;
    use crate::protocol::{
        AccountFallback, AccountName, DirectoryChoice, ProjectAccount, ProjectAccountRef,
    };
    use crate::providers::SettingValue;
    use crate::test_support::{app_state, ScratchDir};
    use crate::test_support::{code_of, param_of};
    use serde_json::json;
    use std::path::Path;

    fn doc_account(name: &str, directory: &str) -> serde_json::Value {
        json!({
            "name": name,
            "directory": directory,
            "disabled": false,
        })
    }

    fn agent_entry(
        agent: &str,
        default_name: Option<&str>,
        accounts: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        json!({
            "agent": agent,
            "default_account": { "name": default_name, "position": 0 },
            "accounts": accounts,
        })
    }

    fn with_accounts(
        mut document: serde_json::Value,
        added: &[&str],
        agents: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        document["accounts"] = json!({ "added_agents": added, "agents": agents });
        document
    }

    fn path_text(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    /// A state whose agents' default directories are inside `area`, so nothing of the real home
    /// directory is looked at or linked into.
    fn account_state(label: &str, area: &Path) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = app_state(label);
        state.set_agent_availability(
            AgentId::ALL
                .into_iter()
                .map(|agent| {
                    let default = directory(area, &format!("default-{}", agent.wire_name()));
                    crate::protocol::AgentAvailability {
                        agent,
                        availability: crate::protocol::Availability::Available,
                        default_account_dir: Some(path_text(&default)),
                    }
                })
                .collect(),
        );
        (state, dir)
    }

    fn claude_account(state: &Arc<AppState>, id: &str, name: &str, dir: &Path) {
        let mut stored = account(id, AgentId::CLAUDE, name);
        stored.config_dir = path_text(dir);
        state.store.insert_account(&stored).unwrap();
    }

    async fn inspected_accounts(state: &Arc<AppState>, document: &serde_json::Value) -> Inspection {
        inspect(state, document).await.unwrap()
    }

    #[tokio::test]
    async fn each_account_of_a_document_is_inspected_as_the_add_account_form_would_judge_it() {
        let area = ScratchDir::new("import-inspect-accounts-area");
        let (state, _state_dir) = account_state("import-inspect-accounts", &area);
        claude_account(&state, "claude-work", "Work", &area.join("work"));
        let home = crate::paths::home_dir();
        let document = with_accounts(
            document(Vec::new()),
            &[],
            vec![agent_entry(
                "claude",
                None,
                vec![
                    doc_account(" work ", &path_text(&area.join("elsewhere"))),
                    doc_account("Fresh", &path_text(&area.join("fresh"))),
                    doc_account("Clash", &path_text(&area.join("work"))),
                    doc_account("Default", &path_text(&area.join("a"))),
                    doc_account("  ", &path_text(&area.join("b"))),
                    doc_account("Relative", "a/b"),
                    doc_account("Twin dir", &path_text(&area.join("fresh"))),
                    doc_account("FRESH", &path_text(&area.join("c"))),
                    doc_account("InDefault", &path_text(&area.join("default-claude"))),
                    doc_account("Bare", "bare-name"),
                    doc_account("Home", "~/.claude-home"),
                ],
            )],
        );

        let inspection = inspected_accounts(&state, &document).await;

        let claude = &inspection.accounts.unwrap().agents[0];
        let rows: Vec<(AccountStatus, Option<&str>, Option<&str>)> = claude
            .accounts
            .iter()
            .map(|row| {
                (
                    row.status,
                    row.in_use_by.as_deref(),
                    row.error.as_ref().map(|error| error.code.as_str()),
                )
            })
            .collect();
        use AccountStatus::{DirectoryInUse, Existing, Invalid, New};
        assert_eq!(
            rows,
            [
                (Existing, None, None),
                (New, None, None),
                (DirectoryInUse, Some("Work"), None),
                (Invalid, None, Some(error_code::ACCOUNT_NAME_TAKEN.as_str())),
                (Invalid, None, Some(error_code::FIELD_REQUIRED.as_str())),
                (
                    Invalid,
                    None,
                    Some(error_code::CONFIG_DIR_NOT_ABSOLUTE.as_str())
                ),
                (DirectoryInUse, Some("Fresh"), None),
                (Invalid, None, Some(error_code::ACCOUNT_NAME_TAKEN.as_str())),
                (DirectoryInUse, Some("Default"), None),
                (
                    Invalid,
                    None,
                    Some(error_code::CONFIG_DIR_NOT_ABSOLUTE.as_str())
                ),
                (New, None, None),
            ]
        );
        let proposed = |at: usize| claude.accounts[at].proposed_directory.clone();
        assert_eq!(proposed(1), Some(path_text(&area.join("fresh"))));
        assert_eq!(proposed(5), None);
        assert_eq!(proposed(9), None, "a bare name is no directory");
        assert_eq!(proposed(10), Some(path_text(&home.join(".claude-home"))));
        assert_eq!(claude.accounts[1].directory, path_text(&area.join("fresh")));
        // Nothing was created by looking.
        assert_eq!(state.store.list_accounts().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_directory_written_on_another_operating_system_is_marked_and_proposes_nothing() {
        let area = ScratchDir::new("import-foreign-account-area");
        let (state, _state_dir) = account_state("import-foreign-account", &area);
        let document = with_accounts(
            document(Vec::new()),
            &[],
            vec![agent_entry(
                "claude",
                None,
                vec![
                    doc_account("Foreign", foreign_path()),
                    doc_account("Own", &path_text(&area.join("own"))),
                    doc_account("Relative", "a/b"),
                    doc_account("Home", "~/.claude-home"),
                ],
            )],
        );

        let inspection = inspected_accounts(&state, &document).await;

        let rows = &inspection.accounts.unwrap().agents[0].accounts;
        let marks: Vec<bool> = rows.iter().map(|row| row.foreign_directory).collect();
        assert_eq!(marks, [true, false, false, false]);
        assert_eq!(rows[0].status, AccountStatus::Invalid);
        assert_eq!(rows[0].proposed_directory, None);
    }

    #[tokio::test]
    async fn the_disabled_values_and_the_agents_to_add_are_reported() {
        let area = ScratchDir::new("import-inspect-agents-area");
        let (state, _state_dir) = account_state("import-inspect-agents", &area);
        state
            .store
            .set_added_agents_if_undecided(&[AgentId::CLAUDE])
            .unwrap();
        let mut off = doc_account("Off", &path_text(&area.join("off")));
        off["disabled"] = json!(true);
        let document = with_accounts(
            document(Vec::new()),
            &["codex", "claude", "codex", "grok"],
            vec![agent_entry("claude", None, vec![off])],
        );

        let accounts = inspected_accounts(&state, &document)
            .await
            .accounts
            .unwrap();

        assert_eq!(accounts.agents_to_add, [AgentId::CODEX, AgentId::GROK]);
        let row = &accounts.agents[0].accounts[0];
        assert!(row.disabled);
    }

    #[tokio::test]
    async fn the_default_account_name_applies_only_while_nothing_stands_in_its_way() {
        let area = ScratchDir::new("import-inspect-default-names-area");
        let (state, _state_dir) = account_state("import-inspect-default-names", &area);
        let mut taken = account("grok-taken", AgentId::GROK, "Taken");
        taken.config_dir = path_text(&area.join("taken"));
        state.store.insert_account(&taken).unwrap();
        let unnamed = with_accounts(
            document(Vec::new()),
            &[],
            vec![
                agent_entry("claude", Some("Personal"), Vec::new()),
                agent_entry(
                    "codex",
                    Some("Other"),
                    vec![doc_account("Other", &path_text(&area.join("other")))],
                ),
                agent_entry("grok", Some("taken"), Vec::new()),
            ],
        );

        let agents = inspected_accounts(&state, &unnamed)
            .await
            .accounts
            .unwrap()
            .agents;

        let applies: Vec<bool> = agents.iter().map(|a| a.default_name_applies).collect();
        assert_eq!(applies, [true, false, false]);
        assert_eq!(agents[0].default_name.as_deref(), Some("Personal"));

        // Not while the machine's default account has a name of its own, nor for the word the
        // daemon uses for an unnamed one (Grok's default account is unnamed here, so only the word
        // keeps it out).
        state
            .store
            .set_default_account_name(AgentId::CLAUDE, Some("Mine"))
            .unwrap();
        let named = with_accounts(
            document(Vec::new()),
            &[],
            vec![
                agent_entry("claude", Some("Personal"), Vec::new()),
                agent_entry("grok", Some(" default "), Vec::new()),
            ],
        );
        let applies: Vec<bool> = inspected_accounts(&state, &named)
            .await
            .accounts
            .unwrap()
            .agents
            .iter()
            .map(|a| a.default_name_applies)
            .collect();
        assert_eq!(applies, [false, false]);
    }

    #[tokio::test]
    async fn a_reference_to_an_account_the_import_would_create_is_listed_as_a_fallback_like_one_matching_nothing(
    ) {
        let area = ScratchDir::new("import-inspect-references-area");
        let (state, _state_dir) = account_state("import-inspect-references", &area);
        claude_account(&state, "claude-work", "Work", &area.join("work"));
        let mut hub = project("p", "/p");
        hub.accounts[AgentId::CLAUDE] = ProjectAccountRef::Account {
            name: "work".to_string(),
        };
        hub.accounts[AgentId::CODEX] = ProjectAccountRef::Account {
            name: "Fresh".to_string(),
        };
        hub.accounts[AgentId::GROK] = ProjectAccountRef::Account {
            name: "Nowhere".to_string(),
        };
        let mut hub = console("Hub", vec![hub]);
        hub.accounts[AgentId::CODEX] = Some(AccountName {
            name: "fresh".to_string(),
        });
        let document = with_accounts(
            document(vec![hub]),
            &[],
            vec![
                agent_entry("claude", None, vec![doc_account("Work", "/x")]),
                agent_entry(
                    "codex",
                    None,
                    vec![doc_account("Fresh", &path_text(&area.join("fresh")))],
                ),
            ],
        );

        let inspection = inspected_accounts(&state, &document).await;

        let fallback = |agent, name: &str| AccountFallback {
            agent,
            name: name.to_string(),
        };
        let console = &inspection.consoles[0];
        assert_eq!(
            console.account_fallbacks,
            [fallback(AgentId::CODEX, "fresh")]
        );
        assert_eq!(
            console.projects[0].account_fallbacks,
            [
                fallback(AgentId::CODEX, "Fresh"),
                fallback(AgentId::GROK, "Nowhere")
            ],
            "an existing account is no fallback"
        );
    }

    fn resolve(
        agent: AgentId,
        account: usize,
        directory: Option<&str>,
        leave_out: bool,
    ) -> AccountResolution {
        AccountResolution {
            agent,
            account,
            directory: directory.map(str::to_string),
            leave_out,
        }
    }

    #[tokio::test]
    async fn accounts_are_created_first_so_references_bind_to_them_and_each_one_is_settled_alone() {
        let area = ScratchDir::new("import-apply-accounts-area");
        let (state, _state_dir) = account_state("import-apply-accounts", &area);
        claude_account(
            &state,
            "claude-existing",
            "Existing",
            &area.join("existing"),
        );
        state
            .store
            .set_added_agents_if_undecided(&[AgentId::CLAUDE])
            .unwrap();
        let mut disabled = doc_account("New", &path_text(&area.join("new")));
        disabled["disabled"] = json!(true);
        let mut project_using = project("p", &path_text(&directory(&area, "p")));
        project_using.accounts[AgentId::CLAUDE] = ProjectAccountRef::Account {
            name: "new".to_string(),
        };
        project_using.accounts[AgentId::CODEX] = ProjectAccountRef::Account {
            name: "Left".to_string(),
        };
        let mut hub = console("Hub", vec![project_using]);
        hub.accounts[AgentId::CLAUDE] = Some(AccountName {
            name: "NEW".to_string(),
        });
        let document = with_accounts(
            document(vec![hub]),
            &["claude", "codex"],
            vec![
                agent_entry(
                    "claude",
                    None,
                    vec![
                        doc_account("Existing", &path_text(&area.join("other"))),
                        disabled,
                        doc_account("Left", &path_text(&area.join("left"))),
                        doc_account("Moved", &path_text(&area.join("moved-from"))),
                        doc_account("Clash", &path_text(&area.join("existing"))),
                        doc_account("", &path_text(&area.join("blank"))),
                        doc_account("Bare", "~/bare-segment-in-home"),
                    ],
                ),
                agent_entry(
                    "codex",
                    None,
                    vec![doc_account("Left", &path_text(&area.join("codex-left")))],
                ),
            ],
        );
        let moved_to = area.join("moved-to");

        let imported = apply(
            &state,
            &document,
            vec![at(0, 0, DirectoryChoice::ExportedPath)],
            ApplyOptions {
                settings: Vec::new(),
                accounts: Some(vec![
                    resolve(AgentId::CLAUDE, 2, None, true),
                    resolve(AgentId::CLAUDE, 3, Some(&path_text(&moved_to)), false),
                    resolve(AgentId::CODEX, 0, None, true),
                ]),
                consoles: true,
                clone_dir: true,
                agents: None,
            },
        )
        .await
        .unwrap();

        let accounts = imported.accounts.unwrap();
        assert_eq!(accounts.agents_added, [AgentId::CODEX]);
        assert_eq!(
            state.store.added_agents().unwrap(),
            Some(vec![AgentId::CLAUDE, AgentId::CODEX])
        );
        let claude = &accounts.agents[0];
        use AccountOutcome::{Created, Existing, Failed, LeftOut};
        let outcomes: Vec<_> = claude.accounts.iter().map(|a| a.status).collect();
        assert_eq!(
            outcomes,
            [Existing, Created, LeftOut, Created, Failed, Failed, Created]
        );
        assert_eq!(
            claude.accounts[4].error.as_ref().unwrap().code,
            error_code::CONFIG_DIR_TAKEN
        );
        assert_eq!(
            claude.accounts[5].error.as_ref().unwrap().code,
            error_code::FIELD_REQUIRED
        );
        assert!(claude.accounts[2].account.is_none());

        let stored = state.store.list_accounts().unwrap();
        let by_name = |name: &str| stored.iter().find(|a| a.name == name).cloned();
        let existing = by_name("Existing").unwrap();
        assert_eq!(
            existing.config_dir,
            path_text(&area.join("existing")),
            "untouched"
        );
        assert_eq!(
            claude.accounts[0].account.as_deref(),
            Some("claude-existing")
        );
        let created = by_name("New").unwrap();
        assert!(created.disabled && !created.share_default_config);
        assert_eq!(created.config_dir, path_text(&area.join("new")));
        assert_eq!(by_name("Moved").unwrap().config_dir, path_text(&moved_to));
        assert_eq!(
            by_name("Bare").unwrap().config_dir,
            path_text(&crate::paths::home_dir().join("bare-segment-in-home"))
        );
        assert!(by_name("Left").is_none() && by_name("Clash").is_none());
        assert_eq!(stored.len(), 4);

        // Placed after the account the machine had, in the document's order.
        let order = state.store.account_order(AgentId::CLAUDE).unwrap();
        assert_eq!(
            order,
            [
                None,
                Some("claude-existing".to_string()),
                Some(created.id.clone()),
                Some(by_name("Moved").unwrap().id),
                Some(by_name("Bare").unwrap().id),
            ]
        );

        // The references bound to the account the import created; the one to a left-out account
        // fell back, and is reported so.
        let console = &state.store.list_consoles().unwrap()[0];
        assert_eq!(
            console.accounts[AgentId::CLAUDE].as_deref(),
            Some(created.id.as_str())
        );
        let project = &stored_projects(&state)[0];
        assert_eq!(
            project.accounts[AgentId::CLAUDE],
            ProjectAccount::Account { id: created.id }
        );
        assert_eq!(project.accounts[AgentId::CODEX], ProjectAccount::Default);
        assert_eq!(imported.consoles[0].account_fallbacks, []);
        assert_eq!(
            imported.consoles[0].projects[0].account_fallbacks,
            [AccountFallback {
                agent: AgentId::CODEX,
                name: "Left".to_string(),
            }]
        );
    }

    #[tokio::test]
    async fn each_account_is_reported_as_it_is_handled_the_left_out_one_never() {
        use super::super::tests::progress::reported;
        let area = ScratchDir::new("import-apply-progress-area");
        let (state, _state_dir) = account_state("import-apply-progress", &area);
        claude_account(&state, "known", "Known", &area.join("known"));
        let document = with_accounts(
            document(Vec::new()),
            &[],
            vec![agent_entry(
                "claude",
                None,
                vec![
                    doc_account("Known", &path_text(&area.join("elsewhere"))),
                    doc_account("Fine", &path_text(&area.join("fine"))),
                    // The name an account of the document took before it.
                    doc_account("fine", &path_text(&area.join("again"))),
                    doc_account("Skipped", &path_text(&area.join("skipped"))),
                ],
            )],
        );
        let mut events = state.subscribe();

        apply_reporting(
            &state,
            &document,
            Vec::new(),
            ApplyOptions {
                settings: Vec::new(),
                accounts: Some(vec![AccountResolution {
                    agent: AgentId::CLAUDE,
                    account: 3,
                    directory: None,
                    leave_out: true,
                }]),
                consoles: true,
                clone_dir: true,
                agents: None,
            },
            Some("accounts".to_string()),
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            reported(&mut events, "accounts"),
            [
                "account Claude Code.0: waiting",
                "account Claude Code.1: waiting",
                "account Claude Code.2: waiting",
                "account Claude Code.0: done",
                "account Claude Code.1: creating",
                "account Claude Code.1: done",
                "account Claude Code.2: creating",
                "account Claude Code.2: failed",
            ]
        );
    }

    #[tokio::test]
    async fn an_existing_account_is_announced_and_done_even_when_the_choice_leaves_it_out() {
        use super::super::tests::progress::reported;
        let area = ScratchDir::new("import-apply-progress-existing-area");
        let (state, _state_dir) = account_state("import-apply-progress-existing", &area);
        claude_account(&state, "known", "Known", &area.join("known"));
        let document = with_accounts(
            document(Vec::new()),
            &[],
            vec![agent_entry(
                "claude",
                None,
                vec![doc_account("Known", &path_text(&area.join("elsewhere")))],
            )],
        );
        let mut events = state.subscribe();

        let imported = apply_reporting(
            &state,
            &document,
            Vec::new(),
            ApplyOptions {
                settings: Vec::new(),
                accounts: Some(vec![AccountResolution {
                    agent: AgentId::CLAUDE,
                    account: 0,
                    directory: None,
                    leave_out: true,
                }]),
                consoles: true,
                clone_dir: true,
                agents: None,
            },
            Some("existing".to_string()),
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            imported.accounts.unwrap().agents[0].accounts[0].status,
            AccountOutcome::Existing
        );
        assert_eq!(
            reported(&mut events, "existing"),
            [
                "account Claude Code.0: waiting",
                "account Claude Code.0: done"
            ]
        );
    }

    /// Sharing the agent's default configuration moves files outside Octoboard's own directory, so
    /// it is never turned on by an import: a document of an earlier build that carries the flag has
    /// it ignored, and the account is created unshared with the agent's directory untouched.
    #[tokio::test]
    async fn an_account_the_document_says_shares_is_created_unshared() {
        let area = ScratchDir::new("import-apply-sharing-area");
        let (state, _state_dir) = account_state("import-apply-sharing", &area);
        let default_dir = directory(&area, "default-claude");
        std::fs::write(default_dir.join("settings.json"), b"{}").unwrap();
        let mut shared = doc_account("Shared", &path_text(&area.join("shared")));
        shared["share_default_config"] = json!(true);
        let document = with_accounts(
            document(Vec::new()),
            &[],
            vec![agent_entry("claude", None, vec![shared])],
        );

        let inspected = inspected_accounts(&state, &document)
            .await
            .accounts
            .unwrap();
        assert_eq!(inspected.agents[0].accounts[0].status, AccountStatus::New);
        let imported = apply(
            &state,
            &document,
            Vec::new(),
            ApplyOptions {
                settings: Vec::new(),
                accounts: Some(Vec::new()),
                consoles: true,
                clone_dir: true,
                agents: None,
            },
        )
        .await
        .unwrap();

        let claude = &imported.accounts.unwrap().agents[0];
        assert_eq!(claude.accounts[0].status, AccountOutcome::Created);
        let stored = state.store.list_accounts().unwrap();
        assert_eq!(stored.len(), 1);
        assert!(!stored[0].share_default_config);
        assert!(!stored[0].share_default_config_incomplete);
        assert!(!area.join("shared").join("settings.json").exists());
        assert!(!default_dir.join("settings.json").is_symlink());
    }

    #[tokio::test]
    async fn the_default_accounts_disabled_flag_is_reported_and_applied_as_the_document_has_it() {
        let area = ScratchDir::new("import-default-disabled-area");
        let (state, _state_dir) = account_state("import-default-disabled", &area);
        state
            .store
            .set_default_account_disabled(AgentId::GROK, true)
            .unwrap();
        let entry = |agent: &str, disabled: Option<bool>| {
            let mut entry = agent_entry(agent, None, Vec::new());
            entry["default_account"]["disabled"] = json!(disabled);
            entry
        };
        let document = with_accounts(
            document(Vec::new()),
            &[],
            vec![
                entry("claude", Some(true)),
                entry("codex", Some(false)),
                entry("grok", Some(false)),
            ],
        );

        let inspected = inspected_accounts(&state, &document)
            .await
            .accounts
            .unwrap();
        let reported: Vec<(Option<bool>, bool)> = inspected
            .agents
            .iter()
            .map(|a| (a.default_disabled, a.default_disabled_applies))
            .collect();
        // Claude turns off, Codex is off nowhere and stays so, Grok turns back on.
        assert_eq!(
            reported,
            [
                (Some(true), true),
                (Some(false), false),
                (Some(false), true)
            ]
        );
        assert!(!state
            .store
            .disabled_default_accounts()
            .unwrap()
            .contains("claude"));

        let imported = apply(
            &state,
            &document,
            Vec::new(),
            ApplyOptions {
                settings: Vec::new(),
                accounts: Some(Vec::new()),
                consoles: true,
                clone_dir: true,
                agents: None,
            },
        )
        .await
        .unwrap();

        let applied: Vec<Option<bool>> = imported
            .accounts
            .unwrap()
            .agents
            .iter()
            .map(|a| a.default_disabled_applied)
            .collect();
        assert_eq!(applied, [Some(true), None, Some(false)]);
        let disabled = state.store.disabled_default_accounts().unwrap();
        assert!(disabled.contains("claude") && !disabled.contains("grok"));
    }

    #[tokio::test]
    async fn the_default_account_name_is_applied_by_the_rule_and_an_absent_disabled_flag_changes_nothing(
    ) {
        let area = ScratchDir::new("import-apply-default-name-area");
        let (state, _state_dir) = account_state("import-apply-default-name", &area);
        state
            .store
            .set_default_account_name(AgentId::GROK, Some("Mine"))
            .unwrap();
        let document = with_accounts(
            document(Vec::new()),
            &[],
            vec![
                agent_entry("claude", Some(" Personal "), Vec::new()),
                agent_entry(
                    "codex",
                    Some("same"),
                    vec![doc_account("Same", &path_text(&area.join("same")))],
                ),
                agent_entry("grok", Some("Theirs"), Vec::new()),
            ],
        );

        let imported = apply(
            &state,
            &document,
            Vec::new(),
            ApplyOptions {
                settings: Vec::new(),
                accounts: Some(Vec::new()),
                consoles: true,
                clone_dir: true,
                agents: None,
            },
        )
        .await
        .unwrap();

        let applied: Vec<bool> = imported
            .accounts
            .unwrap()
            .agents
            .iter()
            .map(|a| a.default_name_applied)
            .collect();
        assert_eq!(applied, [true, false, false]);
        let names = state.store.default_account_names().unwrap();
        assert_eq!(names.get("claude").map(String::as_str), Some("Personal"));
        assert_eq!(names.get("grok").map(String::as_str), Some("Mine"));
        assert!(!names.contains_key("codex"));
        assert!(state.store.disabled_default_accounts().unwrap().is_empty());
    }

    #[test]
    fn a_default_account_that_got_a_name_meanwhile_is_not_named_again() {
        let area = ScratchDir::new("import-name-default-race-area");
        let (state, _state_dir) = account_state("import-name-default-race", &area);

        assert!(coordinator::accounts::name_unnamed_default_account(
            &state,
            AgentId::CLAUDE,
            " Personal "
        )
        .unwrap());
        // A rename made since the check an inspection reports leaves the machine's name alone.
        assert!(!coordinator::accounts::name_unnamed_default_account(
            &state,
            AgentId::CLAUDE,
            "Other"
        )
        .unwrap());
        let names = state.store.default_account_names().unwrap();
        assert_eq!(names.get("claude").map(String::as_str), Some("Personal"));

        // A blank name or the word for an unnamed one clears nothing here.
        for name in ["  ", "DEFAULT"] {
            assert!(!coordinator::accounts::name_unnamed_default_account(
                &state,
                AgentId::GROK,
                name
            )
            .unwrap());
        }

        // A name an account has meanwhile taken is refused, the one refusal an import expects.
        state
            .store
            .insert_account(&account("codex-work", AgentId::CODEX, "Work"))
            .unwrap();
        let err =
            coordinator::accounts::name_unnamed_default_account(&state, AgentId::CODEX, "work")
                .expect_err("the name is taken");
        assert!(is_name_taken(&err));
        assert!(!state
            .store
            .default_account_names()
            .unwrap()
            .contains_key("codex"));
    }

    #[tokio::test]
    async fn accounts_are_created_only_when_asked_for_and_a_document_without_them_is_refused_first()
    {
        let area = ScratchDir::new("import-apply-accounts-asked-area");
        let (state, _state_dir) = account_state("import-apply-accounts-asked", &area);
        let with = with_accounts(
            document(vec![console("Hub", Vec::new())]),
            &["claude"],
            vec![agent_entry(
                "claude",
                None,
                vec![doc_account("Work", &path_text(&area.join("work")))],
            )],
        );

        let imported = apply(
            &state,
            &with,
            Vec::new(),
            ApplyOptions {
                settings: Vec::new(),
                accounts: None,
                consoles: true,
                clone_dir: true,
                agents: None,
            },
        )
        .await
        .unwrap();

        assert!(imported.accounts.is_none());
        assert!(state.store.list_accounts().unwrap().is_empty());
        assert_eq!(state.store.added_agents().unwrap(), None);
        assert_eq!(state.store.list_consoles().unwrap().len(), 1);

        let before = state.store.list_consoles().unwrap().len();
        let err = apply(
            &state,
            &document(vec![console("Other", Vec::new())]),
            Vec::new(),
            ApplyOptions {
                settings: Vec::new(),
                accounts: Some(Vec::new()),
                consoles: true,
                clone_dir: true,
                agents: None,
            },
        )
        .await
        .expect_err("no accounts in the document");
        assert_eq!(code_of(&err), error_code::IMPORT_ACCOUNTS_MISSING);
        assert_eq!(state.store.list_consoles().unwrap().len(), before);
    }

    #[tokio::test]
    async fn only_the_agents_asked_for_are_applied_and_one_the_document_lacks_is_refused() {
        let area = ScratchDir::new("import-apply-agents-area");
        let (state, _state_dir) = account_state("import-apply-agents", &area);
        state
            .store
            .set_added_agents_if_undecided(&[AgentId::GROK])
            .unwrap();
        // The machine has the account "Left" of Claude Code already; "Right" only the document has.
        claude_account(&state, "claude-left", "Left", &area.join("left"));
        let referring_to = |name: &str, account: &str| {
            let mut referring = console(name, Vec::new());
            referring.accounts[AgentId::CLAUDE] = Some(AccountName {
                name: account.to_string(),
            });
            referring
        };
        let mut claude_entry = agent_entry(
            "claude",
            Some("Main"),
            vec![
                doc_account("Left", &path_text(&area.join("left"))),
                doc_account("Right", &path_text(&area.join("right"))),
            ],
        );
        claude_entry["default_account"]["disabled"] = json!(true);
        let mut document = with_accounts(
            document(vec![
                referring_to("Hub", "Left"),
                referring_to("Second", "Right"),
            ]),
            &["claude", "codex"],
            vec![
                claude_entry,
                agent_entry(
                    "codex",
                    Some("Primary"),
                    vec![doc_account("Kept", &path_text(&area.join("kept")))],
                ),
            ],
        );
        document["accounts"]["agent_settings"] = json!({
            "auto_select_account": true,
            "auto_switch_on_usage_limit": true,
            "auto_start_quota_windows": true,
            "agent_tags": { "claude": ["one"], "codex": ["two"] },
            "provider_settings": {
                "claude": { "approval_mode": "accept_edits" },
                "codex": { "approval_reviewer": "auto_review" },
            },
        });
        let options = |agents: Vec<AgentId>| ApplyOptions {
            settings: Vec::new(),
            accounts: Some(Vec::new()),
            consoles: true,
            clone_dir: true,
            agents: Some(agents),
        };

        let err = apply(
            &state,
            &document,
            Vec::new(),
            options(vec![AgentId::CODEX, AgentId::GROK]),
        )
        .await
        .expect_err("the document carries nothing for Grok");
        assert_eq!(code_of(&err), error_code::IMPORT_AGENT_MISSING);
        assert_eq!(param_of(&err, "agent"), AgentId::GROK.label());
        assert_eq!(state.store.list_accounts().unwrap().len(), 1);
        assert!(state.store.list_consoles().unwrap().is_empty());
        assert_eq!(
            state.store.added_agents().unwrap(),
            Some(vec![AgentId::GROK])
        );

        let imported = apply(
            &state,
            &document,
            Vec::new(),
            options(vec![AgentId::CODEX, AgentId::CODEX]),
        )
        .await
        .unwrap();

        let accounts = imported.accounts.unwrap();
        assert_eq!(accounts.agents_added, [AgentId::CODEX]);
        assert_eq!(
            state.store.added_agents().unwrap(),
            Some(vec![AgentId::GROK, AgentId::CODEX])
        );
        let reported: Vec<(AgentId, usize, bool)> = accounts
            .agents
            .iter()
            .map(|a| (a.agent, a.accounts.len(), a.default_name_applied))
            .collect();
        assert_eq!(reported, [(AgentId::CODEX, 1, true)]);
        let stored = state.store.list_accounts().unwrap();
        assert_eq!(stored.len(), 2);
        assert!(stored.iter().any(|a| a.name == "Left"));
        assert!(stored.iter().any(|a| a.name == "Kept"));
        let names = state.store.default_account_names().unwrap();
        assert_eq!(names.keys().collect::<Vec<_>>(), ["codex"]);
        // The switches are no agent's and still apply; the tags are those of the agent applied.
        let settings = accounts.agent_settings_applied.unwrap();
        assert!(settings.auto_select_account);
        assert_eq!(settings.agent_tags.keys().collect::<Vec<_>>(), ["codex"]);
        assert_eq!(
            state.store.agent_tags().unwrap().keys().collect::<Vec<_>>(),
            ["codex"]
        );
        // The agent left out has its provider settings and its default account's flag untouched.
        let chosen = state.store.provider_settings().unwrap();
        assert_eq!(chosen["claude"]["approval_mode"], None);
        assert_eq!(
            chosen["codex"]["approval_reviewer"],
            Some(SettingValue::Choice("auto_review".to_string()))
        );
        assert!(state.store.disabled_default_accounts().unwrap().is_empty());
        // The account of the agent left out is not created, so a reference to it falls back, but
        // one to an account the machine has already still binds to it.
        let consoles = state.store.list_consoles().unwrap();
        let by_name = |name: &str| consoles.iter().find(|c| c.name == name).unwrap();
        assert_eq!(
            by_name("Hub").accounts[AgentId::CLAUDE].as_deref(),
            Some("claude-left")
        );
        assert_eq!(by_name("Second").accounts[AgentId::CLAUDE], None);
        let fallbacks = |name: &str| {
            imported
                .consoles
                .iter()
                .find(|c| c.name == name)
                .unwrap()
                .account_fallbacks
                .clone()
        };
        assert_eq!(fallbacks("Hub"), []);
        assert_eq!(
            fallbacks("Second"),
            [AccountFallback {
                agent: AgentId::CLAUDE,
                name: "Right".to_string(),
            }]
        );
    }

    #[tokio::test]
    async fn an_empty_list_of_agents_applies_only_the_three_switches() {
        let area = ScratchDir::new("import-apply-no-agents-area");
        let (state, _state_dir) = account_state("import-apply-no-agents", &area);
        let mut document = with_accounts(
            document(vec![console("Hub", Vec::new())]),
            &["codex"],
            vec![agent_entry(
                "codex",
                Some("Primary"),
                vec![doc_account("Kept", &path_text(&area.join("kept")))],
            )],
        );
        document["accounts"]["agent_settings"] = json!({
            "auto_select_account": true,
            "auto_switch_on_usage_limit": true,
            "auto_start_quota_windows": true,
            "agent_tags": { "codex": ["two"] },
            "provider_settings": { "codex": { "approval_reviewer": "auto_review" } },
        });

        let imported = apply(
            &state,
            &document,
            Vec::new(),
            ApplyOptions {
                settings: Vec::new(),
                accounts: Some(Vec::new()),
                consoles: false,
                clone_dir: true,
                agents: Some(Vec::new()),
            },
        )
        .await
        .unwrap();

        let accounts = imported.accounts.unwrap();
        assert!(accounts.agents_added.is_empty());
        assert!(accounts.agents.is_empty());
        assert!(state.store.list_accounts().unwrap().is_empty());
        assert!(state.store.default_account_names().unwrap().is_empty());
        let settings = accounts.agent_settings_applied.unwrap();
        assert!(settings.auto_select_account);
        assert!(settings.agent_tags.is_empty());
        assert!(settings.provider_settings.is_empty());
        assert!(state.store.agent_tags().unwrap().is_empty());
        assert_eq!(
            state.store.provider_settings().unwrap()["codex"]["approval_reviewer"],
            None
        );
    }

    #[tokio::test]
    async fn without_the_consoles_none_is_created_and_their_resolutions_are_not_read() {
        let area = ScratchDir::new("import-apply-no-consoles-area");
        let (state, _state_dir) = account_state("import-apply-no-consoles", &area);
        let project_dir = area.join("site");
        std::fs::create_dir_all(&project_dir).unwrap();
        let document = with_accounts(
            document(vec![console(
                "Hub",
                vec![project("Site", &path_text(&project_dir))],
            )]),
            &["claude"],
            vec![agent_entry(
                "claude",
                None,
                vec![doc_account("Work", &path_text(&area.join("work")))],
            )],
        );
        let consoles_before = state.store.list_consoles().unwrap().len();
        let projects_before = state.store.list_projects().unwrap().len();

        // A resolution for a project the document does not have is no problem when the consoles
        // are not imported: it is not read.
        let imported = apply(
            &state,
            &document,
            vec![at(7, 7, DirectoryChoice::ExportedPath)],
            ApplyOptions {
                settings: Vec::new(),
                accounts: Some(Vec::new()),
                consoles: false,
                clone_dir: true,
                agents: None,
            },
        )
        .await
        .unwrap();

        assert!(imported.consoles.is_empty());
        assert_eq!(state.store.list_consoles().unwrap().len(), consoles_before);
        assert_eq!(state.store.list_projects().unwrap().len(), projects_before);
        let accounts = imported.accounts.unwrap();
        assert_eq!(
            accounts.agents[0].accounts[0].status,
            AccountOutcome::Created
        );

        // The same resolution is refused while the consoles are imported.
        let err = apply(
            &state,
            &document,
            vec![at(7, 7, DirectoryChoice::ExportedPath)],
            ApplyOptions {
                settings: Vec::new(),
                accounts: None,
                consoles: true,
                clone_dir: true,
                agents: None,
            },
        )
        .await
        .expect_err("no such project");
        assert_eq!(code_of(&err), error_code::IMPORT_RESOLUTION_INVALID);
    }

    #[tokio::test]
    async fn an_account_resolution_that_cannot_be_right_refuses_the_import_before_anything_is_created(
    ) {
        let area = ScratchDir::new("import-apply-account-resolutions-area");
        let (state, _state_dir) = account_state("import-apply-account-resolutions", &area);
        let document = with_accounts(
            document(vec![console("Hub", Vec::new())]),
            &["claude"],
            vec![agent_entry(
                "claude",
                None,
                vec![doc_account("Work", &path_text(&area.join("work")))],
            )],
        );
        let cases = [
            vec![resolve(AgentId::CLAUDE, 1, None, true)],
            vec![resolve(AgentId::CODEX, 0, None, true)],
            vec![
                resolve(AgentId::CLAUDE, 0, None, true),
                resolve(AgentId::CLAUDE, 0, Some("/elsewhere"), false),
            ],
            vec![resolve(AgentId::CLAUDE, 0, Some("relative/dir"), false)],
            vec![resolve(AgentId::CLAUDE, 0, Some("/elsewhere"), true)],
            vec![resolve(AgentId::CLAUDE, 0, None, false)],
        ];
        for resolutions in cases {
            let err = apply(
                &state,
                &document,
                Vec::new(),
                ApplyOptions {
                    settings: Vec::new(),
                    accounts: Some(resolutions),
                    consoles: true,
                    clone_dir: true,
                    agents: None,
                },
            )
            .await
            .expect_err("refused");
            assert_eq!(code_of(&err), error_code::IMPORT_RESOLUTION_INVALID);
        }
        assert!(state.store.list_accounts().unwrap().is_empty());
        assert!(state.store.list_consoles().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_accounts_of_an_export_are_imported_in_their_order_with_their_names_and_flags() {
        let area = ScratchDir::new("import-accounts-round-trip-area");
        let (source, _source_dir) = account_state("import-accounts-round-trip-source", &area);
        let (target_area, work) = (
            ScratchDir::new("import-accounts-round-trip-target"),
            area.join("work"),
        );
        let (target, _target_dir) =
            account_state("import-accounts-round-trip-target-state", &target_area);
        claude_account(&source, "claude-1", "Work", &work);
        let mut team = account("claude-2", AgentId::CLAUDE, "Team");
        team.config_dir = path_text(&area.join("team"));
        team.disabled = true;
        source.store.insert_account(&team).unwrap();
        source
            .store
            .reorder_accounts(
                AgentId::CLAUDE,
                &[
                    Some("claude-2".to_string()),
                    None,
                    Some("claude-1".to_string()),
                ],
            )
            .unwrap();
        source
            .store
            .set_default_account_name(AgentId::CLAUDE, Some("Personal"))
            .unwrap();
        source
            .store
            .set_added_agents_if_undecided(&[AgentId::CODEX, AgentId::CLAUDE])
            .unwrap();
        target
            .store
            .set_added_agents_if_undecided(&[AgentId::CLAUDE])
            .unwrap();
        let exported = serde_json::to_value(
            export::build(
                &source.store,
                &export::Selection {
                    consoles: export::ConsoleScope::None,
                    settings: Vec::new(),
                    accounts: true,
                    paths: true,
                },
            )
            .unwrap(),
        )
        .unwrap();

        apply(
            &target,
            &exported,
            Vec::new(),
            ApplyOptions {
                settings: Vec::new(),
                accounts: Some(Vec::new()),
                consoles: true,
                clone_dir: true,
                agents: None,
            },
        )
        .await
        .unwrap();

        let stored = target.store.list_accounts().unwrap();
        let ids: Vec<_> = target
            .store
            .account_order(AgentId::CLAUDE)
            .unwrap()
            .into_iter()
            .map(|id| id.map(|id| stored.iter().find(|a| a.id == id).unwrap().name.clone()))
            .collect();
        assert_eq!(
            ids,
            [None, Some("Team".to_string()), Some("Work".to_string())],
            "the document's order after the default account, which keeps the machine's place"
        );
        let team = stored.iter().find(|a| a.name == "Team").unwrap();
        assert!(team.disabled);
        assert_eq!(team.config_dir, path_text(&area.join("team")));
        assert_eq!(
            target.store.added_agents().unwrap(),
            Some(vec![AgentId::CLAUDE, AgentId::CODEX])
        );
        assert_eq!(
            target.store.default_account_name(AgentId::CLAUDE).unwrap(),
            "Personal"
        );
    }
}
