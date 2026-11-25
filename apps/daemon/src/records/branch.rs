//! The current branch of a record that is a tree: one file in which each line names the line it
//! follows, so that going back to an earlier point and carrying on from there leaves the abandoned
//! lines in the file beside the new ones. The conversation as it stands is then the line written
//! last and its ancestry, not the file read top to bottom. How one provider's lines name
//! themselves and what they follow is its store's ([`Branching`], handed out by
//! `RecordStore::branching`); what a tree comes to is here, the same for every such provider.
//!
//! A line that names nothing (a header, say) belongs to every tree branch. [`Steps`] tracks the
//! other way a record can fork: appending a suffix at an earlier ordinal.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use serde_json::Value;

use crate::records::conversation::for_each_line;
use crate::records::line_file;
use crate::records::Stretch;

/// What one line of a tree-shaped record names: itself, and the line it follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub id: String,
    /// `None` for a line that starts the conversation over, following nothing.
    pub parent: Option<String>,
}

/// How the lines of one provider's tree-shaped record link (`RecordStore::branching`).
pub trait Branching: Send + Sync {
    /// What `line` names, or `None` for a line outside the tree, which every branch keeps.
    fn link(&self, line: &Value) -> Option<Link>;
}

/// Where a record read line by line stands in its tree: the line read last that names itself.
/// The conversation as it stands is that line's ancestry.
#[derive(Debug, Default)]
pub struct Leaf(Option<String>);

impl Leaf {
    /// Takes the next line's `link` and says whether the line carries on from where the record
    /// stood. A line that does not (it follows an earlier line, or nothing) leaves lines read
    /// before it off the conversation: whoever folded them reads the record again from its start,
    /// through [`on_branch`].
    pub fn follow(&mut self, link: Option<Link>) -> bool {
        let Some(link) = link else {
            return true;
        };
        let carries_on = link.parent == self.0;
        self.0 = Some(link.id);
        carries_on
    }
}

pub type StepIndex = fn(&Value) -> Option<u64>;

/// The output positions before the steps on the current branch.
#[derive(Default)]
pub struct Steps(BTreeMap<u64, usize>);

impl Steps {
    /// A write at an earlier step replaces that step and everything after it. Returns the
    /// output length to restore before reading the replacement.
    pub fn follow(&mut self, step: u64, output_len: usize) -> Option<usize> {
        let cut = self.0.range(step..).next().map(|(_, &position)| position);
        if cut.is_some() {
            drop(self.0.split_off(&step));
        }
        self.0.insert(step, cut.unwrap_or(output_len));
        cut
    }
}

/// Of the lines whose links are `links`, in order, which are on the current branch: the last line
/// that names itself, the lines it descends from, and every line that names nothing. A parent
/// that no line names ends the ancestry there, and so does one already walked (a record whose
/// lines loop back on themselves). Of two lines of one name, the first is the one a child follows.
pub fn on_branch(links: &[Option<Link>]) -> Vec<bool> {
    let mut first: HashMap<&str, usize> = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        if let Some(link) = link {
            first.entry(link.id.as_str()).or_insert(index);
        }
    }
    let mut kept: Vec<bool> = links.iter().map(Option::is_none).collect();
    let mut walked: HashSet<usize> = HashSet::new();
    let mut at = links.iter().rposition(Option::is_some);
    while let Some(index) = at {
        if !walked.insert(index) {
            break;
        }
        kept[index] = true;
        at = links[index]
            .as_ref()
            .and_then(|link| link.parent.as_deref())
            .and_then(|parent| first.get(parent).copied());
    }
    kept
}

/// Calls `visit` with each line of `stretches` that parses and is on the conversation's current
/// branch, in order: every line for a record that is a list (`branching` is `None`), as
/// `for_each_line` reads it. A tree is read twice, once for its links alone, so no more than one
/// line is held at a time; a line appended between the two reads is left for the next reading.
pub fn for_each_line_on_branch(
    branching: Option<&dyn Branching>,
    stretches: &[Stretch],
    mut visit: impl FnMut(&Value),
) -> std::io::Result<()> {
    let Some(branching) = branching else {
        return for_each_line(stretches, visit);
    };
    let mut links = Vec::new();
    for_each_line(stretches, |line| links.push(branching.link(line)))?;
    let kept = on_branch(&links);
    let mut index = 0;
    for_each_line(stretches, |line| {
        if kept.get(index).copied().unwrap_or(false) {
            visit(line);
        }
        index += 1;
    })
}

/// Calls `visit` with each of `rows`, the lines of a conversation kept as rows
/// (`crate::records::rows`), that is on the conversation's current branch, in order, with its
/// place among `rows`: every row when `branching` is `None`.
pub fn for_each_row_on_branch(
    branching: Option<&dyn Branching>,
    rows: &[Value],
    mut visit: impl FnMut(usize, &Value),
) {
    let kept = branching.map(|branching| {
        let links: Vec<Option<Link>> = rows.iter().map(|row| branching.link(row)).collect();
        on_branch(&links)
    });
    for (index, row) in rows.iter().enumerate() {
        let on = kept
            .as_ref()
            .is_none_or(|kept| kept.get(index).copied().unwrap_or(false));
        if on {
            visit(index, row);
        }
    }
}

/// The lines of the first `end` bytes of the file at `path` that are on the current branch of
/// the tree those bytes hold, each handed to `visit` with its starting byte offset, as text: for
/// a reader that follows a growing record by offset and has to read it again from its start once
/// a line forks off ([`Leaf::follow`]). A line that does not parse names nothing, and is kept.
pub fn read_branch_to(
    branching: &dyn Branching,
    path: &Path,
    end: u64,
    mut visit: impl FnMut(u64, &str),
) -> std::io::Result<()> {
    let link_of = |line: &str| {
        serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|line| branching.link(&line))
    };
    let mut links = Vec::new();
    line_file::read_range(path, 0, end, |_, line| links.push(link_of(line)))?;
    let kept = on_branch(&links);
    let mut index = 0;
    line_file::read_range(path, 0, end, |at, line| {
        if kept.get(index).copied().unwrap_or(false) {
            visit(at, line);
        }
        index += 1;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(id: &str, parent: Option<&str>) -> Option<Link> {
        Some(Link {
            id: id.to_owned(),
            parent: parent.map(str::to_owned),
        })
    }

    /// The current branch is the last linked line and its ancestry; an abandoned line is off it,
    /// and a line that names nothing is on every branch.
    #[test]
    fn the_branch_is_the_last_line_and_its_ancestry() {
        let links = [
            None,
            link("a", None),
            link("b", Some("a")),
            link("c", Some("b")),
            None,
            link("d", Some("a")),
            link("e", Some("d")),
        ];
        assert_eq!(
            on_branch(&links),
            [true, true, false, false, true, true, true]
        );
        // A list in which each line follows the one before it is kept whole.
        let list = [link("a", None), link("b", Some("a")), link("c", Some("b"))];
        assert_eq!(on_branch(&list), [true, true, true]);
        assert_eq!(on_branch(&[]), Vec::<bool>::new());
        assert_eq!(on_branch(&[None, None]), [true, true]);
    }

    /// A line that follows nothing starts the conversation over; a parent no line names ends the
    /// ancestry; a record whose lines loop is walked once; of two lines of one name the first is
    /// the parent.
    #[test]
    fn a_new_root_an_unknown_parent_and_a_loop_end_the_ancestry() {
        let rerooted = [link("a", None), link("b", Some("a")), link("c", None)];
        assert_eq!(on_branch(&rerooted), [false, false, true]);

        let orphan = [link("a", None), link("b", Some("gone"))];
        assert_eq!(on_branch(&orphan), [false, true]);

        let looping = [link("a", Some("b")), link("b", Some("a"))];
        assert_eq!(on_branch(&looping), [true, true]);

        let twice = [
            link("a", None),
            link("x", Some("a")),
            link("a", Some("x")),
            link("b", Some("a")),
        ];
        assert_eq!(on_branch(&twice), [true, false, false, true]);
    }

    /// A tree in a file is read along its current branch, by parsed line and by byte offset; with
    /// no links to go by, the same file is read whole.
    #[test]
    fn a_file_is_read_along_its_current_branch() {
        use crate::records::fixtures::{test_record, CLAUDE_ID, TREE_BRANCH, TREE_TRUNK};
        let _declared = crate::providers::test_provider::with_tree_records();
        let branching = crate::adapter::provider(crate::protocol::AgentId::TEST)
            .records()
            .branching();
        assert!(branching.is_some());
        let dir = crate::test_support::ScratchDir::new("agent-records-branch");
        let lines = format!("{TREE_TRUNK}{TREE_BRANCH}");
        let record = test_record(&dir.join("account"), CLAUDE_ID, &lines);
        let stretches = [Stretch {
            path: record.clone(),
            cut: None,
        }];
        let ids = |branching: Option<&dyn Branching>| {
            let mut ids = Vec::new();
            for_each_line_on_branch(branching, &stretches, |line| {
                ids.push(line["id"].as_str().unwrap_or("-").to_owned());
            })
            .unwrap();
            ids
        };
        assert_eq!(ids(branching), ["-", "a", "b", "e", "f"]);
        assert_eq!(ids(None), ["-", "a", "b", "c", "d", "e", "f"]);

        // By offset: each kept line with where it starts, and nothing past the end given.
        let mut read = Vec::new();
        read_branch_to(
            branching.unwrap(),
            &record,
            TREE_TRUNK.len() as u64,
            |at, line| read.push((at, line.to_owned())),
        )
        .unwrap();
        let trunk: Vec<&str> = TREE_TRUNK.split_inclusive('\n').collect();
        let mut expected = Vec::new();
        let mut at = 0;
        for line in &trunk {
            expected.push((at, (*line).to_owned()));
            at += line.len() as u64;
        }
        assert_eq!(read, expected, "the trunk alone has no abandoned line");
    }

    /// Rows are held to the same branch as a file's lines: the abandoned ones are left out, each
    /// kept one comes with its place, and with no links to go by every row is kept.
    #[test]
    fn rows_are_read_along_their_current_branch() {
        use crate::records::fixtures::{TREE_BRANCH, TREE_TRUNK};
        let _declared = crate::providers::test_provider::with_tree_records();
        let branching = crate::adapter::provider(crate::protocol::AgentId::TEST)
            .records()
            .branching();
        let rows: Vec<Value> = format!("{TREE_TRUNK}{TREE_BRANCH}")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let read = |branching: Option<&dyn Branching>| {
            let mut read = Vec::new();
            for_each_row_on_branch(branching, &rows, |index, row| {
                read.push((index, row["id"].as_str().unwrap_or("-").to_owned()));
            });
            read
        };
        let named = |pairs: &[(usize, &str)]| -> Vec<(usize, String)> {
            pairs
                .iter()
                .map(|(index, id)| (*index, (*id).to_owned()))
                .collect()
        };
        assert_eq!(
            read(branching),
            named(&[(0, "-"), (1, "a"), (2, "b"), (5, "e"), (6, "f")])
        );
        assert_eq!(read(None).len(), 7);
    }

    /// A leaf carries on while each line follows the one before it, and says so when one does
    /// not: a line following an earlier one, or nothing once the record has begun.
    #[test]
    fn a_leaf_says_when_a_line_forks_off() {
        let mut leaf = Leaf::default();
        assert!(leaf.follow(None));
        assert!(leaf.follow(link("a", None)));
        assert!(leaf.follow(link("b", Some("a"))));
        assert!(leaf.follow(None));
        assert!(leaf.follow(link("c", Some("b"))));
        assert!(!leaf.follow(link("d", Some("a"))));
        assert!(leaf.follow(link("e", Some("d"))));
        assert!(!leaf.follow(link("f", None)));
    }
}
