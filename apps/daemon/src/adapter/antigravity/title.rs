//! The session's task title. Antigravity CLI sets the terminal title only to what a configured
//! `title` command prints, run through the shell with a JSON payload on its standard input at
//! each change of the agent's state (1.5.14). The payload names the conversation
//! (`conversation_id`) but not its title, although the CLI's release notes say a
//! `conversation_title` field was added in 1.1.27: it was absent from every payload measured,
//! before and after the title was generated. The title the CLI generates for a conversation
//! during its first turn is kept in `antigravity-cli/annotations/<conversation id>.pbtxt` as one
//! protobuf text line, `title:"<text>"`, with `"` and `\` escaped and other text as UTF-8.
//!
//! So each launch gives the CLI a title command of its own, in its `--settings` (never saved):
//! Octoboard's own helper ([`HELPER`], run as `octoboardd agent-helper`) given the annotations'
//! directory, read through the launch's link to the data directory, which prints the title the
//! payload's conversation's annotation names, and nothing before there is one. The terminal title
//! the CLI then sets is the title, escaped as the file has it, which [`task_title`] reads back.
//! The command line is a POSIX shell's, as the launch's hook commands are, so it reaches the CLI
//! on Windows the way they do; no script and no tool of the host's (`sed`) is needed.

use std::io::{Read, Write};
use std::path::Path;

use serde_json::{json, Value};

use crate::adapter::files::SMALL_FILE_LIMIT;
use crate::hostfs::read_regular_text;
use crate::providers::antigravity::APP_DATA_DIR;
use crate::providers::AgentId;
use crate::records::locate::plausible_id;

/// The settings key of the title command.
pub(super) const SETTING: &str = "title";

/// The name of the helper the title command runs ([`crate::adapter::helper_command`]).
pub(super) const HELPER: &str = "title";

/// The directory of the conversations' annotations, inside the CLI's data directory.
pub(super) const ANNOTATIONS_DIR: &str = "annotations";

/// The most of the payload that is read; one is well under a kilobyte (1.5.14).
const PAYLOAD_LIMIT: u64 = 64 * 1024;

/// The setting that runs the title helper of the daemon's executable `self_exe` on the
/// annotations of the launch whose directory is `dir`.
pub(super) fn command_setting(self_exe: &str, dir: &Path) -> Value {
    let annotations = dir.join(APP_DATA_DIR).join(ANNOTATIONS_DIR);
    json!({
        "type": "command",
        "command": crate::adapter::helper_command(
            self_exe,
            AgentId::ANTIGRAVITY,
            HELPER,
            &[&annotations.to_string_lossy()],
        ),
    })
}

/// Runs the title helper with `args`, the annotations' directory alone: reads the payload on
/// stdin and prints the title of its conversation, or nothing. Exits 0 whatever happens, since
/// the CLI may show a failing command.
pub(super) fn run_helper(args: &[String]) -> i32 {
    let mut payload = Vec::new();
    let read = std::io::stdin()
        .take(PAYLOAD_LIMIT)
        .read_to_end(&mut payload);
    if let (Ok(_), [annotations]) = (read, args) {
        if let Some(title) = title_of(&payload, Path::new(annotations)) {
            let _ = writeln!(std::io::stdout(), "{title}");
        }
    }
    0
}

/// The title, escaped as the file has it, that the annotation in `annotations` of the conversation
/// `payload` names; `None` for a payload that names no conversation yet (the first payloads of a
/// new one have `conversation_id` empty), a conversation id that is not one, or a conversation
/// with no annotation or no title in it.
fn title_of(payload: &[u8], annotations: &Path) -> Option<String> {
    let payload: Value = serde_json::from_slice(payload).ok()?;
    let id = payload.get("conversation_id")?.as_str()?;
    // An id is a UUID: anything else could name a file outside the directory.
    if !plausible_id(id) {
        return None;
    }
    // A regular file only, and only its first part: an annotation is one short line.
    let annotation = read_regular_text(&annotation_file(annotations, id), SMALL_FILE_LIMIT).ok()?;
    annotation.lines().find_map(|line| {
        line.strip_prefix("title:\"")
            .and_then(|rest| rest.strip_suffix('"'))
            .map(str::to_owned)
    })
}

pub(super) fn annotation_file(annotations: &Path, id: &str) -> std::path::PathBuf {
    annotations.join(format!("{id}.pbtxt"))
}

/// The task title a terminal title the launch's command set carries: the annotation's text with
/// its protobuf text escapes undone, on one line; `None` for an empty title, which the CLI sets
/// before a conversation has a title and as it exits.
pub(super) fn task_title(raw: &str) -> Option<String> {
    let title = unescaped(raw.trim());
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then_some(title)
}

/// `text` with the escapes of a protobuf text string undone: `\"`, `\'`, `\\`, `\n`, `\r`, `\t`,
/// up to three octal digits and `\x` with up to two hex digits, each of the last two a byte. A
/// backslash before anything else is kept.
fn unescaped(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'\\' || at + 1 == bytes.len() {
            out.push(bytes[at]);
            at += 1;
            continue;
        }
        let next = bytes[at + 1];
        at += 2;
        match next {
            b'"' | b'\'' | b'\\' => out.push(next),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'0'..=b'7' => {
                let mut value = u32::from(next - b'0');
                for _ in 0..2 {
                    match bytes.get(at) {
                        Some(digit @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(digit - b'0');
                            at += 1;
                        }
                        _ => break,
                    }
                }
                out.push((value & 0xff) as u8);
            }
            b'x' => {
                let digits = bytes[at..]
                    .iter()
                    .take(2)
                    .take_while(|byte| byte.is_ascii_hexdigit())
                    .count();
                match u8::from_str_radix(&text[at..at + digits], 16) {
                    Ok(value) if digits > 0 => {
                        out.push(value);
                        at += digits;
                    }
                    _ => out.extend_from_slice(b"\\x"),
                }
            }
            other => out.extend_from_slice(&[b'\\', other]),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    /// The title of a conversation as the launch's command printed it, escaped as its annotation
    /// keeps it (1.5.14: `title:"苹果 \"quoted\" title"` set the terminal title to
    /// `苹果 \"quoted\" title`), reads unescaped; an empty title, set before there is one and at
    /// exit, carries none.
    #[test]
    fn the_title_is_the_annotation_s_text_unescaped() {
        assert_eq!(
            task_title(r#"苹果 \"quoted\" title"#).as_deref(),
            Some(r#"苹果 "quoted" title"#)
        );
        assert_eq!(
            task_title("Create And Remove File").as_deref(),
            Some("Create And Remove File")
        );
        assert_eq!(task_title(""), None);
        assert_eq!(task_title("   "), None);
        assert_eq!(task_title(r"a\\b").as_deref(), Some(r"a\b"));
        assert_eq!(task_title(r"two\nrows").as_deref(), Some("two rows"));
        // Octal and hex bytes make up UTF-8 as the other escapes do.
        assert_eq!(
            task_title(r"\350\213\271\xe6\x9e\x9c").as_deref(),
            Some("苹果")
        );
        // A backslash before anything else, or at the end, stays.
        assert_eq!(task_title(r"a\qb\").as_deref(), Some(r"a\qb\"));
        assert_eq!(task_title(r"\xg").as_deref(), Some(r"\xg"));
    }

    /// The setting runs the daemon's title helper with the annotations' directory under the
    /// launch's data directory as its argument, quoted as one shell word.
    #[test]
    fn the_command_runs_the_title_helper_on_the_launch_s_annotations() {
        let scratch = ScratchDir::new("antigravity-title-command");
        let dir = scratch.join("it's launch");
        let setting = command_setting("/opt/octoboard/octoboardd", &dir);
        assert_eq!(setting["type"], "command");
        let annotations = dir.join("antigravity-cli").join("annotations");
        assert_eq!(
            setting["command"],
            crate::adapter::helper_command(
                "/opt/octoboard/octoboardd",
                AgentId::ANTIGRAVITY,
                "title",
                &[&annotations.to_string_lossy()],
            )
        );
        assert!(setting["command"]
            .as_str()
            .unwrap()
            .contains(r"it'\''s launch"));
        assert!(!dir.exists(), "nothing is written for the command");
    }

    /// A helper the provider does not have runs nothing.
    #[test]
    fn a_helper_the_provider_does_not_have_runs_nothing() {
        let provider = crate::adapter::provider(AgentId::ANTIGRAVITY);
        assert_eq!(provider.run_helper("no-such-helper", &[]), None);
    }

    /// The title of the payload's conversation is its annotation's, escaped as the file has it;
    /// a payload with no conversation yet, a conversation with no annotation, and an id that would
    /// name a file elsewhere have none.
    #[test]
    fn the_title_is_that_of_the_payload_s_conversation() {
        let scratch = ScratchDir::new("antigravity-title-helper");
        let annotations = scratch.join("annotations");
        std::fs::create_dir_all(&annotations).unwrap();
        std::fs::write(
            annotations.join("c911c80f-aaf7-413f-83ac-5a57e05a6263.pbtxt"),
            "title:\"苹果 \\\"quoted\\\" title\"\n",
        )
        .unwrap();
        std::fs::write(scratch.join("outside.pbtxt"), "title:\"outside\"\n").unwrap();
        let title = |payload: &str| title_of(payload.as_bytes(), &annotations);

        assert_eq!(
            title(
                r#"{"cwd":"/p","session_id":"c911c80f-aaf7-413f-83ac-5a57e05a6263","conversation_id":"c911c80f-aaf7-413f-83ac-5a57e05a6263","agent_state":"idle"}"#
            )
            .as_deref(),
            Some("苹果 \\\"quoted\\\" title")
        );
        assert_eq!(
            title(r#"{"cwd":"/p","session_id":"","conversation_id":"","agent_state":"idle"}"#),
            None
        );
        assert_eq!(
            title(r#"{"conversation_id":"00000000-0000-0000-0000-000000000000"}"#),
            None
        );
        assert_eq!(title(r#"{"conversation_id":"../outside"}"#), None);
        assert_eq!(title("not json"), None);
        // Only the exact shape of a UUID is taken: an id of letters and dashes that names a file
        // of the directory is not one, nor is a UUID with anything around it.
        std::fs::write(annotations.join("not-an-id.pbtxt"), "title:\"other\"\n").unwrap();
        assert_eq!(title(r#"{"conversation_id":"not-an-id"}"#), None);
        std::fs::write(
            annotations.join("c911c80f-aaf7-413f-83ac-5a57e05a6263-x.pbtxt"),
            "title:\"other\"\n",
        )
        .unwrap();
        assert_eq!(
            title(r#"{"conversation_id":"c911c80f-aaf7-413f-83ac-5a57e05a6263-x"}"#),
            None
        );
    }

    /// An annotation that is not a regular file gives no title, and a FIFO in its place does not
    /// hold the helper, which the CLI runs at every change of state.
    #[cfg(unix)]
    #[test]
    fn an_annotation_that_is_no_regular_file_gives_no_title() {
        let scratch = ScratchDir::new("antigravity-title-fifo");
        let annotations = scratch.join("annotations");
        std::fs::create_dir_all(&annotations).unwrap();
        let id = "c911c80f-aaf7-413f-83ac-5a57e05a6263";
        let fifo = annotations.join(format!("{id}.pbtxt"));
        crate::test_support::mkfifo(&fifo);
        let payload = format!(r#"{{"conversation_id":"{id}"}}"#);
        assert_eq!(title_of(payload.as_bytes(), &annotations), None);
    }
}
