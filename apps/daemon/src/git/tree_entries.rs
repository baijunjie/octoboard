//! The entries of `git ls-files --stage -z` and `git ls-tree -r -z`, which both print
//! `<mode> <rest>\t<path>` and end each entry with a NUL.

/// The gitlink mode: a submodule's commit recorded in place of a file.
const GITLINK_MODE: &[u8] = b"160000";

/// One entry of a listing.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    /// Whether the entry is a gitlink, a submodule's place, not a file of this repository.
    pub is_gitlink: bool,
    pub path: &'a [u8],
}

/// The entries in `stdout`; anything with no tab in it is not an entry and is left out.
pub fn entries(stdout: &[u8]) -> impl Iterator<Item = Entry<'_>> {
    stdout.split(|&b| b == 0).filter_map(|entry| {
        let tab = entry.iter().position(|&b| b == b'\t')?;
        let (head, path) = (&entry[..tab], &entry[tab + 1..]);
        let mode = head.split(|&b| b == b' ').next().unwrap_or_default();
        Some(Entry {
            is_gitlink: mode == GITLINK_MODE,
            path,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_gives_each_path_and_marks_the_gitlinks() {
        let listing = b"100644 abc 0\ta b\x00160000 def 0\tsub\x00120000 123\tlink\0\0stray";
        let found: Vec<_> = entries(listing).collect();
        assert_eq!(
            found,
            vec![
                Entry {
                    is_gitlink: false,
                    path: b"a b"
                },
                Entry {
                    is_gitlink: true,
                    path: b"sub"
                },
                Entry {
                    is_gitlink: false,
                    path: b"link"
                },
            ]
        );
    }
}
