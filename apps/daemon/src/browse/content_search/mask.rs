//! A content search's file mask: comma-separated globs a file must match one of, and `!` globs
//! that leave a file out (see `file_mask` of `search_project_content` in `apps/daemon/PROTOCOL.md`).

use anyhow::Result;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use std::path::Path;

use super::refusal;
use crate::wire_path::RelPath;

/// The file mask: the globs a file must match one of (any file, when there are none) and those
/// that leave a file out. A glob with no `/` is matched against the file's name, one with a `/`
/// against its path relative to the project (whatever directory the search is limited to), a
/// leading `/` dropped. `*` does not cross a `/`, and matching is case-sensitive.
#[derive(Debug)]
pub(super) struct Mask {
    include_names: GlobSet,
    include_paths: GlobSet,
    exclude_names: GlobSet,
    exclude_paths: GlobSet,
    any_included: bool,
}

impl Mask {
    pub(super) fn parse(mask: &str) -> Result<Self> {
        let mut sets = [
            GlobSetBuilder::new(),
            GlobSetBuilder::new(),
            GlobSetBuilder::new(),
            GlobSetBuilder::new(),
        ];
        let mut any_included = false;
        for glob in mask
            .split(',')
            .map(str::trim)
            .filter(|glob| !glob.is_empty())
        {
            let (excluded, pattern) = match glob.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, glob),
            };
            let by_path = pattern.contains('/');
            let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
            let compiled = GlobBuilder::new(pattern)
                .literal_separator(true)
                .backslash_escape(true)
                .build()
                .map_err(|err| refusal::invalid_mask(glob, &err))?;
            any_included |= !excluded;
            sets[usize::from(excluded) * 2 + usize::from(by_path)].add(compiled);
        }
        let [include_names, include_paths, exclude_names, exclude_paths] =
            sets.map(|set| set.build().expect("each glob compiled on its own"));
        Ok(Self {
            include_names,
            include_paths,
            exclude_names,
            exclude_paths,
            any_included,
        })
    }

    pub(super) fn admits(&self, path: &RelPath) -> bool {
        let text = String::from_utf8_lossy(path.as_bytes());
        let name = text.rsplit('/').next().unwrap_or(&text);
        let path = Path::new(text.as_ref());
        let name = Path::new(name);
        if self.exclude_names.is_match(name) || self.exclude_paths.is_match(path) {
            return false;
        }
        !self.any_included || self.include_names.is_match(name) || self.include_paths.is_match(path)
    }
}
