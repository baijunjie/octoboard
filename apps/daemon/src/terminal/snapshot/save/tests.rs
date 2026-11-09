use crate::terminal::saved_screen::SavedScreen;
use crate::terminal::snapshot::tests::replay;
use crate::terminal::snapshot::{assert_same_state, TerminalSnapshot};

/// The screen `original` saves, and a fresh terminal of its size that took it in, as a restore
/// does.
fn restored(original: &mut TerminalSnapshot) -> (SavedScreen, TerminalSnapshot) {
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
    let (saved, mut restored) = restored(&mut original);
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

/// What a program does after the screen was saved on the alternate screen: leaves it, leaves it
/// and restores the cursor, or draws and goes back and forth.
const AFTER_THE_ALTERNATE_SCREEN: [&[u8]; 3] = [
    b"\x1b[?1049lnormal again",
    b"\x1b[?1049l\x1b8restored",
    b"app\x1b[?1049l\x1b[2bx\x1b[?1049h\x1b[?1049l\x1b8y",
];

/// Asserts that the screen `build` makes, on the alternate screen over a drawn normal one, saves
/// exact; that the terminal it is restored into is in the state the original is in; that both go
/// to the same state on the program's next output (the normal screen's rows, scrollback, cursor,
/// pen and saved cursor included); and that saving the restored terminal again is exact.
fn assert_normal_screen_comes_back(build: impl Fn() -> TerminalSnapshot, context: &str) {
    for after in AFTER_THE_ALTERNATE_SCREEN {
        let mut original = build();
        let (saved, mut restored) = restored(&mut original);
        assert!(!saved.inexact, "{context}");
        assert_same_state(&restored, &original);
        let (again, _) = self::restored(&mut restored);
        assert!(!again.inexact, "{context}: saved again");
        original.process(after);
        restored.process(after);
        assert_same_state(&restored, &original);
        assert_eq!(restored.unexposed.titles, original.unexposed.titles);
    }
}

fn assert_normal_screen_behind_comes_back(before: &[u8]) {
    assert_normal_screen_comes_back(
        || replay(before, 6, 20),
        &format!("{:?}", String::from_utf8_lossy(before)),
    );
}

/// A normal screen drawn on, hidden behind the alternate one, is saved exact and comes back when
/// the program leaves the alternate screen: its scrollback past the screen's height, a moved
/// cursor, a coloured or hyperlinked pen, a cursor waiting to wrap, wide characters.
#[test]
fn a_drawn_normal_screen_behind_the_alternate_one_is_saved_exact_and_comes_back() {
    for before in [
        &b"history\r\nmore history\r\n\x1b[?1049h\x1b[Happ"[..],
        b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\x1b[H\x1b[?1049happ",
        b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine\r\nten\r\n$ \x1b[?1049h\x1b[2;3Happ",
        b"text\x1b[3;4H\x1b[?1049happ",
        b"\x1b[1;31mred bold\x1b[?1049happ",
        b"\x1b]8;;https://example.com\x07link\x1b[?1049happ",
        b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07 plain\x1b[?1049happ",
        b"abcdefghijklmnopqrst\x1b[?1049happ",
        b"abcdefghijklmnopqrstu\x1b[?1049happ",
        "wide \u{754c}\u{754c}\u{754c}\r\n\u{754c}e\u{301}\x1b[?1049happ".as_bytes(),
        "0123456789012345678\u{754c}\x1b[?1049happ".as_bytes(),
    ] {
        assert_normal_screen_behind_comes_back(before);
    }
}

/// Entering the alternate screen clears it with the pen of the normal screen's cursor, so a
/// normal screen whose pen has a background colour is hidden behind an alternate screen of that
/// colour until the program clears it. The program here resets the pen and clears the alternate
/// screen, which leaves blank cells, so the screen is exact, and the restored alternate screen is
/// as the original's.
#[test]
fn a_background_pen_on_the_normal_screen_does_not_colour_the_restored_alternate_screen() {
    for before in [
        &b"\x1b[41mred\x1b[?1049h\x1b[0m\x1b[2Japp"[..],
        b"\x1b[1;31;44mred on blue\r\nmore\x1b[?1049h\x1b[0m\x1b[2J\x1b[2;3Happ",
    ] {
        assert_normal_screen_behind_comes_back(before);
    }
}

/// Saving leaves the terminal as it was: it is the state a fresh replay of the same bytes is in,
/// on the alternate screen and after the program leaves it, and goes on the same way.
#[test]
fn saving_behind_the_alternate_screen_leaves_the_terminal_unchanged() {
    for before in [
        &b"history\r\nmore history\r\n\x1b[?1049h\x1b[Happ"[..],
        b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\n\x1b[31mtext\x1b[3;4H\x1b[?1049happ",
        b"abcdefghijklmnopqrst\x1b[?1049happ",
        b"\x1b]8;;https://example.com\x07link\x1b[?1049h\x1b[2;3Happ",
        b"\x1b[2;5r\x1b[?6h\x1b[2;3Htext\x1b[?1049happ",
    ] {
        let mut original = replay(before, 6, 20);
        let _ = original.save();
        let mut fresh = replay(before, 6, 20);
        assert_same_state(&original, &fresh);
        for after in [
            &b" more on the alternate screen"[..],
            b"\x1b[?1049lnormal again",
            b"\x1b[?1049l\x1b8restored",
            b"\x1b8\x1b[?1049l\x1b8restored again\x1b[2b",
        ] {
            original.process(after);
            fresh.process(after);
            assert_same_state(&original, &fresh);
        }
    }
}

/// A normal screen that was never drawn on, from the start or since a reset, is exact too, and
/// stays so when saved again after the restore.
#[test]
fn a_blank_normal_screen_behind_the_alternate_one_is_saved_exact() {
    for before in [
        &b"\x1b[?1049h\x1b[2;3Happ"[..],
        b"drawn\r\n\x1bc\x1b[?1049h\x1b[2;3Happ",
        b"\x1b]2;a title\x07\x1b[?25l\x1b[?1049happ",
    ] {
        assert_goes_on_exactly(before, b"\x1b[?1049lnormal again");
        let (_, mut once) = restored(&mut replay(before, 6, 20));
        let (again, _) = restored(&mut once);
        assert!(!again.inexact, "{:?}", String::from_utf8_lossy(before));
    }
}

/// A change of width while the alternate screen is shown may have reflowed the normal screen
/// behind it away from the cursor entering saved there, which a replay cannot rebuild: the screen
/// is inexact, and saving it still leaves the terminal as it was. A change of height alone does
/// not, and neither does a change of width over a blank normal screen.
#[test]
fn a_width_change_behind_the_alternate_screen_makes_a_drawn_normal_screen_inexact() {
    let before = b"one\r\ntwo\r\nthree\r\n$ prompt text\x1b[?1049happ";
    for (rows, cols) in [(6, 30), (6, 12), (8, 12)] {
        let mut original = replay(before, 6, 20);
        original.resize(rows, cols);
        let (saved, _) = restored(&mut original);
        assert!(saved.inexact, "{rows}x{cols}");
        let mut fresh = replay(before, 6, 20);
        fresh.resize(rows, cols);
        assert_same_state(&original, &fresh);
        for after in [&b"more"[..], b"\x1b[?1049lnormal again\x1b8"] {
            original.process(after);
            fresh.process(after);
            assert_same_state(&original, &fresh);
        }
    }

    let mut original = replay(before, 6, 20);
    original.resize(9, 20);
    let (saved, _) = restored(&mut original);
    assert!(!saved.inexact, "a change of height alone");

    let mut original = replay(b"\x1b[?1049happ", 6, 20);
    original.resize(6, 30);
    let (saved, _) = restored(&mut original);
    assert!(!saved.inexact, "a blank normal screen");
}

/// A width change while the normal screen is shown does not matter: the saved cursor is not in
/// play there, and the screen is saved as it is.
#[test]
fn a_width_change_before_the_alternate_screen_is_entered_leaves_it_exact() {
    assert_normal_screen_comes_back(
        || {
            let mut terminal = replay(b"one\r\ntwo\r\n$ text", 6, 20);
            terminal.resize(6, 30);
            terminal.process(b"\x1b[?1049happ");
            terminal
        },
        "resized before entering",
    );
}

/// Once the program enters the alternate screen again after the width changed, in a later chunk
/// than the resize or in the one that leaves it, the normal screen is exact again.
#[test]
fn entering_the_alternate_screen_again_after_a_width_change_makes_it_exact_again() {
    let before = b"one\r\ntwo\r\n$ prompt text\x1b[?1049happ";
    for (what, chunks) in [
        (
            "left and entered in later chunks",
            &[&b"\x1b[?1049l"[..], b"more\x1b[?1049happ"][..],
        ),
        (
            "left and entered in one chunk",
            &[&b"\x1b[?1049lnormal\x1b[?1049happ"[..]][..],
        ),
        (
            "left and entered with nothing between",
            &[&b"\x1b[?1049l\x1b[?1049happ"[..]][..],
        ),
    ] {
        assert_normal_screen_comes_back(
            || {
                let mut terminal = replay(before, 6, 20);
                terminal.resize(6, 30);
                for chunk in chunks {
                    terminal.process(chunk);
                }
                terminal
            },
            what,
        );
    }
}

/// A reset (RIS) on the alternate screen leaves it, and the width change it followed no longer
/// matters: entering the alternate screen again, in a later chunk or in the same one, is exact
/// again, over the blank normal screen the reset leaves and over one drawn on after it, which
/// comes back when the program leaves the alternate screen.
#[test]
fn a_reset_on_the_alternate_screen_after_a_width_change_makes_it_exact_again() {
    let before = b"one\r\ntwo\r\n$ prompt text\x1b[?1049happ";
    for (what, chunks) in [
        (
            "reset and entered in later chunks",
            &[&b"\x1bc"[..], b"\x1b[?1049happ"][..],
        ),
        (
            "reset and entered in one chunk",
            &[&b"\x1bc\x1b[?1049happ"[..]][..],
        ),
        (
            "reset, drawn on and entered in one chunk",
            &[&b"\x1bcnormal text\r\nmore\x1b[?1049happ"[..]][..],
        ),
        (
            "reset and drawn on in one chunk, entered in the next",
            &[&b"\x1bcnormal text\r\nmore"[..], b"\x1b[?1049happ"][..],
        ),
        (
            "reset in one chunk, drawn on and entered in the next",
            &[&b"\x1bc"[..], b"normal text\x1b[3;3H\x1b[?1049happ"][..],
        ),
    ] {
        assert_normal_screen_comes_back(
            || {
                let mut terminal = replay(before, 6, 20);
                terminal.resize(6, 30);
                for chunk in chunks {
                    terminal.process(chunk);
                }
                terminal
            },
            what,
        );
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
        let (saved, _) = restored(&mut replay(before, 6, 20));
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
    let (saved, mut restored) = restored(&mut original);
    assert!(!saved.inexact);
    assert_eq!(restored.unexposed.titles, original.unexposed.titles);
    assert_eq!(restored.unexposed.title, original.unexposed.title);
    for _ in 0..4 {
        original.process(b"\x1b[23t");
        restored.process(b"\x1b[23t");
        assert_eq!(restored.unexposed.title, original.unexposed.title);
    }
}

/// A normal screen the program drew on or only moved the cursor of, between leaving the alternate
/// screen and entering it again in one chunk, is saved exact; one with nothing between is still
/// blank.
#[test]
fn leaving_and_entering_the_alternate_screen_in_one_chunk_keeps_what_came_between() {
    for between in [&b"normal text"[..], b"\x1b[3;3H", b"\x1b[32mgreen\r\nmore"] {
        assert_normal_screen_comes_back(
            || {
                let mut terminal = replay(b"\x1b[?1049happ", 6, 20);
                let chunk = [&b"\x1b[?1049l"[..], between, b"\x1b[?1049happ"].concat();
                terminal.process(&chunk);
                terminal
            },
            &format!("{:?}", String::from_utf8_lossy(between)),
        );
    }
    // With nothing between, the normal screen is still blank.
    let mut original = replay(b"\x1b[?1049happ", 6, 20);
    original.process(b"\x1b[?1049l\x1b[?1049happ");
    let (saved, _) = restored(&mut original);
    assert!(!saved.inexact);
}

/// A character set other than the default on the cursor of the normal screen behind the alternate
/// one is not rebuilt, so the screen is inexact; designated and put back before entering, it is
/// exact.
#[test]
fn a_character_set_on_the_normal_screen_behind_the_alternate_one_makes_it_inexact() {
    for before in [
        &b"text\x1b(0lqk\x1b[?1049h\x1b(Bapp"[..],
        b"text\x1b(0\x1b[?1049h\x1b(Bapp",
    ] {
        let (saved, _) = restored(&mut replay(before, 6, 20));
        assert!(saved.inexact, "{:?}", String::from_utf8_lossy(before));
    }
    assert_normal_screen_behind_comes_back(b"text\x1b(0lqk\x1b(B more\x1b[?1049happ");
}

/// The saved bytes write the normal screen behind the alternate one although a character set
/// makes the save inexact: once the program leaves the alternate screen, the restored terminal is
/// in the original's state but for the character sets.
#[test]
fn a_normal_screen_with_a_character_set_behind_the_alternate_one_is_still_written() {
    for before in [
        &b"drawn\r\n\x1b(0q\x1b[?1049h\x1b(Bapp"[..],
        b"text\x1b(0lqk\x1b[?1049h\x1b(Bapp",
        b"text\x1b(0\x1b[2;3H\x1b[?1049h\x1b(Bapp",
    ] {
        let context = String::from_utf8_lossy(before).into_owned();
        let mut original = replay(before, 6, 20);
        let (saved, mut restored) = restored(&mut original);
        assert!(saved.inexact, "{context:?}");
        original.process(b"\x1b[?1049l");
        restored.process(b"\x1b[?1049l");
        assert_same_state(&restored, &original);
    }
}

/// A restored blank normal screen is known to be blank, so that a program entering the
/// alternate screen over it after the restore is saved exact again; also where the character to
/// repeat had to be drawn again, which happens on the alternate screen.
#[test]
fn a_restored_blank_normal_screen_stays_known_to_be_blank() {
    for before in [&b"\x1b]2;a title\x07"[..], b"\x1b[?1049hx\x1b[?1049l"] {
        let context = String::from_utf8_lossy(before).into_owned();
        let mut original = replay(before, 6, 20);
        let (saved, mut restored) = restored(&mut original);
        assert!(!saved.inexact, "{context:?}");
        assert_same_state(&restored, &original);
        assert_eq!(restored.unexposed.preceding, original.unexposed.preceding);
        for terminal in [&mut original, &mut restored] {
            terminal.process(b"\x1b[?1049h\x1b[2;3Happ\x1b[2b");
        }
        assert_same_state(&restored, &original);
        let (again, _) = self::restored(&mut restored);
        assert!(!again.inexact, "{context:?}");
    }
}

/// A cursor saved while the alternate screen was shown stays with it, hidden, once the program
/// leaves it: a screen saved on the normal one then is inexact, until a reset forgets it.
#[test]
fn a_cursor_the_alternate_screen_keeps_saved_makes_the_normal_screen_inexact() {
    let output = b"\x1b[?1049h\x1b[3;3H\x1b7app\x1b[?1049l";
    let (saved, _) = restored(&mut replay(output, 6, 20));
    assert!(saved.inexact);
    let (saved, _) = restored(&mut replay(
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
    let (_, mut restored) = restored(&mut original);
    for terminal in [&mut original, &mut restored] {
        terminal.process(b"\x1b[?1049lback on the normal screen");
    }
    assert_same_state(&restored, &original);
    let (again, _) = self::restored(&mut restored);
    assert!(!again.inexact);
}

/// Saving leaves the terminal as it was also where the program saved a cursor on the alternate
/// screen, at a moved position, before: it is the state a fresh replay of the same bytes is in,
/// and restoring that cursor on the alternate screen or after leaving it goes the same way.
#[test]
fn saving_with_a_cursor_saved_on_the_alternate_screen_leaves_the_terminal_unchanged() {
    for before in [
        &b"history\r\ntext\x1b[?1049h\x1b[3;4H\x1b7\x1b[Happ"[..],
        b"one\r\ntwo\x1b[2;2H\x1b[?1049h\x1b[4;6H\x1b7\x1b[1;1Happ\x1b[5;9H",
        b"\x1b[31mred\x1b[?1049h\x1b[2;3H\x1b[1m\x1b7\x1b[0mapp",
    ] {
        let context = String::from_utf8_lossy(before).into_owned();
        let mut original = replay(before, 6, 20);
        let _ = original.save();
        let mut fresh = replay(before, 6, 20);
        assert_same_state(&original, &fresh);
        for after in [
            &b"\x1b8on the alternate screen"[..],
            b"\x1b8\x1b[?1049l\x1b8restored",
            b"\x1b[?1049lnormal again\x1b8",
            b"\x1b[?1049l\x1b[?1049h\x1b8again",
            b"\x1b8\x1b[2bx\x1b[?1049l\x1b[2by",
        ] {
            original.process(after);
            fresh.process(after);
            assert_same_state(&original, &fresh);
            assert_eq!(
                original.unexposed.alternate_saved_cursor, fresh.unexposed.alternate_saved_cursor,
                "{context:?}"
            );
        }
    }
}

/// A normal pen with a background colour, where the program does not clear the alternate screen
/// after entering it, tints the alternate screen: the restored screen has the cells tinted as the
/// original's, and goes on as it does.
///
/// Whether the save is exact is not asserted: the tinted alternate screen has no blank default
/// cell for `write_repeat` to draw the character a repeat draws again on, and its tinted blanks
/// are drawn as spaces even where the program drew nothing (the last case), so the screen is
/// saved inexact for that, which is not what this case is about.
#[test]
fn a_background_pen_that_tints_the_alternate_screen_comes_back_tinted() {
    for before in [
        &b"\x1b[41mred\x1b[?1049happ"[..],
        b"\x1b[41mred\x1b[3;4H\x1b[?1049happ",
        b"\x1b[1;31;44mred on blue\r\nmore\x1b[2;3H\x1b[?1049happ\x1b[4;5Hmore",
        b"\x1b[41mred\x1b[?1049h",
    ] {
        let mut original = replay(before, 6, 20);
        let (_, mut restored) = restored(&mut original);
        assert_same_state(&restored, &original);
        for after in AFTER_THE_ALTERNATE_SCREEN {
            original.process(after);
            restored.process(after);
            assert_same_state(&restored, &original);
        }
    }
}
