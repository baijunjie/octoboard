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

use super::AgentId;

/// One agent's on-disk layout. Paths are relative to the agent's configuration directory unless
/// a field says otherwise.
#[derive(Debug, PartialEq, Eq)]
pub struct AgentLayout {
    /// The environment variable the agent reads its configuration directory from: the one an
    /// adapter exports for a pinned account and a shell snapshot may already carry for the
    /// default one.
    pub config_dir_var: &'static str,
    /// The configuration directory's name under the home directory when the variable is unset.
    pub default_dir: &'static str,
    /// The directory a session's records are kept under, and the one a resume reads.
    pub records_dir: &'static str,
    /// Where the agent moves the records it archives, which a resume does not read.
    pub archived_records_dir: Option<&'static str>,
    /// How many levels below [`records_dir`](Self::records_dir) a session's record, a file or a
    /// directory, sits, counting its own name: the one depth a record to be removed is taken at.
    /// An agent whose index names a file inside the record (Grok Build's session marker) matches
    /// it one level deeper.
    pub record_level: usize,
    /// How many levels below the records directory a search for records goes (1 being the
    /// directory's own entries). Exact for an agent that files a record by its working directory,
    /// generous for one that files it under dates it does not look it up by.
    pub search_depth: usize,
    /// A file directly under the configuration directory that names every conversation of the
    /// account together.
    pub title_index: Option<&'static str>,
    /// A file in a session's own record directory that names the conversation alone.
    pub title_summary: Option<&'static str>,
}

/// The layout of `agent`.
pub(crate) fn layout(agent: AgentId) -> &'static AgentLayout {
    agent.descriptor().layout
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every registered provider names its directories, and searches its records directory at
    /// least as deep as a record sits.
    #[test]
    fn every_layout_names_its_directories_and_reaches_its_records() {
        for agent in AgentId::ALL {
            let layout = layout(agent);
            assert!(!layout.config_dir_var.is_empty(), "{agent:?}");
            assert!(!layout.default_dir.is_empty(), "{agent:?}");
            assert!(!layout.records_dir.is_empty(), "{agent:?}");
            assert!(layout.record_level >= 1, "{agent:?}");
            assert!(layout.search_depth >= layout.record_level, "{agent:?}");
        }
    }
}
