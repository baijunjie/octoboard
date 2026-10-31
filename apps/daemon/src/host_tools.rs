//! The host operations that requests reach the machine through and that a test stands in for: the
//! user's own `git`, as the branch cleanup runs it, moving something to the Trash, as every
//! removal that offers it does, and the shell a shell terminal runs with the environment it is
//! started in. A test puts stand-ins in their place, so no test runs the user's login shell or
//! fills their Trash.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;

use crate::git::branch_git::BranchGit;
use crate::hostfs::trash::{move_to_trash, TrashFailure};

#[derive(Clone, Copy)]
pub struct Tools {
    pub git: fn() -> Result<BranchGit>,
    pub trash: fn(&Path) -> Result<(), TrashFailure>,
    /// The shell a shell terminal runs.
    pub shell: fn() -> crate::shell_terminal::ShellCommand,
    /// The environment a shell terminal is started in, as an agent launch takes it.
    pub shell_env: fn() -> Result<HashMap<String, String>>,
}

/// Held on `AppState`.
pub struct HostTools {
    tools: Mutex<Tools>,
}

impl Default for HostTools {
    fn default() -> Self {
        Self {
            tools: Mutex::new(Tools {
                git: BranchGit::of_user_shell,
                trash: move_to_trash,
                shell: crate::shell_terminal::login_shell,
                shell_env: crate::env_shell::launch_snapshot,
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
