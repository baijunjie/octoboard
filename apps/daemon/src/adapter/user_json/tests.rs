use serde_json::json;

use super::*;

// The four ways the providers' own readers read today, each from what its agent's CLI was measured
// to take (`docs/agent-cli-reference.md`); a provider states its own when it adopts the reader.

/// Comments and a byte order mark, no trailing comma: Qwen Code's settings files.
const COMMENTS_AND_MARK: Takes = Takes {
    byte_order_mark: true,
    comments: true,
    trailing_commas: false,
    leading_comment_lines: false,
};

/// Comments and trailing commas, no byte order mark: OpenCode's inline configuration.
const COMMENTS_AND_COMMAS: Takes = Takes {
    byte_order_mark: false,
    comments: true,
    trailing_commas: true,
    leading_comment_lines: false,
};

/// A byte order mark in front of strict JSON: Antigravity CLI's `hooks.json`, `mcp_config.json`
/// and `settings.json`.
const MARK_ONLY: Takes = Takes {
    byte_order_mark: true,
    comments: false,
    trailing_commas: false,
    leading_comment_lines: false,
};

/// Comment lines above strict JSON: GitHub Copilot CLI's `config.json`.
const HEADER_LINES: Takes = Takes {
    byte_order_mark: false,
    comments: false,
    trailing_commas: false,
    leading_comment_lines: true,
};

const STRICT: Takes = Takes {
    byte_order_mark: false,
    comments: false,
    trailing_commas: false,
    leading_comment_lines: false,
};

fn read(text: &str, takes: Takes) -> Value {
    parse(text, takes).unwrap_or_else(|why| panic!("{text:?}: {why}"))
}

fn refused(text: &str, takes: Takes) {
    if let Ok(value) = parse(text, takes) {
        panic!("{text:?} was read as {value}");
    }
}

/// One text per thing an agent may take beyond JSON, each of which is JSON once that thing is off.
const WITH_MARK: &str = "\u{FEFF}{\"a\": 1}";
const WITH_LINE_COMMENT: &str = "{\"a\": 1} // said";
const WITH_BLOCK_COMMENT: &str = "{/* said */\"a\": 1}";
const WITH_TRAILING_COMMA: &str = "{\"a\": 1,}";
const WITH_HEADER_LINE: &str = "// said\n{\"a\": 1}";

#[test]
fn the_default_takes_nothing_beyond_json() {
    assert_eq!(Takes::default(), STRICT);
    assert_eq!(
        read("{\"a\": [1, \"x\"]}", STRICT),
        json!({ "a": [1, "x"] })
    );
    for text in [
        WITH_MARK,
        WITH_LINE_COMMENT,
        WITH_BLOCK_COMMENT,
        WITH_TRAILING_COMMA,
        WITH_HEADER_LINE,
    ] {
        refused(text, STRICT);
    }
}

/// Whether a caller's allowances take the one thing a case's text needs.
type Allowed = fn(Takes) -> bool;

/// Each thing is taken only where the caller says its agent takes it, whatever else it takes.
#[test]
fn each_allowance_is_taken_only_where_the_caller_says_so() {
    let cases: [(&str, Allowed); 5] = [
        (WITH_MARK, |takes| takes.byte_order_mark),
        (WITH_LINE_COMMENT, |takes| takes.comments),
        (WITH_BLOCK_COMMENT, |takes| takes.comments),
        (WITH_TRAILING_COMMA, |takes| takes.trailing_commas),
        // A comment line above the JSON is a comment like any other to an agent that takes those.
        (WITH_HEADER_LINE, |takes| {
            takes.leading_comment_lines || takes.comments
        }),
    ];
    for bits in 0..16u8 {
        let takes = Takes {
            byte_order_mark: bits & 1 != 0,
            comments: bits & 2 != 0,
            trailing_commas: bits & 4 != 0,
            leading_comment_lines: bits & 8 != 0,
        };
        for (text, taken) in cases {
            match parse(text, takes) {
                Ok(value) => {
                    assert!(taken(takes), "{text:?} was read with {takes:?}");
                    assert_eq!(value, json!({ "a": 1 }), "{text:?} with {takes:?}");
                }
                Err(_) => assert!(!taken(takes), "{text:?} was refused with {takes:?}"),
            }
        }
    }
}

/// `//` and `/* */` comments are taken off, wherever they stand outside a string.
#[test]
fn comments_outside_strings_are_taken_off() {
    let text = "{\n  // a line comment\n  \"a\": 1, /* a block\n  spanning lines */ \"b\": \"x\", // trailing\n  /* before */ \"c\": [1, /* inside */ 2]\n}\n// after the object";
    assert_eq!(
        read(text, COMMENTS_AND_MARK),
        json!({ "a": 1, "b": "x", "c": [1, 2] })
    );
    // A comment of any kind is also the whole of the text between values.
    assert_eq!(
        read("/**/{/**/\"a\"/**/:/**/1/**/}/**/", COMMENTS_AND_MARK),
        json!({ "a": 1 })
    );
    // A line comment ends at a carriage return as at a line feed.
    assert_eq!(
        read("{ // said\r\"a\": 1 }", COMMENTS_AND_MARK),
        json!({ "a": 1 })
    );
}

/// Comment markers, a comma before a closing bracket and an agent's own substitution syntax are
/// text inside a string, whatever escapes come before them.
#[test]
fn what_is_inside_a_string_is_left_alone() {
    let text = r#"{
  "url": "https://example.test/a//b",
  "block": "/* not a comment */ text",
  "quote": "escaped \" // still the string",
  "comma": ",}",
  "list": "say \" // still inside \", ]",
  "env": "{env:HOME}/bin",
  "path": "C:\\" // a comment after a string that ends in an escaped backslash
}"#;
    let expected = json!({
        "url": "https://example.test/a//b",
        "block": "/* not a comment */ text",
        "quote": "escaped \" // still the string",
        "comma": ",}",
        "list": "say \" // still inside \", ]",
        "env": "{env:HOME}/bin",
        "path": "C:\\",
    });
    assert_eq!(read(text, COMMENTS_AND_MARK), expected);
    assert_eq!(read(text, COMMENTS_AND_COMMAS), expected);
    // With nothing taken, the string is as much a string: only the comment after it refuses.
    assert_eq!(
        read(r#"{"a": "// x", "b": "/* y */", "c": ",]"}"#, STRICT),
        json!({ "a": "// x", "b": "/* y */", "c": ",]" })
    );
}

/// An agent that takes comments and a byte order mark, and no trailing comma.
#[test]
fn comments_and_a_mark_without_trailing_commas() {
    assert_eq!(read(WITH_MARK, COMMENTS_AND_MARK), json!({ "a": 1 }));
    assert_eq!(
        read("\u{FEFF}// comment\n{\"a\": 1}", COMMENTS_AND_MARK),
        json!({ "a": 1 })
    );
    // The value is whatever JSON it is: what a caller needs it to be is the caller's to judge.
    assert_eq!(read("[1, 2]", COMMENTS_AND_MARK), json!([1, 2]));
    assert_eq!(read("null", COMMENTS_AND_MARK), Value::Null);
    for text in [
        "",
        "   \n",
        "// only a comment",
        "{ not json",
        WITH_TRAILING_COMMA,
        "[1, 2,]",
        // The mark is taken in front of the text only.
        "{\"a\": \u{FEFF}1}",
        "\u{FEFF}\u{FEFF}{\"a\": 1}",
    ] {
        refused(text, COMMENTS_AND_MARK);
    }
}

/// An agent that takes comments and a comma before a closing bracket or brace.
#[test]
fn comments_and_trailing_commas() {
    let expected = json!({ "a": 1, "b": [1, 2], "c": { "d": true } });
    for text in [
        "{\"a\": 1, // line comment\n \"b\": [1, 2,], \"c\": {\"d\": true,},}",
        "/* leading */ {\"a\": 1 /* mid */, \"b\": [1, /* in */ 2], \"c\": {\"d\": true}} // last",
        "{\"a\": 1,\n \"b\": [1, 2, // end of the list\n ],\n \"c\": {\"d\": true, /* x */ },\n}",
        "{\"a\": 1, \"b\": [1, 2], \"c\": {\"d\": true}}",
    ] {
        assert_eq!(read(text, COMMENTS_AND_COMMAS), expected, "{text}");
    }
    assert_eq!(
        read("[\"x\",]", COMMENTS_AND_COMMAS),
        json!(["x"]),
        "a comma after a string"
    );
    assert_eq!(
        read("[[1,],{\"a\":null,},]", COMMENTS_AND_COMMAS),
        json!([[1], { "a": null }])
    );
    for text in [
        r#"{"theme": "x", "broken": }"#,
        // A substitution written outside a string is JSON only once the agent has replaced it.
        r#"{"theme": "x", "model": {env:MODEL}}"#,
        r#"{"theme": "x" /* never closed"#,
        r#"{"theme": "x",, "other": 1}"#,
        "// only a comment",
        // A comma with no value before it is not a trailing one.
        "[,]",
        "{,}",
        "[1,,]",
        "{\"a\":,}",
        // Nor is one outside every bracket.
        "{\"a\": 1},",
        WITH_MARK,
    ] {
        refused(text, COMMENTS_AND_COMMAS);
    }
}

/// An agent that reads strict JSON with a byte order mark taken off refuses a comment and a
/// trailing comma.
#[test]
fn a_mark_in_front_of_strict_json() {
    assert_eq!(
        read(
            "\u{FEFF}{\"mcpServers\":{\"mine\":{\"command\":\"x\"}}}",
            MARK_ONLY
        ),
        json!({ "mcpServers": { "mine": { "command": "x" } } })
    );
    assert_eq!(read("{}", MARK_ONLY), json!({}));
    assert_eq!(
        read("{\"mcpServers\":null}", MARK_ONLY),
        json!({ "mcpServers": null })
    );
    for text in [
        "{ // a comment\n}",
        "/* a comment */ {}",
        r#"{"a":1,}"#,
        WITH_HEADER_LINE,
        // What the agent reads as nothing configured is no JSON value: the caller's to take.
        "",
        "  \n\t ",
        "\u{FEFF}",
    ] {
        refused(text, MARK_ONLY);
    }
}

/// An agent whose file opens with `//` comment lines above strict JSON.
#[test]
fn comment_lines_above_strict_json() {
    let state = "// The agent keeps its state in this file.\n// Do not edit it by hand.\n{\n  \"loggedInUsers\": [\n    { \"host\": \"https://github.com\", \"login\": \"octocat\" }\n  ]\n}\n";
    let expected = json!({
        "loggedInUsers": [{ "host": "https://github.com", "login": "octocat" }],
    });
    assert_eq!(read(state, HEADER_LINES), expected);
    // Indented comment lines go too, with blank lines among them and either line ending, and the
    // host's `//` inside a value is not a comment.
    let indented = format!(
        "   // a comment\r\n\n\t// another\n{}",
        state.lines().skip(2).collect::<Vec<_>>().join("\n")
    );
    assert_eq!(read(&indented, HEADER_LINES), expected);
    assert_eq!(read("{\"a\": 1}", HEADER_LINES), json!({ "a": 1 }));
    for text in [
        "// only comments\n// and nothing else\n",
        "// only a comment",
        "",
        // A comment below the first line of JSON, after a value or in a block is not a header.
        "{\n// inside\n\"a\": 1}",
        "{\"a\": 1}\n// below",
        WITH_LINE_COMMENT,
        "/* a block */\n{\"a\": 1}",
        "// a header\n/* then a block */\n{\"a\": 1}",
        WITH_TRAILING_COMMA,
        "\u{FEFF}// a header\n{\"a\": 1}",
        // JSON cut short, or not JSON.
        "// a header\n{\"loggedInUsers\":[{\"login\":\"octocat\"",
        "not json at all",
    ] {
        refused(text, HEADER_LINES);
    }
}

/// A comment that is never closed runs to the end of the text.
#[test]
fn an_open_block_comment_runs_to_the_end() {
    assert_eq!(
        read("{\"a\": 1} /* never closed", COMMENTS_AND_MARK),
        json!({ "a": 1 })
    );
    refused("{\"a\": 1 /* never closed }", COMMENTS_AND_MARK);
}

/// What is taken off leaves every other character where it was, so a parse error names the line
/// and column of the text as written.
#[test]
fn positions_are_those_of_the_text_as_written() {
    let all = Takes {
        byte_order_mark: true,
        comments: true,
        trailing_commas: true,
        leading_comment_lines: true,
    };
    let text = "  // header\n{\n  /* x */ \"a\": [1, 2, /* y */ ], // z\n  \"b\": oops,\n}";
    let strict = strict_text(text, all);
    assert_eq!(strict.len(), text.len());
    for (written, left) in text.chars().zip(strict.chars()) {
        assert!(
            left == written || left == ' ',
            "{written:?} became {left:?}"
        );
        assert_eq!(left == '\n', written == '\n');
    }
    let err = parse(text, all).expect_err("`oops` is no value");
    assert_eq!((err.line(), err.column()), (4, 8));
    // The mark is dropped, not blanked: it is in front of the first line and no parser counts it.
    assert_eq!(strict_text(WITH_MARK, all), "{\"a\": 1}");
    assert_eq!(strict_text(WITH_MARK, STRICT), WITH_MARK);
}
