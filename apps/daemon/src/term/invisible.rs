//! The characters an agent's prompt treats as invisible, taken out of a message before it is
//! written into the session.
//!
//! Claude Code checks a prompt for invisible characters as it is submitted, and when it finds any
//! it removes them and does not submit: the cleaned text is left in the input box with "Removed N
//! invisible characters · review and press Enter to send". The Enter that follows a message in
//! the same write is then spent on that hold, so the message sits unsent in the session while the
//! write itself succeeded. Bidirectional isolates are the common case: the UI wraps the values it
//! substitutes into its copy in them, so they keep their order in right-to-left text, and an
//! agent that copies such a value into a report carries them along.
//!
//! So everything that check removes is removed here first, and what it would keep is kept: the
//! set and the emoji exceptions below follow Claude Code 2.1.304's own check, as recorded in
//! "Writing into a running session" in `docs/agent-cli-reference.md`. Its other exceptions are not
//! followed, and those characters are removed instead: a joiner or zero-width space between letters
//! of particular scripts and a direction mark inside a right-to-left line rest on Unicode script
//! data the daemon does not carry, and the three tag-sequence flags (England, Scotland, Wales) are
//! too rare to be worth a rule of their own, so each arrives as a plain black flag. That is the
//! safe direction: removing more costs a joined letterform, a direction mark or a flag, keeping one
//! too many costs the message.

/// U+200D ZERO WIDTH JOINER.
const ZWJ: char = '\u{200D}';
/// U+FE0E and U+FE0F, the text and emoji presentation selectors.
const TEXT_PRESENTATION: char = '\u{FE0E}';
const EMOJI_PRESENTATION: char = '\u{FE0F}';
/// U+20E3 COMBINING ENCLOSING KEYCAP, which turns a digit, `#` or `*` into a keycap emoji.
const KEYCAP: char = '\u{20E3}';

/// Removes the characters Claude Code's prompt would remove as invisible, and turns a line or
/// paragraph separator into a newline, which is what that check turns them into.
///
/// Control characters (`Cc`) are expected to be gone already; the caller strips those for a
/// reason of its own.
pub(super) fn strip_invisible(text: &str) -> String {
    if !text.chars().any(is_invisible) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    // The emoji exceptions look back at what has been written, not at the original text: that is
    // what the agent's check will see.
    let mut written = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        if !is_invisible(c) {
            written.push(c);
            continue;
        }
        if matches!(c, '\u{2028}' | '\u{2029}') {
            written.push('\n');
            continue;
        }
        // Read from the original text: when the next character is itself invisible it is never a
        // pictograph or the keycap mark, so the joiner or selector goes too — at most one removal
        // more than the agent's check needs, never one fewer.
        let next = chars.get(i + 1).copied();
        let keep = match c {
            ZWJ => joins_emoji(&written, next),
            TEXT_PRESENTATION | EMOJI_PRESENTATION => {
                selects_presentation(written.chars().next_back(), next)
            }
            _ => false,
        };
        if keep {
            written.push(c);
        }
    }
    written
}

/// A joiner between two pictographs, as in 🧑‍💻: the one before it may carry an emoji
/// presentation selector or a skin tone, which the check looks past (twice at most).
fn joins_emoji(written: &str, next: Option<char>) -> bool {
    let mut before = written.chars().rev();
    let mut base = before.next();
    for _ in 0..2 {
        match base {
            Some(c) if c == EMOJI_PRESENTATION || is_skin_tone(c) => base = before.next(),
            _ => break,
        }
    }
    base.is_some_and(is_extended_pictographic) && next.is_some_and(is_extended_pictographic)
}

/// A presentation selector right after an emoji, as in ⚠️, or between a keycap's base and the
/// keycap mark, as in 1️⃣.
fn selects_presentation(previous: Option<char>, next: Option<char>) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    let emoji = is_extended_pictographic(previous)
        || is_skin_tone(previous)
        || ('\u{1F1E6}'..='\u{1F1FF}').contains(&previous);
    let keycap = matches!(previous, '0'..='9' | '#' | '*') && next == Some(KEYCAP);
    emoji || keycap
}

fn is_skin_tone(c: char) -> bool {
    ('\u{1F3FB}'..='\u{1F3FF}').contains(&c)
}

/// The characters, apart from control characters, that Claude Code 2.1.304's prompt removes as
/// invisible unless an exception keeps them.
fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{AD}'
            | '\u{34F}'
            | '\u{61C}'
            | '\u{115F}'
            | '\u{1160}'
            | '\u{17B4}'
            | '\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF0}'..='\u{FFFB}'
            | '\u{1107F}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{16FE4}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

fn is_extended_pictographic(c: char) -> bool {
    let c = c as u32;
    EXTENDED_PICTOGRAPHIC
        .binary_search_by(|&(start, end)| {
            if end < c {
                std::cmp::Ordering::Less
            } else if start > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// The `Extended_Pictographic` property of Unicode 17.0, as inclusive ranges in order. It is
/// contained in 16.0's set, so it is no wider than what an agent whose runtime is on 16.0 tests
/// against — the direction that matters, since a joiner kept here that the agent would remove is
/// what holds a message back. The Unicode version Claude Code's own runtime carries is not known.
const EXTENDED_PICTOGRAPHIC: &[(u32, u32)] = &[
    (0x00A9, 0x00A9),
    (0x00AE, 0x00AE),
    (0x203C, 0x203C),
    (0x2049, 0x2049),
    (0x2122, 0x2122),
    (0x2139, 0x2139),
    (0x2194, 0x2199),
    (0x21A9, 0x21AA),
    (0x231A, 0x231B),
    (0x2328, 0x2328),
    (0x23CF, 0x23CF),
    (0x23E9, 0x23F3),
    (0x23F8, 0x23FA),
    (0x24C2, 0x24C2),
    (0x25AA, 0x25AB),
    (0x25B6, 0x25B6),
    (0x25C0, 0x25C0),
    (0x25FB, 0x25FE),
    (0x2600, 0x2604),
    (0x260E, 0x260E),
    (0x2611, 0x2611),
    (0x2614, 0x2615),
    (0x2618, 0x2618),
    (0x261D, 0x261D),
    (0x2620, 0x2620),
    (0x2622, 0x2623),
    (0x2626, 0x2626),
    (0x262A, 0x262A),
    (0x262E, 0x262F),
    (0x2638, 0x263A),
    (0x2640, 0x2640),
    (0x2642, 0x2642),
    (0x2648, 0x2653),
    (0x265F, 0x2660),
    (0x2663, 0x2663),
    (0x2665, 0x2666),
    (0x2668, 0x2668),
    (0x267B, 0x267B),
    (0x267E, 0x267F),
    (0x2692, 0x2697),
    (0x2699, 0x2699),
    (0x269B, 0x269C),
    (0x26A0, 0x26A1),
    (0x26A7, 0x26A7),
    (0x26AA, 0x26AB),
    (0x26B0, 0x26B1),
    (0x26BD, 0x26BE),
    (0x26C4, 0x26C5),
    (0x26C8, 0x26C8),
    (0x26CE, 0x26CF),
    (0x26D1, 0x26D1),
    (0x26D3, 0x26D4),
    (0x26E9, 0x26EA),
    (0x26F0, 0x26F5),
    (0x26F7, 0x26FA),
    (0x26FD, 0x26FD),
    (0x2702, 0x2702),
    (0x2705, 0x2705),
    (0x2708, 0x270D),
    (0x270F, 0x270F),
    (0x2712, 0x2712),
    (0x2714, 0x2714),
    (0x2716, 0x2716),
    (0x271D, 0x271D),
    (0x2721, 0x2721),
    (0x2728, 0x2728),
    (0x2733, 0x2734),
    (0x2744, 0x2744),
    (0x2747, 0x2747),
    (0x274C, 0x274C),
    (0x274E, 0x274E),
    (0x2753, 0x2755),
    (0x2757, 0x2757),
    (0x2763, 0x2764),
    (0x2795, 0x2797),
    (0x27A1, 0x27A1),
    (0x27B0, 0x27B0),
    (0x27BF, 0x27BF),
    (0x2934, 0x2935),
    (0x2B05, 0x2B07),
    (0x2B1B, 0x2B1C),
    (0x2B50, 0x2B50),
    (0x2B55, 0x2B55),
    (0x3030, 0x3030),
    (0x303D, 0x303D),
    (0x3297, 0x3297),
    (0x3299, 0x3299),
    (0x1F004, 0x1F004),
    (0x1F02C, 0x1F02F),
    (0x1F094, 0x1F09F),
    (0x1F0AF, 0x1F0B0),
    (0x1F0C0, 0x1F0C0),
    (0x1F0CF, 0x1F0D0),
    (0x1F0F6, 0x1F0FF),
    (0x1F170, 0x1F171),
    (0x1F17E, 0x1F17F),
    (0x1F18E, 0x1F18E),
    (0x1F191, 0x1F19A),
    (0x1F1AE, 0x1F1E5),
    (0x1F201, 0x1F20F),
    (0x1F21A, 0x1F21A),
    (0x1F22F, 0x1F22F),
    (0x1F232, 0x1F23A),
    (0x1F23C, 0x1F23F),
    (0x1F249, 0x1F25F),
    (0x1F266, 0x1F321),
    (0x1F324, 0x1F393),
    (0x1F396, 0x1F397),
    (0x1F399, 0x1F39B),
    (0x1F39E, 0x1F3F0),
    (0x1F3F3, 0x1F3F5),
    (0x1F3F7, 0x1F3FA),
    (0x1F400, 0x1F4FD),
    (0x1F4FF, 0x1F53D),
    (0x1F549, 0x1F54E),
    (0x1F550, 0x1F567),
    (0x1F56F, 0x1F570),
    (0x1F573, 0x1F57A),
    (0x1F587, 0x1F587),
    (0x1F58A, 0x1F58D),
    (0x1F590, 0x1F590),
    (0x1F595, 0x1F596),
    (0x1F5A4, 0x1F5A5),
    (0x1F5A8, 0x1F5A8),
    (0x1F5B1, 0x1F5B2),
    (0x1F5BC, 0x1F5BC),
    (0x1F5C2, 0x1F5C4),
    (0x1F5D1, 0x1F5D3),
    (0x1F5DC, 0x1F5DE),
    (0x1F5E1, 0x1F5E1),
    (0x1F5E3, 0x1F5E3),
    (0x1F5E8, 0x1F5E8),
    (0x1F5EF, 0x1F5EF),
    (0x1F5F3, 0x1F5F3),
    (0x1F5FA, 0x1F64F),
    (0x1F680, 0x1F6C5),
    (0x1F6CB, 0x1F6D2),
    (0x1F6D5, 0x1F6E5),
    (0x1F6E9, 0x1F6E9),
    (0x1F6EB, 0x1F6F0),
    (0x1F6F3, 0x1F6FF),
    (0x1F7DA, 0x1F7FF),
    (0x1F80C, 0x1F80F),
    (0x1F848, 0x1F84F),
    (0x1F85A, 0x1F85F),
    (0x1F888, 0x1F88F),
    (0x1F8AE, 0x1F8AF),
    (0x1F8BC, 0x1F8BF),
    (0x1F8C2, 0x1F8CF),
    (0x1F8D9, 0x1F8FF),
    (0x1F90C, 0x1F93A),
    (0x1F93C, 0x1F945),
    (0x1F947, 0x1F9FF),
    (0x1FA58, 0x1FA5F),
    (0x1FA6E, 0x1FAFF),
    (0x1FC00, 0x1FFFD),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bidirectional_isolates_are_removed() {
        assert_eq!(
            strip_invisible("Branch \u{2068}feature/login\u{2069} is ready"),
            "Branch feature/login is ready"
        );
    }

    #[test]
    fn zero_width_and_formatting_characters_are_removed() {
        let text = "a\u{200B}b\u{200C}c\u{200E}d\u{202E}e\u{2060}f\u{FEFF}g\u{AD}h\u{E0041}i";
        assert_eq!(strip_invisible(text), "abcdefghi");
    }

    #[test]
    fn line_and_paragraph_separators_become_newlines() {
        assert_eq!(
            strip_invisible("one\u{2028}two\u{2029}three"),
            "one\ntwo\nthree"
        );
    }

    #[test]
    fn emoji_sequences_keep_their_joiners_and_selectors() {
        for emoji in [
            "\u{1F9D1}\u{200D}\u{1F4BB}",                  // technologist
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}", // family
            "\u{1F44D}\u{1F3FD}",                          // thumbs up, medium skin tone
            "\u{1F9D1}\u{1F3FD}\u{200D}\u{1F4BB}",         // technologist, skin tone
            "\u{2764}\u{FE0F}\u{200D}\u{1F525}",           // heart on fire
            "\u{26A0}\u{FE0F}",                            // warning
            "\u{2714}\u{FE0E}",                            // check mark, text style
            "1\u{FE0F}\u{20E3}",                           // keycap one
        ] {
            let text = format!("status {emoji} done");
            assert_eq!(strip_invisible(&text), text, "{emoji:?}");
        }
    }

    /// The check looks back past two modifiers at most, so a pictograph three modifiers back does
    /// not hold the joiner.
    #[test]
    fn a_joiner_looks_back_past_two_modifiers_at_most() {
        assert_eq!(
            strip_invisible("\u{1F9D1}\u{1F3FD}\u{FE0F}\u{200D}\u{1F4BB}"),
            "\u{1F9D1}\u{1F3FD}\u{FE0F}\u{200D}\u{1F4BB}"
        );
        assert_eq!(
            strip_invisible("\u{1F9D1}\u{1F3FD}\u{1F3FD}\u{1F3FD}\u{200D}\u{1F4BB}"),
            "\u{1F9D1}\u{1F3FD}\u{1F3FD}\u{1F3FD}\u{1F4BB}"
        );
    }

    #[test]
    fn a_selector_after_a_regional_indicator_is_kept() {
        assert_eq!(strip_invisible("\u{1F1EF}\u{FE0F}"), "\u{1F1EF}\u{FE0F}");
    }

    #[test]
    fn a_joiner_or_selector_outside_an_emoji_is_removed() {
        assert_eq!(strip_invisible("a\u{200D}b"), "ab");
        assert_eq!(strip_invisible("\u{200D}\u{1F4BB}"), "\u{1F4BB}");
        assert_eq!(strip_invisible("\u{1F9D1}\u{200D}"), "\u{1F9D1}");
        assert_eq!(strip_invisible("x\u{FE0F}"), "x");
        assert_eq!(
            strip_invisible("\u{26A0}\u{FE0F}\u{FE0F}"),
            "\u{26A0}\u{FE0F}"
        );
        assert_eq!(strip_invisible("1\u{FE0F}"), "1");
    }

    /// A joiner whose next neighbour is removed is removed too — one removal more than needed, the
    /// safe direction — while one whose removed neighbour came before it is judged against what is
    /// left, which is what the agent sees.
    #[test]
    fn a_joiner_is_judged_by_the_neighbours_that_are_written() {
        assert_eq!(
            strip_invisible("\u{1F9D1}\u{200D}\u{2069}\u{1F4BB}"),
            "\u{1F9D1}\u{1F4BB}"
        );
        assert_eq!(
            strip_invisible("\u{1F9D1}\u{2069}\u{200D}\u{1F4BB}"),
            "\u{1F9D1}\u{200D}\u{1F4BB}"
        );
        assert_eq!(
            strip_invisible("\u{1F9D1}\u{200D}\u{200D}\u{1F4BB}"),
            "\u{1F9D1}\u{200D}\u{1F4BB}"
        );
    }

    #[test]
    fn visible_text_passes_through_untouched() {
        let text = "Plain, 中文, العربية, हिन्दी, ไทย, \u{E0B0} private use, \u{A0}no-break";
        assert_eq!(strip_invisible(text), text);
    }
}
