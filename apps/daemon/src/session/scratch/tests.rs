use super::*;
use crate::protocol::SessionStatus;
use crate::session::test_support::fixture;
use crate::state::test_support::insert_record;
use crate::test_support::{app_state, code_of};

/// A session whose process runs keeps its files in its scratch directory, and makes no composer
/// directory.
#[tokio::test]
async fn a_running_sessions_files_go_in_its_scratch_directory() {
    let f = fixture(true);
    let home = files_home(&f.state, &f.id).unwrap();
    assert_eq!(home, f.run.join(&f.id));
    assert!(!f.state.composer_dir(&f.id).exists());
}

/// An interrupted session has no scratch directory: its files go into its composer directory,
/// made for the owner alone, and made once.
#[test]
fn an_interrupted_sessions_files_go_in_its_composer_directory() {
    let (state, dir) = app_state("scratch-files-home-interrupted");
    insert_record(&state, &dir, "s", SessionStatus::Interrupted);
    let home = files_home(&state, "s").unwrap();
    assert_eq!(home, state.composer_dir("s"));
    assert!(home.is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&home).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "open to others: {mode:o}");
    }
    assert_eq!(files_home(&state, "s").unwrap(), home);
}

/// An archived session, a session that reads as running with no process behind it (starting or
/// ending), and one that does not exist take no files: the refusal is `session_not_running` or
/// `unknown_session`, and no directory is made.
#[test]
fn an_archived_starting_or_unknown_session_takes_no_files() {
    for status in [
        SessionStatus::Archived,
        SessionStatus::Starting,
        SessionStatus::Working,
        SessionStatus::Idle,
    ] {
        let (state, dir) = app_state("scratch-files-home-refused");
        insert_record(&state, &dir, "s", status);
        let refused = files_home(&state, "s").unwrap_err();
        assert_eq!(
            code_of(&refused),
            error_code::SESSION_NOT_RUNNING,
            "{status:?}"
        );
        assert!(!state.composer_dir("s").exists(), "{status:?}");
    }
    let (state, _dir) = app_state("scratch-files-home-unknown");
    let refused = files_home(&state, "gone").unwrap_err();
    assert_eq!(code_of(&refused), error_code::UNKNOWN_SESSION);
}
