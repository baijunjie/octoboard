//! Holds the daemon's top-level modules to their dependency direction by reading the crate's own
//! source. Every `crate::` path in production code, in a `use` or inline, is an edge from the
//! top-level module the file sits in to the one the path names; an edge may point only to the
//! module's own layer or one below it, unless `UPWARD` lists it. The layers, and why each upward
//! edge is there, are in `docs/structure/daemon.md`, section "Dependency direction", which keeps
//! its table in step with `LAYERS` and `UPWARD` here.
//!
//! Only production code counts: a file reached only through a `#[cfg(test)]` module, a file under
//! `#![cfg(test)]`, and any item, variant, match arm, field or statement under a `cfg` predicate
//! that holds only in tests (`cfg(test)`, `cfg(all(test, …))`, `cfg(any(test, all(test, …)))`) are
//! left out, since tests reach across modules to build their fixtures. The scan (`scan`) is
//! lexical, not the compiler's: comments (doc links included) and the contents of string and
//! character literals are blanked before paths are read; a name a `use` brings in from another
//! module (`use crate::mcp;`, `use crate::session::term;`) is followed through the file wherever it
//! starts a path, without regard to the block the `use` sits in, but a child module's
//! `use super::*` that inherits such an import from its parent is not followed; a glob import is
//! seen only as the import of its module; and nothing a macro expands to is seen beyond the tokens
//! written at its call. Methods and fields reached through a value (`state.store.get_session(…)`)
//! are not paths and are not seen at all, so an entry for a type admits everything the type
//! exposes. A test-only field of a tuple struct or tuple variant whose type holds a comma inside
//! angle brackets (`HashMap<K, V>`) is cut short at that comma, so the rest of its type is read as
//! production code: this can only add an edge, never hide one.

mod scan;

use std::collections::{BTreeMap, BTreeSet};

use scan::{scan_crate, Scan};

const RULE_DOC: &str = "docs/structure/daemon.md, section \"Dependency direction\"";
const THIS_FILE: &str = "apps/daemon/src/layering_tests/mod.rs";

/// The layers, lowest first. A module may use its own layer and the layers below it.
const LAYERS: &[(&str, &[&str])] = &[
    (
        "foundation",
        &[
            "blocking_pass",
            "interface_language",
            "loopback",
            "text",
            "time",
            "wire_path",
        ],
    ),
    ("providers", &["providers"]),
    ("wire", &["protocol"]),
    (
        "host",
        &[
            "env_shell",
            "git",
            "host_open",
            "hostfs",
            "instance_lock",
            "log_file",
            "paths",
            "processes",
        ],
    ),
    ("storage", &["git_workflow", "store"]),
    ("state", &["state"]),
    (
        "workspace",
        &[
            "branch_cleanup",
            "branch_origin",
            "browse",
            "host_tools",
            "path_links",
            "project_git",
            "removal_safety",
            "revert_gate",
            "terminal",
        ],
    ),
    (
        "agent sessions",
        &[
            "accounts",
            "adapter",
            "agents",
            "handoff",
            "hooks",
            "messaging",
            "records",
            "session",
            "session_git_hooks",
            "session_tools",
            "titles",
        ],
    ),
    (
        "control",
        &["config_document", "coordinator", "mcp", "session_request"],
    ),
    ("entry", &["cli", "server"]),
];

/// Items of the crate root (`main.rs`) any module may name.
const ROOT_ITEMS: &[&str] = &["APP_NAME"];

/// `AppState` holds the live state of every module, so `state` may name the types of any module
/// outside the entry layer, and their associated items (`Outbox::new`), whatever the layer;
/// anything else it reaches upward goes through `UPWARD`.
const STATE: &str = "state";

/// The layer of the ways into the daemon, which nothing the daemon holds may name.
const ENTRY: &str = "entry";

/// An edge allowed to point to a higher layer.
struct Upward {
    from: &'static str,
    to: &'static str,
    /// What it covers of the paths into the target module: those starting with one of these,
    /// given after the target module.
    items: &'static [&'static str],
    /// Why it points upward; the table in the rule's doc carries the same reason.
    reason: &'static str,
}

const UPWARD: &[Upward] = &[
    Upward {
        from: "state",
        to: "accounts",
        items: &[
            "shared_config::in_use",
            "shared_config::runs_under",
            "shared_config::sharing_in_progress",
        ],
        reason: "the sharing locks and claims AppState holds decide what an account's sharing \
                 change covers by shared_config's own rules, and refuse with its error",
    },
    Upward {
        from: "state",
        to: "handoff",
        items: &["package_file::remove_in"],
        reason: "forgetting a session removes its handoff package; remove_in is std-only file work",
    },
    Upward {
        from: "state",
        to: "project_git",
        items: &["header::apply_header"],
        reason: "the git status AppState holds takes a fresh header the way project_git reads it",
    },
    Upward {
        from: "state",
        to: "titles",
        items: &["project_names", "resolve"],
        reason: "a session's title is resolved where AppState publishes it",
    },
    Upward {
        from: "messaging",
        to: "coordinator",
        items: &[
            "lifecycle::archive_finished",
            "lifecycle::Finish",
            "lifecycle::Finished",
            "lifecycle::Unfinished",
            "lifecycle::unfinished",
        ],
        reason: "messaging::reporting archives a session that reported it is done, and checks \
                 beforehand that it may be, through the coordinator's archive of a finished \
                 session",
    },
];

#[test]
fn every_top_level_module_depends_only_downward_or_as_listed() {
    let scan = scan_crate();
    let mut problems = Vec::new();
    let layer_of = check_layers(&scan, &mut problems);
    let usage = check_edges(&scan, &layer_of, &mut problems);
    check_stale_entries(&layer_of, &usage, &mut problems);
    assert!(
        problems.is_empty(),
        "the daemon's top-level modules break their dependency direction:\n  {}\n\nA module may \
         use only its own layer and the layers below it ({RULE_DOC}). Turn the dependency \
         around, or move what it needs to a lower layer; an edge that has to point upward goes \
         into `UPWARD` in {THIS_FILE}, with its reason, and into that section's table.",
        problems.join("\n  ")
    );
}

/// Every production module in exactly one layer, and every layered module a production one.
/// Returns each layered module's layer.
fn check_layers(scan: &Scan, problems: &mut Vec<String>) -> BTreeMap<&'static str, usize> {
    let mut layer_of = BTreeMap::new();
    for (index, (layer, modules)) in LAYERS.iter().enumerate() {
        for module in *modules {
            if let Some(earlier) = layer_of.insert(*module, index) {
                problems.push(format!(
                    "`{module}` is in two layers, `{}` and `{layer}`",
                    LAYERS[earlier].0
                ));
            }
            if !scan.modules.contains(*module) {
                problems.push(format!(
                    "layer `{layer}` names `{module}`, which main.rs does not declare as a module"
                ));
            }
        }
    }
    for module in &scan.modules {
        if !layer_of.contains_key(module.as_str()) {
            problems.push(format!(
                "`{module}` is in no layer; add it to `LAYERS` in {THIS_FILE} and to the layer \
                 table in {RULE_DOC}"
            ));
        }
    }
    layer_of
}

/// Which `UPWARD` entries, and which of their items, an edge relied on.
struct Usage {
    entries: Vec<bool>,
    items: BTreeSet<(usize, &'static str)>,
}

/// Every edge downward, within its layer, or allowed: by `ROOT_ITEMS`, by the `state` exception,
/// or by an `UPWARD` entry.
fn check_edges(
    scan: &Scan,
    layer_of: &BTreeMap<&'static str, usize>,
    problems: &mut Vec<String>,
) -> Usage {
    let mut usage = Usage {
        entries: vec![false; UPWARD.len()],
        items: BTreeSet::new(),
    };
    for edge in &scan.edges {
        if edge.from == edge.to {
            continue;
        }
        let here = format!("apps/daemon/src/{}:{}", edge.file, edge.line);
        let named = if edge.path.is_empty() {
            format!("crate::{}", edge.to)
        } else {
            format!("crate::{}::{}", edge.to, edge.path.join("::"))
        };
        if !scan.modules.contains(&edge.to) {
            if !ROOT_ITEMS.contains(&edge.to.as_str()) {
                problems.push(format!(
                    "{here}: `{named}` is an item of main.rs that is not in `ROOT_ITEMS`"
                ));
            }
            continue;
        }
        let (Some(&from), Some(&to)) = (
            layer_of.get(edge.from.as_str()),
            layer_of.get(edge.to.as_str()),
        ) else {
            continue;
        };
        if to <= from || (edge.from == STATE && LAYERS[to].0 != ENTRY && names_a_type(&edge.path)) {
            continue;
        }
        let allowed = UPWARD.iter().enumerate().find_map(|(index, upward)| {
            if upward.from != edge.from || upward.to != edge.to {
                return None;
            }
            upward
                .items
                .iter()
                .find(|item| starts_with(&edge.path, item))
                .map(|item| (index, *item))
        });
        match allowed {
            Some((index, item)) => {
                usage.entries[index] = true;
                usage.items.insert((index, item));
            }
            None => problems.push(format!(
                "{here}: `{}` (layer `{}`) uses `{named}`, in the higher layer `{}`",
                edge.from, LAYERS[from].0, LAYERS[to].0
            )),
        }
    }
    usage
}

/// Every `UPWARD` entry an upward edge between layered modules, and every entry and item still
/// relied on, so the list shrinks as the code does.
fn check_stale_entries(
    layer_of: &BTreeMap<&'static str, usize>,
    usage: &Usage,
    problems: &mut Vec<String>,
) {
    for (index, upward) in UPWARD.iter().enumerate() {
        let name = format!("`{} -> {}` ({})", upward.from, upward.to, upward.reason);
        match (layer_of.get(upward.from), layer_of.get(upward.to)) {
            (Some(from), Some(to)) if to > from => {}
            _ => {
                problems.push(format!(
                    "`UPWARD` lists {name}, which is not an upward edge between layered modules"
                ));
                continue;
            }
        }
        if !usage.entries[index] {
            problems.push(format!(
                "`UPWARD` lists {name}, which no production code uses any more; remove it here \
                 and from {RULE_DOC}"
            ));
        }
        for item in upward.items {
            if !usage.items.contains(&(index, *item)) {
                problems.push(format!(
                    "`UPWARD` lists {name} for `{item}`, which no production code uses any \
                     more; remove it here and from {RULE_DOC}"
                ));
            }
        }
    }
}

/// Whether `path` is a module path ending in a type (`lane::Gate`), or going on to a single item
/// of that type (`lane::Gate::with_budget`).
fn names_a_type(path: &[String]) -> bool {
    let is_type = |segment: &String| {
        segment.starts_with(|c: char| c.is_ascii_uppercase())
            && segment.contains(|c: char| c.is_ascii_lowercase())
    };
    let Some(position) = path.iter().position(is_type) else {
        return false;
    };
    let after = path.len() - position - 1;
    path[..position]
        .iter()
        .all(|s| s.starts_with(|c: char| c.is_ascii_lowercase()))
        && after <= 1
}

fn starts_with(path: &[String], item: &str) -> bool {
    let item: Vec<&str> = item.split("::").collect();
    path.len() >= item.len() && path.iter().zip(&item).all(|(a, b)| a == b)
}
