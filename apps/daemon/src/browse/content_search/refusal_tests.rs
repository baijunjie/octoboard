//! Why a content search's query or file mask is refused: the reason and the position a client
//! words the refusal from (`browse::content_search::refusal`).

use super::{Compiled, Query};
use crate::protocol::{error_code, CodedError, Params};

fn refusal(query: Query) -> CodedError {
    let err = Compiled::new(&query).err().expect("refused");
    err.downcast_ref::<CodedError>().unwrap().clone()
}

fn regex(text: &str) -> Params {
    let coded = refusal(Query {
        text: text.to_string(),
        regex: true,
        ..Query::default()
    });
    assert_eq!(coded.code, error_code::INVALID_SEARCH_PATTERN, "{coded:?}");
    coded.params
}

fn mask(mask: &str) -> Params {
    let coded = refusal(Query {
        text: "x".to_string(),
        file_mask: Some(mask.to_string()),
        ..Query::default()
    });
    assert_eq!(coded.code, error_code::INVALID_FILE_MASK, "{coded:?}");
    // The refuser's own message names the glob.
    assert!(coded.message.contains(&coded.params["glob"]), "{coded:?}");
    coded.params
}

fn cause_and_offset(params: &Params) -> (&str, Option<&str>) {
    (
        params["cause"].as_str(),
        params.get("offset").map(String::as_str),
    )
}

#[test]
fn a_regular_expression_is_refused_with_its_reason_and_where_it_fails() {
    for (text, cause, offset) in [
        ("reindex(", "unclosed_group", "7"),
        ("a(b|c", "unclosed_group", "1"),
        ("ab)", "unopened_group", "2"),
        ("x[abc", "unclosed_class", "1"),
        ("*abc", "invalid_repetition", "0"),
        ("a{2,1}", "invalid_repetition", "1"),
        ("a\\qb", "invalid_escape", "1"),
        ("\\p{NoSuchClass}", "invalid_escape", "0"),
        ("a(?=b)", "unsupported", "1"),
        ("(a)\\1", "unsupported", "3"),
    ] {
        let params = regex(text);
        assert_eq!(
            cause_and_offset(&params),
            (cause, Some(offset)),
            "{text}: {params:?}"
        );
        assert!(!params["detail"].is_empty(), "{text}: the detail stays");
    }
}

#[test]
fn the_offset_counts_utf16_code_units_of_the_query() {
    // "检索" is two UTF-16 units (six bytes), "😀" two units (four bytes).
    assert_eq!(
        cause_and_offset(&regex("检索(")),
        ("unclosed_group", Some("2"))
    );
    assert_eq!(
        cause_and_offset(&regex("😀a)")),
        ("unopened_group", Some("3"))
    );
}

#[test]
fn a_pattern_that_must_match_a_line_break_is_refused_without_an_offset() {
    assert_eq!(cause_and_offset(&regex("a\\nb")), ("line_break", None));
}

#[test]
fn a_glob_is_refused_with_its_reason() {
    for (given, glob, cause) in [
        ("*.ts, src/[a", "src/[a", "unclosed_class"),
        ("*.{md", "*.{md", "unclosed_alternation"),
        // The comma separates globs, so a brace around one splits it.
        ("*.{md,ts}", "*.{md", "unclosed_alternation"),
        ("a}", "a}", "unopened_alternation"),
        ("[z-a]", "[z-a]", "invalid_range"),
        ("!a\\", "!a\\", "other"),
    ] {
        let params = mask(given);
        assert_eq!(
            (params["glob"].as_str(), params["cause"].as_str()),
            (glob, cause),
            "{given}: {params:?}"
        );
        assert!(!params["detail"].is_empty(), "{given}: the detail stays");
    }
}
