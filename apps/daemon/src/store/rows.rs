//! What the table files share: the enum text conversion and the helpers that read the columns every
//! table lays out alike.

use anyhow::Result;
use rusqlite::Row;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::protocol::{Agent, Launch};

/// Which of the three consecutive per-agent columns (or pairs of columns) of a `consoles` or
/// `projects` row an agent's are.
pub(super) fn column_slot(agent: Agent) -> usize {
    match agent {
        Agent::Claude => 0,
        Agent::Codex => 1,
        Agent::Grok => 2,
    }
}

/// The model and effort in the two columns from `model_column` on.
pub(super) fn launch_from_row(row: &Row<'_>, model_column: usize) -> rusqlite::Result<Launch> {
    Ok(Launch {
        model: row.get(model_column)?,
        effort: row.get(model_column + 1)?,
    })
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
