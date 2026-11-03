//! The agents the user has added to the application and the order of the added agents and of each
//! agent's accounts; both are stored in the settings row.

use super::Result;
use rusqlite::{params, Connection};

use super::rows::enum_to_text;
use super::settings::{
    read_json_setting, read_json_setting_opt, registered_keys, write_json_setting,
};
use super::Store;
use crate::protocol::{error_code, AgentId, CodedError};

/// What `Store::reorder_added_agents` did.
#[derive(Debug, PartialEq, Eq)]
pub enum ReorderOutcome {
    /// The list has not been decided, so there is nothing to put in order.
    Undecided,
    /// The order is not a permutation of the list: an agent is missing, repeated or not added.
    NotAPermutation,
    /// The list already had this order.
    Unchanged,
    Reordered,
}

impl Store {
    /// The agents the user has added, in the order the user set (a newly added agent goes last);
    /// `None` while the list has never been decided (a fresh database, until
    /// `availability::fill_added_agents` fills it). A list saved empty on purpose is `Some` of
    /// nothing, which is what tells the two apart.
    pub fn added_agents(&self) -> Result<Option<Vec<AgentId>>> {
        read_added_agents(&self.lock())
    }

    /// Whether `agent` is on the added list; while the list is undecided every agent is.
    pub fn agent_is_added(&self, agent: AgentId) -> Result<bool> {
        Ok(self
            .added_agents()?
            .is_none_or(|added| added.contains(&agent)))
    }

    /// Stores the list only while it is still undecided (unset, or unreadable), so a list the user
    /// has since edited is never overwritten. Whether it was stored. The read and the write are
    /// one step under the store's lock.
    pub fn set_added_agents_if_undecided(&self, agents: &[AgentId]) -> Result<bool> {
        let conn = self.lock();
        if read_added_agents(&conn)?.is_some() {
            return Ok(false);
        }
        write_added_agents(&conn, agents)?;
        Ok(true)
    }

    /// Appends the agent to a decided list. Whether it was appended: `false` when it was already
    /// there. Refused with an error while the list is undecided, since appending would decide it
    /// as just this agent; the caller fills it first. The read and the write are one step under
    /// the store's lock.
    pub fn add_added_agent(&self, agent: AgentId) -> Result<bool> {
        let conn = self.lock();
        let mut agents = read_added_agents(&conn)?
            .ok_or_else(|| anyhow::anyhow!("the added agents are not decided yet"))?;
        if agents.contains(&agent) {
            return Ok(false);
        }
        agents.push(agent);
        write_added_agents(&conn, &agents)?;
        Ok(true)
    }

    /// Puts a decided list in the given order, and says what came of it. The read and the write are
    /// one step under the store's lock.
    pub fn reorder_added_agents(&self, order: &[AgentId]) -> Result<ReorderOutcome> {
        let conn = self.lock();
        let Some(current) = read_added_agents(&conn)? else {
            return Ok(ReorderOutcome::Undecided);
        };
        let mut sorted_current = current.clone();
        let mut sorted_order = order.to_vec();
        sorted_current.sort_by_key(|agent| agent.wire_name());
        sorted_order.sort_by_key(|agent| agent.wire_name());
        if sorted_current != sorted_order {
            return Ok(ReorderOutcome::NotAPermutation);
        }
        if current == order {
            return Ok(ReorderOutcome::Unchanged);
        }
        write_added_agents(&conn, order)?;
        Ok(ReorderOutcome::Reordered)
    }

    pub fn account_order(&self, agent: AgentId) -> Result<Vec<Option<String>>> {
        Ok(read_account_order(&self.lock())?
            .remove(agent.wire_name())
            .unwrap_or_default())
    }

    /// Validates and saves an agent's display order under one lock, so an account edit cannot
    /// race the validation. Other agents' orders and the accounts' storage order stay untouched.
    pub fn reorder_accounts(&self, agent: AgentId, order: &[Option<String>]) -> Result<bool> {
        let conn = self.lock();
        let mut current_accounts = conn
            .prepare("SELECT id FROM accounts WHERE agent = ?1")?
            .query_map(params![enum_to_text(&agent)], |row| row.get::<_, String>(0))?
            .map(|id| id.map(Some))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        current_accounts.push(None);
        current_accounts.sort();
        let mut sorted_order = order.to_vec();
        sorted_order.sort();
        if current_accounts != sorted_order {
            return Err(CodedError::raised(
                error_code::ACCOUNT_ORDER_INVALID,
                "the new order must name each account of the agent and its default exactly once"
                    .to_string(),
                &[],
            )
            .into());
        }
        let mut orders = read_stored_account_order(&conn)?;
        if orders
            .get(agent.wire_name())
            .is_some_and(|current| current == order)
        {
            return Ok(false);
        }
        orders.insert(agent.wire_name().to_string(), order.to_vec());
        write_json_setting(&conn, "account_order", Some(&orders))?;
        Ok(true)
    }

    /// Takes the agent off the list, leaving its accounts alone. Whether it was on it. Refused with
    /// an error while the list is undecided, as `add_added_agent` is.
    pub fn remove_added_agent(&self, agent: AgentId) -> Result<bool> {
        let conn = self.lock();
        let mut agents = read_added_agents(&conn)?
            .ok_or_else(|| anyhow::anyhow!("the added agents are not decided yet"))?;
        let before = agents.len();
        agents.retain(|added| *added != agent);
        if agents.len() == before {
            return Ok(false);
        }
        write_added_agents(&conn, &agents)?;
        Ok(true)
    }
}

/// The stored account orders, with those of providers this build does not register, which a
/// rewrite keeps.
fn read_stored_account_order(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, Vec<Option<String>>>> {
    read_json_setting(conn, "account_order")
}

/// The account orders of the registered providers.
pub(super) fn read_account_order(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, Vec<Option<String>>>> {
    Ok(registered_keys(read_stored_account_order(conn)?))
}

/// The registered providers on the stored list, which also names providers this build does not
/// register. A malformed value reads as an undecided list, so the next fill replaces it.
pub(super) fn read_added_agents(conn: &Connection) -> Result<Option<Vec<AgentId>>> {
    let stored: Option<Vec<String>> = read_json_setting_opt(conn, "added_agents")?;
    Ok(stored.map(|ids| {
        ids.iter()
            .filter_map(|id| AgentId::from_wire_name(id))
            .collect()
    }))
}

/// Stores the list, followed by the providers this build does not register that the stored list
/// names, so that a build which knows them finds them still added.
fn write_added_agents(conn: &Connection, agents: &[AgentId]) -> Result<()> {
    let stored: Vec<String> = read_json_setting_opt(conn, "added_agents")?.unwrap_or_default();
    let unregistered = stored
        .iter()
        .filter(|id| AgentId::from_wire_name(id).is_none())
        .map(String::as_str);
    let list: Vec<&str> = agents
        .iter()
        .map(|agent| agent.wire_name())
        .chain(unregistered)
        .collect();
    write_json_setting(conn, "added_agents", Some(&list))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::store::test_support::*;

    #[test]
    fn account_order_validates_current_membership_and_persists_per_agent() {
        let path = temp_db("account-order");
        let store = Store::open(&path).unwrap();
        assert!(store.get_settings().unwrap().account_order.is_empty());
        for (id, agent) in [
            ("one", AgentId::CLAUDE),
            ("two", AgentId::CLAUDE),
            ("other", AgentId::CODEX),
        ] {
            store
                .insert_account(&account(id, agent, &format!("/accounts/{id}")))
                .unwrap();
        }
        let stored_ids = || {
            store
                .list_accounts()
                .unwrap()
                .into_iter()
                .map(|account| account.id)
                .collect::<Vec<_>>()
        };
        let original_ids = stored_ids();
        let order = vec![Some("two".to_string()), None, Some("one".to_string())];
        assert!(store.reorder_accounts(AgentId::CLAUDE, &order).unwrap());
        assert!(!store.reorder_accounts(AgentId::CLAUDE, &order).unwrap());
        assert!(store
            .reorder_accounts(AgentId::CODEX, &[None, Some("other".to_string())])
            .unwrap());
        assert_eq!(stored_ids(), original_ids);
        let before = store.get_settings().unwrap().account_order;
        for invalid in [
            vec![None, Some("one".to_string())],
            vec![Some("one".to_string()), Some("two".to_string())],
            vec![None, None, Some("one".to_string())],
            vec![None, Some("one".to_string()), Some("one".to_string())],
            vec![None, Some("one".to_string()), Some("unknown".to_string())],
            vec![None, Some("one".to_string()), Some("other".to_string())],
            vec![],
        ] {
            let err: anyhow::Error = store
                .reorder_accounts(AgentId::CLAUDE, &invalid)
                .unwrap_err()
                .into();
            assert_eq!(
                crate::protocol::error_parts(&err).0,
                error_code::ACCOUNT_ORDER_INVALID
            );
            assert_eq!(store.get_settings().unwrap().account_order, before);
        }
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.get_settings().unwrap().account_order, before);
        store.delete_account("one").unwrap();
        assert!(store.reorder_accounts(AgentId::CLAUDE, &order).is_err());
        assert!(store
            .reorder_accounts(AgentId::CLAUDE, &[Some("two".to_string()), None])
            .unwrap());
        store
            .insert_account(&account("new", AgentId::CLAUDE, "/accounts/new"))
            .unwrap();
        assert!(store
            .reorder_accounts(AgentId::CLAUDE, &[Some("two".to_string()), None])
            .is_err());
        assert_eq!(
            store.get_settings().unwrap().account_order["codex"],
            before["codex"]
        );
    }

    /// The added agents are undecided on a fresh database, survive a restart once decided, and a
    /// list saved empty is still decided.
    #[test]
    fn added_agents_are_undecided_until_filled_and_survive_a_restart() {
        let path = temp_db("added-agents");
        let store = Store::open(&path).unwrap();
        assert_eq!(store.added_agents().unwrap(), None);
        assert_eq!(store.get_settings().unwrap().added_agents, None);

        assert!(store
            .set_added_agents_if_undecided(&[AgentId::CODEX, AgentId::CLAUDE])
            .unwrap());
        assert!(
            !store
                .set_added_agents_if_undecided(&[AgentId::GROK])
                .unwrap(),
            "decided"
        );
        assert!(store.add_added_agent(AgentId::GROK).unwrap());
        assert!(!store.add_added_agent(AgentId::GROK).unwrap());
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.get_settings().unwrap().added_agents,
            Some(vec![AgentId::CODEX, AgentId::CLAUDE, AgentId::GROK])
        );

        for agent in AgentId::ALL {
            store.remove_added_agent(agent).unwrap();
        }
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.added_agents().unwrap(), Some(vec![]));
        assert!(!store.set_added_agents_if_undecided(&AgentId::ALL).unwrap());
    }

    /// A stored value that does not read as a list counts as undecided, so the fill replaces it,
    /// and an append is refused rather than deciding the list as just that agent.
    #[test]
    fn a_malformed_added_agents_value_is_filled_and_not_appended_to() {
        let path = temp_db("added-agents-malformed");
        let store = Store::open(&path).unwrap();
        store
            .lock()
            .execute("UPDATE settings SET added_agents = 'not json'", [])
            .unwrap();
        assert_eq!(store.added_agents().unwrap(), None);
        assert!(store.add_added_agent(AgentId::CLAUDE).is_err());

        assert!(store
            .set_added_agents_if_undecided(&[AgentId::CODEX])
            .unwrap());
        assert_eq!(store.added_agents().unwrap(), Some(vec![AgentId::CODEX]));
    }
}
