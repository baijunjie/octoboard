//! How a message names what it talks about: a session by its kind, id and title, where it runs,
//! who it reports to, and a body quoted line by line.
//!
//! What a model or the user wrote (a title, a name, a note) may span several lines, which would let
//! it pass for the lines of Octoboard's own around it, so it is put on one line or quoted before it
//! goes into a message.

use std::sync::Arc;

use crate::protocol::Session;
use crate::state::AppState;

/// The text on one line, for quoting inline into a message. What a model or the user wrote may span
/// several lines, which would let it pass for the lines of Octoboard's own around it.
pub(crate) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A session as a message names it, short: its id and title, as in `` `s1` ("Fix the build") ``.
pub(crate) fn label(session: &Session) -> String {
    format!("`{}` (\"{}\")", session.id, one_line(&session.title))
}

/// Each session as an id and its title, for a refusal that names sessions to the model.
pub(crate) fn session_labels<'a>(sessions: impl IntoIterator<Item = &'a Session>) -> String {
    sessions
        .into_iter()
        .map(label)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A session as a message names it: its kind, id and title.
pub(crate) fn named_session(session: &Session) -> String {
    format!("the {} session {}", session.kind.name(), label(session))
}

/// Who a session reports to, as the records say.
pub(crate) enum ReportsTo {
    /// Bound to nobody: it reports to the user.
    User,
    /// Bound to this session, its owner.
    Session(Box<Session>),
    /// Bound to a session whose record cannot be read.
    Unreadable,
}

/// Who `session` reports to.
pub(crate) fn reports_to(state: &Arc<AppState>, session: &Session) -> ReportsTo {
    match session
        .bound_to
        .as_deref()
        .map(|id| state.store.get_session(id))
    {
        Some(Ok(Some(owner))) => ReportsTo::Session(Box::new(owner)),
        Some(_) => ReportsTo::Unreadable,
        None => ReportsTo::User,
    }
}

/// Where a session runs, with the name already put on one line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Place {
    Project(String),
    Console(String),
}

impl std::fmt::Display for Place {
    /// ` in the project "…"`, or ` in the console "…"`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Place::Project(name) => write!(f, " in the project \"{name}\""),
            Place::Console(name) => write!(f, " in the console \"{name}\""),
        }
    }
}

/// Where `session` runs: its project, or its console for a console session, and `None` when the
/// record is gone.
pub(crate) fn place(state: &Arc<AppState>, session: &Session) -> Option<Place> {
    match &session.project_id {
        Some(id) => state
            .store
            .get_project(id)
            .ok()
            .flatten()
            .map(|project| Place::Project(one_line(&project.name))),
        None => state
            .store
            .get_console(&session.console_id)
            .ok()
            .flatten()
            .map(|console| Place::Console(one_line(&console.name))),
    }
}

/// Where `session` runs, for a message to name it (see [`Place`]), and nothing when the record is
/// gone.
pub(crate) fn place_of(state: &Arc<AppState>, session: &Session) -> String {
    place(state, session)
        .map(|place| place.to_string())
        .unwrap_or_default()
}

/// The body with every line quoted, blank ones included, so that no line of it can read as a
/// header of Octoboard's own. U+2028 and U+2029 break lines too: control-character stripping
/// leaves them in, and a model may read them as line breaks.
pub(crate) fn quote(text: &str) -> String {
    text.split(['\n', '\u{2028}', '\u{2029}'])
        .map(|line| format!("> {}", line.strip_suffix('\r').unwrap_or(line)))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multi_line_text_is_put_on_one_line_or_quoted_line_by_line() {
        assert_eq!(one_line("a\n## b\r\n  c"), "a ## b c");
        assert_eq!(quote("a\n\nb\u{2028}c"), "> a\n> \n> b\n> c");
    }

    #[test]
    fn a_place_is_worded_by_what_it_is() {
        assert_eq!(
            Place::Project("api".to_string()).to_string(),
            " in the project \"api\""
        );
        assert_eq!(
            Place::Console("Main".to_string()).to_string(),
            " in the console \"Main\""
        );
    }
}
