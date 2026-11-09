use crate::terminal::saved_screen::SavedScreen;
use crate::terminal::snapshot::tests::replay;
use crate::terminal::snapshot::{assert_same_state, TerminalSnapshot};

/// The screen `original` saves, and a fresh terminal of its size that took it in, as a restore
/// does.
fn restored(original: &TerminalSnapshot) -> (SavedScreen, TerminalSnapshot) {
    let saved = original.save().expect("nothing too long to keep");
    let mut restored = TerminalSnapshot::new(saved.rows, saved.cols);
    restored.set_drawn(saved.drawn);
    restored.process(&saved.bytes);
    (saved, restored)
}

/// Asserts that the screen saved after `before` is exact, and that the terminal it is restored
/// into takes `after` in to the state the original takes it to.
fn assert_goes_on_exactly(before: &[u8], after: &[u8]) {
    let context = String::from_utf8_lossy(before).into_owned();
    let mut original = replay(before, 6, 20);
    let (saved, mut restored) = restored(&original);
    assert!(!saved.inexact, "{context:?}");
    original.process(after);
    restored.process(after);
    assert_same_state(&restored, &original);
    assert_eq!(
        restored.unexposed.titles, original.unexposed.titles,
        "{context:?}"
    );
}

#[test]
fn a_plain_screen_is_saved_exact_and_goes_on_as_the_original_does() {
    assert_goes_on_exactly(b"line one\r\nline two\r\n$ ", b"typed\r\nmore");
}

/// A repeat (REP) after the restore draws the character the original would, wherever the paint
/// left the restored parser's last one: at a trailing blank the paint does not draw, after a
/// cursor move, in insert mode, in origin mode, after a wide or a zero-width character.
#[test]
fn a_repeat_after_the_restore_draws_the_character_the_original_would() {
    for before in [
        &b"prompt $ "[..],
        b"abc",
        b"x\x1b[5;5H",
        b"\x1b[31mred\x1b[0m \x1b[4h",
        b"\x1b[2;5r\x1b[?6h\x1b[2;3H\x1b7in region ",
        "wide \u{754c}".as_bytes(),
        "e\u{301}".as_bytes(),
    ] {
        assert_goes_on_exactly(before, b"\x1b[3b|\x1b[2b");
    }
}

/// The normal screen hidden behind the alternate one is not saved: a screen saved there is exact
/// only while nothing was drawn on the normal one, from the start or since a reset, and stays so
/// when saved again after the restore. Anything drawn on it, or scrolled into its scrollback,
/// makes it inexact.
#[test]
fn behind_the_alternate_screen_only_a_blank_normal_screen_is_exact() {
    for before in [
        &b"history\r\nmore history\r\n\x1b[?1049h\x1b[Happ"[..],
        b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\x1b[H\x1b[?1049happ",
        b"\x1b[3;4H\x1b[?1049happ",
        b"\x1b[41m\x1b[?1049happ",
    ] {
        let (saved, _) = restored(&replay(before, 6, 20));
        assert!(saved.inexact, "{:?}", String::from_utf8_lossy(before));
    }
    for before in [
        &b"\x1b[?1049h\x1b[2;3Happ"[..],
        b"drawn\r\n\x1bc\x1b[?1049h\x1b[2;3Happ",
        b"\x1b]2;a title\x07\x1b[?25l\x1b[?1049happ",
    ] {
        assert_goes_on_exactly(before, b"\x1b[?1049lnormal again");
        let (_, once) = restored(&replay(before, 6, 20));
        let (again, _) = restored(&once);
        assert!(!again.inexact, "{:?}", String::from_utf8_lossy(before));
    }
}

/// State the saved bytes do not rebuild marks the screen inexact: tab stops changed, a character
/// set designated or shifted to, palette colours changed, a cursor the paint cannot place, or a
/// character to repeat with no blank cells to draw it on.
#[test]
fn state_the_saved_bytes_leave_out_marks_the_screen_inexact() {
    for (what, before) in [
        ("a tab stop set", &b"\x1b[5G\x1bHtext"[..]),
        ("the tab stops cleared", b"\x1b[3gtext"),
        ("line drawing designated", b"\x1b(0lqk"),
        ("the second character set shifted to", b"\x0etext"),
        ("a palette colour", b"\x1b]4;1;rgb:ff/00/00\x07text"),
        ("the default foreground", b"\x1b]10;rgb:ff/ff/ff\x07text"),
        (
            "a saved cursor above the region in origin mode",
            b"\x1b7\x1b[2;5r\x1b[?6htext",
        ),
        ("a character to repeat with nowhere blank", b"y\x1b#8"),
    ] {
        let (saved, _) = restored(&replay(before, 6, 20));
        assert!(saved.inexact, "{what}");
    }
}

/// State put back as a reset leaves it is exact again.
#[test]
fn state_put_back_as_a_reset_leaves_it_is_exact_again() {
    for before in [
        &b"\x1b(0lqk\x1b(B text"[..],
        b"\x0etext\x0f more",
        b"\x1b]4;1;rgb:ff/00/00\x07\x1b]104\x07text",
        b"\x1b[5G\x1bH\x1bcafter a reset",
    ] {
        assert_goes_on_exactly(before, b" and on\x1b[2b\tafter a tab");
    }
}

/// The titles a program pushed are pushed again, so that popping them after the restore gives
/// back the titles the original's pops do.
#[test]
fn the_titles_pushed_are_pushed_again() {
    let before = b"\x1b[22t\x1b]2;one\x07\x1b[22t\x1b]2;two\x07\x1b[22t\x1b]2;three\x07text";
    let mut original = replay(before, 6, 20);
    let (saved, mut restored) = restored(&original);
    assert!(!saved.inexact);
    assert_eq!(restored.unexposed.titles, original.unexposed.titles);
    assert_eq!(restored.unexposed.title, original.unexposed.title);
    for _ in 0..4 {
        original.process(b"\x1b[23t");
        restored.process(b"\x1b[23t");
        assert_eq!(restored.unexposed.title, original.unexposed.title);
    }
}

/// Output that leaves the alternate screen, draws on the normal one or only moves its cursor, and
/// enters the alternate screen again, all in one chunk, leaves a normal screen the saved bytes do
/// not rebuild.
#[test]
fn leaving_and_entering_the_alternate_screen_in_one_chunk_counts_what_came_between() {
    for between in [&b"normal text"[..], b"\x1b[3;3H"] {
        let mut original = replay(b"\x1b[?1049happ", 6, 20);
        let chunk = [&b"\x1b[?1049l"[..], between, b"\x1b[?1049happ"].concat();
        original.process(&chunk);
        let (saved, _) = restored(&original);
        assert!(saved.inexact, "{:?}", String::from_utf8_lossy(between));
    }
    // With nothing between, the normal screen is still blank.
    let mut original = replay(b"\x1b[?1049happ", 6, 20);
    original.process(b"\x1b[?1049l\x1b[?1049happ");
    let (saved, _) = restored(&original);
    assert!(!saved.inexact);
}

/// A restored blank normal screen is known to be blank, so that a program entering the
/// alternate screen over it after the restore is saved exact again; also where the character to
/// repeat had to be drawn again, which happens on the alternate screen.
#[test]
fn a_restored_blank_normal_screen_stays_known_to_be_blank() {
    for before in [&b"\x1b]2;a title\x07"[..], b"\x1b[?1049hx\x1b[?1049l"] {
        let context = String::from_utf8_lossy(before).into_owned();
        let mut original = replay(before, 6, 20);
        let (saved, mut restored) = restored(&original);
        assert!(!saved.inexact, "{context:?}");
        assert_same_state(&restored, &original);
        assert_eq!(restored.unexposed.preceding, original.unexposed.preceding);
        for terminal in [&mut original, &mut restored] {
            terminal.process(b"\x1b[?1049h\x1b[2;3Happ\x1b[2b");
        }
        assert_same_state(&restored, &original);
        let (again, _) = self::restored(&restored);
        assert!(!again.inexact, "{context:?}");
    }
}

/// A cursor saved while the alternate screen was shown stays with it, hidden, once the program
/// leaves it: a screen saved on the normal one then is inexact, until a reset forgets it.
#[test]
fn a_cursor_the_alternate_screen_keeps_saved_makes_the_normal_screen_inexact() {
    let output = b"\x1b[?1049h\x1b[3;3H\x1b7app\x1b[?1049l";
    let (saved, _) = restored(&replay(output, 6, 20));
    assert!(saved.inexact);
    let (saved, _) = restored(&replay(
        &[&output[..], b"\x1bcafter a reset"].concat(),
        6,
        20,
    ));
    assert!(!saved.inexact);
}

/// A program restored on the alternate screen, with no cursor saved there, that then leaves it,
/// leaves a screen saved exact again: the restore saved no cursor on the alternate screen.
#[test]
fn a_screen_restored_on_the_alternate_screen_and_left_saves_exact_again() {
    let mut original = replay(b"\x1b[?1049h\x1b[2;3Happ", 6, 20);
    let (_, mut restored) = restored(&original);
    for terminal in [&mut original, &mut restored] {
        terminal.process(b"\x1b[?1049lback on the normal screen");
    }
    assert_same_state(&restored, &original);
    let (again, _) = self::restored(&restored);
    assert!(!again.inexact);
}
