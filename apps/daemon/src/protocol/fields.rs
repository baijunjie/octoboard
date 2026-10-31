//! The serde helpers the wire types name in their field attributes: reading a request field that
//! tells a left-out key from a `null`, and the defaults and skips of the rest.

use serde::{Deserialize, Deserializer};

pub(super) fn is_false(value: &bool) -> bool {
    !value
}

pub(super) fn is_true(value: &bool) -> bool {
    *value
}

pub(super) fn is_zero(count: &usize) -> bool {
    *count == 0
}

pub(super) fn default_true() -> bool {
    true
}

/// A request field that is absent (`None`), cleared (`Some(None)`) or given.
pub type Present<T> = Option<Option<T>>;

/// Deserializes a present field — `null` included — as `Some`, leaving `None` to mean "the field
/// was not sent at all". Used for the fields where clearing a value and leaving it alone are
/// different requests.
pub(crate) fn present_option<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}
