//! The fixed texts Octoboard writes into sessions in its own name: the prefix every notice begins
//! with, how each kind of message it writes begins (the `*_OPENING` constants), by which its
//! messages are told apart from what the user typed when a session's history is read, the brief a
//! session is handed, the prompt of a session that takes another's work over, the opening prompts
//! of a session started at another session's request or handed branches by the session that
//! started it, a report panel form submission, the title a brief earns, and how the notice that a
//! session waits for the user begins, with how one kept for an owner is recognised, and the prompt
//! a session is sent to carry on with its turn after a restart.
//!
//! The texts are what the agents read, and a session's history is read back by how they begin, so
//! a change of wording is a change of behaviour.

use std::path::Path;

use crate::branch_origin::handover::{
    branch_noun_and_pronoun, named_branches_with_targets, Handover,
};
use crate::messaging::naming;
use crate::protocol::Session;

/// How every notice Octoboard writes into a session begins, worded after it at several places. A
/// macro so that a constant can be built on it with `concat!`.
macro_rules! notice_prefix {
    () => {
        "Message from Octoboard"
    };
}
pub(crate) use notice_prefix;

/// [`notice_prefix!`] as a value.
pub(crate) const NOTICE_PREFIX: &str = notice_prefix!();

/// `text` with its first letter capitalised, to begin a sentence.
pub(crate) fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// How [`render_brief`] begins.
pub(crate) const BRIEF_OPENING: &str = "## Goal";

/// How a report written into its owner begins.
pub(crate) const REPORT_OPENING: &str = "Report from session ";

/// How [`render_page_submission`] begins.
pub(crate) const PAGE_SUBMISSION_OPENING: &str = "Report panel form submission";

/// What a session is handed as its opening prompt, rendered from the console session's `brief`.
///
/// A fixed template rather than something the console session composes, so what a session is
/// handed does not vary with the console session's mood; a field the console session left out is
/// omitted entirely rather than sent as an empty heading, which would tell the session there was
/// something to say and then say nothing.
pub fn render_brief(
    goal: &str,
    context: Option<&str>,
    acceptance: Option<&str>,
    constraints: Option<&str>,
) -> String {
    let mut prompt = format!("{BRIEF_OPENING}\n\n{}\n", goal.trim());
    for (heading, body) in [
        ("Context", context),
        ("Acceptance", acceptance),
        ("Constraints", constraints),
    ] {
        if let Some(body) = body.map(str::trim).filter(|body| !body.is_empty()) {
            prompt.push_str(&format!("\n## {heading}\n\n{body}\n"));
        }
    }
    prompt
}

/// How [`takeover_prompt`] begins.
pub(crate) const TAKEOVER_OPENING: &str = "You are taking over the work of the ";

/// The receiving session's opening prompt: where the package is, to be read before anything else,
/// and the note of the owner that asked for the handoff, when it left one.
pub(crate) fn takeover_prompt(original: &Session, package: &Path, note: Option<&str>) -> String {
    let note = note.map_or_else(String::new, |note| {
        format!(
            "\n\nThe owner that handed the work off left you this note:\n\n{}",
            naming::quote(note)
        )
    });
    format!(
        "{TAKEOVER_OPENING}{kind} session {label}, a {agent} \
         session. Octoboard is handing it off to you because the work moves to another agent. Its \
         conversation cannot be resumed here, so Octoboard built a handoff package from its \
         records:\n\n{path}\n\nRead that file first, all of it, before you do anything else. It \
         holds its whole conversation (every user message and every reply, with tool output left \
         out), its latest todo list, the files its edits changed, the git state of the branches \
         it was responsible for, and the sessions it owned. Work out the goal, what was tried and \
         what comes next from it, and carry on where the original stopped; do not start over. \
         If the package names an earlier handoff package, read that one too, and so on back \
         along the chain, before you carry on.\n\n\
         Once you are running, Octoboard moves the original's place to you: its owner, its \
         branches and its sessions. It sends you a message saying what you now hold, and archives \
         the original; do not message the original.{note}",
        kind = original.role.kind(),
        label = naming::label(original),
        agent = original.agent.label(),
        path = package.display(),
    )
}

/// What a report panel form submission is written into the console session as. Plain prose, like
/// a report: a header line naming the page, then the submitted data.
///
/// `data` is arbitrary JSON from the form, so it is rendered readably only in the shape a form
/// normally takes — an object of scalar values, one `key: value` line each — and falls back to
/// pretty-printed JSON for anything else, rather than guessing at a layout for nested data.
pub(crate) fn render_page_submission(page_id: &str, data: &serde_json::Value) -> String {
    let mut message = format!("{PAGE_SUBMISSION_OPENING} — page {page_id}\n\n");
    match data.as_object().filter(|fields| {
        !fields.is_empty()
            && fields
                .values()
                .all(|value| !value.is_object() && !value.is_array())
    }) {
        Some(fields) => {
            for (key, value) in fields {
                let shown = match value {
                    serde_json::Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                message.push_str(&format!("{key}: {shown}\n"));
            }
        }
        None => {
            message
                .push_str(&serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string()));
        }
    }
    message.trim_end().to_string()
}

/// The session title a dispatched brief earns, taken from its goal. Several sessions dispatched
/// into one project are otherwise all named for the project and indistinguishable in the menu.
pub fn title_from_goal(goal: &str) -> String {
    const MAX: usize = 48;
    let first_line = goal
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    if first_line.chars().count() <= MAX {
        return first_line.to_string();
    }
    // Cut on a character boundary, and on a word where there is one close enough to the limit.
    let truncated: String = first_line.chars().take(MAX).collect();
    match truncated.rfind(' ') {
        Some(at) if at >= MAX / 2 => format!("{}…", &truncated[..at]),
        _ => format!("{truncated}…"),
    }
}

/// How [`branches_opening_prompt`] begins.
pub(crate) const BRANCHES_OPENING: &str = "The session `";

/// What follows the session's title in [`branches_opening_prompt`], which tells it from a user's
/// message that happens to begin like it.
pub(crate) const BRANCHES_MARKER: &str = "\"), which started you, also handed the ";

/// The opening prompt of a session `new_session` hands branches to: the brief, and that the
/// session starting it handed it the branches and it is now the one responsible for them.
pub(crate) fn branches_opening_prompt(
    caller: &Session,
    handover: &Handover,
    brief: &str,
) -> String {
    let (noun, them) = branch_noun_and_pronoun(&handover.branches);
    format!(
        "{BRANCHES_OPENING}{id}` (\"{title}{BRANCHES_MARKER}{noun} {} over to \
         you: you are now the session responsible for {them}, so carry on the work on {them}. \
         This is the brief it wrote:\n\n{brief}",
        named_branches_with_targets(&handover.branches, &handover.targets),
        id = caller.id,
        title = naming::one_line(&caller.title),
    )
}

/// How [`request_opening_prompt`] begins.
pub(crate) const REQUEST_OPENING: &str = "You were started at the request of the session ";

/// The opening prompt of a session started at `caller`'s request: whoever reads the brief is to
/// know who wrote it, how the request was approved, and that it is not that session's to drive.
pub(crate) fn request_opening_prompt(
    caller: &Session,
    handover: Option<&Handover>,
    automatic: bool,
    brief: &str,
) -> String {
    let approval = if automatic {
        "automatically, under the user's setting to approve session requests without asking"
    } else {
        "with the user's approval"
    };
    let handed = handover.map_or_else(String::new, |handover| {
        let (noun, them) = branch_noun_and_pronoun(&handover.branches);
        format!(
            " It also handed the {noun} {} over to you, with the same approval: you are now the \
             session responsible for {them}, so carry on the work on {them}.",
            named_branches_with_targets(&handover.branches, &handover.targets)
        )
    });
    format!(
        "{REQUEST_OPENING}{label}, {approval}. Nobody \
         owns you but the user: that session is not your owner, and it and you can only share \
         information with each other.{handed} This is the brief it wrote:\n\n{brief}",
        label = naming::label(caller),
    )
}

/// How the notice of [`crate::messaging::owner_notices::tell_owner_it_waits`] about `session`
/// begins, up to its title, which a rename between two waits changes. [`WAITING_PHRASE`] follows
/// the title.
pub(crate) fn waiting_opening(session: &Session) -> String {
    format!(
        "{NOTICE_PREFIX}: the {} session `{}` (\"",
        session.role.kind(),
        session.id
    )
}

/// What follows the title in the first line of that notice.
pub(crate) const WAITING_PHRASE: &str = "\") is waiting for the user";

/// Whether `text`, kept for an owner, is the notice that `session` waits for the user, by which a
/// kept one is found again to be dropped once it is out of date (see
/// [`crate::messaging::delivery::write_to_owner`]).
pub(crate) fn is_waiting_notice(text: &str, session: &Session) -> bool {
    text.strip_prefix(&waiting_opening(session))
        .and_then(|rest| rest.lines().next())
        .is_some_and(|line| line.contains(WAITING_PHRASE))
}

/// The id of the session that a kept notice says waits for the user, if it is that notice.
pub(crate) fn waiting_notice_session(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(NOTICE_PREFIX)?.strip_prefix(": the ")?;
    let (_, rest) = rest.split_once(" session `")?;
    let (id, rest) = rest.split_once("` (\"")?;
    rest.lines().next()?.contains(WAITING_PHRASE).then_some(id)
}

/// What a continued session is told to do, in Claude Code's own words for its auto-resume. A macro
/// so that [`CONTINUE_PROMPT`] can be built on it with `concat!`.
macro_rules! continue_words {
    () => {
        "Continue the task you were working on when it was interrupted; do not repeat work that \
         is already complete."
    };
}

/// The prompt a continued session is sent: a notice from Octoboard, so that the session's history
/// tells it from what the user typed.
pub(crate) const CONTINUE_PROMPT: &str = concat!(
    notice_prefix!(),
    ": your session was restarted. ",
    continue_words!()
);

/// What follows a notice the continued session is sent with, in place of [`CONTINUE_PROMPT`].
pub(crate) const CONTINUE_AFTER_NOTICE: &str = continue_words!();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Agent, SessionStatus};

    /// A field the console session left out is omitted entirely: an empty heading tells the
    /// session there was something to say and then says nothing.
    #[test]
    fn a_brief_omits_the_sections_it_was_given_nothing_for() {
        let full = render_brief(
            "ship it",
            Some("background"),
            Some("tests pass"),
            Some("do not push"),
        );
        for heading in ["## Goal", "## Context", "## Acceptance", "## Constraints"] {
            assert!(full.contains(heading), "{heading} missing from {full}");
        }

        let bare = render_brief("ship it", None, Some("   "), None);
        assert!(bare.contains("## Goal"));
        assert!(!bare.contains("## Context"));
        // Whitespace is nothing to say either.
        assert!(!bare.contains("## Acceptance"));
    }

    /// The normal case: a form submits an object of scalar fields, rendered as readable
    /// `key: value` lines.
    #[test]
    fn a_submission_of_scalar_fields_renders_as_key_value_lines() {
        let rendered = render_page_submission(
            "page-7",
            &serde_json::json!({ "name": "Ada", "age": 30, "subscribe": true }),
        );
        assert!(rendered.contains("page page-7"));
        assert!(rendered.contains("name: Ada"));
        assert!(rendered.contains("age: 30"));
        assert!(rendered.contains("subscribe: true"));
    }

    /// A form's field order is the author's reading order, not alphabetical — `serde_json` is
    /// built with `preserve_order` for exactly this, so a submission's fields must come out in the
    /// order they were sent, not in whatever order a `BTreeMap` would impose.
    #[test]
    fn a_submission_keeps_the_fields_in_their_sent_order() {
        let rendered = render_page_submission(
            "page-1",
            &serde_json::json!({ "who": "Ada", "page": "page-1" }),
        );
        let who_at = rendered
            .find("who: Ada")
            .expect("the who field is rendered");
        let page_at = rendered
            .find("page: page-1")
            .expect("the page field is rendered");
        assert!(
            who_at < page_at,
            "`who` was sent before `page` and must render before it: {rendered}"
        );
    }

    /// Anything that is not an object of scalars — nested data, an array, a bare value — falls back
    /// to pretty-printed JSON rather than a guessed layout.
    #[test]
    fn a_submission_of_nested_data_falls_back_to_pretty_json() {
        let rendered =
            render_page_submission("page-7", &serde_json::json!({ "answers": ["a", "b"] }));
        assert!(rendered.contains('{'));
        assert!(rendered.contains("\"answers\""));
        assert!(rendered.contains("\"a\""));
    }

    /// Several sessions in one project have to be told apart in the menu, and the goal is the only
    /// thing that distinguishes them.
    #[test]
    fn a_title_from_a_goal_stays_short_and_readable() {
        assert_eq!(
            title_from_goal("Fix the login retry"),
            "Fix the login retry"
        );
        assert_eq!(title_from_goal("\n\n  Fix login  \nmore"), "Fix login");

        let long = title_from_goal(
            "Replace the hand-rolled retry logic in the authentication client with something sane",
        );
        assert!(long.ends_with('…'));
        assert!(long.chars().count() <= 49);
        // Cut on a word, not mid-word.
        assert!(!long.contains("som…"));

        // A single long word still has to be cut, and on a character boundary.
        let unbroken = title_from_goal(&"日".repeat(80));
        assert_eq!(unbroken.chars().count(), 49);
    }

    /// The opening prompt says the user approved the request, or that Octoboard did under the
    /// user's setting.
    #[test]
    fn the_opening_prompt_words_the_approval_by_who_gave_it() {
        let caller =
            crate::test_support::project_session("loose", Agent::Claude, SessionStatus::Idle);
        let asked = request_opening_prompt(&caller, None, false, "the brief");
        assert!(
            asked.contains("), with the user's approval. Nobody owns you"),
            "{asked}"
        );
        assert!(
            asked.ends_with("This is the brief it wrote:\n\nthe brief"),
            "{asked}"
        );
        let waived = request_opening_prompt(&caller, None, true, "the brief");
        assert!(
            waived.contains(
                "), automatically, under the user's setting to approve session requests without \
                 asking. Nobody owns you"
            ),
            "{waived}"
        );
        assert!(!waived.contains("with the user's approval"), "{waived}");
    }
}
