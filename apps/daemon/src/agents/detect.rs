//! Which agent a project is set up for, read from the marker files at the top level of its
//! directory or repository.

use std::path::Path;

use crate::protocol::AgentId;

/// The agent a project directory is set up for, read from marker files at its top level (see
/// [`detect_agent_from_names`]); `None` when the directory cannot be read.
pub fn detect_agent(dir: &Path) -> Option<AgentId> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    detect_agent_from_names(names.iter().map(String::as_str))
}

/// The agent a repository is set up for, from the names at its top level, or `None` when they name
/// no agent or more than one — a repository carrying both `CLAUDE.md` and `AGENTS.md` is common,
/// and picking one of them would be a guess. Each provider's own markers point at it; a generic
/// marker (`AGENTS.md`, read by Codex and Grok Build alike) points at the first registered provider
/// that reads it, and at none while a provider reading it is marked by its own.
pub(crate) fn detect_agent_from_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Option<AgentId> {
    let names: Vec<&str> = names.into_iter().collect();
    let present = |markers: &[&str]| markers.iter().any(|marker| names.contains(marker));
    let own: Vec<AgentId> = AgentId::ALL
        .into_iter()
        .filter(|agent| present(agent.descriptor().detection.own))
        .collect();
    let mut marked = own.clone();
    let mut claimed: Vec<&str> = Vec::new();
    for agent in AgentId::ALL {
        for marker in agent.descriptor().detection.generic {
            if claimed.contains(marker) || !names.contains(marker) {
                continue;
            }
            claimed.push(marker);
            let read_by_a_marked_reader = own
                .iter()
                .any(|reader| reader.descriptor().detection.generic.contains(marker));
            if !read_by_a_marked_reader && !marked.contains(&agent) {
                marked.push(agent);
            }
        }
    }
    match marked.as_slice() {
        [agent] => Some(*agent),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::detect_agent;
    use crate::protocol::AgentId;
    use crate::test_support::ScratchDir;

    fn detected_with(label: &str, markers: &[&str]) -> Option<AgentId> {
        let dir = ScratchDir::new(&format!("agents-detect-{label}"));
        for marker in markers {
            if marker.starts_with('.') {
                std::fs::create_dir(dir.join(marker)).expect("marker dir");
            } else {
                std::fs::write(dir.join(marker), "").expect("marker file");
            }
        }
        detect_agent(&dir)
    }

    #[test]
    fn detect_agent_reads_the_marker_files() {
        assert_eq!(
            detected_with("claude", &["CLAUDE.md"]),
            Some(AgentId::CLAUDE)
        );
        assert_eq!(detected_with("codex", &["AGENTS.md"]), Some(AgentId::CODEX));
        // `AGENTS.md` is generic, so `.grok` claims it.
        assert_eq!(
            detected_with("grok", &[".grok", "AGENTS.md"]),
            Some(AgentId::GROK)
        );
        assert_eq!(detected_with("both", &["CLAUDE.md", "AGENTS.md"]), None);
        assert_eq!(
            detected_with("codex-own", &[".codex", "AGENTS.md"]),
            Some(AgentId::CODEX)
        );
        assert_eq!(
            detected_with("generic-dir", &[".agents"]),
            Some(AgentId::CODEX)
        );
        assert_eq!(detected_with("own-pair", &[".codex", ".grok"]), None);
        assert_eq!(
            detected_with("claude-dir", &[".claude"]),
            Some(AgentId::CLAUDE)
        );
        assert_eq!(detected_with("none", &[]), None);
    }
}
