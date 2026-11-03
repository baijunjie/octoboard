//! Which agent a project is set up for, read from the marker files at the top level of its
//! directory or repository.

use std::path::Path;

use crate::protocol::Agent;

/// The agent a project directory is set up for, read from marker files at its top level (see
/// [`detect_agent_from_names`]); `None` when the directory cannot be read.
pub fn detect_agent(dir: &Path) -> Option<Agent> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    detect_agent_from_names(names.iter().map(String::as_str))
}

/// The agent a repository is set up for, from the names at its top level, or `None` when they name
/// no agent or more than one — a repository carrying both `CLAUDE.md` and `AGENTS.md` is common,
/// and picking one of them would be a guess. `AGENTS.md` and `.agents` are generic, read by Grok
/// Build as well, so they point at Codex only when `.grok` is absent.
pub(crate) fn detect_agent_from_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Option<Agent> {
    let (mut claude, mut grok, mut codex, mut generic) = (false, false, false, false);
    for name in names {
        match name {
            "CLAUDE.md" | ".claude" => claude = true,
            ".grok" => grok = true,
            ".codex" => codex = true,
            "AGENTS.md" | ".agents" => generic = true,
            _ => {}
        }
    }
    let codex = codex || (!grok && generic);
    match (claude, codex, grok) {
        (true, false, false) => Some(Agent::Claude),
        (false, true, false) => Some(Agent::Codex),
        (false, false, true) => Some(Agent::Grok),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::detect_agent;
    use crate::protocol::Agent;
    use crate::test_support::ScratchDir;

    fn detected_with(label: &str, markers: &[&str]) -> Option<Agent> {
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
        assert_eq!(detected_with("claude", &["CLAUDE.md"]), Some(Agent::Claude));
        assert_eq!(detected_with("codex", &["AGENTS.md"]), Some(Agent::Codex));
        // `AGENTS.md` is generic, so `.grok` claims it.
        assert_eq!(
            detected_with("grok", &[".grok", "AGENTS.md"]),
            Some(Agent::Grok)
        );
        assert_eq!(detected_with("both", &["CLAUDE.md", "AGENTS.md"]), None);
        assert_eq!(detected_with("none", &[]), None);
    }
}
