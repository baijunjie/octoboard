//! How far a trusted directory reaches, judged by where directories really are
//! ([`crate::hostfs::place`]).
//!
//! A trusted directory covers a project when the project's directory, once every symbolic link on
//! its path is resolved, is that directory or lies below it ([`Place::lies_in`]). Anything that
//! cannot be resolved covers nothing and is covered by nothing: a match grants trust, so there is
//! no fallback to the text, which would let a project reached through a link count as inside
//! again.
//!
//! A trusted directory whose real directory is the filesystem root, the home directory or one
//! above it covers nothing ([`too_broad_to_trust`]). None is ever added as one, but a trusted path
//! that is a link can be repointed later, and it must not then cover everything the user has.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use crate::hostfs::place::{followed_directory_identity, Identity, Place};
use crate::paths;

/// How long looking at one path on the file system may take. Past it the path counts as one that
/// cannot be resolved, so a directory on a mount that does not answer holds up a trust decision by
/// this much at most, and never pushes a confirmation through.
pub(super) const LOOK_DEADLINE: Duration = Duration::from_secs(5);

/// The directories no trusted directory may be, as identities: the filesystem root, and the home
/// directory and every directory above it, as `home` is written and as it really is — the same
/// directories a folder offered for trusting is refused for (`trustable_parent`). With no usable
/// home directory, the root alone. Blocking.
pub(super) fn too_broad_to_trust(home: &Path) -> HashSet<Identity> {
    let mut broad: HashSet<Identity> = followed_directory_identity(Path::new("/"))
        .into_iter()
        .collect();
    if let Some(home) = paths::known_home(home) {
        broad.extend(home.ancestors().filter_map(followed_directory_identity));
        if let Some(place) = Place::of(&home) {
            broad.extend(place.path_identities());
        }
    }
    broad
}
