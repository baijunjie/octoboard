//! Spelling a path a repository holds as an argument `git` takes for exactly that path.
//!
//! `git` checks a pathspec, and the path operand of a command such as `check-attr`, for an
//! absolute path before it joins it to the directory it runs in, and dies with "'…' is outside
//! repository" when the absolute path lies outside the worktree. On Windows a name that starts with
//! one character and a colon is absolute there — a drive prefix — so `a:b`, which a repository made
//! on another host can hold, fails every read that names it. Literal pathspecs do not help: they
//! only stop magic and wildcards from being read, and the check runs either way. Nor is the path
//! itself safe from the normalization the check comes with, which on Windows also turns every `\`
//! into `/`.
//!
//! So a path never goes to `git` bare. Where `git` reads pathspec magic it goes with `top`
//! ([`literal`], [`exact`]): the path is taken from the worktree's root exactly as given, neither
//! checked nor normalized. Such a command runs without `GIT_LITERAL_PATHSPECS`, under which the
//! magic would be read as part of the name, and every path it is given has to carry magic of its
//! own. Where no magic is read — a path operand, a command that refuses magic — the path goes as
//! `./<path>` ([`relative`]), which escapes the drive check but not the normalization.

/// `path` (relative to the worktree's root) as a pathspec that matches that path and, where it is
/// a directory, everything below it: `:(top,literal)<path>`.
pub(crate) fn literal(path: &[u8]) -> Vec<u8> {
    [b":(top,literal)".as_slice(), path].concat()
}

/// `path` (relative to the worktree's root) as a pathspec that excludes it and everything below it:
/// `:(top,exclude,literal)<path>`.
pub(crate) fn excluded(path: &[u8]) -> Vec<u8> {
    [b":(top,exclude,literal)".as_slice(), path].concat()
}

/// `path` (relative to the directory `git` runs in) as `./<path>`, for a path operand or a command
/// that reads no pathspec magic. After `./` the name no longer begins with what could be a drive,
/// and the `.` component is dropped by the normalization `git` gives every relative path anyway;
/// that same normalization, on Windows, reads a `\` in a name as a separator.
pub(crate) fn relative(path: &[u8]) -> Vec<u8> {
    [b"./".as_slice(), path].concat()
}

/// Pathspecs that match each of `paths` (relative to the worktree's root) exactly: the path itself,
/// literally, and nothing below it (`<path>/**` excluded, the path's own glob characters escaped). A
/// plain path would also match everything under it when it is a directory on one side — a file
/// replaced by a directory — and excluding `<path>/` instead would exclude a submodule at the path
/// itself, which a pathspec takes for a directory. `git` applies an exclusion to every path of the
/// command, so a path with another of `paths` below it (a rename from `foo` to `foo/bar`) gets
/// none: that would take the other path out too. What then lies below it is matched as well, and
/// the caller sorts it out.
pub(crate) fn exact(paths: &[&[u8]]) -> Vec<Vec<u8>> {
    let mut specs: Vec<Vec<u8>> = Vec::new();
    for path in paths {
        let exact = literal(path);
        if specs.contains(&exact) {
            continue;
        }
        specs.push(exact);
        if holds_another(path, paths) {
            continue;
        }
        let mut below = b":(top,exclude,glob)".to_vec();
        for &byte in *path {
            if matches!(byte, b'*' | b'?' | b'[' | b']' | b'\\') {
                below.push(b'\\');
            }
            below.push(byte);
        }
        below.extend_from_slice(b"/**");
        specs.push(below);
    }
    specs
}

/// Whether another of `paths` lies below `path`.
fn holds_another(path: &[u8], paths: &[&[u8]]) -> bool {
    paths.iter().any(|other| {
        other.len() > path.len() + 1 && other.starts_with(path) && other[path.len()] == b'/'
    })
}

/// Whether one of `paths` lies below another (a rename from `foo` to `foo/bar`): the one case
/// [`exact`] cannot keep what lies below a path out of a command.
pub(crate) fn paths_nest(paths: &[&[u8]]) -> bool {
    paths.iter().any(|path| holds_another(path, paths))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_starts_like_a_drive_is_never_given_bare() {
        assert_eq!(literal(b"a:b"), b":(top,literal)a:b");
        assert_eq!(excluded(b"a:b"), b":(top,exclude,literal)a:b");
        assert_eq!(relative(b"a:b"), b"./a:b");
        assert_eq!(relative(b":(glob)x"), b"./:(glob)x");
    }

    #[test]
    fn an_exact_path_excludes_what_lies_below_it_unless_another_path_does() {
        assert_eq!(
            exact(&[b"a*", b"a*"]),
            [
                b":(top,literal)a*".to_vec(),
                br":(top,exclude,glob)a\*/**".to_vec()
            ]
        );
        assert_eq!(
            exact(&[b"foo", b"foo/bar"]),
            [
                b":(top,literal)foo".to_vec(),
                b":(top,literal)foo/bar".to_vec(),
                b":(top,exclude,glob)foo/bar/**".to_vec(),
            ]
        );
        assert!(paths_nest(&[b"foo", b"foo/bar"]));
        assert!(!paths_nest(&[b"foo", b"foobar"]));
    }
}
