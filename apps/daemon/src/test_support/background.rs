//! The commands a stand-in agent starts in the background, as an agent CLI starts each shell
//! command, spelled for each host: perl in a terminal session of its own on Unix, a fixture
//! process (`fixture`) on Windows, where nothing is in a session and every process is told by its
//! job.

use std::path::Path;

#[cfg(windows)]
use super::fixture::Steps;
#[cfg(unix)]
use super::perl_write;

/// One command of a stand-in agent, started in the background.
pub(crate) struct Background {
    /// The command as a line of the agent's shell script.
    #[cfg(unix)]
    script: String,
    #[cfg(windows)]
    command: Command,
}

/// What a [`Background`] is on Windows: a process called `name` running `steps`, working in `cwd`.
#[cfg(windows)]
struct Command {
    name: String,
    steps: Steps,
    cwd: Option<std::path::PathBuf>,
}

impl Background {
    /// A command that writes its pid to `pid_file` and becomes `sleep`: the process of an agent's
    /// dev server, in a terminal session of its own on Unix.
    pub(crate) fn sleeper(pid_file: &Path) -> Self {
        #[cfg(unix)]
        return Self::perl(&format!(
            "{} exec \"/bin/sleep\", \"300\";",
            perl_write(pid_file, "$$")
        ));
        #[cfg(windows)]
        return Self::process("sleep", Steps::new().write_pid(pid_file).sleep(300));
    }

    /// Perl `body`, run in a terminal session of its own.
    #[cfg(unix)]
    pub(crate) fn perl(body: &str) -> Self {
        Self {
            script: format!("/usr/bin/perl -MPOSIX -e 'POSIX::setsid(); {body}'"),
        }
    }

    /// A process called `name` running `steps`.
    #[cfg(windows)]
    pub(crate) fn process(name: &str, steps: Steps) -> Self {
        Self {
            command: Command {
                name: name.to_string(),
                steps,
                cwd: None,
            },
        }
    }

    /// The command working in `dir`.
    pub(crate) fn in_dir(self, dir: &Path) -> Self {
        #[cfg(unix)]
        return Self {
            script: format!("(cd '{}' && exec {})", dir.display(), self.script),
        };
        #[cfg(windows)]
        return Self {
            command: Command {
                cwd: Some(dir.into()),
                ..self.command
            },
        };
    }

    /// The line of the stand-in's shell script that starts the command.
    #[cfg(unix)]
    pub(crate) fn line(&self) -> String {
        format!("{} &\n", self.script)
    }

    /// The step that starts the command.
    #[cfg(windows)]
    pub(crate) fn start(self, steps: Steps) -> Steps {
        let Command {
            name,
            steps: own,
            cwd,
        } = self.command;
        steps.spawn_in(cwd.as_deref(), &name, own)
    }
}
