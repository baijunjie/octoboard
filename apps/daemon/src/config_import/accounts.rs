//! The accounts region of a configuration document: what an inspection says importing each account
//! would do, and the creation of the accounts as the Add account form would, with the added agents
//! and the default accounts' names around them. The account helpers it calls are the
//! coordinator's, so an imported account passes the checks a hand-added one does.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{ImportFailure, Rebinder};
use crate::account_dirs::{account_dir_key, normalize_account_dir};
use crate::availability;
use crate::config_export::{self, DocumentAccount, DocumentAccounts};
use crate::coordinator;
use crate::protocol::{error_code, Agent, CodedError};
use crate::state::AppState;

/// The choice made for one account of the document when applying: another directory, or leaving
/// the account out. Exactly one of the two is given.
#[derive(Debug, Deserialize)]
pub struct AccountResolution {
    pub agent: Agent,
    /// The account's index among the document's accounts of `agent`, in the order it lists them.
    pub account: usize,
    /// An absolute path or one starting with `~/`.
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub leave_out: bool,
}

/// What importing one account of the document would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    /// It would be created.
    New,
    /// An account of the same agent has its name already; it is used as it is and never modified.
    Existing,
    /// The proposed directory is already used by another account of the agent, the default account
    /// included.
    DirectoryInUse,
    /// The Add account form would refuse its name or directory.
    Invalid,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectedAccount {
    pub name: String,
    /// The directory as the document records it.
    pub directory: String,
    /// The directory as it resolves on this machine (`~/` expanded); absent when the document's
    /// cannot be resolved.
    pub proposed_directory: Option<String>,
    pub status: AccountStatus,
    /// The name of the account that uses the proposed directory, for `directory_in_use`.
    pub in_use_by: Option<String>,
    /// Why it is `invalid`.
    pub error: Option<ImportFailure>,
    pub share_default_config: bool,
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectedAgentAccounts {
    pub agent: Agent,
    pub accounts: Vec<InspectedAccount>,
    /// The name the document gives the agent's default account, when it gives one.
    pub default_name: Option<String>,
    /// Whether importing would apply `default_name`: this machine's default account has no name of
    /// its own and the name collides with no account of the agent, those the import creates
    /// included.
    pub default_name_applies: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectedAccounts {
    /// The agents of the document not yet on this machine's added agents, which importing would
    /// add, in the document's order.
    pub agents_to_add: Vec<Agent>,
    pub agents: Vec<InspectedAgentAccounts>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountOutcome {
    Created,
    /// An account of the agent with the name was there; used as it is.
    Existing,
    /// A resolution said to leave it out.
    LeftOut,
    /// It could not be created; `error` says why.
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedAccount {
    pub name: String,
    pub status: AccountOutcome,
    /// The id of the account created, or of the one that existed.
    pub account: Option<String>,
    pub error: Option<ImportFailure>,
    /// Why sharing the default configuration, which the document asked for, was not turned on for
    /// an account that was created all the same.
    pub sharing_warning: Option<ImportFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedAgentAccounts {
    pub agent: Agent,
    pub accounts: Vec<ImportedAccount>,
    /// Whether the document's name for the default account was applied.
    pub default_name_applied: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedAccounts {
    /// The agents appended to the added agents, in the order they were.
    pub agents_added: Vec<Agent>,
    pub agents: Vec<ImportedAgentAccounts>,
}

/// One agent's part of a document's accounts. A hand-edited document may list an agent twice:
/// the parts are read as one, the accounts in the order they are listed.
struct AgentAccounts<'a> {
    agent: Agent,
    default_name: Option<&'a str>,
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
                    accounts: Vec::new(),
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[at];
        group.default_name = group.default_name.or(named);
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
    agent: Agent,
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
    let name = match coordinator::normalize_account_name(&account.name) {
        Ok(name) => name,
        Err(err) => return Ok(invalid(proposed.ok(), err)),
    };
    let key = coordinator::account_name_key(&name);
    let taken_by = match coordinator::colliding_account_name(state, agent, &name, None)? {
        Some(existing) => Some(existing),
        None => earlier
            .iter()
            .find(|earlier| earlier.name_key == key)
            .map(|earlier| earlier.name.clone()),
    };
    if let Some(existing) = taken_by {
        return Ok(invalid(
            proposed.ok(),
            coordinator::account_name_taken(agent, &name, &existing),
        ));
    }
    let dir = match proposed {
        Ok(dir) => dir,
        Err(err) => return Ok(invalid(None, err)),
    };
    let in_use_by = match coordinator::colliding_account_dir(state, agent, &dir, None)? {
        Some(existing) => Some(existing),
        None => {
            let key = account_dir_key(&dir);
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
    agent: Agent,
    name: &str,
    created: &HashSet<(Agent, String)>,
) -> Result<bool> {
    let key = coordinator::account_name_key(name);
    if coordinator::clears_default_account_name(name)
        || state
            .store
            .default_account_names()?
            .contains_key(agent.wire_name())
        || created.contains(&(agent, key))
    {
        return Ok(false);
    }
    Ok(coordinator::colliding_owned_account_name(state, agent, name, None)?.is_none())
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
            let key = coordinator::account_name_key(&account.name);
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
                        dir_key: account_dir_key(&dir),
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
                proposed_directory,
                status,
                in_use_by,
                error,
                share_default_config: account.share_default_config,
                disabled: account.disabled,
            });
        }
        let default_name_applies = match group.default_name {
            Some(name) => default_name_applies(state, group.agent, name, &created)?,
            None => false,
        };
        agents.push(InspectedAgentAccounts {
            agent: group.agent,
            accounts,
            default_name: group.default_name.map(str::to_string),
            default_name_applies,
        });
    }
    Ok(InspectedAccounts {
        agents_to_add,
        agents,
    })
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
) -> Result<HashMap<(Agent, usize), AccountChoice>> {
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
                coordinator::absolute_path(&directory)
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

/// Creates the document's accounts as the Add account form would, with the added agents and the
/// default accounts' names around them.
///
/// First every agent of the document missing from the added agents is appended (a list not yet
/// decided is first filled, and where it stays undecided nothing is appended, since every agent
/// counts as added until it is). Then each account in the document's order: one whose name an
/// account of the agent had before the import is used as it is, a left-out one is skipped, and any
/// other goes through the form's checks, a failure of which is recorded for that account alone.
/// A created account takes the document's disabled flag, comes after the agent's other accounts in
/// its display order, and has sharing turned on afterwards where the document says so, a failure
/// of which is a warning on the account. The default account's name is applied last, under the
/// rule an inspection reports.
pub(super) async fn apply_accounts(
    state: &Arc<AppState>,
    document: &DocumentAccounts,
    mut chosen: HashMap<(Agent, usize), AccountChoice>,
) -> Result<ImportedAccounts> {
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
            coordinator::broadcast_settings(state)?;
        }
    }

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
                sharing_warning: None,
            };
            if let Some(id) = before.find(group.agent, &account.name) {
                accounts.push(outcome(AccountOutcome::Existing, Some(id)));
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
            match coordinator::insert_new_account(
                state,
                group.agent,
                account.name.clone(),
                directory,
                account.disabled,
            ) {
                Ok(created) => {
                    let mut imported = outcome(AccountOutcome::Created, Some(created.id.clone()));
                    if account.share_default_config {
                        if let Err(err) =
                            crate::shared_config::set(state, Some(created.id.clone()), true).await
                        {
                            imported.sharing_warning = Some(ImportFailure::of(&err));
                        }
                    }
                    created_ids.push(created.id);
                    accounts.push(imported);
                }
                Err(err) => {
                    let mut failed = outcome(AccountOutcome::Failed, None);
                    failed.error = Some(ImportFailure::of(&err));
                    accounts.push(failed);
                }
            }
        }
        if !created_ids.is_empty() {
            place_after_existing(state, group.agent, &created_ids);
        }
        agents.push((group.agent, group.default_name, accounts));
    }

    let mut imported = Vec::new();
    for (agent, default_name, accounts) in agents {
        let mut default_name_applied = false;
        if let Some(name) = default_name {
            // The rule an inspection reports, checked under the lock the write takes: a name
            // given to the default account since is kept, and a name an account has taken since
            // is refused, which is expected here and not reported.
            match coordinator::name_unnamed_default_account(state, agent, name) {
                Ok(written) => default_name_applied = written,
                Err(err) if is_name_taken(&err) => {}
                Err(err) => {
                    tracing::warn!(%err, "naming the default account after an import failed")
                }
            }
        }
        imported.push(ImportedAgentAccounts {
            agent,
            accounts,
            default_name_applied,
        });
    }
    Ok(ImportedAccounts {
        agents_added,
        agents: imported,
    })
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
fn place_after_existing(state: &Arc<AppState>, agent: Agent, created: &[String]) {
    let placed = || -> Result<bool> {
        let owned: Vec<String> = state
            .store
            .list_accounts()?
            .into_iter()
            .filter(|account| account.agent == agent)
            .map(|account| account.id)
            .collect();
        let mut order = config_export::display_order(&state.store.account_order(agent)?, &owned);
        order.retain(|id| id.as_ref().is_none_or(|id| !created.contains(id)));
        order.extend(created.iter().cloned().map(Some));
        state.store.reorder_accounts(agent, &order)
    };
    match placed() {
        Ok(true) => {
            if let Err(err) = coordinator::broadcast_settings(state) {
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
        account, at, code_of, console, directory, document, project, stored_projects,
    };
    use super::super::{apply, inspect, AccountFallback, DirectoryChoice, Inspection};
    use super::*;
    use crate::config_export::{AccountName, ProjectAccountRef};
    use crate::protocol::ProjectAccount;
    use crate::test_support::{app_state, ScratchDir};
    use serde_json::json;
    use std::path::Path;

    fn doc_account(name: &str, directory: &str) -> serde_json::Value {
        json!({
            "name": name,
            "directory": directory,
            "share_default_config": false,
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
            Agent::ALL
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
        let mut stored = account(id, Agent::Claude, name);
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
    async fn the_sharing_and_disabled_values_and_the_agents_to_add_are_reported() {
        let area = ScratchDir::new("import-inspect-agents-area");
        let (state, _state_dir) = account_state("import-inspect-agents", &area);
        state
            .store
            .set_added_agents_if_undecided(&[Agent::Claude])
            .unwrap();
        let mut shared = doc_account("Shared", &path_text(&area.join("shared")));
        shared["share_default_config"] = json!(true);
        shared["disabled"] = json!(true);
        let document = with_accounts(
            document(Vec::new()),
            &["codex", "claude", "codex", "grok"],
            vec![agent_entry("claude", None, vec![shared])],
        );

        let accounts = inspected_accounts(&state, &document)
            .await
            .accounts
            .unwrap();

        assert_eq!(accounts.agents_to_add, [Agent::Codex, Agent::Grok]);
        let row = &accounts.agents[0].accounts[0];
        assert!(row.share_default_config && row.disabled);
    }

    #[tokio::test]
    async fn the_default_account_name_applies_only_while_nothing_stands_in_its_way() {
        let area = ScratchDir::new("import-inspect-default-names-area");
        let (state, _state_dir) = account_state("import-inspect-default-names", &area);
        let mut taken = account("grok-taken", Agent::Grok, "Taken");
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
            .set_default_account_name(Agent::Claude, Some("Mine"))
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
        hub.claude_account = ProjectAccountRef::Account {
            name: "work".to_string(),
        };
        hub.codex_account = ProjectAccountRef::Account {
            name: "Fresh".to_string(),
        };
        hub.grok_account = ProjectAccountRef::Account {
            name: "Nowhere".to_string(),
        };
        let mut hub = console("Hub", vec![hub]);
        hub.codex_account = Some(AccountName {
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
        assert_eq!(console.account_fallbacks, [fallback(Agent::Codex, "fresh")]);
        assert_eq!(
            console.projects[0].account_fallbacks,
            [
                fallback(Agent::Codex, "Fresh"),
                fallback(Agent::Grok, "Nowhere")
            ],
            "an existing account is no fallback"
        );
    }

    fn resolve(
        agent: Agent,
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
            .set_added_agents_if_undecided(&[Agent::Claude])
            .unwrap();
        let mut disabled = doc_account("New", &path_text(&area.join("new")));
        disabled["disabled"] = json!(true);
        let mut project_using = project("p", &path_text(&directory(&area, "p")));
        project_using.claude_account = ProjectAccountRef::Account {
            name: "new".to_string(),
        };
        project_using.codex_account = ProjectAccountRef::Account {
            name: "Left".to_string(),
        };
        let mut hub = console("Hub", vec![project_using]);
        hub.claude_account = Some(AccountName {
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
            false,
            Some(vec![
                resolve(Agent::Claude, 2, None, true),
                resolve(Agent::Claude, 3, Some(&path_text(&moved_to)), false),
                resolve(Agent::Codex, 0, None, true),
            ]),
            true,
        )
        .await
        .unwrap();

        let accounts = imported.accounts.unwrap();
        assert_eq!(accounts.agents_added, [Agent::Codex]);
        assert_eq!(
            state.store.added_agents().unwrap(),
            Some(vec![Agent::Claude, Agent::Codex])
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
        let order = state.store.account_order(Agent::Claude).unwrap();
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
            console.claude_account_id.as_deref(),
            Some(created.id.as_str())
        );
        let project = &stored_projects(&state)[0];
        assert_eq!(
            project.claude_account_id,
            ProjectAccount::Account { id: created.id }
        );
        assert_eq!(project.codex_account_id, ProjectAccount::Default);
        assert_eq!(imported.consoles[0].account_fallbacks, []);
        assert_eq!(
            imported.consoles[0].projects[0].account_fallbacks,
            [AccountFallback {
                agent: Agent::Codex,
                name: "Left".to_string(),
            }]
        );
    }

    #[tokio::test]
    async fn a_sharing_that_fails_is_a_warning_on_the_account_that_was_created_all_the_same() {
        let area = ScratchDir::new("import-apply-sharing-area");
        let (state, _state_dir) = account_state("import-apply-sharing", &area);
        // An account directory that cannot be made, since a file is where its parent should be.
        let blocker = area.join("blocker");
        std::fs::write(&blocker, b"").unwrap();
        let shared = |name: &str, dir: &Path| {
            let mut account = doc_account(name, &path_text(dir));
            account["share_default_config"] = json!(true);
            account
        };
        let document = with_accounts(
            document(Vec::new()),
            &[],
            vec![agent_entry(
                "claude",
                None,
                vec![
                    shared("Fine", &area.join("fine")),
                    shared("Blocked", &blocker.join("inside")),
                    doc_account("Plain", &path_text(&area.join("plain"))),
                ],
            )],
        );

        let imported = apply(&state, &document, Vec::new(), false, Some(Vec::new()), true)
            .await
            .unwrap();

        let claude = &imported.accounts.unwrap().agents[0];
        assert!(claude
            .accounts
            .iter()
            .all(|a| a.status == AccountOutcome::Created));
        assert!(claude.accounts[0].sharing_warning.is_none());
        let warning = claude.accounts[1].sharing_warning.as_ref().expect("warned");
        assert_eq!(warning.code, error_code::ACCOUNT_SHARING_FAILED);
        assert!(claude.accounts[2].sharing_warning.is_none());
        let stored = state.store.list_accounts().unwrap();
        assert_eq!(stored.len(), 3, "the account stays created");
        assert!(
            stored
                .iter()
                .find(|a| a.name == "Fine")
                .unwrap()
                .share_default_config
        );
    }

    #[tokio::test]
    async fn the_default_account_name_is_applied_by_the_rule_and_its_disabled_flag_is_never_applied(
    ) {
        let area = ScratchDir::new("import-apply-default-name-area");
        let (state, _state_dir) = account_state("import-apply-default-name", &area);
        state
            .store
            .set_default_account_name(Agent::Grok, Some("Mine"))
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

        let imported = apply(&state, &document, Vec::new(), false, Some(Vec::new()), true)
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

        assert!(
            coordinator::name_unnamed_default_account(&state, Agent::Claude, " Personal ").unwrap()
        );
        // A rename made since the check an inspection reports leaves the machine's name alone.
        assert!(
            !coordinator::name_unnamed_default_account(&state, Agent::Claude, "Other").unwrap()
        );
        let names = state.store.default_account_names().unwrap();
        assert_eq!(names.get("claude").map(String::as_str), Some("Personal"));

        // A blank name or the word for an unnamed one clears nothing here.
        for name in ["  ", "DEFAULT"] {
            assert!(!coordinator::name_unnamed_default_account(&state, Agent::Grok, name).unwrap());
        }

        // A name an account has meanwhile taken is refused, the one refusal an import expects.
        state
            .store
            .insert_account(&account("codex-work", Agent::Codex, "Work"))
            .unwrap();
        let err = coordinator::name_unnamed_default_account(&state, Agent::Codex, "work")
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

        let imported = apply(&state, &with, Vec::new(), false, None, true)
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
            false,
            Some(Vec::new()),
            true,
        )
        .await
        .expect_err("no accounts in the document");
        assert_eq!(code_of(&err), error_code::IMPORT_ACCOUNTS_MISSING);
        assert_eq!(state.store.list_consoles().unwrap().len(), before);
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
            false,
            Some(Vec::new()),
            false,
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
            false,
            None,
            true,
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
            vec![resolve(Agent::Claude, 1, None, true)],
            vec![resolve(Agent::Codex, 0, None, true)],
            vec![
                resolve(Agent::Claude, 0, None, true),
                resolve(Agent::Claude, 0, Some("/elsewhere"), false),
            ],
            vec![resolve(Agent::Claude, 0, Some("relative/dir"), false)],
            vec![resolve(Agent::Claude, 0, Some("/elsewhere"), true)],
            vec![resolve(Agent::Claude, 0, None, false)],
        ];
        for resolutions in cases {
            let err = apply(
                &state,
                &document,
                Vec::new(),
                false,
                Some(resolutions),
                true,
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
        let mut team = account("claude-2", Agent::Claude, "Team");
        team.config_dir = path_text(&area.join("team"));
        team.disabled = true;
        source.store.insert_account(&team).unwrap();
        source
            .store
            .reorder_accounts(
                Agent::Claude,
                &[
                    Some("claude-2".to_string()),
                    None,
                    Some("claude-1".to_string()),
                ],
            )
            .unwrap();
        source
            .store
            .set_default_account_name(Agent::Claude, Some("Personal"))
            .unwrap();
        source
            .store
            .set_added_agents_if_undecided(&[Agent::Codex, Agent::Claude])
            .unwrap();
        target
            .store
            .set_added_agents_if_undecided(&[Agent::Claude])
            .unwrap();
        let exported = serde_json::to_value(
            config_export::build(
                &source.store,
                &config_export::Selection {
                    consoles: config_export::ConsoleScope::None,
                    settings: false,
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
            false,
            Some(Vec::new()),
            true,
        )
        .await
        .unwrap();

        let stored = target.store.list_accounts().unwrap();
        let ids: Vec<_> = target
            .store
            .account_order(Agent::Claude)
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
            Some(vec![Agent::Claude, Agent::Codex])
        );
        assert_eq!(
            target.store.default_account_name(Agent::Claude).unwrap(),
            "Personal"
        );
    }
}
