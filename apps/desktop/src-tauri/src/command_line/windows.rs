//! The targets that are not macOS, Linux included, have no command-line tool to link: installing
//! answers that, and removing finds nothing to remove.

use super::{Error, State};

const UNSUPPORTED: &str = "the command-line tool is offered by the macOS application only";

pub fn status() -> Result<State, Error> {
    Ok(State::NotInstalled)
}

pub fn install() -> Result<State, Error> {
    Err(Error::failed(UNSUPPORTED))
}

pub fn remove() -> Result<State, Error> {
    Ok(State::NotInstalled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_where_there_is_no_tool_finds_nothing_to_remove_and_installing_is_refused() {
        assert_eq!(remove(), Ok(State::NotInstalled));
        assert_eq!(status(), Ok(State::NotInstalled));
        assert!(install().is_err());
    }
}
