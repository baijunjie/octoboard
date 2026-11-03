//! The device and inode that tell files and directories apart whatever their paths look like.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::lexically_normalise;

/// A file or directory told apart by what it is on the file system: its device and inode.
pub type Identity = (u64, u64);

/// A set of directories, told apart by what they are on the file system and not by how their
/// paths are written, so a symbolic link, a case-only respelling on a case-insensitive file
/// system and the plain path all name one member. A directory is its device and inode, which
/// settles both without assuming how a given file system folds case (`canonicalize` does return
/// the on-disk spelling on APFS, but that is the file system's doing, not a promise). A path with
/// nothing at it, or that cannot be read, has no identity and is told apart lexically
/// ([`lexically_normalise`]) instead; every member is also kept by its text. Blocking: every call
/// reads the file system.
#[derive(Default)]
pub struct DirectoryIdentities {
    inodes: HashSet<Identity>,
    lexical: HashSet<PathBuf>,
}

impl DirectoryIdentities {
    pub fn from_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        let mut identities = Self::default();
        for path in paths {
            identities.insert(Path::new(path));
        }
        identities
    }

    /// Only the text of each path, with no file system read: what a set built on a thread that
    /// outran its deadline falls back to.
    pub fn from_text<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            inodes: HashSet::new(),
            lexical: paths
                .into_iter()
                .map(|path| lexically_normalise(Path::new(path)))
                .collect(),
        }
    }

    pub fn insert(&mut self, path: &Path) {
        self.insert_resolved(&Resolved::of(path));
    }

    /// [`Self::insert`] for a path already looked at: reads nothing.
    pub fn insert_resolved(&mut self, candidate: &Resolved) {
        self.lexical.insert(lexically_normalise(&candidate.path));
        if let Some(inode) = candidate.inode {
            self.inodes.insert(inode);
        }
    }

    /// Whether `path` names a member: the same text once normalised, or the same directory on
    /// disk. The text counts even where `path` cannot be read, so a byte-identical path always
    /// matches.
    #[cfg(test)]
    pub fn contains(&self, path: &Path) -> bool {
        self.contains_resolved(&Resolved::of(path))
    }

    /// What `contains` answers, for a path already looked at: reads nothing.
    pub fn contains_resolved(&self, candidate: &Resolved) -> bool {
        self.lexical.contains(&lexically_normalise(&candidate.path))
            || candidate
                .inode
                .is_some_and(|inode| self.inodes.contains(&inode))
    }
}

/// A path with its identity read ahead of time, so a caller on an async worker can look at a
/// set of directories on a blocking thread once and then ask [`DirectoryIdentities`] about them
/// without touching the file system again. Blocking to make: [`Self::of`] reads the file system.
pub struct Resolved {
    pub path: PathBuf,
    inode: Option<Identity>,
}

impl Resolved {
    pub fn of(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let inode = of_path(&path);
        Self { path, inode }
    }
}

/// The device and inode `metadata` gives a file, as they are: the inode is 0 where the file system
/// reports none, which the caller decides what to make of.
pub fn raw(metadata: &std::fs::Metadata) -> Identity {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino())
}

/// The device and inode `metadata` gives, or `None` where the file system reports no inode (0), so
/// there is nothing to tell the file apart by.
pub fn known(metadata: &std::fs::Metadata) -> Option<Identity> {
    let identity = raw(metadata);
    (identity.1 != 0).then_some(identity)
}

/// The device and inode of what `path` leads to, following links; `None` where it cannot be read
/// or the file system reports no inode (0).
fn of_path(path: &Path) -> Option<Identity> {
    known(&std::fs::metadata(path).ok()?)
}

/// The device and inode of the directory `path` leads to, following links; `None` where it cannot
/// be read, is not a directory or the file system reports no inode (0).
pub fn of_directory(path: &Path) -> Option<Identity> {
    directory(&std::fs::metadata(path).ok()?)
}

/// [`known`] for a directory; `None` for anything else.
pub(crate) fn directory(metadata: &std::fs::Metadata) -> Option<Identity> {
    if metadata.is_dir() {
        known(metadata)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::DirectoryIdentities;
    use crate::test_support::ScratchDir;
    use std::path::Path;

    /// A symbolic link to a directory is the directory; another directory and a path with nothing
    /// at it are not.
    #[test]
    fn a_link_to_a_directory_is_that_directory() {
        let dir = ScratchDir::new("hostfs-identities-link");
        let real = dir.join("real");
        std::fs::create_dir(&real).expect("real");
        std::os::unix::fs::symlink(&real, dir.join("link")).expect("link");
        std::fs::create_dir(dir.join("other")).expect("other");

        let identities = DirectoryIdentities::from_paths([real.to_str().unwrap()]);

        assert!(identities.contains(&real));
        assert!(identities.contains(&dir.join("link")));
        assert!(identities.contains(&dir.join("./link/../real/")));
        assert!(!identities.contains(&dir.join("other")));
        assert!(!identities.contains(&dir.join("missing")));
    }

    /// A case-only respelling is the directory where the file system folds case; where it does
    /// not, it is another path and is not claimed to be this one.
    #[test]
    fn a_case_only_respelling_is_that_directory_on_a_case_insensitive_file_system() {
        let dir = ScratchDir::new("hostfs-identities-case");
        let real = dir.join("Project");
        std::fs::create_dir(&real).expect("project");
        let respelled = dir.join("pROJECT");
        let folds_case = respelled.exists();

        let identities = DirectoryIdentities::from_paths([real.to_str().unwrap()]);

        assert_eq!(identities.contains(&respelled), folds_case);
    }

    /// A path with nothing at it is compared by its text, written one way.
    #[test]
    fn a_path_with_nothing_at_it_is_compared_by_its_text() {
        let identities = DirectoryIdentities::from_paths(["/nowhere/octoboard/project"]);

        assert!(identities.contains(Path::new("/nowhere/octoboard/./x/../project/")));
        assert!(!identities.contains(Path::new("/nowhere/octoboard/project2")));
    }
}
