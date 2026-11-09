//! The configuration document that carries the stored configuration between devices: building it
//! for `export_configuration` (`export.rs`) and reading one back (`import/`, after `upgrade.rs` has
//! brought an older version up to the current one).

pub(crate) mod export;
pub(crate) mod import;
pub(crate) mod upgrade;

use std::collections::BTreeMap;

use crate::protocol::{Launch, PerAgent};
use crate::providers::{settings_where, SettingValue};

/// The entries of `choices` (provider id, then setting id) that name a setting a configuration
/// document carries (`SettingDescriptor::exported`) with a value it takes. What an export writes
/// and an import reads: a document from a build that knows more providers or settings lists
/// entries this one skips, and a setting that stays on its machine is never carried.
pub(crate) fn exported_provider_settings<'a>(
    choices: impl IntoIterator<Item = (&'a String, &'a BTreeMap<String, SettingValue>)>,
) -> BTreeMap<String, BTreeMap<String, SettingValue>> {
    let choices = choices
        .into_iter()
        .map(|(provider, settings)| (provider.clone(), settings.clone()));
    settings_where(choices, |setting, value| {
        setting.exported && setting.accepts(value)
    })
}

/// `launches` with the compatibility switches cut down to the ones a configuration document
/// carries (`SettingDescriptor::exported`) that the agent lets a launch hold. What an export writes
/// for a project's and a console's launch settings and an import reads: a switch this build does
/// not know, or one that stays on its machine, is dropped.
pub(crate) fn exported_launches(launches: PerAgent<Launch>) -> PerAgent<Launch> {
    launches.map(|agent, mut launch| {
        launch.compatibility.retain(|setting, _| {
            agent
                .descriptor()
                .setting(setting)
                .is_some_and(|setting| setting.exported && setting.overridable())
        });
        launch
    })
}
