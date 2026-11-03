//! The settings of each provider (`crate::providers::SettingDescriptor`): the user's explicit
//! choices, one row per provider and setting, and the value of every setting in effect.

use std::collections::BTreeMap;

use rusqlite::{params, Connection};

use super::agents::read_added_agents;
use super::Result;
use super::Store;
use crate::protocol::AgentId;
use crate::providers::{descriptors, ProviderSettings, SettingValue};

type Choices = BTreeMap<String, BTreeMap<String, SettingValue>>;

impl Store {
    /// The choices the user made, by provider id and then setting id, whatever the build declares:
    /// a row naming a provider or setting this build does not know is kept and listed. A value that
    /// is not the JSON of a setting value is not a choice.
    pub fn provider_setting_choices(&self) -> Result<Choices> {
        read_choices(&self.lock())
    }

    /// The value in effect of every setting of every registered provider, by provider id and then
    /// setting id: a switch is the user's choice or else its default rule's value, a choice setting
    /// the user's or else `None`.
    pub fn provider_settings(
        &self,
    ) -> Result<BTreeMap<String, BTreeMap<String, Option<SettingValue>>>> {
        read_provider_settings(&self.lock())
    }

    /// `agent`'s settings in effect, as its launch reads them.
    pub fn provider_settings_of(&self, agent: AgentId) -> Result<ProviderSettings> {
        let mut all = read_provider_settings(&self.lock())?;
        Ok(ProviderSettings::new(
            all.remove(agent.wire_name()).unwrap_or_default(),
        ))
    }

    /// Records the user's choice `value` for `agent`'s setting `setting`, or with `None` forgets
    /// it. The caller has checked the setting is declared and takes the value. Whether the stored
    /// choice changed.
    pub fn set_provider_setting(
        &self,
        agent: AgentId,
        setting: &str,
        value: Option<&SettingValue>,
    ) -> Result<bool> {
        let conn = self.lock();
        let changed = match value {
            Some(value) => conn.execute(
                "INSERT INTO provider_settings (provider, setting, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT (provider, setting) DO UPDATE SET value = ?3
                 WHERE value IS NOT ?3",
                params![agent.wire_name(), setting, serde_json::to_string(value)?],
            )?,
            None => conn.execute(
                "DELETE FROM provider_settings WHERE provider = ?1 AND setting = ?2",
                params![agent.wire_name(), setting],
            )?,
        };
        Ok(changed > 0)
    }
}

fn read_choices(conn: &Connection) -> Result<Choices> {
    let mut statement = conn.prepare("SELECT provider, setting, value FROM provider_settings")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut choices = Choices::new();
    for (provider, setting, text) in rows {
        match serde_json::from_str(&text) {
            Ok(value) => {
                choices.entry(provider).or_default().insert(setting, value);
            }
            Err(err) => {
                tracing::warn!(%err, provider, setting, "a provider setting is not a setting value; reading it as unset");
            }
        }
    }
    Ok(choices)
}

/// [`Store::provider_settings`], under a lock the caller holds.
pub(super) fn read_provider_settings(
    conn: &Connection,
) -> Result<BTreeMap<String, BTreeMap<String, Option<SettingValue>>>> {
    let choices = read_choices(conn)?;
    let added = read_added_agents(conn)?;
    Ok(descriptors()
        .map(|descriptor| {
            let chosen = choices.get(descriptor.id);
            let values = descriptor
                .settings
                .iter()
                .map(|setting| {
                    (
                        setting.id.to_string(),
                        setting.in_effect(
                            chosen.and_then(|chosen| chosen.get(setting.id)),
                            added.as_deref(),
                        ),
                    )
                })
                .collect();
            (descriptor.id.to_string(), values)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::*;

    fn switch(on: bool) -> SettingValue {
        SettingValue::Switch(on)
    }

    fn choice(text: &str) -> SettingValue {
        SettingValue::Choice(text.to_string())
    }

    fn compatibility(store: &Store, agent: AgentId) -> Option<SettingValue> {
        store.provider_settings().unwrap()[agent.wire_name()]["claude_compatibility"].clone()
    }

    /// Until the user chooses, Grok Build's Claude compatibility is on and Codex's follows whether
    /// Claude Code is an added agent; a choice is stored per provider and wins from then on;
    /// Claude Code never has the setting.
    #[test]
    fn the_claude_compatibility_follows_added_agents_until_chosen_and_round_trips_per_agent() {
        let path = temp_db("claude-compatibility");
        let store = Store::open(&path).expect("store");
        assert_eq!(compatibility(&store, AgentId::CODEX), Some(switch(false)));
        assert_eq!(compatibility(&store, AgentId::GROK), Some(switch(true)));
        assert!(!store.provider_settings().unwrap()["claude"].contains_key("claude_compatibility"));

        store
            .set_added_agents_if_undecided(&[AgentId::CODEX])
            .unwrap();
        assert_eq!(compatibility(&store, AgentId::CODEX), Some(switch(false)));
        store.add_added_agent(AgentId::CLAUDE).unwrap();
        assert_eq!(compatibility(&store, AgentId::CODEX), Some(switch(true)));
        assert!(store.provider_setting_choices().unwrap().is_empty());

        let set = |agent, value: Option<SettingValue>| {
            store
                .set_provider_setting(agent, "claude_compatibility", value.as_ref())
                .unwrap()
        };
        assert!(set(AgentId::CODEX, Some(switch(false))));
        assert!(!set(AgentId::CODEX, Some(switch(false))));
        assert_eq!(
            store.provider_setting_choices().unwrap(),
            [(
                "codex".to_string(),
                [("claude_compatibility".to_string(), switch(false))].into()
            )]
            .into()
        );
        assert_eq!(compatibility(&store, AgentId::CODEX), Some(switch(false)));
        assert_eq!(compatibility(&store, AgentId::GROK), Some(switch(true)));
        assert_eq!(
            store.get_settings().unwrap().provider_settings["codex"]["claude_compatibility"],
            Some(switch(false))
        );

        // A choice outlives the added agents, whichever way it went.
        assert!(set(AgentId::CODEX, Some(switch(true))));
        store.remove_added_agent(AgentId::CLAUDE).unwrap();
        assert_eq!(compatibility(&store, AgentId::CODEX), Some(switch(true)));
        assert!(set(AgentId::GROK, Some(switch(false))));
        assert_eq!(compatibility(&store, AgentId::GROK), Some(switch(false)));

        // Clearing goes back to the default rule, and clearing twice changes nothing.
        assert!(set(AgentId::GROK, None));
        assert!(!set(AgentId::GROK, None));
        assert_eq!(compatibility(&store, AgentId::GROK), Some(switch(true)));
    }

    /// A choice setting reads as unset until chosen, round-trips across a reopen, and clears; a
    /// stored value the setting does not take reads as unset, and a row of a setting this build
    /// does not declare stays stored and stays out of the values in effect.
    #[test]
    fn choice_settings_round_trip_and_stored_values_that_do_not_fit_are_ignored() {
        let path = temp_db("provider-choices");
        let store = Store::open(&path).unwrap();
        let reviewer = |store: &Store| {
            store.provider_settings().unwrap()["codex"]["approval_reviewer"].clone()
        };
        let mode =
            |store: &Store| store.provider_settings().unwrap()["claude"]["approval_mode"].clone();
        assert_eq!(reviewer(&store), None);
        assert_eq!(mode(&store), None);

        let auto_review = choice("auto_review");
        let bypass = choice("bypass");
        assert!(store
            .set_provider_setting(AgentId::CODEX, "approval_reviewer", Some(&auto_review))
            .unwrap());
        store
            .set_provider_setting(AgentId::CLAUDE, "approval_mode", Some(&bypass))
            .unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(reviewer(&store), Some(auto_review.clone()));
        assert_eq!(
            store
                .provider_settings_of(AgentId::CLAUDE)
                .unwrap()
                .choice("approval_mode"),
            Some("bypass")
        );
        assert_eq!(
            store
                .provider_settings_of(AgentId::CODEX)
                .unwrap()
                .choice("approval_mode"),
            None
        );

        store
            .lock()
            .execute_batch(
                "UPDATE provider_settings SET value = '\"nobody\"' WHERE provider = 'codex';
                 INSERT INTO provider_settings VALUES ('future', 'thing', 'true');
                 INSERT INTO provider_settings VALUES ('claude', 'broken', '{ not json');",
            )
            .unwrap();
        assert_eq!(reviewer(&store), None);
        assert!(!store.provider_settings().unwrap().contains_key("future"));
        let choices = store.provider_setting_choices().unwrap();
        assert_eq!(choices["future"]["thing"], switch(true));
        assert!(!choices["claude"].contains_key("broken"));

        assert!(store
            .set_provider_setting(AgentId::CODEX, "approval_reviewer", None)
            .unwrap());
        assert!(store
            .set_provider_setting(AgentId::CLAUDE, "approval_mode", None)
            .unwrap());
        assert_eq!(mode(&store), None);
        assert!(store
            .provider_setting_choices()
            .unwrap()
            .contains_key("future"));
    }

    /// The test-only provider's one choice setting reads as unset until chosen, then round-trips
    /// across a reopen and clears, without touching any other provider's settings.
    #[test]
    fn a_provider_with_only_a_choice_setting_round_trips_it_through_its_own_settings() {
        let path = temp_db("provider-test-agent");
        let store = Store::open(&path).unwrap();
        let mode = |store: &Store| {
            store
                .provider_settings_of(AgentId::TEST)
                .unwrap()
                .choice("mode")
                .map(str::to_string)
        };
        assert_eq!(
            store.provider_settings().unwrap()["test_agent"]["mode"],
            None
        );
        assert_eq!(mode(&store), None);
        assert!(store
            .set_provider_setting(AgentId::TEST, "mode", Some(&choice("reckless")))
            .unwrap());
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(mode(&store).as_deref(), Some("reckless"));
        assert_eq!(
            store.provider_settings().unwrap()["test_agent"]["mode"],
            Some(choice("reckless"))
        );
        assert_eq!(
            store
                .provider_settings_of(AgentId::CLAUDE)
                .unwrap()
                .choice("mode"),
            None
        );
        // A value the setting does not take reads as unset.
        store
            .lock()
            .execute_batch(
                "UPDATE provider_settings SET value = '\"nobody\"' WHERE provider = 'test_agent';",
            )
            .unwrap();
        assert_eq!(mode(&store), None);
        assert!(store
            .set_provider_setting(AgentId::TEST, "mode", None)
            .unwrap());
        assert!(store.provider_setting_choices().unwrap().is_empty());
    }
}
