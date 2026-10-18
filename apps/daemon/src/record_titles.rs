//! The titles an agent's own records give a session: the name the conversation was given, and for
//! an agent that names nothing the opening line of the first prompt. Only titles are kept: an
//! account's shared title file is held for the sessions the daemon knows alone, and a prompt is
//! cut to a line. Which files hold a title, and how a line or a file says it, is each agent's own
//! knowledge.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::adapter::AgentAdapter;
use crate::line_file::{Change, LineFile};
use crate::session_title::RecordTitles;

/// The longest title taken from a record's name for the conversation.
const MAX_NAMED_CHARS: usize = 200;
/// The longest stand-in taken from a prompt.
const MAX_PROMPT_CHARS: usize = 80;

/// What one session's own record has given so far.
#[derive(Default)]
pub struct Titles {
    /// The name, and the precedence it was found at.
    named: Option<(u8, String)>,
    prompt: Option<String>,
}

impl Titles {
    /// Takes what one line of the conversation's record says.
    pub fn fold_line(&mut self, adapter: &dyn AgentAdapter, line: &str) {
        if let Some(title) = adapter.named_title(line) {
            let current = self.named.as_ref().map(|(precedence, _)| *precedence);
            if current.is_none_or(|current| title.precedence >= current) {
                if let Some(text) = named_title(&title.text) {
                    self.named = Some((title.precedence, text));
                }
            }
        }
        if self.prompt.is_none() {
            self.prompt = adapter
                .prompt_text(line)
                .and_then(|prompt| prompt_title(&prompt));
        }
    }

    /// Takes the small JSON file that names a conversation.
    pub fn fold_summary(&mut self, adapter: &dyn AgentAdapter, summary: &serde_json::Value) {
        if let Some(text) = adapter
            .summary_title(summary)
            .and_then(|title| named_title(&title))
        {
            self.named = Some((0, text));
        }
    }

    /// What was read before no longer holds.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The titles to store: what was read, else what is stored, for a record that cannot be read
    /// now. `shared` is the account's shared file's name for the conversation, which wins.
    pub fn merged(&self, shared: Option<&str>, stored: &RecordTitles) -> RecordTitles {
        RecordTitles {
            named: shared
                .map(str::to_owned)
                .or_else(|| self.named.as_ref().map(|(_, text)| text.clone()))
                .or_else(|| stored.named.clone()),
            prompt: self.prompt.clone().or_else(|| stored.prompt.clone()),
        }
    }
}

/// One account's file that names every conversation of the account, read incrementally, of which
/// only the names of the conversations asked for are kept.
#[derive(Default)]
pub struct SharedTitles {
    file: LineFile,
    wanted: HashSet<String>,
    names: HashMap<String, String>,
}

impl SharedTitles {
    /// Reads what the file gained, for the conversations `wanted`. A conversation not asked for
    /// before may have been named already, so the file is read again from its start.
    pub fn refresh(&mut self, adapter: &dyn AgentAdapter, path: &Path, wanted: HashSet<String>) {
        if !wanted.is_subset(&self.wanted) {
            self.file = LineFile::default();
            self.names.clear();
        }
        self.wanted = wanted;
        self.names.retain(|id, _| self.wanted.contains(id));
        let Ok(change) = self.file.check(path) else {
            return;
        };
        if change == Change::Restarted {
            self.names.clear();
        }
        if change == Change::Unchanged {
            return;
        }
        let (wanted, names) = (&self.wanted, &mut self.names);
        let _ = self.file.read(path, |line| {
            if let Some((id, text)) = adapter.indexed_title(line) {
                if let (true, Some(text)) = (wanted.contains(&id), named_title(&text)) {
                    names.insert(id, text);
                }
            }
        });
    }

    pub fn name(&self, id: &str) -> Option<&str> {
        self.names.get(id).map(String::as_str)
    }
}

/// A title an agent's record names the conversation with, on one line and bounded.
fn named_title(text: &str) -> Option<String> {
    tidy(text, MAX_NAMED_CHARS)
}

/// A stand-in title from a prompt: its first line that is not a markdown heading, on one line and
/// bounded.
fn prompt_title(prompt: &str) -> Option<String> {
    let line = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))?;
    tidy(line, MAX_PROMPT_CHARS)
}

/// `text` with every run of whitespace collapsed to one space, cut to `max` characters with an
/// ellipsis. `None` when nothing is left.
fn tidy(text: &str, max: usize) -> Option<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= max {
        return Some(text);
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    Some(cut)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::adapter_for;
    use crate::protocol::Agent;

    fn capture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join(name)
    }

    fn replay(agent: Agent, name: &str) -> Titles {
        let adapter = adapter_for(agent);
        let mut titles = Titles::default();
        for line in std::fs::read_to_string(capture(name)).unwrap().lines() {
            titles.fold_line(adapter.as_ref(), line);
        }
        titles
    }

    #[test]
    fn committed_captures_replay_through_each_agents_readers() {
        // Claude: the user's rename, the last line of the transcript, outranks the generated title
        // before it; the prompt in `last-prompt` is no title for an agent that names its own.
        let claude = replay(Agent::Claude, "titles_claude_2.1.301.jsonl");
        let stored = RecordTitles::default();
        assert_eq!(
            claude.merged(None, &stored),
            RecordTitles {
                named: Some("Renamed by the user".to_owned()),
                prompt: None
            }
        );
        let generated: String = std::fs::read_to_string(capture("titles_claude_2.1.301.jsonl"))
            .unwrap()
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join("\n");
        let mut titles = Titles::default();
        let adapter = adapter_for(Agent::Claude);
        generated
            .lines()
            .for_each(|line| titles.fold_line(adapter.as_ref(), line));
        assert_eq!(
            titles.merged(None, &stored).named.as_deref(),
            Some("Pseudo terminal explanation")
        );

        // Codex: the first typed prompt, in either generation's event, and not the injected
        // context beside it.
        for (name, prompt) in [
            (
                "titles_codex_0.163.0_rollout.jsonl",
                "Explain in one sentence what a mutex is.",
            ),
            ("titles_codex_0.142.5_rollout.jsonl", "Fix the flaky test"),
        ] {
            let codex = replay(Agent::Codex, name).merged(None, &stored);
            assert_eq!(codex.prompt.as_deref(), Some(prompt), "{name}");
            assert_eq!(codex.named, None, "{name}");
        }

        // Grok: the summary's generated title, and no title from one that has none.
        let adapter = adapter_for(Agent::Grok);
        let summary = |text: &str| serde_json::from_str(text).unwrap();
        let mut grok = Titles::default();
        grok.fold_summary(
            adapter.as_ref(),
            &summary(&std::fs::read_to_string(capture("titles_grok_1.0.60_summary.json")).unwrap()),
        );
        assert_eq!(
            grok.merged(None, &stored).named.as_deref(),
            Some("Explain in one sentence what a pseudo terminal is. Do")
        );
        let mut grok = Titles::default();
        grok.fold_summary(
            adapter.as_ref(),
            &summary(r#"{"session_summary":"A prompt, no title"}"#),
        );
        assert_eq!(grok.merged(None, &stored), stored);
        grok.fold_summary(
            adapter.as_ref(),
            &summary(r#"{"generated_title":"Now titled"}"#),
        );
        assert_eq!(
            grok.merged(None, &stored).named.as_deref(),
            Some("Now titled")
        );
    }

    #[test]
    fn a_shared_file_keeps_the_names_of_the_conversations_asked_for_only() {
        let adapter = adapter_for(Agent::Codex);
        let path = capture("titles_codex_0.163.0_index.jsonl");
        let (renamed, mutex) = (
            "01a143fc-9184-7b70-9728-4b5e3cefaefd",
            "01a14419-4b1b-70c1-846d-f8ec09fece89",
        );
        let wanted = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect();
        let mut shared = SharedTitles::default();
        shared.refresh(adapter.as_ref(), &path, wanted(&[mutex]));
        assert_eq!(shared.name(mutex), Some("Explain mutex briefly"));
        assert_eq!(shared.name(renamed), None, "not asked for, so not kept");
        // Asked for later: the file is read again, and the last line of an id wins.
        shared.refresh(adapter.as_ref(), &path, wanted(&[mutex, renamed]));
        assert_eq!(shared.name(renamed), Some("Renamed with /rename"));
        shared.refresh(adapter.as_ref(), &path, wanted(&[renamed]));
        assert_eq!(shared.name(mutex), None, "no longer asked for");
        assert_eq!(shared.name(renamed), Some("Renamed with /rename"));
    }

    #[test]
    fn prompts_give_their_first_line_that_is_not_a_heading_bounded() {
        assert_eq!(
            prompt_title("\n# Plan\n\n  Fix   the\ttitle  \nmore").as_deref(),
            Some("Fix the title")
        );
        assert_eq!(prompt_title("# only\n\n"), None);
        let long = prompt_title(&"word ".repeat(40)).unwrap();
        assert_eq!(long.chars().count(), MAX_PROMPT_CHARS);
        assert!(long.ends_with("word…"));
    }
}
