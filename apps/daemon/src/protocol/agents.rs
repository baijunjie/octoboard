//! The values the wire types keep once per provider:
//! [`PerAgent`], a launch's model and effort, and [`Change`], an update request's value for one
//! agent's setting. The agents themselves are the provider registry's (`crate::providers`).

use std::collections::BTreeMap;

use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::fields::{present_option, Present};

pub use crate::providers::AgentId;
use crate::providers::PROVIDER_COUNT;

/// One value for each registered provider: a map keyed by [`AgentId`] that always holds an entry
/// for every one of them, serialised as `{"<provider id>": …, …}` in the registry's order. Reading
/// one, a key left out is the default value and a key naming no registered provider is ignored.
#[cfg_attr(test, derive(ts_rs::TS), ts(bound = "T: ts_rs::TS"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerAgent<T>(#[cfg_attr(test, ts(type = "Record<Agent, T>"))] [T; PROVIDER_COUNT]);

impl<T: Default> Default for PerAgent<T> {
    fn default() -> Self {
        Self::from_fn(|_| T::default())
    }
}

impl<T> PerAgent<T> {
    pub fn from_fn(mut f: impl FnMut(AgentId) -> T) -> Self {
        Self(std::array::from_fn(|index| f(AgentId::ALL[index])))
    }

    /// As [`Self::from_fn`], for a value that can fail to build, such as a store row.
    pub fn try_from_fn<E>(mut f: impl FnMut(AgentId) -> Result<T, E>) -> Result<Self, E> {
        let mut values = Vec::with_capacity(PROVIDER_COUNT);
        for agent in AgentId::ALL {
            values.push(f(agent)?);
        }
        Ok(Self(
            values.try_into().ok().expect("one value per provider"),
        ))
    }

    pub fn get(&self, agent: AgentId) -> &T {
        &self.0[agent.index()]
    }

    pub fn get_mut(&mut self, agent: AgentId) -> &mut T {
        &mut self.0[agent.index()]
    }

    /// In [`AgentId::ALL`] order.
    pub fn iter(&self) -> impl Iterator<Item = (AgentId, &T)> {
        AgentId::ALL.into_iter().zip(self.0.iter())
    }

    /// In [`AgentId::ALL`] order.
    #[allow(clippy::should_implement_trait)]
    pub fn into_iter(self) -> impl Iterator<Item = (AgentId, T)> {
        AgentId::ALL.into_iter().zip(self.0)
    }

    /// The values with `agent`'s replaced by `value`.
    pub fn with(mut self, agent: AgentId, value: T) -> Self {
        *self.get_mut(agent) = value;
        self
    }

    pub fn map<U>(self, mut f: impl FnMut(AgentId, T) -> U) -> PerAgent<U> {
        let mut values = self.0.into_iter();
        PerAgent::from_fn(|agent| f(agent, values.next().expect("one value per provider")))
    }
}

impl<T> std::ops::Index<AgentId> for PerAgent<T> {
    type Output = T;

    fn index(&self, agent: AgentId) -> &T {
        self.get(agent)
    }
}

impl<T> std::ops::IndexMut<AgentId> for PerAgent<T> {
    fn index_mut(&mut self, agent: AgentId) -> &mut T {
        self.get_mut(agent)
    }
}

impl<T: Serialize> Serialize for PerAgent<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(PROVIDER_COUNT))?;
        for (agent, value) in self.iter() {
            map.serialize_entry(agent.wire_name(), value)?;
        }
        map.end()
    }
}

impl<'de, T: Deserialize<'de> + Default> Deserialize<'de> for PerAgent<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PerAgentVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T: Deserialize<'de> + Default> Visitor<'de> for PerAgentVisitor<T> {
            type Value = PerAgent<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map keyed by provider id")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values: PerAgent<Option<T>> = PerAgent::from_fn(|_| None);
                while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
                    match AgentId::from_wire_name(&key) {
                        Some(agent) => *values.get_mut(agent) = Some(map.next_value()?),
                        None => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(values.map(|_, value| value.unwrap_or_default()))
            }
        }

        deserializer.deserialize_map(PerAgentVisitor(std::marker::PhantomData))
    }
}

/// The model and reasoning effort an agent's sessions are launched with, in the agent's own
/// spelling, and the compatibility switches the level holds. `null` leaves the choice to the
/// agent.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launch {
    pub model: Option<String>,
    pub effort: Option<String>,
    /// The compatibility switches (provider settings declaring `compatibility_with`) this level
    /// holds its own value for, by setting id: only the values it holds, so a switch left out
    /// follows the level behind it, down to the agent's value in `Settings::provider_settings`.
    #[serde(default)]
    pub compatibility: BTreeMap<String, bool>,
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

/// A change to one agent's launch settings: absent fields leave their values alone. A model or
/// effort of `null` or a blank string uses the agent's own default; compatibility entries follow
/// the level behind on `null` and hold their own boolean otherwise.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Default, Deserialize)]
pub struct LaunchChange {
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub model: Present<String>,
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub effort: Present<String>,
    /// Per compatibility switch: absent leaves it alone, `null` goes back to following the level
    /// behind, a boolean holds the level's own value.
    #[serde(default)]
    #[cfg_attr(test, ts(optional, as = "Option<BTreeMap<String, Option<bool>>>"))]
    pub compatibility: BTreeMap<String, Option<bool>>,
}

impl LaunchChange {
    /// Whether the change touches none of the fields.
    pub fn is_keep(&self) -> bool {
        self.model.is_none() && self.effort.is_none() && self.compatibility.is_empty()
    }
}
