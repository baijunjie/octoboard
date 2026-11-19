//! The index of a memory as a session is given it at launch: a console session's in its
//! instruction file, a project session's in its role description. Both hold the index between
//! tags of their own, newest first, within one size limit, and escape a closing tag in what the
//! entries say, so that it cannot end the index early and have what follows read as Octoboard's.

use crate::protocol::MemoryIndexEntry;
use crate::time::utc_date;

/// About how many bytes the index may take. The console session's built-in text is some 21 KiB,
/// and Codex reads at most 32 KiB of instruction files by default; the user's own text needs the
/// rest. A project session's role description is held to the same.
pub(super) const MEMORY_INDEX_LIMIT: usize = 4 * 1024;

/// `text` with the `<` of every closing tag `close_tag` turned into `&lt;`, matching the tag's name
/// without regard to case and whatever follows it, so `</USER_INSTRUCTIONS>` and
/// `</user_instructions >` are escaped too.
pub(super) fn escape_close_tag(text: &str, close_tag: &str) -> String {
    // ASCII lowercasing moves no byte, and the tag is ASCII, so a match in `lowered` sits at the
    // same char boundary in `text`.
    let prefix = close_tag.trim_end_matches('>');
    let lowered = text.to_ascii_lowercase();
    let mut escaped = String::with_capacity(text.len());
    let mut rest = 0;
    for (at, _) in lowered.match_indices(prefix) {
        escaped.push_str(&text[rest..at]);
        escaped.push_str("&lt;");
        rest = at + 1;
    }
    escaped.push_str(&text[rest..]);
    escaped
}

/// The index of `entries`, which come in the store's order (the most recently updated first, as
/// every listing of a memory gives them), between `tag`'s opening and closing tags: one line an
/// entry, in that order, as many as fit in [`MEMORY_INDEX_LIMIT`], its id, a mark for a local
/// one, the date of its last update (UTC) and its summary; `(no entries yet)` for none. After the
/// closing tag, a line says how many older entries `list_memories` lists that `whole` (the text
/// the index is in) leaves out.
pub(super) fn memory_index(entries: &[MemoryIndexEntry], tag: &str, whole: &str) -> String {
    let close_tag = format!("</{tag}>");
    let mut index = String::new();
    let mut listed = 0;
    for entry in entries {
        let local = if entry.local {
            "this machine only, "
        } else {
            ""
        };
        let line = escape_close_tag(
            &format!(
                "- `{}` ({local}updated {}): {}\n",
                entry.id,
                utc_date(entry.updated_at),
                entry.summary
            ),
            &close_tag,
        );
        if index.len() + line.len() > MEMORY_INDEX_LIMIT {
            break;
        }
        index.push_str(&line);
        listed += 1;
    }
    if entries.is_empty() {
        index.push_str("(no entries yet)\n");
    }
    let mut text = format!("<{tag}>\n{index}{close_tag}");
    let unlisted = entries.len() - listed;
    if unlisted > 0 {
        text.push_str(&format!(
            "\n\n{unlisted} older {} not listed here, to keep {whole} short; `list_memories` \
             lists every entry.",
            if unlisted == 1 {
                "entry is"
            } else {
                "entries are"
            }
        ));
    }
    text
}
