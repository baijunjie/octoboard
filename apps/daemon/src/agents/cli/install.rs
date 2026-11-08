//! How an agent's CLI was installed, told by where its binary lies once its links are followed,
//! and the command that updates an install of that method. The facts of each provider's packaging
//! (`CliPackaging`) say which directories mark its own installer and which package or Homebrew
//! name it has; nothing here names a provider.
//!
//! An install is recognised only by a path that says so. A package manager that is not npm
//! (pnpm, Yarn, Bun) installs under `node_modules` too, and `npm install -g` would put a second
//! copy elsewhere, so a path through one of their directories is not an npm install, and neither
//! is one the daemon cannot tell more of: both are `unknown`, which is never updated.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::adapter::CliPackaging;
use crate::platform::shell::ResolvedBinary;
use crate::protocol::CliInstallMethod;

/// Directories that mark an install another package manager than npm made.
const OTHER_PACKAGE_MANAGERS: &[&str] = &[".pnpm", ".bun", ".yarn", "yarn"];

/// Where Volta keeps what it installs; it hooks `npm install -g` itself, so it is given no prefix.
const VOLTA: &str = ".volta";

/// An install of an agent's CLI as its binary's path shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Install {
    pub method: CliInstallMethod,
    /// For an npm install, the global prefix it lies under (`<prefix>/lib/node_modules/<package>`,
    /// or `<prefix>/node_modules/<package>` on Windows); absent where none can be told.
    npm_prefix: Option<PathBuf>,
    /// For a Homebrew install, where it lies and under which name.
    homebrew: Option<Homebrew>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Homebrew {
    /// The Homebrew prefix: the directory that holds `Cellar` or `Caskroom` and `bin/brew`.
    prefix: PathBuf,
    name: &'static str,
    cask: bool,
}

impl Install {
    fn of(method: CliInstallMethod) -> Self {
        Self {
            method,
            npm_prefix: None,
            homebrew: None,
        }
    }

    pub fn unknown() -> Self {
        Self::of(CliInstallMethod::Unknown)
    }

    /// Tells the install of the CLI that `binary` starts. A launcher that starts the CLI through
    /// an interpreter names the script as a leading argument, so those are looked at as well.
    pub fn of_binary(packaging: &CliPackaging, binary: &ResolvedBinary) -> Self {
        let candidates = binary
            .leading_args
            .iter()
            .map(PathBuf::from)
            .chain(std::iter::once(binary.program.clone()));
        for candidate in candidates {
            let resolved = crate::hostfs::canonical(&candidate).unwrap_or(candidate);
            let install = Self::of_path(packaging, &resolved);
            if install.method != CliInstallMethod::Unknown {
                return install;
            }
        }
        Self::unknown()
    }

    /// Tells the install a binary at `path`, with its links followed, belongs to.
    pub fn of_path(packaging: &CliPackaging, path: &Path) -> Self {
        let parts = names(path);
        if let Some(package) = packaging.npm_package {
            let mut run = vec!["node_modules"];
            run.extend(package.split('/'));
            if let Some(at) = find_run(&parts, &run) {
                if parts
                    .iter()
                    .any(|part| OTHER_PACKAGE_MANAGERS.contains(&part.as_str()))
                {
                    return Self::unknown();
                }
                return Self {
                    npm_prefix: npm_prefix(path, &parts, at),
                    ..Self::of(CliInstallMethod::Npm)
                };
            }
        }
        if packaging
            .native_layouts
            .iter()
            .any(|layout| find_run(&parts, layout).is_some())
        {
            return Self::of(CliInstallMethod::Native);
        }
        for (directory, name, cask) in [
            ("cellar", packaging.homebrew_formula, false),
            ("caskroom", packaging.homebrew_cask, true),
        ] {
            let Some(name) = name else { continue };
            if let Some(at) = find_run(&parts, &[directory, name]) {
                return Self {
                    homebrew: Some(Homebrew {
                        prefix: leading(path, at),
                        name,
                        cask,
                    }),
                    ..Self::of(CliInstallMethod::Homebrew)
                };
            }
        }
        Self::unknown()
    }

    /// The command that updates this install of the CLI `binary` starts, with the shell
    /// environment the tool it runs resolves on; `None` for an install that is not updated, or
    /// whose tool (`npm`, `brew`) is not to be found.
    pub fn update_command(
        &self,
        packaging: &CliPackaging,
        binary: &ResolvedBinary,
        shell_env: &HashMap<String, String>,
    ) -> Option<ToolCommand> {
        match self.method {
            CliInstallMethod::Native => (!packaging.native_update.is_empty()).then(|| {
                ToolCommand::new(
                    binary.clone(),
                    packaging.native_update.iter().map(OsString::from).collect(),
                    true,
                )
            }),
            CliInstallMethod::Npm => {
                let npm = crate::env_shell::resolve_binary("npm", shell_env).ok()?;
                let mut args: Vec<OsString> = vec!["install".into(), "-g".into()];
                if let Some(prefix) = &self.npm_prefix {
                    args.extend(["--prefix".into(), prefix.clone().into_os_string()]);
                }
                args.push(format!("{}@latest", packaging.npm_package?).into());
                Some(ToolCommand::new(npm, args, false))
            }
            CliInstallMethod::Homebrew => {
                let homebrew = self.homebrew.as_ref()?;
                let mut args: Vec<OsString> = vec!["upgrade".into()];
                if homebrew.cask {
                    args.push("--cask".into());
                }
                args.push(homebrew.name.into());
                Some(ToolCommand::new(self.brew(shell_env)?, args, false))
            }
            CliInstallMethod::Unknown => None,
        }
    }

    /// The command that reads Homebrew's own record of the latest version of the package this
    /// install is of, from the metadata it already holds (`brew info`, with its automatic update
    /// of that metadata off); `None` where this is no Homebrew install or `brew` is not found.
    pub fn homebrew_info_command(
        &self,
        shell_env: &HashMap<String, String>,
    ) -> Option<ToolCommand> {
        let homebrew = self.homebrew.as_ref()?;
        let mut args: Vec<OsString> = vec!["info".into(), "--json=v2".into()];
        if homebrew.cask {
            args.push("--cask".into());
        }
        args.push(homebrew.name.into());
        let mut command = ToolCommand::new(self.brew(shell_env)?, args, false);
        command
            .env
            .push(("HOMEBREW_NO_AUTO_UPDATE".into(), "1".into()));
        Some(command)
    }

    /// The `brew` of the prefix the install lies under, else the one on the shell's `PATH`.
    fn brew(&self, shell_env: &HashMap<String, String>) -> Option<ResolvedBinary> {
        let own = self
            .homebrew
            .as_ref()
            .map(|homebrew| homebrew.prefix.join("bin").join("brew"))
            .filter(|brew| brew.is_file());
        match own {
            Some(brew) => Some(ResolvedBinary::new(brew)),
            None => crate::env_shell::resolve_binary("brew", shell_env).ok(),
        }
    }
}

/// A program and its arguments, run as the user's shell would run it: the snapshot's environment
/// and no more, from the home directory.
#[derive(Debug, Clone)]
pub(crate) struct ToolCommand {
    program: ResolvedBinary,
    args: Vec<OsString>,
    /// Variables set over the snapshot's.
    env: Vec<(OsString, OsString)>,
    /// Whether the program is an agent's own CLI, which is stopped as one is, with all it started.
    agent_cli: bool,
}

impl ToolCommand {
    fn new(program: ResolvedBinary, args: Vec<OsString>, agent_cli: bool) -> Self {
        Self {
            program,
            args,
            env: Vec::new(),
            agent_cli,
        }
    }

    pub fn is_agent_cli(&self) -> bool {
        self.agent_cli
    }

    pub fn command(&self, shell_env: &HashMap<String, String>) -> Command {
        let mut command = self.program.command();
        command.args(&self.args).env_clear().envs(shell_env);
        command.envs(self.env.iter().map(|(name, value)| (name, value)));
        command.current_dir(crate::paths::home_dir());
        command
    }

    /// The command as the user would read it, for the log.
    pub fn display(&self) -> String {
        std::iter::once(&self.program.program.as_os_str().to_owned())
            .chain(self.program.leading_args.iter())
            .chain(self.args.iter())
            .map(|part| part.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The normal components of `path`, lower-cased: what an install's layout is matched on.
fn names(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}

/// Where in `parts` the consecutive components `run` first begin.
fn find_run(parts: &[String], run: &[&str]) -> Option<usize> {
    if run.is_empty() {
        return None;
    }
    parts
        .windows(run.len())
        .position(|window| window.iter().zip(run).all(|(part, name)| part == name))
}

/// The beginning of `path` up to, and not including, its `count`th normal component.
fn leading(path: &Path, count: usize) -> PathBuf {
    let mut normal = 0;
    let mut kept = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::Normal(_)) {
            if normal == count {
                break;
            }
            normal += 1;
        }
        kept.push(component);
    }
    kept
}

/// The global prefix of an npm install whose `node_modules` is the `at`th component of `path`:
/// the directory above `lib`, or above `node_modules` itself where there is no `lib`. None for a
/// Volta install, which npm is not told a prefix of.
fn npm_prefix(path: &Path, parts: &[String], at: usize) -> Option<PathBuf> {
    if parts.iter().any(|part| part == VOLTA) {
        return None;
    }
    let above = if at > 0 && parts[at - 1] == "lib" {
        at - 1
    } else {
        at
    };
    Some(leading(path, above))
}
