use super::*;
use crate::protocol::{quota_unavailable_code as code, Agent, QuotaUnavailable};

fn quota(unavailable: Option<(crate::protocol::Code, &str)>) -> AccountQuota {
    AccountQuota {
        agent: Agent::Claude,
        account: None,
        fetched_at: 1,
        plan: None,
        email: None,
        windows: Vec::new(),
        unavailable: unavailable.map(|(c, m)| QuotaUnavailable::new(c, m, &[])),
        reset_credits: None,
    }
}

fn failed(message: &str) -> Outcome {
    Outcome::Failed(message.to_string())
}

#[test]
fn redaction_masks_keys_tokens_bearers_and_emails() {
    let text = "failed for me@example.com with sk-ant-oat01-abcdef and Bearer abc123 \
        eyJhbGciOiJIUzI1NiJ9.payload.sig refresh_token=xyz \
        QWxhZGRpbjpvcGVuIHNlc2FtZTEyMzQ1Njc4OTBhYmNkZWY= ok";
    let out = redact(text);
    for secret in [
        "me@example.com",
        "sk-ant",
        "abc123",
        "eyJ",
        "xyz",
        "QWxhZGRpbjpv",
    ] {
        assert!(!out.contains(secret), "{secret} leaked: {out}");
    }
    assert!(out.starts_with("failed for [redacted] with"));
    assert!(out.ends_with(" ok"));
}

#[test]
fn redaction_masks_the_value_after_a_secret_name_in_json_and_pairs() {
    for (text, secret) in [
        (r#"{"refresh_token":"abc"}"#, "abc"),
        (r#"{"accessToken": "abc"}"#, "abc"),
        ("access_token: abc", "abc"),
        ("password: hunter2", "hunter2"),
        ("api_key: X1", "X1"),
        ("Authorization: abc", "abc"),
        ("Authorization: Bearer abc", "abc"),
        ("password = hunter2", "hunter2"),
    ] {
        let out = redact(text);
        assert!(!out.contains(secret), "{text} -> {out}");
        assert!(out.contains(MASK), "{text} -> {out}");
    }
    // What is not a value stays.
    assert_eq!(redact("the login worked"), "the login worked");
}

#[test]
fn redaction_leaves_ordinary_text_alone() {
    let text = "reading Claude Code's quota failed: ended without replying (exit status: 1)";
    assert_eq!(redact(text), text);
    assert_eq!(
        redact("/Users/x/.claude/accounts"),
        "/Users/x/.claude/accounts"
    );
}

#[test]
fn the_stderr_tail_keeps_the_last_lines_redacted() {
    let mut stderr = String::new();
    for n in 0..20 {
        stderr.push_str(&format!("line {n}\n"));
    }
    stderr.push_str("login as me@example.com failed\n");
    let tail = stderr_tail(stderr.as_bytes());
    assert!(tail.ends_with("login as [redacted] failed"));
    assert!(!tail.contains("line 0 "));
    assert_eq!(tail.split(" | ").count(), STDERR_TAIL_LINES);
    assert_eq!(stderr_tail(b""), "");
}

#[test]
fn the_stderr_tail_is_bounded_by_bytes_on_a_char_boundary() {
    // One ASCII byte first, then lines of two-byte characters, so that the cut of the last
    // `STDERR_TAIL_BYTES` lands inside a character and falls short of `STDERR_TAIL_LINES` lines.
    let line = "é".repeat(230);
    let stderr = format!("a{}", format!("{line}\n").repeat(20));
    assert!(!stderr.is_char_boundary(stderr.len() - STDERR_TAIL_BYTES));
    let tail = stderr_tail(stderr.as_bytes());
    let parts: Vec<&str> = tail.split(" | ").collect();
    assert_eq!(parts.len(), 5, "{tail}");
    assert_eq!(parts[4], line);
    assert!(parts[0].chars().count() < 230 && parts[0].chars().all(|c| c == 'é'));
}

#[test]
fn a_reading_is_classified_by_what_it_says_of_the_sign_in() {
    assert_eq!(classify(&quota(None)), Outcome::SignedIn);
    assert_eq!(
        classify(&quota(Some((code::NOT_SIGNED_IN, "x")))),
        Outcome::SignedOut
    );
    assert_eq!(
        classify(&quota(Some((code::CONFIG_DIR_MISSING, "x")))),
        Outcome::SignedOut
    );
    assert_eq!(
        classify(&quota(Some((code::NO_WINDOWS, "x")))),
        Outcome::SignedIn
    );
    assert_eq!(
        classify(&quota(Some((code::TIMED_OUT, "me@example.com timed out")))),
        failed("[redacted] timed out")
    );
}

#[test]
fn only_a_change_of_sign_in_is_an_event() {
    let mut observed = Observed::default();
    // The first sighting, signed in or not, is no transition.
    assert_eq!(advance(&mut observed, &Outcome::SignedIn), []);
    assert_eq!(advance(&mut observed, &Outcome::SignedIn), []);
    assert_eq!(
        advance(&mut observed, &Outcome::SignedOut),
        [Event::SignedOut { was: None }]
    );
    assert_eq!(advance(&mut observed, &Outcome::SignedOut), []);
    assert_eq!(
        advance(&mut observed, &Outcome::SignedIn),
        [Event::SignedIn]
    );
    assert_eq!(advance(&mut Observed::default(), &Outcome::SignedOut), []);
}

#[test]
fn a_failure_is_logged_when_it_starts_or_changes_and_when_it_recovers() {
    let mut observed = Observed::default();
    assert_eq!(advance(&mut observed, &failed("a")), [Event::Failing]);
    assert_eq!(advance(&mut observed, &failed("a")), []);
    assert_eq!(advance(&mut observed, &failed("b")), [Event::Failing]);
    assert_eq!(
        advance(&mut observed, &Outcome::SignedIn),
        [Event::Recovered("b".to_string())]
    );
    assert_eq!(advance(&mut observed, &Outcome::SignedIn), []);
}

#[test]
fn a_failure_in_between_does_not_hide_a_lost_sign_in() {
    let mut observed = Observed::default();
    advance(&mut observed, &Outcome::SignedIn);
    assert_eq!(advance(&mut observed, &failed("a")), [Event::Failing]);
    assert_eq!(
        advance(&mut observed, &Outcome::SignedOut),
        [Event::SignedOut {
            was: Some("a".to_string())
        }]
    );
}
