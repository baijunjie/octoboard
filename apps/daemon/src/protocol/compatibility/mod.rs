//! The compatibility guard: the schema of `contracts/protocol/` may only grow since the last
//! release, unless the protocol generation was raised. See "Versions and compatibility" in
//! `apps/daemon/PROTOCOL.md`.
//!
//! `contracts/protocol/released/` is the snapshot: `release.json` records the version of the last
//! release and the protocol generation it carried, and, once a release has carried the schema, that
//! release's four schema files sit beside it. The release script refreshes both. The guard reads
//! the record against the current `GENERATION`:
//! - a lower generation than the record's fails: a generation is never lowered;
//! - a higher one passes without comparing, since raising it is how a break is made;
//! - the same one compares each current file with its snapshot, and fails when the record names
//!   this generation but a snapshot file is missing. A release that predates the schema is a record
//!   of an older generation with no files, so a generation no release has carried yet may still
//!   change in place.
//!
//! The comparison reads each node of a schema as the set of values it accepts, whichever way the
//! schema spells it: `$ref`, `allOf` and sibling keywords are folded together, the two forms of a
//! unit enum are one, and `null` is a kind of value whether it is a `type`, an `anyOf` member or an
//! `enum` entry. A tagged `oneOf` is matched by its tag's `const`, never by position, and an
//! untagged union by alternative, as the direction asks below. What a client built against the
//! snapshot relies on differs by direction:
//! - **Inputs** may only accept more: no removed property or variant, no newly required property,
//!   no lost kind of value, narrowed integer range or shrunk enumeration. Each old alternative has
//!   to be kept by some new one.
//! - **Outputs** may only give more: no removed property or variant, no required property that may
//!   now be left out, no integer range that widens, and the kinds of value a property takes are
//!   the same (a lost type and a new kind are both breaks). Enumerations are not compared: a
//!   client takes a value it does not know as the enum's fallback, so they may grow, and losing
//!   one breaks nobody. Added events, variants and properties pass. The daemon may send any new
//!   alternative a client takes for an old variant, so each new alternative that carries the whole
//!   tag of an old one (every new one, for an untagged old one) has to keep one of them; an old
//!   variant no new alternative carries is removed.
//!
//! A description, `$comment` or title is never compared. A keyword the guard does not read fails
//! the comparison rather than being skipped, so that it is taught when the schema first uses it.

mod compare;
mod norm;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;

use super::schema::Direction;
use super::GENERATION;
use compare::compare;

/// Where the snapshot is committed, beside the files it is a snapshot of.
const RELEASED: &str = "../../contracts/protocol/released";
const RECORD_FILE: &str = "release.json";
const FILE_SUFFIX: &str = ".schema.json";

/// Ends every failure: what to do about it.
const WAY_OUT: &str =
    "make the change additive, or raise `GENERATION` in apps/daemon/src/protocol/version.rs";

/// `release.json`: the last release, and the protocol generation it carried.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: String,
    generation: u32,
}

struct Released {
    record: Record,
    /// The snapshot's schema files by name.
    files: BTreeMap<String, Value>,
}

fn read_released() -> Released {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(RELEASED);
    let record = std::fs::read_to_string(directory.join(RECORD_FILE))
        .unwrap_or_else(|error| panic!("contracts/protocol/released/{RECORD_FILE}: {error}"));
    let record: Record = serde_json::from_str(&record)
        .unwrap_or_else(|error| panic!("contracts/protocol/released/{RECORD_FILE}: {error}"));
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(&directory).unwrap().flatten() {
        let name = entry.file_name().into_string().unwrap();
        if entry.file_type().unwrap().is_file() && name.ends_with(FILE_SUFFIX) {
            let text = std::fs::read_to_string(entry.path()).unwrap();
            let schema = serde_json::from_str(&text)
                .unwrap_or_else(|error| panic!("contracts/protocol/released/{name}: {error}"));
            files.insert(name, schema);
        }
    }
    Released { record, files }
}

/// The failures of the current files (name, direction, schema) against the snapshot, at the
/// current `generation`; empty when the protocol only grew or its generation was raised.
fn check(
    generation: u32,
    current: &[(&str, Direction, Value)],
    released: &Released,
) -> Vec<String> {
    let Record {
        version,
        generation: released_generation,
    } = &released.record;
    if generation < *released_generation {
        return vec![format!(
            "the protocol generation is {generation}, below {released_generation}, which \
             release {version} carried: a generation is never lowered"
        )];
    }
    if generation > *released_generation {
        return Vec::new();
    }
    // A record that names this generation without its files is a snapshot that was not committed
    // whole; a file only the current schema has is an addition.
    if released.files.is_empty() {
        return vec![format!(
            "contracts/protocol/released/ holds no schema files, though {RECORD_FILE} says release \
             {version} carried generation {generation}: restore the snapshot the release script \
             wrote, or raise `GENERATION` in apps/daemon/src/protocol/version.rs if the generation \
             was not released with the schema"
        )];
    }
    let mut failures = Vec::new();
    for (file, direction, schema) in current {
        if let Some(snapshot) = released.files.get(*file) {
            failures.extend(compare(file, *direction, snapshot, schema));
        }
    }
    // A snapshot file the current schema no longer renders is a channel or direction removed.
    for file in released.files.keys() {
        if !current.iter().any(|(name, _, _)| name == file) {
            failures.push(format!(
                "{file}: the file is removed, though release {version} carried it; {WAY_OUT}"
            ));
        }
    }
    failures
}

#[test]
fn the_schema_only_grows_since_the_last_release() {
    let failures = check(
        GENERATION,
        &super::schema::rendered_files(),
        &read_released(),
    );
    assert!(
        failures.is_empty(),
        "the protocol schema changed in a way a client of the last release cannot take:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_snapshot_is_a_record_and_the_files_of_a_release_that_carried_them() {
    let released = read_released();
    assert!(!released.record.version.is_empty());
    // A snapshot of the schema is all four files of the generation it states, or none of them.
    if released.record.generation == GENERATION {
        let names: Vec<&str> = released.files.keys().map(String::as_str).collect();
        let current: Vec<&str> = super::schema::rendered_files()
            .iter()
            .map(|(file, _, _)| *file)
            .collect();
        assert_eq!(names, current);
    }
}

#[test]
fn the_current_schema_holds_against_itself() {
    let current = super::schema::rendered_files();
    let released = Released {
        record: Record {
            version: "current".to_string(),
            generation: GENERATION,
        },
        files: current
            .iter()
            .map(|(file, _, schema)| (file.to_string(), schema.clone()))
            .collect(),
    };
    assert_eq!(check(GENERATION, &current, &released), Vec::<String>::new());
}
