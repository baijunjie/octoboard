//! Following the symbolic links on the way to a path one component at a time, so that a caller can
//! refuse where they lead before the disk is asked to resolve them all (see [`leads_into`]).

use std::collections::VecDeque;
use std::path::{Component, Path, PathBuf};

/// The most links [`leads_into`] follows. The host stops a resolution at fewer (32 on macOS, 40 on
/// Linux, 63 on Windows), so a chain it would resolve is walked whole, and a loop ends the walk
/// without an answer, left to the host's own loop error.
const MAX_LINKS: usize = 64;

/// Whether the path `spelling`, an absolute path as written, leads into a place `refuses` names:
/// some prefix of it, or of where a link on the way leads, as written. The walk goes component by
/// component and never looks at a prefix `refuses` names, so a link to a network share or a device
/// is found without the host reaching out to it, which resolving the path would.
///
/// `refuses` takes a prefix whose links so far are resolved, and whether components remain after
/// it (a prefix may be refused only as the place the walk ends, not as one it passes through). A
/// prefix is never judged bare: a drive or share is judged with its root separator, and alone only
/// where nothing follows it. Each link met is read (`symlink_metadata`, `read_link`), its target
/// taken against the folder it lies in, and the walk goes on from there, a `..` in a target
/// stepping out of the folder reached so far. A link whose target `names_local_volume` is not
/// followed: the folder it lies in is the spelling to judge. A component that is not there, or any
/// other failure looking at one, ends the walk with `false`: what [`super::canonical`] then makes
/// of the path is for the caller to judge. Blocking. A link changed after it was read is not seen.
pub fn leads_into(
    spelling: &Path,
    refuses: &dyn Fn(&Path, bool) -> bool,
    names_local_volume: &dyn Fn(&Path) -> bool,
) -> bool {
    let mut reached = PathBuf::new();
    // Each component of what is left to walk, as a path of its own so a link's target can join it.
    let mut ahead: VecDeque<PathBuf> = spelling
        .components()
        .map(|component| PathBuf::from(component.as_os_str()))
        .collect();
    let mut links = 0;
    while let Some(part) = ahead.pop_front() {
        let Some(component) = part.components().next() else {
            continue;
        };
        match component {
            Component::Prefix(prefix) => {
                reached = PathBuf::from(prefix.as_os_str());
                let rooted = ahead.front().is_some_and(|next| {
                    matches!(next.components().next(), Some(Component::RootDir))
                });
                if rooted {
                    continue;
                }
            }
            Component::RootDir => {
                // A root alone keeps the drive or share it is read under.
                reached = match reached.components().next() {
                    Some(first @ Component::Prefix(_)) => PathBuf::from(first.as_os_str()),
                    _ => PathBuf::new(),
                };
                reached.push(Component::RootDir);
            }
            Component::CurDir => continue,
            Component::ParentDir => {
                reached.pop();
            }
            Component::Normal(name) => reached.push(name),
        }
        if refuses(&reached, !ahead.is_empty()) {
            return true;
        }
        if !matches!(component, Component::Normal(_)) {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&reached) else {
            return false;
        };
        if !metadata.is_symlink() {
            continue;
        }
        links += 1;
        if links > MAX_LINKS {
            return false;
        }
        let Ok(target) = std::fs::read_link(&reached) else {
            return false;
        };
        if names_local_volume(&target) {
            continue;
        }
        reached.pop();
        // The target's components come before whatever of the spelling is left.
        for part in target.components().rev() {
            ahead.push_front(PathBuf::from(part.as_os_str()));
        }
    }
    // The walk may end on a `.` or a link with no target, which judged nothing as the last place.
    refuses(&reached, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{symlink, ScratchDir};

    fn canonical(path: &Path) -> PathBuf {
        crate::hostfs::canonical(path).unwrap()
    }

    /// A judgement that names `place` and what lies below it, and counts nothing else: the
    /// scratch folder stands in for a network place, so nothing real is read.
    #[cfg(unix)]
    fn names(place: &Path) -> impl Fn(&Path, bool) -> bool + '_ {
        move |prefix, _| prefix.starts_with(place)
    }

    #[cfg(unix)]
    fn no_volume(_: &Path) -> bool {
        false
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_refused_place_is_found_through_any_number_of_links_and_dots() {
        let dir = ScratchDir::new("hostfs-links-lead");
        let base = canonical(&dir);
        let place = base.join("share");
        std::fs::create_dir_all(place.join("inner")).unwrap();
        std::fs::create_dir(base.join("home")).unwrap();
        symlink(&place, base.join("home/direct")).unwrap();
        symlink("direct", base.join("home/chain")).unwrap();
        symlink("../share/inner", base.join("home/relative")).unwrap();
        symlink("home/chain", base.join("outer")).unwrap();
        let refuses = names(&place);
        for spelling in [
            "home/direct",
            "home/chain",
            "home/relative",
            "outer/inner",
            "home/chain/inner/../inner",
            "share",
        ] {
            assert!(
                leads_into(&base.join(spelling), &refuses, &no_volume),
                "{spelling}"
            );
        }
        for spelling in ["home", "home/missing", "missing/direct", ""] {
            assert!(
                !leads_into(&base.join(spelling), &refuses, &no_volume),
                "{spelling}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_refused_place_is_never_looked_at() {
        let dir = ScratchDir::new("hostfs-links-never-looked");
        let base = canonical(&dir);
        symlink(base.join("share/target"), base.join("link")).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());
        let place = base.join("share");
        let refuses = |prefix: &Path, _: bool| {
            seen.borrow_mut().push(prefix.to_path_buf());
            prefix.starts_with(&place)
        };
        // `share` is not there; a look at it would be a failure, and the walk stops at the name.
        assert!(leads_into(&base.join("link/deeper"), &refuses, &no_volume));
        let seen = seen.into_inner();
        assert_eq!(seen.last(), Some(&place), "{seen:?}");
        // Nothing below it was named: the walk stopped at the place itself.
        assert_eq!(
            seen.iter()
                .filter(|prefix| prefix.starts_with(&place))
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_loop_of_links_ends_the_walk_without_an_answer() {
        let dir = ScratchDir::new("hostfs-links-loop");
        let base = canonical(&dir);
        symlink("b", base.join("a")).unwrap();
        symlink("a", base.join("b")).unwrap();
        assert!(!leads_into(&base.join("a/x"), &|_, _| false, &no_volume));
    }

    #[cfg(unix)]
    #[test]
    fn a_dot_dot_in_a_target_steps_out_of_the_folder_reached_so_far() {
        let dir = ScratchDir::new("hostfs-links-dotdot");
        let base = canonical(&dir);
        let place = base.join("share");
        std::fs::create_dir_all(base.join("a/b")).unwrap();
        std::fs::create_dir(&place).unwrap();
        symlink("../../share", base.join("a/b/up")).unwrap();
        assert!(leads_into(&base.join("a/b/up"), &names(&place), &no_volume));
        assert!(!leads_into(&base.join("a/b"), &names(&place), &no_volume));
    }

    /// A prefix is refused only as the place the walk ends when `refuses` says so: the walk tells
    /// it whether components remain, and a link that lands on it ends the walk there.
    #[cfg(unix)]
    #[test]
    fn a_prefix_is_judged_with_whether_anything_follows_it() {
        let dir = ScratchDir::new("hostfs-links-ends");
        let base = canonical(&dir);
        let ancestor = base.join("share");
        let allowed = ancestor.join("project");
        std::fs::create_dir_all(&allowed).unwrap();
        symlink(&ancestor, base.join("onto")).unwrap();
        symlink(allowed.join(".."), base.join("back-up")).unwrap();
        // The folder holding the one place allowed is passed through, never where the walk ends.
        let refuses = |prefix: &Path, more: bool| {
            prefix.starts_with(&ancestor)
                && !prefix.starts_with(&allowed)
                && !(more && allowed.starts_with(prefix))
        };
        for (spelling, ends_in_refusal) in [
            ("share/project", false),
            ("share/project/x", false),
            ("share", true),
            ("onto", true),
            ("back-up", true),
            ("onto/project", false),
        ] {
            assert_eq!(
                leads_into(&base.join(spelling), &refuses, &no_volume),
                ends_in_refusal,
                "{spelling}"
            );
        }
    }

    /// A walk that ends on a `.` (a link to its own folder) judges the folder it ends in as the
    /// place it ends.
    #[cfg(unix)]
    #[test]
    fn a_walk_ending_on_a_dot_judges_the_last_place_as_the_end() {
        let dir = ScratchDir::new("hostfs-links-dot-end");
        let base = canonical(&dir);
        let share = base.join("share");
        std::fs::create_dir(&share).unwrap();
        symlink(".", share.join("dot")).unwrap();
        let refuses = |prefix: &Path, more: bool| prefix == share && !more;
        assert!(leads_into(&share.join("dot"), &refuses, &no_volume));
        assert!(!leads_into(&share.join("dot/x"), &refuses, &no_volume));
    }

    /// A link whose target names a local volume is not followed, and not refused for it: the
    /// folder it lies in is the spelling judged.
    #[cfg(unix)]
    #[test]
    fn a_link_to_a_local_volume_is_not_followed() {
        let dir = ScratchDir::new("hostfs-links-volume");
        let base = canonical(&dir);
        let place = base.join("share");
        std::fs::create_dir(&place).unwrap();
        symlink(&place, base.join("mount")).unwrap();
        let named = |target: &Path| target == place;
        assert!(!leads_into(&base.join("mount/x"), &names(&place), &named));
        assert!(leads_into(
            &base.join("mount/x"),
            &names(&place),
            &no_volume
        ));
    }

    /// The verbatim form of a drive is judged with its root, never bare, so it is no network
    /// place, and a UNC prefix with nothing after it is judged at once.
    #[cfg(windows)]
    #[test]
    fn a_verbatim_drive_is_judged_with_its_root_and_a_bare_unc_prefix_is_refused() {
        use crate::platform::names::{names_local_volume, reaches_network_or_device};
        let dir = ScratchDir::new("hostfs-links-verbatim");
        let base = canonical(&dir);
        std::fs::create_dir(base.join("folder")).unwrap();
        let judged = std::cell::RefCell::new(Vec::new());
        let refuses = |prefix: &Path, _: bool| {
            judged.borrow_mut().push(prefix.to_path_buf());
            reaches_network_or_device(prefix)
        };
        let verbatim = PathBuf::from(format!(r"\\?\{}", base.join("folder").display()));
        assert!(!leads_into(&verbatim, &refuses, &names_local_volume));
        assert!(judged
            .borrow()
            .iter()
            .all(|prefix| prefix.as_os_str().len() > 6));
        // A link to a verbatim drive path is followed and found clean.
        symlink(&verbatim, base.join("to-verbatim")).unwrap();
        assert!(!leads_into(
            &base.join("to-verbatim"),
            &refuses,
            &names_local_volume
        ));
        // A UNC prefix alone is judged where it stands, before anything is looked at.
        assert!(leads_into(
            Path::new(r"\\octoboard-no-server\share"),
            &refuses,
            &names_local_volume
        ));
        assert!(leads_into(
            Path::new(r"\\octoboard-no-server\share\a"),
            &refuses,
            &names_local_volume
        ));
    }
}
