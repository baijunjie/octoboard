//! The hosts with nothing to hold.

use anyhow::{bail, Result};

pub(super) struct Held;

impl Held {
    pub(super) fn acquire(_reason: &str) -> Result<Self> {
        bail!("this host has no way to be held awake")
    }
}

pub(super) fn available() -> bool {
    false
}
