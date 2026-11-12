use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Output};

use super::{
    account_dir, admitted_command, default_account_dir, failure_detail, NotRun, DETAIL_LINE_LIMIT,
};
use crate::protocol::AgentId;
use crate::test_support::ScratchDir;

/// Each agent's configuration directory: the variable it reads it from, set in the shell
/// snapshot, else its own directory under the snapshot's home; an empty variable counts as
/// unset.
#[test]
fn each_agents_default_account_is_its_variable_or_its_directory_under_home() {
    for (agent, var, dir) in [
        (AgentId::CLAUDE, "CLAUDE_CONFIG_DIR", ".claude"),
        (AgentId::CODEX, "CODEX_HOME", ".codex"),
        (AgentId::GROK, "GROK_HOME", ".grok"),
    ] {
        assert_eq!(crate::providers::layout::layout(agent).config_dir_var, var);
        let home = HashMap::from([("HOME".to_string(), "/home/user".to_string())]);
        assert_eq!(
            default_account_dir(agent, &home),
            PathBuf::from("/home/user").join(dir)
        );
        let mut exported = home.clone();
        exported.insert(var.to_string(), String::new());
        assert_eq!(
            default_account_dir(agent, &exported),
            PathBuf::from("/home/user").join(dir)
        );
        exported.insert(var.to_string(), "/elsewhere".to_string());
        assert_eq!(
            default_account_dir(agent, &exported),
            PathBuf::from("/elsewhere")
        );
    }
}

/// An account's own directory is what its sessions run in, whatever the shell exports; the
/// default account's follows the shell.
#[test]
fn a_pinned_account_dir_wins_over_the_default_one() {
    let shell = HashMap::from([
        ("HOME".to_string(), "/home/user".to_string()),
        ("CLAUDE_CONFIG_DIR".to_string(), "/exported".to_string()),
    ]);
    assert_eq!(
        account_dir(
            AgentId::CLAUDE,
            Some(std::path::Path::new("/pinned")),
            &shell
        ),
        PathBuf::from("/pinned")
    );
    assert_eq!(
        account_dir(AgentId::CLAUDE, None, &shell),
        PathBuf::from("/exported")
    );
}

/// A shell environment whose `PATH` holds no agent binary.
fn no_agents(dir: &Path) -> HashMap<String, String> {
    HashMap::from([
        ("PATH".to_string(), dir.to_string_lossy().into_owned()),
        ("HOME".to_string(), dir.to_string_lossy().into_owned()),
    ])
}

/// A gone directory is refused before the binary is looked for, so the account's missing
/// directory is what a caller reports even where the CLI is not installed either.
#[test]
fn a_missing_directory_is_refused_before_the_binary_is_resolved() {
    let dir = ScratchDir::new("cli-run-missing");
    let gone = dir.join("gone");
    let refused = admitted_command(AgentId::CODEX, &no_agents(&dir), Some(&gone), &[])
        .err()
        .expect("refused");
    assert!(matches!(&refused, NotRun::DirMissing(missing) if *missing == gone));
    assert_eq!(
        refused.detail(),
        format!(
            "the account's configuration directory {} does not exist",
            gone.display()
        )
    );
}

#[test]
fn a_binary_off_the_path_is_named() {
    let dir = ScratchDir::new("cli-run-no-binary");
    let refused = admitted_command(AgentId::GROK, &no_agents(&dir), Some(&dir), &[])
        .err()
        .expect("refused");
    assert!(matches!(refused, NotRun::BinaryNotFound("grok")));
    assert_eq!(
        refused.detail(),
        "`grok` was not found on PATH in the shell environment"
    );
}

/// A launcher whose native program is missing is refused as a launch refuses it, so an account's
/// queries and sign-in report the provider's code rather than a binary that was not found.
#[cfg(windows)]
#[test]
fn a_launcher_without_its_native_program_is_refused_with_the_providers_code() {
    use crate::protocol::Code;
    use crate::providers::codex::NATIVE_PROGRAM_MISSING;

    let dir = ScratchDir::new("cli-run-shim");
    let env = crate::test_support::codex_shim_without_native_program(&dir);
    let refused = admitted_command(AgentId::CODEX, &env, Some(&dir), &[])
        .err()
        .expect("refused");
    let NotRun::BinaryRefused(refusal) = &refused else {
        panic!("refused for another reason: {refused:?}");
    };
    assert_eq!(refusal.code, Code::of_provider(NATIVE_PROGRAM_MISSING));
    assert!(matches!(
        super::resolve_account_binary(AgentId::CODEX, &env),
        Err(super::Unstartable::Refused(_))
    ));
    let listing = crate::agents::models::list(
        AgentId::CODEX,
        &env,
        Some(&dir),
        std::time::Duration::from_secs(5),
    );
    assert_eq!(
        listing.unavailable.map(|reason| reason.code),
        Some(Code::of_provider(NATIVE_PROGRAM_MISSING))
    );
}

/// What a finished query printed; the status is not read by `failure_detail`.
fn printed(stdout: &str, stderr: &str) -> Output {
    Output {
        status: ExitStatus::default(),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

/// Both streams hold a reason when neither is known to be the one that does, so both are kept,
/// stdout first.
#[test]
fn a_failure_detail_keeps_both_streams_when_both_printed() {
    let output = printed("usage limit reached\n", "Ignoring settings: not trusted\n");
    assert_eq!(
        failure_detail(&output, |line| line),
        "usage limit reached (stderr: Ignoring settings: not trusted)"
    );
}

#[test]
fn a_failure_detail_is_the_one_stream_that_printed() {
    assert_eq!(
        failure_detail(&printed("model refused\n", ""), |line| line),
        "model refused"
    );
    assert_eq!(
        failure_detail(&printed("", "not logged in\n"), |line| line),
        "not logged in"
    );
}

#[test]
fn a_failure_detail_is_empty_when_nothing_was_printed() {
    assert_eq!(failure_detail(&printed("", ""), |line| line), "");
    assert_eq!(failure_detail(&printed(" \n\n", "\t\n"), |line| line), "");
}

/// Only the last line of each stream, trimmed, is the reason.
#[test]
fn a_failure_detail_takes_the_last_trimmed_line_of_each_stream() {
    let output = printed("first\n  second one  \n\n", "noise\n\n  not logged in \n");
    assert_eq!(
        failure_detail(&output, |line| line),
        "second one (stderr: not logged in)"
    );
}

/// Each line goes through the caller's transform before the streams are joined, so a caller that
/// redacts sees each line on its own.
#[test]
fn a_failure_detail_passes_each_line_through_the_transform() {
    let output = printed("out\n", "err\n");
    assert_eq!(
        failure_detail(&output, |line| line.to_uppercase()),
        "OUT (stderr: ERR)"
    );
    let seen = std::cell::RefCell::new(Vec::new());
    failure_detail(&output, |line| {
        seen.borrow_mut().push(line.clone());
        line
    });
    let mut seen = seen.into_inner();
    seen.sort();
    assert_eq!(seen, ["err", "out"]);
}

/// A stream whose line the transform empties counts as not printed, with no dangling label.
#[test]
fn a_failure_detail_drops_a_stream_the_transform_empties() {
    let output = printed("secret\n", "warn\n");
    let drop_secret = |line: String| {
        if line == "secret" {
            String::new()
        } else {
            line
        }
    };
    assert_eq!(failure_detail(&output, drop_secret), "warn");
}

/// A line longer than the limit is cut to the limit and marked, on either stream.
#[test]
fn a_failure_detail_cuts_a_long_line_and_marks_the_cut() {
    let long = "x".repeat(DETAIL_LINE_LIMIT + 500);
    let kept = format!("{}…", "x".repeat(DETAIL_LINE_LIMIT));
    assert_eq!(
        failure_detail(&printed(&format!("{long}\n"), ""), |line| line),
        kept
    );
    assert_eq!(
        failure_detail(&printed("", &format!("{long}\n")), |line| line),
        kept
    );
    assert_eq!(
        failure_detail(
            &printed(&format!("{long}\n"), &format!("{long}\n")),
            |line| line
        ),
        format!("{kept} (stderr: {kept})")
    );
}

/// A line of exactly the limit is kept whole, with no mark.
#[test]
fn a_failure_detail_keeps_a_line_of_exactly_the_limit_whole() {
    let exact = "y".repeat(DETAIL_LINE_LIMIT);
    assert_eq!(
        failure_detail(&printed(&format!("{exact}\n"), ""), |line| line),
        exact
    );
    let over = "y".repeat(DETAIL_LINE_LIMIT + 1);
    assert_eq!(
        failure_detail(&printed(&format!("{over}\n"), ""), |line| line),
        format!("{exact}…")
    );
}

/// The limit counts characters, so a multibyte line is cut between characters, never inside one.
#[test]
fn a_failure_detail_cuts_a_multibyte_line_on_a_character_boundary() {
    let long = "界".repeat(DETAIL_LINE_LIMIT + 3);
    assert_eq!(
        failure_detail(&printed(&format!("{long}\n"), ""), |line| line),
        format!("{}…", "界".repeat(DETAIL_LINE_LIMIT))
    );
    // Mixed widths with the limit falling after a four-byte character.
    let mixed = format!("{}{}", "a".repeat(DETAIL_LINE_LIMIT - 1), "😀😀😀");
    assert_eq!(
        failure_detail(&printed("", &format!("{mixed}\n")), |line| line),
        format!("{}😀…", "a".repeat(DETAIL_LINE_LIMIT - 1))
    );
}

/// The cut comes before the caller's transform, so the transform never sees more than the limit
/// plus the mark.
#[test]
fn a_failure_detail_cuts_a_line_before_the_transform_sees_it() {
    let long = "z".repeat(DETAIL_LINE_LIMIT * 4);
    let seen = std::cell::RefCell::new(Vec::new());
    failure_detail(
        &printed(&format!("{long}\n"), &format!("{long}\n")),
        |line| {
            seen.borrow_mut().push(line.chars().count());
            line
        },
    );
    assert_eq!(seen.into_inner(), [DETAIL_LINE_LIMIT + 1; 2]);
}

/// The query's own JSON answer is not a reason: stdout is left out as if it had printed nothing,
/// whether the answer is one line, one object per line, or pretty-printed over many lines.
#[test]
fn a_failure_detail_leaves_out_stdout_that_is_a_json_answer() {
    let pretty_object = "{\n  \"loggedIn\": false,\n  \"headers\": {\n    \"Authorization\": \"Bearer secret\"\n  }\n}\n";
    let pretty_array =
        "[\n  {\n    \"name\": \"srv\",\n    \"env\": { \"TOKEN\": \"secret\" }\n  }\n]\n";
    let ndjson = "{\"name\":\"a\",\"env\":{\"TOKEN\":\"one\"}}\n{\"name\":\"b\",\"env\":{\"TOKEN\":\"two\"}}\n";
    for stdout in [
        "{\"loggedIn\":false,\"headers\":{\"Authorization\":\"Bearer secret\"}}\n",
        "[{\"name\":\"srv\",\"env\":{\"TOKEN\":\"secret\"}}]\n",
        "   {\"padded\":true}   \n",
        "{}\n",
        "[]\n",
        pretty_object,
        pretty_array,
        ndjson,
    ] {
        assert_eq!(
            failure_detail(&printed(stdout, ""), |line| line),
            "",
            "{stdout}"
        );
        assert_eq!(
            failure_detail(&printed(stdout, "not logged in\n"), |line| line),
            "not logged in",
            "{stdout}"
        );
    }
}

/// A text reason that merely starts with a bracket, or a lone bracket, is not JSON and is kept.
#[test]
fn a_failure_detail_keeps_a_stdout_reason_that_only_starts_with_a_bracket() {
    for line in [
        "[ERROR] model not found",
        "[2025-01-01] failed",
        "{",
        "[",
        "}",
        "]",
        "{not json}",
        "[1, 2",
        "{\"cut\":",
    ] {
        assert_eq!(
            failure_detail(&printed(&format!("{line}\n"), ""), |line| line),
            line,
            "{line}"
        );
    }
    assert_eq!(
        failure_detail(&printed("[ERROR] model not found\n", "warn\n"), |line| line),
        "[ERROR] model not found (stderr: warn)"
    );
}

/// Only the last line, or the whole of stdout, decides: JSON earlier in stdout does not hide a
/// reason on its last line.
#[test]
fn a_failure_detail_judges_only_the_last_stdout_line_for_json() {
    let output = printed("{\"a\":1}\nusage limit reached\n", "");
    assert_eq!(failure_detail(&output, |line| line), "usage limit reached");
    let output = printed("usage limit reached\n{\"a\":1}\n", "");
    assert_eq!(failure_detail(&output, |line| line), "");
}

/// A line that merely contains a brace later on, or opens with another character, is a reason.
#[test]
fn a_failure_detail_keeps_a_stdout_line_that_does_not_open_like_json() {
    assert_eq!(
        failure_detail(&printed("error: bad {value} [x]\n", ""), |line| line),
        "error: bad {value} [x]"
    );
}

/// stderr is never left out for opening like JSON.
#[test]
fn a_failure_detail_keeps_a_stderr_line_that_opens_like_json() {
    assert_eq!(
        failure_detail(&printed("", "{\"error\":\"denied\"}\n"), |line| line),
        "{\"error\":\"denied\"}"
    );
    assert_eq!(
        failure_detail(&printed("", "[warn] ignored\n"), |line| line),
        "[warn] ignored"
    );
    assert_eq!(
        failure_detail(&printed("{\"a\":1}\n", "[warn] ignored\n"), |line| line),
        "[warn] ignored"
    );
}

/// The JSON check is on the stdout line itself, so a transform that would empty or rewrite it
/// never runs on it.
#[test]
fn a_failure_detail_does_not_pass_a_left_out_stdout_line_to_the_transform() {
    let seen = std::cell::RefCell::new(Vec::new());
    failure_detail(&printed("{\"a\":1}\n", "warn\n"), |line| {
        seen.borrow_mut().push(line.clone());
        line
    });
    assert_eq!(seen.into_inner(), ["warn"]);
}
