//! The files named by a patch in the `*** Begin Patch` format some agents edit files with (a GPT
//! model's `apply_patch` tool), for the cross-project guard to ask about each: GitHub Copilot CLI
//! and OpenCode both hand the guard such a patch in place of a path.

/// The headers of a patch that name a file, the path following each.
const HEADERS: [&str; 4] = [
    "*** Add File: ",
    "*** Update File: ",
    "*** Delete File: ",
    "*** Move to: ",
];

/// The files `patch` names: the path after each of its file headers, in order, an empty one left
/// out. Text that is no patch names none.
pub(crate) fn patch_targets(patch: &str) -> Vec<String> {
    patch
        .lines()
        .filter_map(|line| HEADERS.iter().find_map(|header| line.strip_prefix(header)))
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file header names its file, in order, the destination of a move included; the patch's
    /// own begin and end lines and its hunks name none.
    #[test]
    fn every_file_header_of_a_patch_names_its_file() {
        let patch = "*** Begin Patch\n*** Add File: a.txt\n+x\n*** Update File: b/c.txt\n\
                     *** Move to: d/e.txt\n@@\n-x\n+y\n*** Delete File: /f.txt\n*** End Patch\n";
        assert_eq!(
            patch_targets(patch),
            ["a.txt", "b/c.txt", "d/e.txt", "/f.txt"]
        );
    }

    /// Text that is no patch, an empty patch and a header with no path name no file.
    #[test]
    fn text_that_names_no_file_gives_none() {
        for text in [
            "ls -la",
            "",
            "*** Begin Patch\n*** End Patch\n",
            "*** Add File: \n",
            "  *** Add File: indented.txt",
        ] {
            assert!(patch_targets(text).is_empty(), "{text:?}");
        }
    }
}
