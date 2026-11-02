//! Telling the remote from the branch in `<remote>/<branch>`. A remote's name may hold a `/`
//! (`team/origin`), as a branch's does, so neither the first nor the last `/` splits it: only the
//! remotes the repository has do.

/// `name` is `<remote>/<branch>`, as `git` shortens an upstream or a remote-tracking branch. The
/// remote is the longest of `remotes` that is a prefix of it followed by `/`, so a remote named
/// `origin` does not take `origin-fork/main`, and a remote named `team/origin` wins over `team`
/// for `team/origin/main`. `None` for a name no remote begins, and for a remote with no branch
/// after it.
pub fn split_remote_branch<'a>(
    name: &'a str,
    remotes: impl IntoIterator<Item = &'a str>,
) -> Option<(&'a str, &'a str)> {
    let mut best: Option<(&str, &str)> = None;
    for remote in remotes {
        let Some(branch) = name
            .strip_prefix(remote)
            .and_then(|rest| rest.strip_prefix('/'))
            .filter(|branch| !branch.is_empty())
        else {
            continue;
        };
        if best.is_none_or(|(longest, _)| remote.len() > longest.len()) {
            best = Some((remote, branch));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_longest_configured_remote_takes_the_name() {
        let remotes = ["team", "origin", "team/origin"];
        let split = |name| split_remote_branch(name, remotes);
        assert_eq!(split("origin/main"), Some(("origin", "main")));
        assert_eq!(split("team/main"), Some(("team", "main")));
        assert_eq!(split("team/origin/main"), Some(("team/origin", "main")));
        assert_eq!(
            split("origin/feature/thing"),
            Some(("origin", "feature/thing"))
        );
        assert_eq!(split("origin-fork/main"), None);
        assert_eq!(split("origin/"), None);
        assert_eq!(split("origin"), None);
        assert_eq!(split("main"), None);
    }
}
