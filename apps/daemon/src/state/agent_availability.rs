//! Each agent's availability, as derived once per daemon start.

use super::AppState;
use crate::protocol::{Agent, AgentAvailability, Event};

impl AppState {
    /// Every agent's current availability, for `Event::Snapshot` — always three entries, each
    /// `NotDetermined` until `crate::agents::availability`'s one-time determination for this run
    /// lands.
    pub fn agent_availability(&self) -> Vec<AgentAvailability> {
        let map = self
            .agent_availability
            .read()
            .expect("agent availability lock poisoned");
        Agent::ALL
            .into_iter()
            .map(|agent| map[&agent].clone())
            .collect()
    }

    /// One agent's current availability, for the launch refusal in
    /// `coordinator::open::open_session`.
    pub fn agent_availability_of(&self, agent: Agent) -> AgentAvailability {
        self.agent_availability
            .read()
            .expect("agent availability lock poisoned")
            .get(&agent)
            .cloned()
            .expect("every agent has an entry from AppState::new")
    }

    /// Merges the given entries into every agent's availability and broadcasts the whole, ordered,
    /// three-entry result — never the argument itself, which may name only some agents. Called once
    /// per daemon start, by `crate::agents::availability::spawn_determine` once its one-time
    /// snapshot lands — never again afterwards, since nothing here re-determines it during a run.
    pub fn set_agent_availability(&self, agent_availability: Vec<AgentAvailability>) {
        {
            let mut map = self
                .agent_availability
                .write()
                .expect("agent availability lock poisoned");
            for entry in agent_availability {
                map.insert(entry.agent, entry);
            }
        }
        self.broadcast(Event::AgentAvailabilityUpdated {
            agent_availability: self.agent_availability(),
        });
    }
}
