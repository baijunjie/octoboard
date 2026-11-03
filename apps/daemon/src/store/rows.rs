//! What the table files share: the enum text conversion and the readers of provider ids.

use anyhow::Result;
use rusqlite::Row;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::protocol::AgentId;

/// The registered provider a stored provider id names, or `None` for an id no provider of this
/// build registers: such a row is kept in the store and left out of what is loaded.
pub(super) fn agent_from_row(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<AgentId>> {
    let id: String = row.get(index)?;
    Ok(AgentId::from_wire_name(&id))
}

/// `provider IN (…)` over `column`: the rows of the registered providers.
pub(super) fn registered_provider_clause(column: &str) -> String {
    let ids = AgentId::ALL
        .iter()
        .map(|agent| format!("'{}'", agent.wire_name().replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{column} IN ({ids})")
}

/// The text form of a protocol enum, taken from its serde representation so the stored value and
/// the wire value can never drift apart.
pub(super) fn enum_to_text<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .expect("protocol enums serialize to a JSON string")
}

pub(super) fn enum_from_row<T: DeserializeOwned>(
    row: &Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    enum_from_text(&text).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, err.into())
    })
}

pub(super) fn enum_from_row_opt<T: DeserializeOwned>(
    row: &Row<'_>,
    index: usize,
) -> rusqlite::Result<Option<T>> {
    let text: Option<String> = row.get(index)?;
    match text {
        None => Ok(None),
        Some(text) => enum_from_text(&text).map(Some).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                err.into(),
            )
        }),
    }
}

pub(super) fn enum_from_text<T: DeserializeOwned>(text: &str) -> Result<T, serde_json::Error> {
    serde_json::from_value(serde_json::Value::String(text.to_string()))
}
