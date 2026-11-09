use super::*;
use crate::holder::protocol::TailAge;
use crate::terminal::snapshot::{MAX_COLS, MAX_ROWS};
use crate::test_support::{app_state, ScratchDir};

fn screen(rows: u16, cols: u16) -> SavedScreen {
    SavedScreen::of_bytes(rows, cols, b"\x1b[Hsaved \x1b[1mscreen")
}

fn at(holder: &str, terminal: &str) -> HeldAt {
    HeldAt {
        holder_id: holder.to_string(),
        terminal_id: terminal.to_string(),
    }
}

fn handover(token: &str, truncated: bool) -> HandoverTail {
    HandoverTail {
        token: token.to_string(),
        since: 0,
        truncated,
        ages: vec![TailAge {
            offset: 0,
            age_ms: 20,
        }],
    }
}

/// The screens `state` has loaded, by holder and terminal.
fn loaded_keys(state: &AppState) -> Vec<(String, String)> {
    let mut keys: Vec<_> = state
        .loaded_restart_screens()
        .screens
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

fn key(holder: &str, terminal: &str) -> (String, String) {
    (holder.to_string(), terminal.to_string())
}

/// The file is written owner-only and whole, with the version of its format, and read back
/// whole; reading it leaves it, so that a start that fails before it adopts anything, or
/// adopts only some, leaves the screens to the next.
#[test]
fn the_saved_screens_round_trip_through_the_file_and_outlive_their_loading() {
    let dir = ScratchDir::new("restart-screens-round-trip");
    let path = dir.join("restart-screens.json");
    replace(
        &path,
        "token-1",
        &[
            (at("h1", "t1"), screen(24, 80)),
            (at("h1", "t2"), screen(10, 40)),
        ],
        &[],
    )
    .expect("the screens were written");
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(written["version"], VERSION);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "the file is open to others: {mode:o}");
    }

    let (state, _state_dir) = app_state("restart-screens-round-trip-state");
    state.load_restart_screens(&path);
    assert!(path.exists(), "loading consumed the file");
    let loaded = state.loaded_restart_screens();
    assert_eq!(loaded.token, "token-1");
    assert_eq!(loaded.screens.len(), 2);
    assert_eq!(
        loaded
            .screens
            .get(&key("h1", "t2"))
            .map(|entry| &entry.screen),
        Some(&screen(10, 40))
    );
    drop(loaded);

    // The start ends before it selects anything; the next one finds every screen still there.
    drop(state);
    let (again, _again_dir) = app_state("restart-screens-round-trip-again");
    again.load_restart_screens(&path);
    assert_eq!(loaded_keys(&again), vec![key("h1", "t1"), key("h1", "t2")]);
}

/// A planned stop replaces the file whole: with the screens it saved and those of the holders
/// still pending carried over, each with the handover it was saved at; with nothing to keep,
/// it removes the file.
#[test]
fn a_planned_stop_replaces_the_file_carrying_over_the_pending_holders_screens() {
    let dir = ScratchDir::new("restart-screens-replace");
    let path = dir.join("restart-screens.json");
    replace(
        &path,
        "old",
        &[
            (at("pending", "t1"), screen(24, 80)),
            (at("gone", "t2"), screen(24, 80)),
        ],
        &[],
    )
    .unwrap();
    let (state, _state_dir) = app_state("restart-screens-replace-state");
    state.load_restart_screens(&path);
    let carried = state.pending_restart_screens(&["pending".to_string()]);
    assert_eq!(carried.len(), 1);
    assert_eq!(carried[0].token, "old");
    replace(
        &path,
        "new",
        &[(at("linked", "t3"), screen(10, 40))],
        &carried,
    )
    .unwrap();

    let (next, _next_dir) = app_state("restart-screens-replace-next");
    next.load_restart_screens(&path);
    assert_eq!(
        loaded_keys(&next),
        vec![key("linked", "t3"), key("pending", "t1")]
    );
    // Each is verified against the handover it was saved at, and carried again at it by the
    // next planned stop.
    let again = next.pending_restart_screens(&["pending".to_string()]);
    assert_eq!(again[0].token, "old");
    let linked = next.pending_restart_screens(&["linked".to_string()]);
    assert_eq!(linked[0].token, "new");
    let size = Some(Size { rows: 24, cols: 80 });
    let old = handover("old", false);
    assert!(verify(&again[0].token, &again[0].screen, Some(&old), size).is_ok());

    replace(&path, "newer", &[], &[]).unwrap();
    assert!(
        !path.exists(),
        "a planned stop that keeps nothing left the file"
    );
}

#[test]
fn a_file_that_does_not_read_gives_nothing_and_is_deleted() {
    let dir = ScratchDir::new("restart-screens-garbage");
    let path = dir.join("restart-screens.json");
    std::fs::write(&path, "{ not json").unwrap();
    let (state, _state_dir) = app_state("restart-screens-garbage-state");
    state.load_restart_screens(&path);
    assert!(!path.exists());
    assert!(loaded_keys(&state).is_empty());
}

/// A file of a version this build does not read, or of none, is not read as its own format:
/// every screen is rebuilt from its tail, and the file goes.
#[test]
fn a_file_of_a_version_this_build_does_not_read_gives_nothing_and_is_deleted() {
    let entry = serde_json::json!({
        "holder_id": "h1",
        "terminal_id": "t1",
        "screen": screen(24, 80),
    });
    for (label, file) in [
        (
            "newer",
            serde_json::json!({ "version": VERSION + 1, "token": "t", "screens": [entry.clone()] }),
        ),
        (
            "none",
            serde_json::json!({ "token": "t", "screens": [entry.clone()] }),
        ),
        (
            "another-shape",
            serde_json::json!({ "version": VERSION + 1, "token": 7, "entries": {} }),
        ),
    ] {
        let dir = ScratchDir::new(&format!("restart-screens-version-{label}"));
        let path = dir.join("restart-screens.json");
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        let (state, _state_dir) = app_state(&format!("restart-screens-version-{label}-state"));
        state.load_restart_screens(&path);
        assert!(!path.exists(), "{label}");
        assert!(loaded_keys(&state).is_empty(), "{label}");
    }
}

/// Selecting a screen takes nothing; committing screens rewrites the file without them, once for
/// all of them, so that a start after a crash finds them no more: the tail since the handover,
/// whose queries were answered once, is never replayed over one again. Committing the last
/// removes the file.
#[test]
fn screens_committed_are_gone_from_the_file_before_anything_else_happens() {
    let dir = ScratchDir::new("restart-screens-take");
    let path = dir.join("restart-screens.json");
    replace(
        &path,
        "token-1",
        &[
            (at("h1", "t1"), screen(24, 80)),
            (at("h1", "t2"), screen(10, 40)),
            (at("h2", "t3"), screen(12, 50)),
        ],
        &[],
    )
    .unwrap();
    let (state, _state_dir) = app_state("restart-screens-take-state");
    state.load_restart_screens(&path);

    let selected = state
        .select_restart_screen(&at("h1", "t2"), None, None)
        .expect("a screen saved for it");
    assert_eq!(selected.saved, screen(10, 40));
    assert!(!selected.followed, "no tail says it follows the screen");
    assert!(state.has_restart_screen(&at("h1", "t2")));

    // One holder's terminals, one of which had no screen saved for it.
    assert!(state.commit_restart_screens(&[at("h1", "t1"), at("h1", "t2"), at("h1", "t9")]));
    assert_eq!(loaded_keys(&state), vec![key("h2", "t3")]);
    let (after_crash, _after_crash_dir) = app_state("restart-screens-take-after-crash");
    after_crash.load_restart_screens(&path);
    assert_eq!(loaded_keys(&after_crash), vec![key("h2", "t3")]);
    assert_eq!(after_crash.loaded_restart_screens().token, "token-1");

    // Committed once only, and nothing to commit for a terminal no screen was saved for.
    assert!(state
        .select_restart_screen(&at("h1", "t1"), None, None)
        .is_none());
    assert!(state.commit_restart_screens(&[at("h1", "t1")]));
    assert!(state.commit_restart_screens(&[at("h9", "t9")]));

    assert!(state.commit_restart_screens(&[at("h2", "t3")]));
    assert!(!path.exists(), "the file outlived its last screen");
}

/// A start that fails while it adopts a holder's terminals, after it committed another holder's,
/// leaves the screens of the one it never got to; a holder adopted late (one that could not
/// be linked to at start) still finds its screen; the screens of a holder whose terminals
/// were all taken over that none of them took are dropped; and those of a holder that is gone
/// stay until what becomes of it is settled, when all are dropped.
#[test]
fn screens_are_committed_holder_by_holder_and_expired_once_their_holder_is_settled() {
    let dir = ScratchDir::new("restart-screens-holders");
    let path = dir.join("restart-screens.json");
    replace(
        &path,
        "token-1",
        &[
            (at("h1", "t1"), screen(24, 80)),
            (at("h1", "gone"), screen(24, 80)),
            (at("late", "t2"), screen(10, 40)),
            (at("dead", "t3"), screen(12, 50)),
            (at("dead", "t4"), screen(12, 50)),
        ],
        &[],
    )
    .unwrap();
    let (state, _state_dir) = app_state("restart-screens-holders-state");
    state.load_restart_screens(&path);

    // h1's terminal is adopted, and then the start fails.
    assert!(state.commit_restart_screens(&[at("h1", "t1")]));
    let (next, _next_dir) = app_state("restart-screens-holders-next");
    next.load_restart_screens(&path);
    assert_eq!(
        loaded_keys(&next),
        vec![
            key("dead", "t3"),
            key("dead", "t4"),
            key("h1", "gone"),
            key("late", "t2")
        ]
    );

    // Or the start goes on: h1's terminals are all adopted, and its other screen expires.
    state.expire_restart_screens(|holder| holder == "h1");
    assert_eq!(
        loaded_keys(&state),
        vec![key("dead", "t3"), key("dead", "t4"), key("late", "t2")]
    );

    // The holder that could not be linked to at start is, later.
    assert!(state.commit_restart_screens(&[at("late", "t2")]));

    // The gone holder's screens are still in the file for a start after a crash until it is
    // settled; then they all go.
    let (after_crash, _after_crash_dir) = app_state("restart-screens-holders-after-crash");
    after_crash.load_restart_screens(&path);
    assert_eq!(
        loaded_keys(&after_crash),
        vec![key("dead", "t3"), key("dead", "t4")]
    );
    state.expire_restart_screens(|holder| holder == "dead");
    assert!(loaded_keys(&state).is_empty());
    assert!(!path.exists());
}

/// Screens kept without a file (as a test keeps them) are committed with nothing to rewrite,
/// each verified against the handover it was kept for.
#[test]
fn a_screen_loaded_from_no_file_is_committed_durably() {
    let (state, _state_dir) = app_state("restart-screens-no-file");
    state.keep_restart_screen("t", at("h1", "t1"), screen(24, 80));
    assert_eq!(
        state.pending_restart_screens(&["h1".to_string()])[0].token,
        "t"
    );
    let size = Some(Size { rows: 24, cols: 80 });
    let followed = state.select_restart_screen(&at("h1", "t1"), Some(&handover("t", false)), size);
    assert!(followed.expect("a screen").followed);
    let other = state.select_restart_screen(&at("h1", "t1"), Some(&handover("u", false)), size);
    assert!(!other.expect("a screen").followed);
    assert!(state.commit_restart_screens(&[at("h1", "t1")]));
    assert!(!state.has_restart_screen(&at("h1", "t1")));
}

/// A screen carried over from an earlier handover keeps that handover's token through the load
/// and through a rewrite as other screens are taken; the file's own screens are written without
/// one.
#[test]
fn a_carried_screen_keeps_its_own_token_through_load_and_rewrite() {
    let dir = ScratchDir::new("restart-screens-carried-token");
    let path = dir.join("restart-screens.json");
    let carried = CarriedScreen {
        token: "old".to_string(),
        at: at("pending", "t1"),
        screen: screen(24, 80),
    };
    replace(
        &path,
        "new",
        &[
            (at("linked", "t2"), screen(10, 40)),
            (at("linked", "t3"), screen(10, 40)),
        ],
        &[carried],
    )
    .unwrap();
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let tokens: Vec<_> = written["screens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|screen| screen.get("token").cloned())
        .collect();
    assert_eq!(tokens.iter().filter(|token| token.is_some()).count(), 1);

    let (state, _state_dir) = app_state("restart-screens-carried-token-state");
    state.load_restart_screens(&path);
    assert!(state.commit_restart_screens(&[at("linked", "t2")]));

    let (next, _next_dir) = app_state("restart-screens-carried-token-next");
    next.load_restart_screens(&path);
    assert_eq!(next.loaded_restart_screens().token, "new");
    assert_eq!(
        next.pending_restart_screens(&["pending".to_string()])[0].token,
        "old"
    );
    assert_eq!(
        next.pending_restart_screens(&["linked".to_string()])[0].token,
        "new"
    );
}

/// During a planned stop the file is the stop's to replace: a screen committed then is gone from
/// the loaded screens, but not durably, so the terminal is not restored from it.
#[test]
fn a_screen_committed_during_a_planned_stop_is_not_committed_durably() {
    let dir = ScratchDir::new("restart-screens-handing-over");
    let path = dir.join("restart-screens.json");
    replace(&path, "t", &[(at("h1", "t1"), screen(24, 80))], &[]).unwrap();
    let (state, _state_dir) = app_state("restart-screens-handing-over-state");
    state.load_restart_screens(&path);
    state
        .handing_over
        .store(true, std::sync::atomic::Ordering::Release);

    assert!(!state.commit_restart_screens(&[at("h1", "t1")]));
    assert!(!state.has_restart_screen(&at("h1", "t1")));
    assert!(
        path.exists(),
        "the file was touched during the planned stop"
    );
}

/// A file that can be neither rewritten nor removed leaves the screens committed not durably
/// committed.
#[cfg(unix)]
#[test]
fn a_screen_whose_file_cannot_be_rewritten_or_removed_is_not_committed_durably() {
    use std::os::unix::fs::PermissionsExt;

    let dir = ScratchDir::new("restart-screens-read-only");
    let screens_dir = dir.join("screens");
    std::fs::create_dir(&screens_dir).unwrap();
    let path = screens_dir.join("restart-screens.json");
    replace(
        &path,
        "t",
        &[
            (at("h1", "t1"), screen(24, 80)),
            (at("h1", "t2"), screen(24, 80)),
        ],
        &[],
    )
    .unwrap();
    let (state, _state_dir) = app_state("restart-screens-read-only-state");
    state.load_restart_screens(&path);

    let set_mode = |mode| {
        std::fs::set_permissions(&screens_dir, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    set_mode(0o500);
    // A user the permissions do not hold back (root, as in a container) cannot show this.
    let probe = screens_dir.join("probe");
    if std::fs::write(&probe, b"").is_ok() {
        std::fs::remove_file(&probe).ok();
        set_mode(0o700);
        eprintln!("skipped: the directory's permissions do not hold this user back");
        return;
    }
    let committed = state.commit_restart_screens(&[at("h1", "t1")]);
    set_mode(0o700);
    assert!(!committed);
}

/// A screen is used only for the handover it was saved at, with the whole tail since and at
/// its own size; anything else falls back to the tail.
#[test]
fn a_saved_screen_verifies_only_for_its_own_handover_whole_and_at_its_size() {
    let size = Some(Size { rows: 24, cols: 80 });
    let saved = screen(24, 80);
    assert_eq!(
        verify("t", &saved, Some(&handover("t", false)), size),
        Ok(())
    );
    for (handover, size) in [
        (None, size),
        (Some(handover("other", false)), size),
        (Some(handover("t", true)), size),
        (
            Some(handover("t", false)),
            Some(Size { rows: 25, cols: 80 }),
        ),
        (
            Some(handover("t", false)),
            Some(Size { rows: 24, cols: 81 }),
        ),
        (Some(handover("t", false)), None),
    ] {
        assert!(
            verify("t", &saved, handover.as_ref(), size).is_err(),
            "{handover:?} at {size:?} verified"
        );
    }
}

/// A size no screen has is not restored, though the terminal reports the same one.
#[test]
fn a_saved_size_outside_what_a_screen_can_have_does_not_verify() {
    for (rows, cols) in [
        (0, 80),
        (24, 0),
        (24, 1),
        (MAX_ROWS + 1, 80),
        (24, MAX_COLS + 1),
    ] {
        assert!(
            verify(
                "t",
                &screen(rows, cols),
                Some(&handover("t", false)),
                Some(Size { rows, cols })
            )
            .is_err(),
            "{rows}x{cols} verified"
        );
    }
    let (rows, cols) = (MAX_ROWS, MAX_COLS);
    assert_eq!(
        verify(
            "t",
            &screen(rows, cols),
            Some(&handover("t", false)),
            Some(Size { rows, cols })
        ),
        Ok(())
    );
}

/// The rule carries the tail's length, the ages from the mark in the handover, and the offline
/// answers of the holder: no ages where it answered a query this build does not know, and no
/// device attributes where it answered them.
#[test]
fn the_replay_rule_is_built_from_the_handover_and_what_the_holder_answered() {
    let measured_at = Instant::now();
    let tail = handover("t", false);

    let rule = replay_rule(Some(&tail), &[], 7, measured_at);
    assert_eq!(rule.len, 7);
    assert_eq!(rule.ages, vec![(0, Duration::from_millis(20))]);
    assert_eq!(rule.measured_at, measured_at);
    assert!(!rule.skip_device_attributes);

    let rule = replay_rule(None, &[], 0, measured_at);
    assert!(rule.ages.is_empty());

    let rule = replay_rule(Some(&tail), &[OfflineAnswer::Unknown], 7, measured_at);
    assert!(rule.ages.is_empty());

    let rule = replay_rule(
        Some(&tail),
        &[OfflineAnswer::PrimaryDeviceAttributes],
        7,
        measured_at,
    );
    assert_eq!(rule.ages.len(), 1);
    assert!(rule.skip_device_attributes);
}
