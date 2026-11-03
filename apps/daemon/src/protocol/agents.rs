//! The agents, and the values the wire types keep once per agent: [`PerAgent`], a launch's model
//! and effort, and [`Change`], an update request's value for one agent's setting.

use serde::{Deserialize, Deserializer, Serialize};

use super::fields::{present_option, Present};

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Agent {
    Claude,
    Codex,
    Grok,
}

impl Agent {
    /// Every agent, in the order a per-agent listing keeps.
    pub const ALL: [Agent; 3] = [Agent::Claude, Agent::Codex, Agent::Grok];

    /// How the agent is named on the wire and in the console session's tools.
    pub fn wire_name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Grok => "grok",
        }
    }

    /// The agent [`Self::wire_name`] names.
    pub fn from_wire_name(name: &str) -> Option<Agent> {
        Agent::ALL
            .into_iter()
            .find(|agent| agent.wire_name() == name)
    }

    /// Whether the agent has the Claude compatibility setting: every agent but Claude Code, which
    /// is what the others are made compatible with.
    pub fn has_claude_compatibility(self) -> bool {
        self != Agent::Claude
    }

    /// The Claude compatibility setting while the user has made no choice for the agent: Grok Build
    /// follows its own default, which is on, and Codex is on while Claude Code is among the
    /// `added` agents (`None` while undecided, which counts as Claude Code not being among them).
    /// Claude Code itself has no such setting and reads as off.
    pub fn claude_compatibility_default(self, added: Option<&[Agent]>) -> bool {
        match self {
            Agent::Claude => false,
            Agent::Codex => added.is_some_and(|added| added.contains(&Agent::Claude)),
            Agent::Grok => true,
        }
    }

    /// How the agent is named in a message shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Grok => "Grok Build",
        }
    }
}

/// One value for each [`Agent`], serialised as `{"claude": …, "codex": …, "grok": …}`. A struct
/// rather than a map, so that adding an agent fails to compile everywhere a value is built or
/// looked up instead of defaulting silently. A key left out when reading is the default value.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerAgent<T> {
    pub claude: T,
    pub codex: T,
    pub grok: T,
}

impl<T> PerAgent<T> {
    pub fn from_fn(mut f: impl FnMut(Agent) -> T) -> Self {
        Self {
            claude: f(Agent::Claude),
            codex: f(Agent::Codex),
            grok: f(Agent::Grok),
        }
    }

    /// As [`Self::from_fn`], for a value that can fail to build, such as a store row.
    pub fn try_from_fn<E>(mut f: impl FnMut(Agent) -> Result<T, E>) -> Result<Self, E> {
        Ok(Self {
            claude: f(Agent::Claude)?,
            codex: f(Agent::Codex)?,
            grok: f(Agent::Grok)?,
        })
    }

    pub fn get(&self, agent: Agent) -> &T {
        match agent {
            Agent::Claude => &self.claude,
            Agent::Codex => &self.codex,
            Agent::Grok => &self.grok,
        }
    }

    pub fn get_mut(&mut self, agent: Agent) -> &mut T {
        match agent {
            Agent::Claude => &mut self.claude,
            Agent::Codex => &mut self.codex,
            Agent::Grok => &mut self.grok,
        }
    }

    /// In [`Agent::ALL`] order.
    pub fn iter(&self) -> impl Iterator<Item = (Agent, &T)> {
        [
            (Agent::Claude, &self.claude),
            (Agent::Codex, &self.codex),
            (Agent::Grok, &self.grok),
        ]
        .into_iter()
    }

    /// In [`Agent::ALL`] order.
    pub fn into_iter(self) -> impl Iterator<Item = (Agent, T)> {
        [
            (Agent::Claude, self.claude),
            (Agent::Codex, self.codex),
            (Agent::Grok, self.grok),
        ]
        .into_iter()
    }

    pub fn map<U>(self, mut f: impl FnMut(Agent, T) -> U) -> PerAgent<U> {
        PerAgent {
            claude: f(Agent::Claude, self.claude),
            codex: f(Agent::Codex, self.codex),
            grok: f(Agent::Grok, self.grok),
        }
    }
}

/// The model and reasoning effort an agent's sessions are launched with, in the agent's own
/// spelling. `null` leaves the choice to the agent.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launch {
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// One agent's value in an update request: the key absent, an explicit `null`, or a value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Change<T> {
    /// The key was not sent: leave the setting alone.
    #[default]
    Keep,
    /// An explicit `null`: back to the default.
    Clear,
    Set(T),
}

impl<T> Change<T> {
    /// The change as the in-memory changes carry it: `None` to leave the setting alone,
    /// `Some(None)` to clear it.
    pub fn into_present(self) -> Present<T> {
        match self {
            Change::Keep => None,
            Change::Clear => Some(None),
            Change::Set(value) => Some(Some(value)),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Change<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Option::<T>::deserialize(deserializer)? {
            None => Change::Clear,
            Some(value) => Change::Set(value),
        })
    }
}

/// A change to one agent's model and effort: each absent for "leave it alone", `null` or a
/// blank string for the agent's own default.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Default, Deserialize)]
pub struct LaunchChange {
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub model: Present<String>,
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub effort: Present<String>,
}

impl LaunchChange {
    /// Whether the change touches neither field.
    pub fn is_keep(&self) -> bool {
        self.model.is_none() && self.effort.is_none()
    }
}
