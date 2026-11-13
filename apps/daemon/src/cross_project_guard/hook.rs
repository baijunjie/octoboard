//! `octoboardd guard-hook`: reads one `PreToolUse` payload on stdin and, when a file-edit tool
//! call targets a path inside another registered project, answers with a `deny` decision whose
//! reason hands the work over to a session of that project. Otherwise it prints nothing.
//!
//! The guard fails open, on every failure: a list that is missing or unreadable, a payload that is
//! not one, a path that cannot be told. A guard that cannot decide must stay silent, and the git
//! layer catches what it lets through at commit time. It exits 0 whatever it prints, and writes
//! nothing to stderr.
//!
//! Which project a path is in is told by what each path leads to, never by its text: the target is
//! resolved to its nearest existing directory (a file the call is about to create has none of its
//! own) and compared with each root by device and inode ([`Place`]), so letter case, links and
//! firmlinks do not matter. Of the roots that hold the path, the deepest one wins: the session's
//! own project and the shared directories pass, so a project registered at the home directory
//! refuses only what no deeper root of the session's claims. A target in the session's own
//! repository passes as well, wherever that repository's working trees are: a linked worktree
//! made after the launch, or a sibling project of a monorepo, is the session's own.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::list::{GuardList, GuardedProject};
use crate::git::repo_files::repository_of;
use crate::hostfs::place::Place;

/// How much of a payload is read: a write call carries the whole file it writes.
const PAYLOAD_LIMIT: u64 = 32 * 1024 * 1024;

/// The most characters of the reason an agent is sure to show the model: Grok shows about the
/// first 270.
const REASON_LIMIT: usize = 270;

/// Runs the guard over stdin with the list in `list_file`, printing the decision if there is one.
pub fn run(list_file: &Path) {
    let mut payload = Vec::new();
    if std::io::stdin()
        .take(PAYLOAD_LIMIT)
        .read_to_end(&mut payload)
        .is_err()
    {
        return;
    }
    let Some(list) = read_list(list_file) else {
        return;
    };
    if let Some(decision) = decide(&list, &payload) {
        println!("{}", ascii_json(&decision.to_string()));
    }
}

/// `json` with every non-ASCII character written as a `\u` escape. A hook's output can pass
/// through a shell that re-decodes it with the console's code page (PowerShell running the
/// Windows script for Codex), and ASCII survives any code page. Outside strings JSON holds only
/// ASCII, so escaping the whole text escapes only string contents.
fn ascii_json(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for ch in json.chars() {
        if ch.is_ascii() {
            out.push(ch);
        } else {
            let mut units = [0u16; 2];
            for unit in ch.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

fn read_list(file: &Path) -> Option<GuardList> {
    let bytes = crate::hostfs::read_regular(file, 4 * 1024 * 1024).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The decision the agent reads for the payload: `Some` only to deny.
fn decide(list: &GuardList, payload: &[u8]) -> Option<Value> {
    let payload: Value = serde_json::from_slice(payload).ok()?;
    let targets = target_paths(&payload);
    if targets.is_empty() {
        return None;
    }
    let cwd = payload_cwd(&payload);
    let places = Places::of(list);
    let refused = targets
        .into_iter()
        .find_map(|target| places.refuse(&absolute(&target, cwd.as_deref())?))?;
    Some(serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason(&refused, &list.tool),
        }
    }))
}

/// The directory relative targets are resolved against: the payload's `cwd`, and for a Grok
/// payload that lacks it the workspace root Grok gives its hooks in the environment.
fn payload_cwd(payload: &Value) -> Option<PathBuf> {
    match payload.get("cwd").and_then(Value::as_str) {
        Some(cwd) => Some(PathBuf::from(cwd)),
        None => std::env::var_os("GROK_WORKSPACE_ROOT").map(PathBuf::from),
    }
}

/// The paths the tool call writes, as the payload names them. Claude Code's file-edit tools name
/// one in `file_path`, and `NotebookEdit` in `notebook_path`. Grok sends its fields in both
/// cases (`tool_input` and `toolInput`) and names the file in `path` as well, and a notebook in
/// `target_notebook`. Codex's `apply_patch` carries the patch text in `command`, which names every
/// file it touches in its headers.
fn target_paths(payload: &Value) -> Vec<String> {
    let Some(input) = ["tool_input", "toolInput"]
        .into_iter()
        .find_map(|key| payload.get(key))
    else {
        return Vec::new();
    };
    let mut paths: Vec<String> = ["file_path", "notebook_path", "path", "target_notebook"]
        .into_iter()
        .filter_map(|key| input.get(key)?.as_str())
        .map(str::to_owned)
        .collect();
    if let Some(patch) = input.get("command").and_then(Value::as_str) {
        paths.extend(patch_paths(patch));
    }
    paths.retain(|path| !path.is_empty());
    paths
}

/// The files an `apply_patch` patch names in its headers, a move's destination included.
/// `command` is also a shell command's text in other tools; the header prefixes are what tell a
/// patch, and a command that merely contains one has no line starting with it.
fn patch_paths(patch: &str) -> impl Iterator<Item = String> + '_ {
    const HEADERS: [&str; 4] = [
        "*** Update File: ",
        "*** Add File: ",
        "*** Delete File: ",
        "*** Move to: ",
    ];
    patch.lines().filter_map(|line| {
        let line = line.trim_start();
        HEADERS
            .iter()
            .find_map(|header| line.strip_prefix(header))
            .map(|path| path.trim().to_owned())
    })
}

/// `target` resolved against the payload's working directory, written one way but in the case the
/// payload gave: it is what the refusal shows, and the places are compared by what they lead to.
/// An absolute target needs no working directory; a relative one is judged only against an
/// absolute one.
fn absolute(target: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let target = Path::new(target);
    let joined = match cwd {
        _ if target.is_absolute() => target.to_path_buf(),
        Some(cwd) if cwd.is_absolute() => cwd.join(target),
        _ => return None,
    };
    Some(crate::hostfs::lexically_normalise(&joined))
}

/// The roots of a list as places. A root that cannot be resolved (a directory that is gone) holds
/// nothing.
struct Places<'a> {
    own: Vec<Place>,
    /// The common directory of the session's own repository.
    repository: Option<PathBuf>,
    shared: Vec<Place>,
    projects: Vec<(&'a GuardedProject, Place)>,
}

/// A refused path and the project it is in, with the project's root, for showing the path short.
struct Refused<'a> {
    path: PathBuf,
    project: &'a GuardedProject,
}

impl<'a> Places<'a> {
    fn of(list: &'a GuardList) -> Self {
        let places = |roots: &[String]| -> Vec<Place> {
            roots
                .iter()
                .filter_map(|root| Place::of(Path::new(root)))
                .collect()
        };
        Self {
            own: places(&list.own),
            repository: list.repository.as_deref().map(PathBuf::from),
            shared: places(&list.shared),
            projects: list
                .projects
                .iter()
                .filter_map(|project| Some((project, Place::of(Path::new(&project.root))?)))
                .collect(),
        }
    }

    /// `path` with the project it is in, when it is in another project and no root of the
    /// session's own or shared directory lies at or below that project's root and holds it too,
    /// and it is not in the session's own repository.
    fn refuse(&self, path: &Path) -> Option<Refused<'a>> {
        let (target, directory) = place_of_target(path)?;
        let mut holding = self
            .projects
            .iter()
            .filter(|(_, place)| target.lies_in(place));
        let mut deepest = holding.next()?;
        for candidate in holding {
            if candidate.1.lies_in(&deepest.1) {
                deepest = candidate;
            }
        }
        if self.repository.is_some() && repository_of(&directory) == self.repository {
            return None;
        }
        let passes = self
            .own
            .iter()
            .chain(&self.shared)
            .any(|root| target.lies_in(root) && root.lies_in(&deepest.1));
        (!passes).then(|| Refused {
            path: path.to_path_buf(),
            project: deepest.0,
        })
    }
}

/// The place the call's target is at: its nearest existing directory above it, with a link the
/// target itself is followed first, and that directory's real path. `None` when nothing above it
/// can be resolved.
fn place_of_target(path: &Path) -> Option<(Place, PathBuf)> {
    let resolved = crate::hostfs::canonical(path).unwrap_or_else(|_| path.to_path_buf());
    resolved.ancestors().skip(1).find_map(Place::resolved)
}

/// The refusal, kept within [`REASON_LIMIT`] by shortening the path alone: a long one is shown
/// relative to the project's root first, and then cut from its start. A project or tool name long
/// enough to overflow by itself is not cut, so the end of the hand-over can still fall past what
/// Grok shows.
fn reason(refused: &Refused<'_>, tool: &str) -> String {
    let name = &refused.project.name;
    let build = |path: &str| {
        format!(
            "Blocked: {path} is in project \"{name}\", not this session's. Don't write it \
             another way. Keep any changes there as they are and call {tool} for a \"{name}\" \
             session, listing the changed files and where they are, so it reviews them and \
             takes over."
        )
    };
    let full = refused.path.to_string_lossy();
    let text = build(&full);
    if text.chars().count() <= REASON_LIMIT {
        return text;
    }
    // The root is told by its key, since the path is in the case the payload gave; what is shown
    // is the path's own text below as many components as the root has.
    let root = Path::new(&refused.project.root);
    let relative = crate::hostfs::lexically_within(&refused.path, root).then(|| {
        refused
            .path
            .components()
            .skip(
                crate::hostfs::lexically_normalise(root)
                    .components()
                    .count(),
            )
            .collect::<PathBuf>()
            .to_string_lossy()
            .into_owned()
    });
    let shown = relative.as_deref().unwrap_or(&full);
    let text = build(shown);
    let over = text.chars().count().saturating_sub(REASON_LIMIT);
    if over == 0 {
        return text;
    }
    // The path's own tail is what names the file; the start is what is given up, and an ellipsis
    // stands for it.
    let keep = shown.chars().count().saturating_sub(over + 1).max(1);
    let tail: String = shown.chars().skip(shown.chars().count() - keep).collect();
    build(&format!("\u{2026}{tail}"))
}

#[cfg(test)]
mod tests;
