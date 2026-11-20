//! The name an attached file is saved under, made from the name the client gives.
//!
//! The agent sees the name, so it stays recognisable: letters and digits of any script, dots,
//! hyphens and underscores are kept, the extension with them. Everything else — a path separator,
//! whitespace, quotes, brackets, colons, control characters — becomes an underscore, so the name
//! can neither leave the attachments directory nor hold anything an agent's path detection or a
//! shell splits on, and the path can be pasted bare.

/// The longest name, in characters, before a collision's number.
const MAX_CHARS: usize = 120;

/// The longest name, in bytes of UTF-8, before a collision's number: Linux file systems take
/// names of at most 255 bytes, which a name of 120 characters of a script written in three or four
/// bytes each would pass, and a collision's `-n` adds a few more.
const MAX_NAME_BYTES: usize = 200;

/// The longest extension kept as one, in characters, its dot included.
const MAX_EXTENSION_CHARS: usize = 16;

/// The name of a file whose own name leaves nothing.
const FALLBACK: &str = "attachment";

/// Names Windows takes for a device in any directory, whatever the extension.
const WINDOWS_DEVICES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// The safe form of `original`, a file's name as the client has it (a path is cut to its last
/// part): at most [`MAX_CHARS`] characters and [`MAX_NAME_BYTES`] bytes of letters, digits, `.`, `-`
/// and `_`, beginning with none of `.` or `-`, nor with `_` except the one a Windows device name
/// is given in front so that it is none, ending with neither `.` nor `_`, and `attachment` where
/// nothing is left.
pub fn safe_name(original: &str) -> String {
    let last = original.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut name = String::with_capacity(last.len());
    for c in last.chars() {
        let c = if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
            c
        } else {
            '_'
        };
        if !(c == '_' && name.ends_with('_')) {
            name.push(c);
        }
    }
    let name = name
        .trim_start_matches(['.', '-', '_'])
        .trim_end_matches(['.', '_']);
    if name.is_empty() {
        return FALLBACK.to_string();
    }
    // Prefixed before the cut, which only takes from the end, so the prefix counts toward the
    // limit and the cut cannot leave a device name behind.
    let device = name.split('.').next().unwrap_or_default();
    if WINDOWS_DEVICES
        .iter()
        .any(|reserved| device.eq_ignore_ascii_case(reserved))
    {
        return shortened(&format!("_{name}"));
    }
    shortened(name)
}

/// The `n`th name to try for a file named `name` (one [`safe_name`] gave): the name itself first,
/// then the name with `-n` before its extension (`report-2.pdf`, `logs-3.tar.gz`).
pub fn numbered(name: &str, n: u32) -> String {
    if n <= 1 {
        return name.to_string();
    }
    let (stem, extension) = split_extension(name);
    format!("{stem}-{n}{extension}")
}

/// `name` cut to [`MAX_CHARS`] characters and [`MAX_NAME_BYTES`] bytes, from the end of its stem so
/// the extension stays, on a character boundary, and with no `.` or `_` left at the end of the
/// stem.
fn shortened(name: &str) -> String {
    if name.chars().count() <= MAX_CHARS && name.len() <= MAX_NAME_BYTES {
        return name.to_string();
    }
    let (stem, extension) = split_extension(name);
    let mut chars_left = MAX_CHARS - extension.chars().count();
    let mut bytes_left = MAX_NAME_BYTES - extension.len();
    let mut cut = String::new();
    for c in stem.chars() {
        if chars_left == 0 || c.len_utf8() > bytes_left {
            break;
        }
        cut.push(c);
        chars_left -= 1;
        bytes_left -= c.len_utf8();
    }
    let stem = cut.trim_end_matches(['.', '_']);
    format!("{stem}{extension}")
}

/// `name` split before its extension, the dot with the extension: the last `.part` of up to
/// [`MAX_EXTENSION_CHARS`] after a stem of anything, together with a `.tar` before it (`.tar.gz`).
/// No extension where there is none or it is longer.
fn split_extension(name: &str) -> (&str, &str) {
    let Some(dot) = name.rfind('.').filter(|&dot| dot > 0) else {
        return (name, "");
    };
    if name[dot..].chars().count() > MAX_EXTENSION_CHARS {
        return (name, "");
    }
    let stem = &name[..dot];
    let tar = stem
        .len()
        .checked_sub(4)
        .filter(|&tar| tar > 0)
        .filter(|&tar| {
            stem.get(tar..)
                .is_some_and(|end| end.eq_ignore_ascii_case(".tar"))
        });
    let dot = tar.unwrap_or(dot);
    (&name[..dot], &name[dot..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_name_is_kept_as_it_is() {
        for name in [
            "report.pdf",
            "build-2026_11.log",
            "main.rs",
            "archive.tar.gz",
            "README",
        ] {
            assert_eq!(safe_name(name), name);
        }
    }

    #[test]
    fn letters_and_digits_of_any_script_are_kept() {
        assert_eq!(safe_name("报告2026.pdf"), "报告2026.pdf");
        assert_eq!(safe_name("Überblick.txt"), "Überblick.txt");
        assert_eq!(safe_name("ログ.txt"), "ログ.txt");
    }

    #[test]
    fn a_path_is_cut_to_its_last_part_and_cannot_climb_out() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("..\\..\\Windows\\win.ini"), "win.ini");
        assert_eq!(safe_name("/var/log/system.log"), "system.log");
        assert_eq!(safe_name(".."), "attachment");
        assert_eq!(safe_name("dir/"), "attachment");
    }

    #[test]
    fn whitespace_quotes_brackets_and_controls_become_one_underscore_each_run() {
        assert_eq!(safe_name("my report (final).pdf"), "my_report_final_.pdf");
        assert_eq!(safe_name("a  \t b.txt"), "a_b.txt");
        assert_eq!(safe_name("it's \"quoted\".md"), "it_s_quoted_.md");
        assert_eq!(
            safe_name("x:y;z,w[1]{2}<3>|4*5?6.log"),
            "x_y_z_w_1_2_3_4_5_6.log"
        );
        assert_eq!(safe_name("bell\u{7}\nline.txt"), "bell_line.txt");
        assert_eq!(safe_name("$HOME`id`.sh"), "HOME_id_.sh");
    }

    #[test]
    fn a_name_never_holds_anything_a_path_detection_splits_on() {
        let names = [
            "my report (final).pdf",
            "\u{a0}non-breaking\u{2003}space.txt",
            "tab\there.log",
            "quote'n\"dquote.txt",
            "emoji 🎉 party.png",
        ];
        for name in names {
            let safe = safe_name(name);
            assert!(
                safe.chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_')),
                "{name:?} gave {safe:?}"
            );
        }
    }

    #[test]
    fn a_name_neither_hides_nor_reads_as_an_option_nor_ends_in_a_dot() {
        assert_eq!(safe_name(".env"), "env");
        assert_eq!(safe_name("...hidden.txt"), "hidden.txt");
        assert_eq!(safe_name("-rf"), "rf");
        assert_eq!(safe_name("--help.txt"), "help.txt");
        assert_eq!(safe_name("_x_"), "x");
        assert_eq!(safe_name("trailing."), "trailing");
        assert_eq!(safe_name("name. "), "name");
    }

    #[test]
    fn a_name_that_leaves_nothing_is_attachment() {
        for name in ["", " ", "...", "///", "🎉", "(\t)", "-_-"] {
            assert_eq!(safe_name(name), "attachment", "{name:?}");
        }
    }

    #[test]
    fn a_windows_device_name_is_given_a_leading_underscore() {
        assert_eq!(safe_name("CON"), "_CON");
        assert_eq!(safe_name("nul.txt"), "_nul.txt");
        assert_eq!(safe_name("Com1.tar.gz"), "_Com1.tar.gz");
        assert_eq!(safe_name("console.txt"), "console.txt");
        assert_eq!(safe_name("COM10.txt"), "COM10.txt");
    }

    #[test]
    fn a_long_name_is_cut_from_its_stem_and_keeps_its_extension() {
        let long = format!("{}.pdf", "a".repeat(300));
        let safe = safe_name(&long);
        assert_eq!(safe.chars().count(), MAX_CHARS);
        assert!(safe.ends_with("a.pdf"), "{safe}");

        // An extension too long to be one is cut as part of the name.
        let odd = format!("x.{}", "e".repeat(200));
        assert_eq!(safe_name(&odd).chars().count(), MAX_CHARS);

        // A cut that ends on a separator does not leave it at the end of the stem.
        let separated = format!("{}_b.log", "a".repeat(MAX_CHARS - 5));
        let safe = safe_name(&separated);
        assert!(safe.ends_with("a.log"), "{safe}");
    }

    #[test]
    fn a_long_name_keeps_a_tar_extension_with_its_compression_suffix() {
        let safe = safe_name(&format!("{}.tar.gz", "a".repeat(300)));
        assert_eq!(safe.chars().count(), MAX_CHARS);
        assert!(safe.ends_with("a.tar.gz"), "{safe}");
    }

    #[test]
    fn a_device_name_left_after_trimming_is_still_prefixed() {
        assert_eq!(safe_name("CON."), "_CON");
        assert_eq!(safe_name("_nul_"), "_nul");
        assert_eq!(safe_name("dir/aux.tar.gz"), "_aux.tar.gz");
    }

    #[test]
    fn a_name_is_never_longer_than_the_limit_even_when_a_device_prefix_is_added() {
        let safe = safe_name(&format!("CON.{}", "e".repeat(200)));
        assert!(
            safe.chars().count() <= MAX_CHARS,
            "{} characters",
            safe.chars().count()
        );
        assert!(safe.starts_with("_CON"), "{safe}");
    }

    #[test]
    fn a_long_name_in_a_script_of_wide_characters_is_cut_to_the_byte_limit() {
        for wide in ["报", "𝒜", "ก"] {
            let long = format!("{}.txt", wide.repeat(150));
            let safe = safe_name(&long);
            assert!(safe.len() <= MAX_NAME_BYTES, "{} bytes: {safe}", safe.len());
            assert!(safe.ends_with(".txt"), "{safe}");
            assert!(
                safe.len() > MAX_NAME_BYTES - 4 - 4,
                "cut no further than needed: {safe}"
            );
            assert!(numbered(&safe, 1000).len() < 255);
        }
    }

    #[test]
    fn a_collision_numbers_the_name_before_its_extension() {
        assert_eq!(numbered("report.pdf", 1), "report.pdf");
        assert_eq!(numbered("report.pdf", 2), "report-2.pdf");
        assert_eq!(numbered("logs.tar.gz", 3), "logs-3.tar.gz");
        assert_eq!(numbered("README", 2), "README-2");
        assert_eq!(numbered("my.notes.v2.md", 2), "my.notes.v2-2.md");
        assert_eq!(numbered("x.tar", 2), "x-2.tar");
        assert_eq!(numbered("报告.gz", 2), "报告-2.gz");
    }
}
