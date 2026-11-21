//! The targets that are not macOS, Linux included, have no command-line tool to link: the commands
//! answer that.

use super::{Error, State};

const UNSUPPORTED: &str = "the command-line tool is offered by the macOS application only";

pub fn status() -> Result<State, Error> {
    Ok(State::NotInstalled)
}

pub fn install() -> Result<State, Error> {
    Err(Error::failed(UNSUPPORTED))
}

pub fn remove() -> Result<State, Error> {
    Err(Error::failed(UNSUPPORTED))
}
