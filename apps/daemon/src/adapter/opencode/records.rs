//! OpenCode keeps its conversations in SQLite. The CLI owns the database, including deletion.

use std::collections::HashSet;
use std::io;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::adapter::account_cli;
use crate::protocol::AgentId;
use crate::records::context::Context;
use crate::records::conversation::{Entry, Todo};
use crate::records::observed::Observed;
use crate::records::rows::{RowAccount, RowRecords};
use crate::records::RecordStore;

const READ_LIMIT: usize = 32 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
// Each read starts a CLI, including one per live follower. Wait for a slot for at most one
// command's budget so a burst of events cannot start an unbounded number of processes.
static SLOTS: (Mutex<usize>, Condvar) = (Mutex::new(2), Condvar::new());

struct Slot;

impl Slot {
    fn acquire() -> io::Result<Self> {
        let (count, available) = &SLOTS;
        let (mut left, _) = available
            .wait_timeout_while(count.lock().unwrap(), TIMEOUT, |left| *left == 0)
            .unwrap();
        if *left == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "OpenCode record readers are busy",
            ));
        }
        *left -= 1;
        Ok(Self)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        *SLOTS.0.lock().unwrap() += 1;
        SLOTS.1.notify_one();
    }
}

pub(super) struct OpenCodeRecords;

fn database_exists(account: &RowAccount<'_>) -> bool {
    account.config_dir.is_none() && super::version::database(account.shell_env).is_file()
}

// Rows come through OpenCode's own command, not a read of `opencode.db` here. The database is in WAL
// mode and a running TUI holds it open, so the turn so far sits in the WAL; the immutable read the
// migration checks use (`version::immutable_database`) ignores the WAL and would miss it, and opening
// it normally would create or checkpoint the sidecars of a file that is OpenCode's. In 2.x the
// conversation is also a projection only its `session export` builds.
fn select(account: &RowAccount<'_>, sql: &str) -> io::Result<Vec<Value>> {
    let _slot = Slot::acquire()?;
    if !database_exists(account) {
        return Err(io::Error::other("OpenCode's database is unavailable"));
    }
    let output = account_cli::query(
        AgentId::OPENCODE,
        account.shell_env,
        None,
        &["db", sql, "--format", "json"],
        TIMEOUT,
        READ_LIMIT,
        None,
    )
    .map_err(|_| io::Error::other("could not run OpenCode's database query"))?;
    if !output.status.success() {
        return Err(io::Error::other("OpenCode's database query failed"));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn query_v2(account: &RowAccount<'_>, args: &[&str], timeout: Duration) -> io::Result<Value> {
    let output = account_cli::query(
        AgentId::OPENCODE,
        account.shell_env,
        None,
        args,
        timeout,
        READ_LIMIT,
        None,
    )
    .map_err(|_| io::Error::other("could not run OpenCode's record query"))?;
    if !output.status.success() {
        let missing = [&output.stdout, &output.stderr].iter().any(|stream| {
            let text = String::from_utf8_lossy(stream);
            text.contains("SessionNotFoundError") || text.contains("Session not found")
        });
        return Err(if missing {
            io::Error::new(io::ErrorKind::NotFound, "OpenCode session is missing")
        } else {
            io::Error::other("OpenCode's record query failed")
        });
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn time_left(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "OpenCode's record query timed out"))
}

fn wait_for_v2_records(account: &RowAccount<'_>, deadline: Instant) -> io::Result<()> {
    if !super::version::legacy_records_pending(account.shell_env) {
        return Ok(());
    }
    loop {
        let status = query_v2(
            account,
            &[
                "api",
                "--standalone",
                "GET",
                "/api/experimental/migration/v1",
            ],
            time_left(deadline)?,
        )?;
        match status["status"].as_str() {
            Some("completed") => return Ok(()),
            Some("required" | "running") => {
                std::thread::sleep(Duration::from_millis(100).min(time_left(deadline)?));
            }
            _ => {
                return Err(io::Error::other(
                    "OpenCode could not migrate its session history",
                ))
            }
        }
    }
}

/// What OpenCode 2 answered, before a launch that resumes a conversation, about whether it still
/// has it ([`resume_answer`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResumeAnswer {
    /// It has it.
    Present,
    /// It said it has no such conversation: the one answer that proves it gone, and the launch is
    /// refused, since OpenCode 2 would start a new conversation under the id it was given.
    Absent,
    /// No answer that says either: no database where it is looked for, a query that could not run,
    /// timed out or failed otherwise, readers busy, or an id not of OpenCode's form. The launch
    /// goes ahead, and OpenCode's own resume says what it finds.
    Unconfirmed,
}

impl ResumeAnswer {
    /// The answer a lookup of the conversation came to: absent only on OpenCode's own word that it
    /// has no such conversation ([`query_v2`]'s `NotFound`), unconfirmed on any other failure.
    pub(super) fn of<T>(lookup: &io::Result<T>) -> Self {
        match lookup {
            Ok(_) => Self::Present,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Self::Absent,
            Err(_) => Self::Unconfirmed,
        }
    }
}

/// Asks OpenCode 2 whether it still has the conversation `id` of the default account, through its
/// own `api` command, for a launch that resumes it. Blocking, at most [`TIMEOUT`] and a reader's
/// wait.
pub(super) fn resume_answer(spec: &super::LaunchSpec<'_>, id: &str) -> ResumeAnswer {
    let account = RowAccount {
        config_dir: None,
        shell_env: spec.shell_env,
    };
    if !OpenCodeRecords.names_conversation(id) || !database_exists(&account) {
        return ResumeAnswer::Unconfirmed;
    }
    let lookup = Slot::acquire().and_then(|_slot| {
        let deadline = Instant::now() + TIMEOUT;
        wait_for_v2_records(&account, deadline)?;
        let path = format!("/api/session/{id}");
        query_v2(
            &account,
            &["api", "--standalone", "GET", &path],
            time_left(deadline)?,
        )
    });
    ResumeAnswer::of(&lookup)
}

fn v2_conversations(account: &RowAccount<'_>) -> Option<HashSet<String>> {
    let _slot = Slot::acquire().ok()?;
    let deadline = Instant::now() + TIMEOUT;
    wait_for_v2_records(account, deadline).ok()?;
    let mut path = "/api/session?limit=1000".to_string();
    let mut ids = HashSet::new();
    let mut seen = HashSet::new();
    loop {
        let left = time_left(deadline).ok()?;
        let page = query_v2(account, &["api", "--standalone", "GET", &path], left).ok()?;
        let sessions = page["data"].as_array()?;
        if sessions.is_empty() {
            return Some(ids);
        }
        for session in sessions {
            ids.insert(session["id"].as_str()?.to_owned());
        }
        let Some(cursor) = page["cursor"]["next"].as_str() else {
            return Some(ids);
        };
        if !cursor
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
            || !seen.insert(cursor.to_owned())
        {
            return None;
        }
        path = format!("/api/session?limit=1000&cursor={cursor}");
    }
}

fn v2_lines(export: &Value) -> io::Result<Vec<Value>> {
    let messages = export["messages"]
        .as_array()
        .ok_or_else(|| io::Error::other("OpenCode's export contains no messages"))?;
    let mut lines = Vec::new();
    for message in messages {
        let (mut info, parts) = match message["type"].as_str() {
            Some("user") => (
                message.clone(),
                json!([{ "type": "text", "text": message["text"] }]),
            ),
            Some("assistant") if message["time"]["completed"].is_null() => break,
            Some("assistant") => {
                let mut info = message.clone();
                info["providerID"] = message["model"]["providerID"].clone();
                info["modelID"] = message["model"]["id"].clone();
                info["variant"] = message["model"]["variant"].clone();
                let parts = info.as_object_mut().and_then(|info| info.remove("content"));
                let mut parts = parts
                    .and_then(|parts| parts.as_array().cloned())
                    .unwrap_or_default();
                for part in parts.iter_mut().filter(|part| part["type"] == "tool") {
                    part["tool"] = part["name"].clone();
                    if part["tool"] == "patch" {
                        part["tool"] = json!("apply_patch");
                    }
                    if let Some(path) = part["state"]["input"]["path"].as_str() {
                        part["state"]["input"]["filePath"] = json!(path);
                    }
                }
                (info, Value::Array(parts))
            }
            _ => continue,
        };
        info["role"] = message["type"].clone();
        lines.push(json!({ "info": info, "parts": parts }));
    }
    Ok(lines)
}

impl RowRecords for OpenCodeRecords {
    fn names_conversation(&self, id: &str) -> bool {
        id.strip_prefix("ses_").is_some_and(|suffix| {
            suffix.len() == 26 && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
    }

    fn conversations(&self, account: &RowAccount<'_>) -> Option<HashSet<String>> {
        if !database_exists(account) {
            return None;
        }
        if super::version::background_v2(account.shell_env)? {
            return v2_conversations(account);
        }
        let rows = select(account, "SELECT id FROM session").ok()?;
        rows.iter()
            .map(|row| Some(row.get("id")?.as_str()?.to_owned()))
            .collect()
    }

    fn lines(&self, account: &RowAccount<'_>, id: &str) -> io::Result<Vec<Value>> {
        if !self.names_conversation(id) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "invalid OpenCode session id",
            ));
        }
        if !database_exists(account) {
            return Err(io::Error::other("OpenCode's database is unavailable"));
        }
        match super::version::background_v2(account.shell_env) {
            Some(true) => {
                let _slot = Slot::acquire()?;
                let deadline = Instant::now() + TIMEOUT;
                wait_for_v2_records(account, deadline)?;
                let export = query_v2(
                    account,
                    &["session", "export", "--standalone", id],
                    time_left(deadline)?,
                )?;
                return v2_lines(&export);
            }
            Some(false) => {}
            None => {
                return Err(io::Error::other(
                    "OpenCode's record commands are unavailable",
                ))
            }
        }
        // A left join from the session distinguishes an empty conversation from an absent one.
        let sql = format!(
            "SELECT m.id, m.data AS info, p.data AS part FROM session s \
             LEFT JOIN message m ON m.session_id = s.id \
             LEFT JOIN part p ON p.message_id = m.id WHERE s.id = '{id}' \
             ORDER BY m.time_created, m.id, p.time_created, p.id"
        );
        let rows = select(account, &sql)?;
        if rows.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "OpenCode session is missing",
            ));
        }
        completed_lines(&rows)
    }

    fn remove(&self, account: &RowAccount<'_>, id: &str) -> anyhow::Result<bool> {
        let _slot = Slot::acquire()?;
        if !self.names_conversation(id) || !database_exists(account) {
            return Ok(false);
        }
        let deadline = Instant::now() + TIMEOUT;
        let args: &[&str] = match super::version::background_v2(account.shell_env) {
            Some(false) => &["session", "delete", id],
            Some(true) => {
                wait_for_v2_records(account, deadline)?;
                &["session", "delete", "--standalone", id]
            }
            None => anyhow::bail!("OpenCode's record commands are unavailable"),
        };
        let output = account_cli::query(
            AgentId::OPENCODE,
            account.shell_env,
            None,
            args,
            time_left(deadline)?,
            64 * 1024,
            None,
        )
        .map_err(|_| anyhow::anyhow!("could not run OpenCode session delete"))?;
        anyhow::ensure!(output.status.success(), "OpenCode session delete failed");
        Ok(true)
    }
}

/// A finished assistant message stays stable as the turn proceeds. Keep the unfinished suffix
/// out of the feed so streamed text can't invalidate its position or the observation boundary.
fn completed_lines(rows: &[Value]) -> io::Result<Vec<Value>> {
    let mut lines = Vec::new();
    for group in rows.chunk_by(|a, b| a["id"] == b["id"]) {
        let Some(info) = group[0]["info"].as_str() else {
            continue;
        };
        let info: Value = serde_json::from_str(info)?;
        if info["role"] == "assistant" && info["time"]["completed"].is_null() {
            break;
        }
        let parts = group
            .iter()
            .filter_map(|row| row["part"].as_str())
            .map(serde_json::from_str)
            .collect::<Result<Vec<Value>, _>>()?;
        lines.push(serde_json::json!({ "info": info, "parts": parts }));
    }
    Ok(lines)
}

impl RecordStore for OpenCodeRecords {
    fn rows(&self) -> Option<&dyn RowRecords> {
        Some(self)
    }

    fn recognises_records(&self) -> bool {
        true
    }

    fn absence_is_conclusive(&self) -> bool {
        true
    }

    fn entries(&self, line: &Value) -> Vec<Entry> {
        let Some(parts) = line["parts"].as_array() else {
            return Vec::new();
        };
        let text = message_text(parts);
        let mut entries = Vec::new();
        if !text.trim().is_empty() {
            match line["info"]["role"].as_str() {
                Some("user") => entries.push(Entry::User(text)),
                Some("assistant") if line["info"]["summary"] == true => {
                    entries.push(Entry::Summary(text))
                }
                Some("assistant") => entries.push(Entry::Assistant(text)),
                _ => {}
            }
        }
        for part in parts {
            if part["type"] == "tool" && part["state"]["status"] == "completed" {
                entries.extend(tool_entry(part));
            }
        }
        entries
    }

    fn is_user_prompt(&self, line: &str) -> Option<bool> {
        let line: Value = serde_json::from_str(line).ok()?;
        let role = line["info"]["role"].as_str()?;
        let parts = line["parts"].as_array()?;
        Some(
            role == "user"
                && parts.iter().any(|part| {
                    part["synthetic"] != true
                        && part["ignored"] != true
                        && (part["type"] == "text" || part["type"] == "file")
                }),
        )
    }

    fn fold_line(
        &self,
        line: &str,
        past_boundary: bool,
        context: &mut Context,
        observed: &mut Observed,
    ) {
        let Ok(line) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let info = &line["info"];
        if info["role"] != "assistant" || info["summary"] == true || !info["error"].is_null() {
            return;
        }
        let tokens = &info["tokens"];
        if let (Some(input), Some(output)) = (tokens["input"].as_u64(), tokens["output"].as_u64()) {
            let total = [
                input,
                output,
                tokens["reasoning"].as_u64().unwrap_or(0),
                tokens["cache"]["read"].as_u64().unwrap_or(0),
                tokens["cache"]["write"].as_u64().unwrap_or(0),
            ]
            .into_iter()
            .fold(0_u64, u64::saturating_add);
            context.observe(total);
        }
        if past_boundary {
            if let (Some(provider), Some(model)) =
                (info["providerID"].as_str(), info["modelID"].as_str())
            {
                let model = format!("{provider}/{model}");
                match info["variant"].as_str() {
                    Some(effort) => observed.fold(Some(model), Some(effort.to_owned())),
                    None => observed.unset_effort(model),
                }
            }
        }
    }
}

fn message_text(parts: &[Value]) -> String {
    parts
        .iter()
        .filter(|part| {
            part["type"] == "text" && part["synthetic"] != true && part["ignored"] != true
        })
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn prompt_text(line: &str) -> Option<String> {
    let line: Value = serde_json::from_str(line).ok()?;
    (line["info"]["role"] == "user")
        .then(|| message_text(line["parts"].as_array().map_or(&[], Vec::as_slice)))
}

fn tool_entry(part: &Value) -> Option<Entry> {
    let input = &part["state"]["input"];
    let changed = match part["tool"].as_str()? {
        "write" | "edit" => vec![input["filePath"].as_str()?.to_owned()],
        "apply_patch" => crate::adapter::patch::patch_targets(input["patchText"].as_str()?),
        "todowrite" => {
            let items = input["todos"]
                .as_array()?
                .iter()
                .filter_map(|todo| {
                    Some(Todo {
                        id: None,
                        text: todo["content"].as_str()?.to_owned(),
                        status: todo["status"].as_str()?.to_owned(),
                    })
                })
                .collect();
            return Some(Entry::Todos {
                items,
                merge: false,
            });
        }
        _ => return None,
    };
    (!changed.is_empty()).then_some(Entry::Changed(changed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fake_agent, ScratchDir};

    const CAPTURE: &str = include_str!("../../../testdata/opencode_db_rows_1.19.6.json");

    #[test]
    fn reads_and_cleans_up_a_conversation_through_the_cli() {
        let root = ScratchDir::new("opencode-records");
        let data = root.join("data");
        std::fs::create_dir_all(data.join("opencode")).unwrap();
        std::fs::write(data.join("opencode/opencode.db"), "").unwrap();
        let capture = root.join("rows.json");
        std::fs::write(&capture, CAPTURE).unwrap();
        let deleted = root.join("removed");
        let id = "ses_abcdefghijklmnopqrstuvwxyz";
        let mut shell_env = fake_agent(
            &root,
            "opencode",
            &format!(
                r#"
if [ "$1" = --version ]; then
  printf '%s\n' '1.19.6'
elif [ "$1" = db ]; then
  case "$2" in
    'SELECT id FROM session') printf '%s' '[{{"id":"{id}"}}]' ;;
    *{id}*) cat '{}' ;;
    *) printf '[]' ;;
  esac
elif [ "$1 $2 $3" = 'session delete {id}' ]; then
  touch '{}'
else
  exit 1
fi
"#,
                capture.display(),
                deleted.display(),
            ),
        );
        shell_env.insert(
            "XDG_DATA_HOME".to_owned(),
            data.to_string_lossy().into_owned(),
        );
        let account = RowAccount {
            config_dir: None,
            shell_env: &shell_env,
        };

        let store = OpenCodeRecords;
        assert!(store.conversations(&account).unwrap().contains(id));
        let lines = store.lines(&account, id).unwrap();
        assert_eq!(store.entries(&lines[0]), [Entry::User("one prompt".into())]);
        assert_eq!(
            store.entries(&lines[1]),
            [Entry::Assistant("Stand-in reply".into())]
        );
        let mut context = Context::default();
        let mut observed = Observed::default();
        store.fold_line(&lines[1].to_string(), true, &mut context, &mut observed);
        assert_eq!(context.context(&store).flatten().unwrap().tokens, 1290);
        assert_eq!(observed.model.as_deref(), Some("stub/m"));
        assert!(store.remove(&account, id).unwrap());
        assert!(deleted.is_file());
    }
    #[test]
    fn reads_the_v2_cli_exports_through_the_feed() {
        let store = OpenCodeRecords;
        for capture in [
            include_str!("../../../testdata/opencode_session_export_2.3.2.json"),
            include_str!("../../../testdata/opencode_session_export_2.4.1.json"),
        ] {
            let export: Value = serde_json::from_str(capture).unwrap();
            let lines = v2_lines(&export).unwrap();
            assert_eq!(lines.len(), 2);
            assert_eq!(store.is_user_prompt(&lines[0].to_string()), Some(true));
            assert_eq!(store.is_user_prompt(&lines[1].to_string()), Some(false));
            assert_eq!(
                store.entries(&lines[0]),
                [Entry::User("M35 measured prompt".into())]
            );
            assert_eq!(
                store.entries(&lines[1]),
                [Entry::Assistant("M35 stand-in answer".into())]
            );
            assert_eq!(lines[1]["info"]["providerID"], "probe");
            assert_eq!(lines[1]["info"]["modelID"], "probe");
        }
    }
    #[test]
    fn v2_waits_for_legacy_records_before_exporting() {
        let root = ScratchDir::new("opencode-upgrade");
        let data = root.join("data/opencode");
        std::fs::create_dir_all(&data).unwrap();
        let db = rusqlite::Connection::open(data.join("opencode.db")).unwrap();
        db.execute_batch("CREATE TABLE session (id TEXT PRIMARY KEY)")
            .unwrap();
        drop(db);

        let capture = root.join("export.json");
        std::fs::write(
            &capture,
            include_str!("../../../testdata/opencode_session_export_2.4.1.json"),
        )
        .unwrap();
        let polled = root.join("polled");
        let completed = root.join("completed");
        let id = "ses_abcdefghijklmnopqrstuvwxyz";
        let mut shell_env = fake_agent(
            &root,
            "opencode",
            &format!(
                r#"
case "$*" in
  --version) printf '%s\n' 'opencode v2.4.1' ;;
  'api --standalone GET /api/experimental/migration/v1')
    if [ -f '{polled}' ]; then
      touch '{completed}'
      printf '%s' '{{"status":"completed"}}'
    else
      touch '{polled}'
      printf '%s' '{{"status":"running"}}'
    fi ;;
  'session export --standalone {id}') [ -f '{completed}' ] && cat '{capture}' ;;
  *) exit 1 ;;
esac
"#,
                polled = polled.display(),
                completed = completed.display(),
                capture = capture.display(),
            ),
        );
        shell_env.insert(
            "XDG_DATA_HOME".into(),
            root.join("data").display().to_string(),
        );
        shell_env.insert(
            "XDG_CONFIG_HOME".into(),
            root.join("config").display().to_string(),
        );
        shell_env.insert(
            "XDG_STATE_HOME".into(),
            root.join("state").display().to_string(),
        );
        let account = RowAccount {
            config_dir: None,
            shell_env: &shell_env,
        };
        let lines = OpenCodeRecords.lines(&account, id).unwrap();
        assert_eq!(
            OpenCodeRecords.entries(&lines[0]),
            [Entry::User("M35 measured prompt".into())]
        );
    }
}
