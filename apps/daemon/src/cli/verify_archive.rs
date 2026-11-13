//! `octoboardd verify-archive <archive> <signature>`: whether a downloaded release archive is the
//! one the release key signed, for the install script (`apps/web/public/install.sh`), which runs
//! the installed daemon to check the archive that is to replace it where `minisign` is not
//! installed. The checks are the self-update's own (`crate::self_update::verify`), with the release
//! key built in: the signature is the key's, and the file it names is the archive's file name, so
//! the script saves the download under the name the release gave it.
//!
//! It reads the two files and changes nothing; it is there in every build, with or without the
//! `self-update` feature, since it needs nothing of the update but the key. Exits 0 when the
//! archive verifies and 1, saying why on stderr, when it does not; clap's own status 2 is a
//! command line it does not take, or a daemon from before this command.

use std::path::Path;

use minisign_verify::PublicKey;

use crate::self_update::verify::{release_key, verify_signature};

/// Runs the command against the release key, returning the exit status.
pub(super) fn run(archive: &Path, signature: &Path) -> i32 {
    match check(archive, signature, &release_key()) {
        Ok(()) => 0,
        Err(reason) => {
            eprintln!("{}: {reason}", archive.display());
            1
        }
    }
}

/// Checks `archive` against the signature in the file `signature` (the manifest's `signature`
/// field, the base64 of a minisign signature file, or that file itself) made with `key`, which
/// must name the archive's file name. The error says which check failed.
fn check(archive: &Path, signature: &Path, key: &PublicKey) -> Result<(), String> {
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "the archive has no file name".to_owned())?;
    let signature = std::fs::read_to_string(signature)
        .map_err(|err| format!("the signature could not be read: {err}"))?;
    // The script has checked the SHA-256 against the manifest's already: only the signature and
    // the name decide here.
    let mut file = std::fs::File::open(archive)
        .map_err(|err| format!("the archive could not be read: {err}"))?;
    verify_signature(&mut file, &signature, name, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    const ARCHIVE: &[u8] =
        include_bytes!("../../testdata/self_update/octoboardd_9.9.9_linux_x86_64.tar.gz");
    const SIGNATURE: &str =
        include_str!("../../testdata/self_update/octoboardd_9.9.9_linux_x86_64.tar.gz.sig");
    const TEST_KEY: &str = include_str!("../../testdata/self_update/test-release-key.txt");
    const NAME: &str = "octoboardd_9.9.9_linux_x86_64.tar.gz";

    fn test_key() -> PublicKey {
        PublicKey::from_base64(TEST_KEY.trim()).unwrap()
    }

    /// The fixture archive saved as `name` and its signature as `signature`, in `dir`.
    fn lay_out(
        dir: &Path,
        name: &str,
        archive: &[u8],
        signature: &str,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let archive_path = dir.join(name);
        std::fs::write(&archive_path, archive).unwrap();
        let signature_path = dir.join("signature");
        std::fs::write(&signature_path, signature).unwrap();
        (archive_path, signature_path)
    }

    #[test]
    fn an_archive_signed_by_the_key_under_the_name_it_was_signed_as_verifies() {
        let dir = ScratchDir::new("verify-archive-good");
        let (archive, signature) = lay_out(&dir, NAME, ARCHIVE, SIGNATURE);
        assert_eq!(check(&archive, &signature, &test_key()), Ok(()));
        // The minisign signature file itself is taken as well as its base64.
        let plain = decoded(SIGNATURE);
        std::fs::write(&signature, plain).unwrap();
        assert_eq!(check(&archive, &signature, &test_key()), Ok(()));
    }

    fn decoded(text: &str) -> String {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(text.trim())
            .unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn an_archive_under_another_name_than_the_signed_one_is_refused() {
        // An older release's archive, signed by the same key, passed off as a newer one.
        let dir = ScratchDir::new("verify-archive-name");
        let (archive, signature) = lay_out(
            &dir,
            "octoboardd_10.0.0_linux_x86_64.tar.gz",
            ARCHIVE,
            SIGNATURE,
        );
        let error = check(&archive, &signature, &test_key()).unwrap_err();
        assert!(error.contains(NAME), "{error}");
    }

    #[test]
    fn a_changed_archive_or_another_key_is_refused() {
        let dir = ScratchDir::new("verify-archive-changed");
        let mut changed = ARCHIVE.to_vec();
        let last = changed.len() - 1;
        changed[last] ^= 1;
        let (archive, signature) = lay_out(&dir, NAME, &changed, SIGNATURE);
        assert!(check(&archive, &signature, &test_key()).is_err());

        let dir = ScratchDir::new("verify-archive-key");
        let (archive, signature) = lay_out(&dir, NAME, ARCHIVE, SIGNATURE);
        // The release key did not sign the test fixture.
        assert!(check(&archive, &signature, &release_key()).is_err());
        assert_eq!(run(&archive, &signature), 1);
    }

    #[test]
    fn a_missing_or_unreadable_signature_is_refused() {
        let dir = ScratchDir::new("verify-archive-missing");
        let (archive, signature) = lay_out(&dir, NAME, ARCHIVE, "not a signature");
        assert!(check(&archive, &signature, &test_key()).is_err());
        assert!(check(&archive, &dir.join("absent"), &test_key()).is_err());
        // A missing archive, with a signature that would verify it.
        std::fs::write(&signature, SIGNATURE).unwrap();
        std::fs::remove_file(&archive).unwrap();
        let error = check(&archive, &signature, &test_key()).unwrap_err();
        assert!(error.contains("could not be read"), "{error}");
    }
}
