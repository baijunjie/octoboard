//! Where an agent keeps its files on disk ([`AgentLayout`]): the variable naming its configuration
//! directory and the directory it uses without one, where inside that directory it keeps a
//! session's records, and the files it keeps beside them. Each provider declares its layout in its
//! own module as part of its descriptor ([`super::ProviderDescriptor::layout`]), beside the names
//! only its own logic uses (Codex's rollout naming, Grok Build's session marker), which are
//! constants there named for the agent; [`layout`] gives an agent's.
//!
//! Code that works for any agent reads the layout's fields; code inside one provider's modules
//! reads that provider's named constant where there is one, and its layout's fields
//! (`codex::LAYOUT.search_depth`) for a value that has none. A project's own `.claude/` or
//! `.codex/` is not the agent's directory, whatever it shares with its name, so a file name taken
//! from a project's directory stays where it is read. How a record is recognised and read is the
//! provider's record store's (`crate::records::RecordStore`): a layout holds names and depths, not
//! rules. The measurements behind them are in `docs/agent-cli-reference.md`, mostly in "Moving a
//! conversation to another config directory" and "Sharing a configuration directory between
//! accounts".

use std::path::{Path, PathBuf};

use super::AgentId;

/// One agent's on-disk layout. Paths are relative to the agent's configuration directory unless
/// a field says otherwise.
#[derive(Debug, PartialEq, Eq)]
pub struct AgentLayout {
    /// The environment variable the agent reads its configuration directory from: the one an
    /// adapter exports for a pinned account and a shell snapshot may already carry for the
    /// default one.
    pub config_dir_var: &'static str,
    /// What [`config_dir_var`](Self::config_dir_var) names. Generic code does not read the two
    /// together: it goes through [`AgentLayout::config_dir_export`] and
    /// [`AgentLayout::config_dir_of_export`].
    pub config_dir_var_names: ConfigDirVarNames,
    /// The configuration directory's name under the home directory when the variable is unset.
    pub default_dir: &'static str,
    /// The directory a session's records are kept under, and the one a resume reads. Empty for an
    /// agent that keeps no record files under its configuration directory: every walk, removal and
    /// relocation of records then has nothing to do. Read it through [`AgentLayout::records`],
    /// never joined as it is, since an empty one would name the configuration directory itself.
    pub records_dir: &'static str,
    /// Where the agent moves the records it archives, which a resume does not read.
    pub archived_records_dir: Option<&'static str>,
    /// How many levels below [`records_dir`](Self::records_dir) a session's record, a file or a
    /// directory, sits, counting its own name: the one depth a record to be removed is taken at.
    /// Zero together with an empty records directory.
    /// An agent whose index names a file inside the record (Grok Build's session marker) matches
    /// it one level deeper.
    pub record_level: usize,
    /// How many levels below the records directory a search for records goes (1 being the
    /// directory's own entries). Exact for an agent that files a record by its working directory,
    /// generous for one that files it under dates it does not look it up by. Zero together with an
    /// empty records directory.
    pub search_depth: usize,
    /// A file directly under the configuration directory that names every conversation of the
    /// account together.
    pub title_index: Option<&'static str>,
    /// A file in a session's own record directory that names the conversation alone, for an agent
    /// that gives it one name whatever the conversation; one named for the record is the record
    /// store's to say (`RecordStore::summary_file`).
    pub title_summary: Option<&'static str>,
}

/// What an agent's configuration-directory variable names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigDirVarNames {
    /// The configuration directory itself.
    Directory,
    /// The configuration directory's parent: the configuration directory is the entry inside it
    /// named like the last component of [`AgentLayout::default_dir`].
    Parent,
}

impl AgentLayout {
    /// The name of the entry a variable that names the parent holds the configuration directory
    /// under: the last component of [`Self::default_dir`].
    fn entry_name(&self) -> Option<&'static std::ffi::OsStr> {
        Path::new(self.default_dir).file_name()
    }

    /// The variable and value that make the agent's CLI use the configuration directory `dir`,
    /// `None` when no value does. For a variable that names the parent, only a directory named
    /// like the entry the CLI looks for inside it can be given (`<parent>/<name>` is the only
    /// directory the CLI then uses), so a directory of another name, one with no parent (the
    /// filesystem root) and a relative one are refused here, and a caller turns `None` into a
    /// refusal, never into an export of something else.
    pub fn config_dir_export(&self, dir: &Path) -> Option<(&'static str, PathBuf)> {
        let value = match self.config_dir_var_names {
            ConfigDirVarNames::Directory => dir,
            ConfigDirVarNames::Parent => {
                if dir.file_name() != self.entry_name() {
                    return None;
                }
                dir.parent()
                    .filter(|parent| !parent.as_os_str().is_empty())?
            }
        };
        Some((self.config_dir_var, value.to_path_buf()))
    }

    /// The name a directory has to have for [`Self::config_dir_export`] to take it, when it has to
    /// have one: the entry of a variable that names the parent.
    pub fn required_dir_name(&self) -> Option<&'static std::ffi::OsStr> {
        match self.config_dir_var_names {
            ConfigDirVarNames::Directory => None,
            ConfigDirVarNames::Parent => self.entry_name(),
        }
    }

    /// The configuration directory the CLI uses when the variable is exported as `value`.
    pub fn config_dir_of_export(&self, value: &Path) -> PathBuf {
        match self.config_dir_var_names {
            ConfigDirVarNames::Directory => value.to_path_buf(),
            ConfigDirVarNames::Parent => value.join(
                self.entry_name()
                    .expect("a layout naming the parent has a named default directory"),
            ),
        }
    }

    /// The directory the agent keeps its session records under, `None` for an agent that keeps no
    /// record files under its configuration directory (an empty [`Self::records_dir`], which
    /// would otherwise name the configuration directory itself).
    pub fn records(&self) -> Option<&'static str> {
        (!self.records_dir.is_empty()).then_some(self.records_dir)
    }
}

/// The layout of `agent`.
pub(crate) fn layout(agent: AgentId) -> &'static AgentLayout {
    agent.descriptor().layout
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layout that names its directories, and searches its records directory at least as deep
    /// as a record sits; one with no records directory has no level and no depth either.
    fn assert_consistent(layout: &AgentLayout, which: &str) {
        assert!(!layout.config_dir_var.is_empty(), "{which}");
        assert!(!layout.default_dir.is_empty(), "{which}");
        if layout.config_dir_var_names == ConfigDirVarNames::Parent {
            assert!(
                Path::new(layout.default_dir).file_name().is_some(),
                "{which}: a variable naming the parent needs a named default directory"
            );
        }
        if layout.records_dir.is_empty() {
            assert_eq!(layout.record_level, 0, "{which}");
            assert_eq!(layout.search_depth, 0, "{which}");
            assert_eq!(layout.archived_records_dir, None, "{which}");
        } else {
            assert!(layout.record_level >= 1, "{which}");
            assert!(layout.search_depth >= layout.record_level, "{which}");
        }
    }

    /// Every registered provider, and every variant of the test-only one, is consistent.
    #[test]
    fn every_layout_names_its_directories_and_reaches_its_records() {
        for agent in AgentId::ALL {
            assert_consistent(layout(agent), &format!("{agent:?}"));
        }
        for variant in super::super::test_provider::VARIANT_LAYOUTS {
            assert_consistent(variant, "a test provider variant");
        }
    }

    const NAMING_DIRECTORY: AgentLayout = AgentLayout {
        config_dir_var: "AGENT_HOME",
        config_dir_var_names: ConfigDirVarNames::Directory,
        default_dir: ".agent",
        records_dir: "sessions",
        archived_records_dir: None,
        record_level: 1,
        search_depth: 1,
        title_index: None,
        title_summary: None,
    };

    const NAMING_PARENT: AgentLayout = AgentLayout {
        config_dir_var_names: ConfigDirVarNames::Parent,
        default_dir: ".local/share/agent",
        ..NAMING_DIRECTORY
    };

    /// A variable that names the configuration directory is exported as the directory and read
    /// back as it is.
    #[test]
    fn a_variable_naming_the_directory_is_exported_and_read_as_the_directory() {
        let dir = Path::new("/accounts/work");
        assert_eq!(
            NAMING_DIRECTORY.config_dir_export(dir),
            Some(("AGENT_HOME", dir.to_path_buf()))
        );
        assert_eq!(NAMING_DIRECTORY.config_dir_of_export(dir), dir);
    }

    /// A variable that names the parent is exported as the directory's parent and read back as
    /// the entry of the default directory's own name inside the value.
    #[test]
    fn a_variable_naming_the_parent_is_exported_and_read_as_its_parent() {
        let dir = Path::new("/accounts/work/agent");
        let (var, value) = NAMING_PARENT.config_dir_export(dir).unwrap();
        assert_eq!(var, "AGENT_HOME");
        assert_eq!(value, Path::new("/accounts/work"));
        assert_eq!(NAMING_PARENT.config_dir_of_export(&value), dir);
        assert_eq!(
            NAMING_PARENT.config_dir_of_export(Path::new("/data")),
            Path::new("/data/agent")
        );
    }

    /// A trailing separator on either side names the same directory: a configuration directory
    /// given with one is exported as the same parent, and a value read back with one stands for the
    /// same entry inside it.
    #[test]
    fn a_trailing_separator_changes_neither_conversion() {
        let (_, value) = NAMING_PARENT
            .config_dir_export(Path::new("/accounts/work/agent/"))
            .unwrap();
        assert_eq!(value, Path::new("/accounts/work"));
        assert_eq!(
            NAMING_PARENT.config_dir_of_export(Path::new("/accounts/work/")),
            Path::new("/accounts/work/agent")
        );
        let (_, value) = NAMING_DIRECTORY
            .config_dir_export(Path::new("/accounts/work/"))
            .unwrap();
        assert_eq!(value, Path::new("/accounts/work"));
        assert_eq!(
            NAMING_DIRECTORY.config_dir_of_export(Path::new("/accounts/work/")),
            Path::new("/accounts/work")
        );
    }

    /// A variable that names the parent takes only a directory of the entry's own name: any other
    /// directory, the filesystem root (which has no parent) and a relative one (whose parent is
    /// empty) are given no export, where the parent would name another directory.
    #[test]
    fn a_directory_the_parent_form_cannot_name_is_not_exported() {
        for dir in [
            "/accounts/work/other",
            "/accounts/work/agent2",
            "/",
            "agent",
            "",
        ] {
            assert_eq!(
                NAMING_PARENT.config_dir_export(Path::new(dir)),
                None,
                "{dir:?}"
            );
        }
        assert_eq!(
            NAMING_PARENT.required_dir_name(),
            Some(std::ffi::OsStr::new("agent"))
        );
        assert_eq!(NAMING_DIRECTORY.required_dir_name(), None);
        assert_eq!(
            NAMING_DIRECTORY.config_dir_export(Path::new("/")),
            Some(("AGENT_HOME", PathBuf::from("/")))
        );
    }

    /// An agent with no records directory has no records path to join.
    #[test]
    fn an_empty_records_directory_is_none() {
        assert_eq!(NAMING_DIRECTORY.records(), Some("sessions"));
        let none = AgentLayout {
            records_dir: "",
            record_level: 0,
            search_depth: 0,
            ..NAMING_DIRECTORY
        };
        assert_eq!(none.records(), None);
    }
}
