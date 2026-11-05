//! What Codex's accounts may share with the default account's configuration directory. Codex's
//! plugins are not recorded under a path, so it has no plugin registry to repoint. The
//! measurements are in `docs/agent-cli-reference.md`'s "Sharing a configuration directory between
//! accounts" section.

use crate::accounts::shared_config::{
    config_dir, config_file, history_dir, history_lines, thread_database, SharedItem,
};
use crate::protocol::AgentId;
use crate::providers::codex::{
    CODEX_ARCHIVED_DIR, CODEX_CONFIG_FILE, CODEX_PROMPT_HISTORY, CODEX_RECORDS_DIR,
    CODEX_TITLE_INDEX,
};

// The thread state database is a file item: where SQLite follows a link, it keeps the `-wal` and
// `-shm` beside the target, so those are not items; where it does not, the host shares no thread
// database at all (`shared_items`). Codex versions it by name (0.162.1 uses
// `state_5.sqlite`), and a new schema generation is a new file beside the old one. Only this one
// generation is listed on purpose; a database of another generation is reported by `repair` as
// unshared (`Repaired::unshared_databases`) until the catalog is updated.
pub(super) const ITEMS: &[SharedItem] = &[
    config_file(CODEX_CONFIG_FILE),
    config_file(AgentId::CODEX.descriptor().instruction_file),
    config_file("AGENTS.override.md"),
    config_file("hooks.json"),
    config_dir("rules"),
    config_dir("skills"),
    config_dir("prompts"),
    config_dir("plugins"),
    config_dir("themes"),
    config_dir("agents"),
    history_dir(CODEX_RECORDS_DIR),
    history_dir(CODEX_ARCHIVED_DIR),
    history_lines(CODEX_TITLE_INDEX),
    history_lines(CODEX_PROMPT_HISTORY),
    thread_database("state_5.sqlite"),
];
