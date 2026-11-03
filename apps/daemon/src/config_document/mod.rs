//! The configuration document that carries the stored configuration between devices: building it
//! for `export_configuration` (`export.rs`) and reading one back (`import/`).

pub(crate) mod export;
pub(crate) mod import;

use std::collections::BTreeMap;

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
