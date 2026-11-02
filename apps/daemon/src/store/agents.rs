//! The agents the user has added to the application and the order of the added agents and of each
//! agent's accounts; both are stored in the settings row.

use anyhow::Result;
use rusqlite::{params, Connection};

use super::rows::enum_to_text;
use super::{Store, SETTINGS_ID};
use crate::protocol::{error_code, Agent, CodedError};

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
    pub fn added_agents(&self) -> Result<Option<Vec<Agent>>> {
        read_added_agents(&self.lock())
    }

    /// Whether `agent` is on the added list; while the list is undecided every agent is.
    pub fn agent_is_added(&self, agent: Agent) -> Result<bool> {
        Ok(self
            .added_agents()?
            .is_none_or(|added| added.contains(&agent)))
    }

    /// Stores the list only while it is still undecided (unset, or unreadable), so a list the user
    /// has since edited is never overwritten. Whether it was stored. The read and the write are
    /// one step under the store's lock.
    pub fn set_added_agents_if_undecided(&self, agents: &[Agent]) -> Result<bool> {
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
    pub fn add_added_agent(&self, agent: Agent) -> Result<bool> {
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
    pub fn reorder_added_agents(&self, order: &[Agent]) -> Result<ReorderOutcome> {
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

    pub fn account_order(&self, agent: Agent) -> Result<Vec<Option<String>>> {
        Ok(read_account_order(&self.lock())?
            .remove(agent.wire_name())
            .unwrap_or_default())
    }

    /// Validates and saves an agent's display order under one lock, so an account edit cannot
    /// race the validation. Other agents' orders and the accounts' storage order stay untouched.
    pub fn reorder_accounts(&self, agent: Agent, order: &[Option<String>]) -> Result<bool> {
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
            ));
        }
        let mut orders = read_account_order(&conn)?;
        if orders
            .get(agent.wire_name())
            .is_some_and(|current| current == order)
        {
            return Ok(false);
        }
        orders.insert(agent.wire_name().to_string(), order.to_vec());
        conn.execute(
            "UPDATE settings SET account_order = ?2 WHERE id = ?1",
            params![SETTINGS_ID, serde_json::to_string(&orders)?],
        )?;
        Ok(true)
    }

    /// Takes the agent off the list, leaving its accounts alone. Whether it was on it. Refused with
    /// an error while the list is undecided, as `add_added_agent` is.
    pub fn remove_added_agent(&self, agent: Agent) -> Result<bool> {
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

fn added_agents_to_text(agents: &[Agent]) -> String {
    serde_json::to_string(agents).expect("a list of agents serializes to JSON")
}

pub(super) fn read_account_order(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, Vec<Option<String>>>> {
    let text: Option<String> = conn.query_row(
        "SELECT account_order FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| row.get(0),
    )?;
    Ok(text
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

/// A malformed value reads as an undecided list, with a warning, so the next fill replaces it. The
/// daemon is the only writer, so this can only mean a hand-edited database, or one written by a later version.
pub(super) fn read_added_agents(conn: &Connection) -> Result<Option<Vec<Agent>>> {
    let text: Option<String> = conn.query_row(
        "SELECT added_agents FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| row.get(0),
    )?;
    Ok(text.and_then(|text| match serde_json::from_str(&text) {
        Ok(agents) => Some(agents),
        Err(err) => {
            tracing::warn!(%err, "the added agents are not a JSON array of agents; reading the list as undecided");
            None
        }
    }))
}

fn write_added_agents(conn: &Connection, agents: &[Agent]) -> Result<()> {
    conn.execute(
        "UPDATE settings SET added_agents = ?2 WHERE id = ?1",
        params![SETTINGS_ID, added_agents_to_text(agents)],
    )?;
    Ok(())
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
            ("one", Agent::Claude),
            ("two", Agent::Claude),
            ("other", Agent::Codex),
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
        assert!(store.reorder_accounts(Agent::Claude, &order).unwrap());
        assert!(!store.reorder_accounts(Agent::Claude, &order).unwrap());
        assert!(store
            .reorder_accounts(Agent::Codex, &[None, Some("other".to_string())])
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
            let err = store.reorder_accounts(Agent::Claude, &invalid).unwrap_err();
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
        assert!(store.reorder_accounts(Agent::Claude, &order).is_err());
        assert!(store
            .reorder_accounts(Agent::Claude, &[Some("two".to_string()), None])
            .unwrap());
        store
            .insert_account(&account("new", Agent::Claude, "/accounts/new"))
            .unwrap();
        assert!(store
            .reorder_accounts(Agent::Claude, &[Some("two".to_string()), None])
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
            .set_added_agents_if_undecided(&[Agent::Codex, Agent::Claude])
            .unwrap());
        assert!(
            !store.set_added_agents_if_undecided(&[Agent::Grok]).unwrap(),
            "decided"
        );
        assert!(store.add_added_agent(Agent::Grok).unwrap());
        assert!(!store.add_added_agent(Agent::Grok).unwrap());
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.get_settings().unwrap().added_agents,
            Some(vec![Agent::Codex, Agent::Claude, Agent::Grok])
        );

        for agent in Agent::ALL {
            store.remove_added_agent(agent).unwrap();
        }
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.added_agents().unwrap(), Some(vec![]));
        assert!(!store.set_added_agents_if_undecided(&Agent::ALL).unwrap());
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
        assert!(store.add_added_agent(Agent::Claude).is_err());

        assert!(store
            .set_added_agents_if_undecided(&[Agent::Codex])
            .unwrap());
        assert_eq!(store.added_agents().unwrap(), Some(vec![Agent::Codex]));
    }
}
