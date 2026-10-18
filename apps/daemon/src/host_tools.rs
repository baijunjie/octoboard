//! The host operations that requests reach the machine through and that a test stands in for: the
//! user's own `git`, as the branch cleanup runs it, and moving a directory to the Trash, as a
//! project's removal and its cleanup do. A test puts stand-ins in their place, so no test runs the
//! user's login shell or fills their Trash.

use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;

use crate::branch_origin::cleanup::Git;
use crate::hostfs;

#[derive(Clone, Copy)]
pub struct Tools {
    pub git: fn() -> Result<Git>,
    pub trash: fn(&Path) -> Result<()>,
}

/// Held on `AppState`.
pub struct HostTools {
    tools: Mutex<Tools>,
}

impl Default for HostTools {
    fn default() -> Self {
        Self {
            tools: Mutex::new(Tools {
                git: Git::of_user_shell,
                trash: hostfs::move_to_trash,
            }),
        }
    }
}

impl HostTools {
    pub fn get(&self) -> Tools {
        *self
            .tools
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(test)]
    pub fn replace(&self, tools: Tools) {
        *self.tools.lock().unwrap() = tools;
    }
}
