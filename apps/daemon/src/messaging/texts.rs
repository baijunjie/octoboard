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

use crate::branch_origin::transfer::{
    branch_noun_and_pronoun, named_branches_with_targets, BranchTransfer,
};
use crate::messaging::naming;
use crate::protocol::{ProcessTree, Session};

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
pub(crate) const REPORT_OPENING: &str = "Report from your child session, ";

/// How a report written into its owner began before the header named the relationship. Archived
/// histories hold such messages, so it is still told as Octoboard's.
pub(crate) const LEGACY_REPORT_OPENING: &str = "Report from session ";

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
            "\n\nThe session that handed the work off left you this note:\n\n{}",
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
         Octoboard has moved the original's place to you: its owner, its branches and its \
         sessions. Your responsibilities are listed below; do not message the original.{note}",
        kind = original.kind.name(),
        label = naming::label(original),
        agent = original.agent.label(),
        path = package.display(),
    )
}

/// What a report panel form submission is written into the console session as. Plain prose, like
/// a report: a header line naming the page, then the submitted data.
///
/// `submission` is what the page's form sent: one `key: value` line per field, in the order sent, a
/// multiple choice joined by `, ` and a boolean or a number as its JSON text. An object holding
/// anything else, nested data included, which a checked submission never does, falls back to
/// pretty-printed JSON rather than a guessed layout.
pub(crate) fn render_page_submission(
    page_id: &str,
    submission: &crate::protocol::Submission,
) -> String {
    let mut message = format!("{PAGE_SUBMISSION_OPENING} — page {page_id}\n\n");
    let shown = |value: &serde_json::Value| match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Array(items) => items
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .map(|items| items.join(", ")),
        serde_json::Value::Object(_) => None,
        other => Some(other.to_string()),
    };
    let lines: Option<Vec<String>> = submission
        .iter()
        .map(|(key, value)| shown(value).map(|shown| format!("{key}: {shown}")))
        .collect();
    match lines {
        Some(lines) => message.push_str(&lines.join("\n")),
        None => message.push_str(
            &serde_json::to_string_pretty(submission).unwrap_or_else(|_| format!("{submission:?}")),
        ),
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

/// The opening prompt of a session `open_session` hands branches to: the brief, and that the
/// session starting it handed it the branches and it is now the one responsible for them.
pub(crate) fn branches_opening_prompt(
    caller: &Session,
    transfer: &BranchTransfer,
    brief: &str,
) -> String {
    let (noun, them) = branch_noun_and_pronoun(&transfer.branches);
    format!(
        "{BRANCHES_OPENING}{id}` (\"{title}{BRANCHES_MARKER}{noun} {} over to \
         you: you are now the session responsible for {them}, so carry on the work on {them}. \
         This is the brief it wrote:\n\n{brief}",
        named_branches_with_targets(&transfer.branches, &transfer.targets),
        id = caller.id,
        title = naming::one_line(&caller.title),
    )
}

/// How [`render_instruction`] begins.
pub(crate) const INSTRUCTION_OPENING: &str = "Instruction from your parent session, ";

/// An instruction `owner` sends its session with `instruct_session` or `resume_session`, headed so
/// that the session tells its parent session's instruction from what the user types into its
/// window, which reaches it with no header. The text itself is left as written, not quoted as
/// information is: it is work the session is to carry out.
pub(crate) fn render_instruction(owner: &Session, text: &str) -> String {
    format!(
        "{INSTRUCTION_OPENING}{}. It directs your work: carry it out.\n\n{text}",
        naming::named_session(owner)
    )
}

/// How [`request_opening_prompt`] begins.
pub(crate) const REQUEST_OPENING: &str = "You were started at the request of the session ";

/// The opening prompt of a session started at `caller`'s request: whoever reads the brief is to
/// know who wrote it, how the request was approved, and that it is not that session's to drive.
pub(crate) fn request_opening_prompt(
    caller: &Session,
    transfer: Option<&BranchTransfer>,
    automatic: bool,
    brief: &str,
) -> String {
    let approval = if automatic {
        "automatically, under the user's setting to approve session requests without asking"
    } else {
        "with the user's approval"
    };
    let transferred = transfer.map_or_else(String::new, |transfer| {
        let (noun, them) = branch_noun_and_pronoun(&transfer.branches);
        format!(
            " It also transferred the {noun} {} to you, with the same approval: you are now the \
             session responsible for {them}, so carry on the work on {them}.",
            named_branches_with_targets(&transfer.branches, &transfer.targets)
        )
    });
    format!(
        "{REQUEST_OPENING}{label}, {approval}. Nobody \
         owns you but the user: that session is not your parent session, and it and you can only share \
         information with each other.{transferred} This is the brief it wrote:\n\n{brief}",
        label = naming::label(caller),
    )
}

/// How the notices about `session` that a later one supersedes begin, up to its title, which a
/// rename between two of them changes: the notice of
/// [`crate::messaging::owner_notices::tell_owner_it_waits`], with [`WAITING_PHRASE`] following
/// the title, and that of [`crate::messaging::idle_children::tell_owner_it_is_idle`], with
/// [`IDLE_PHRASE`].
pub(crate) fn session_notice_opening(session: &Session) -> String {
    format!(
        "{NOTICE_PREFIX}: the {} session `{}` (\"",
        session.kind.name(),
        session.id
    )
}

/// What follows the title in the first line of the notice that a session waits for the user.
pub(crate) const WAITING_PHRASE: &str = "\") is waiting for the user";

/// What follows the title in the first line of the notice that a session stayed idle after its
/// last report.
pub(crate) const IDLE_PHRASE: &str = "\") has been idle";

/// Whether `text`, kept for an owner, is the notice that `session` waits for the user, by which a
/// kept one is found again to be dropped once it is out of date (see
/// [`crate::messaging::delivery::write_to_owner`]).
pub(crate) fn is_waiting_notice(text: &str, session: &Session) -> bool {
    is_notice_about(text, session, WAITING_PHRASE)
}

/// [`is_waiting_notice`], for the notice that `session` stayed idle after its last report.
pub(crate) fn is_idle_notice(text: &str, session: &Session) -> bool {
    is_notice_about(text, session, IDLE_PHRASE)
}

fn is_notice_about(text: &str, session: &Session, phrase: &str) -> bool {
    text.strip_prefix(&session_notice_opening(session))
        .and_then(|rest| rest.lines().next())
        .is_some_and(|line| line.contains(phrase))
}

/// The id of the session that a kept notice says waits for the user, if it is that notice.
pub(crate) fn waiting_notice_session(text: &str) -> Option<&str> {
    notice_session(text, WAITING_PHRASE)
}

/// The id of the session that a kept notice says stayed idle after its last report, if it is that
/// notice.
pub(crate) fn idle_notice_session(text: &str) -> Option<&str> {
    notice_session(text, IDLE_PHRASE)
}

fn notice_session<'a>(text: &'a str, phrase: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(NOTICE_PREFIX)?.strip_prefix(": the ")?;
    let (_, rest) = rest.split_once(" session `")?;
    let (id, rest) = rest.split_once("` (\"")?;
    rest.lines().next()?.contains(phrase).then_some(id)
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

/// How many trees [`processes_ended_by_restart`] names before it counts the rest.
const ENDED_TREES_NAMED: usize = 8;

/// The longest command [`processes_ended_by_restart`] quotes, in characters.
const ENDED_COMMAND_CHARS: usize = 160;

/// What a session is told of the process trees it had running that its restart ended, or `None`
/// when `trees` is empty. A tree is its top process's command, working directory and ports, which
/// is what the session needs to start it again.
pub(crate) fn processes_ended_by_restart(trees: &[ProcessTree]) -> Option<String> {
    if trees.is_empty() {
        return None;
    }
    let mut named: Vec<String> = trees
        .iter()
        .take(ENDED_TREES_NAMED)
        .map(ended_tree)
        .collect();
    if trees.len() > ENDED_TREES_NAMED {
        named.push(format!("and {} more", trees.len() - ENDED_TREES_NAMED));
    }
    Some(format!(
        "The restart ended these processes you had started: {}. Start again any you still need.",
        named.join("; ")
    ))
}

/// One tree as [`processes_ended_by_restart`] names it. The command and the directory are quoted
/// on one line, with the backticks that would end the quotation replaced.
fn ended_tree(tree: &ProcessTree) -> String {
    let command = naming::one_line(&tree.command).replace('`', "'");
    let mut named = if command.is_empty() {
        "a process whose command could not be read".to_string()
    } else if command.chars().count() > ENDED_COMMAND_CHARS {
        let cut: String = command.chars().take(ENDED_COMMAND_CHARS).collect();
        format!("`{cut}…`")
    } else {
        format!("`{command}`")
    };
    if let Some(cwd) = &tree.cwd {
        named.push_str(&format!(
            " in `{}`",
            naming::one_line(cwd).replace('`', "'")
        ));
    }
    match tree.ports.as_slice() {
        [] => {}
        [port] => named.push_str(&format!(" (port {port})")),
        ports => {
            let ports: Vec<String> = ports.iter().map(u16::to_string).collect();
            named.push_str(&format!(" (ports {})", ports.join(", ")));
        }
    }
    named
}

/// The prompt a continued session is sent: [`CONTINUE_PROMPT`], with `ended`
/// ([`processes_ended_by_restart`]) before the words that tell it to carry on when there is any.
pub(crate) fn continue_prompt(ended: Option<&str>) -> String {
    match ended {
        Some(ended) => format!("{} {CONTINUE_AFTER_NOTICE}", restart_ended_notice(ended)),
        None => CONTINUE_PROMPT.to_string(),
    }
}

/// The prompt a continued session is sent after `note`, a notice of its own: the note, `ended`
/// when there is any, then [`CONTINUE_AFTER_NOTICE`].
pub(crate) fn continue_prompt_after(note: &str, ended: Option<&str>) -> String {
    match ended {
        Some(ended) => format!("{note}\n\n{ended}\n\n{CONTINUE_AFTER_NOTICE}"),
        None => format!("{note}\n\n{CONTINUE_AFTER_NOTICE}"),
    }
}

/// How [`restart_ended_notice`] begins, specific to it so that the turn it opens can be credited
/// as reported (see `AppState::credit_turn_opened_by`). [`continue_prompt`] with a text to tell
/// begins the same, which is harmless: the credit is given only when the session is not continued,
/// and an unspent credit goes with the turn bookkeeping when the process ends.
pub(crate) const RESTART_ENDED_OPENING: &str = concat!(notice_prefix!(), restart_ended_words!());

/// The words after the notice prefix that [`RESTART_ENDED_OPENING`] is made of. A macro so that
/// the constant's declaration fits on one line, where the history's check of every `*_OPENING`
/// looks for it being built on `notice_prefix!()`.
macro_rules! restart_ended_words {
    () => {
        ": your session was restarted. The restart ended"
    };
}
use restart_ended_words;

/// The notice a restarted session that is not continued is sent for what its restart ended:
/// `ended` ([`processes_ended_by_restart`]) in Octoboard's own words.
pub(crate) fn restart_ended_notice(ended: &str) -> String {
    debug_assert!(ended.starts_with("The restart ended"));
    format!("{NOTICE_PREFIX}: your session was restarted. {ended}")
}

/// The prompt a session is sent for retry number `attempt` of `of`, after its turn stopped because
/// its model was at capacity.
pub(crate) fn capacity_retry_prompt(attempt: usize, of: usize) -> String {
    format!(
        "{NOTICE_PREFIX}: the model you selected was at capacity, so your last turn stopped. {} is \
         retrying automatically (retry {attempt} of {of}). {CONTINUE_AFTER_NOTICE}",
        crate::APP_NAME
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AgentId, SessionStatus};

    fn tree(command: &str, cwd: Option<&str>, ports: &[u16]) -> ProcessTree {
        ProcessTree {
            id: "1:2".to_string(),
            session_id: Some("s".to_string()),
            kept: false,
            command: command.to_string(),
            cwd: cwd.map(str::to_string),
            cwd_missing: false,
            started_at: 0,
            process_count: 1,
            memory_bytes: 0,
            ports: ports.to_vec(),
            keepable: true,
            unkeepable_reason: None,
        }
    }

    /// Nothing ended, nothing said: the prompt and the notice are what they were.
    #[test]
    fn nothing_is_added_when_no_tree_ended() {
        assert_eq!(processes_ended_by_restart(&[]), None);
        assert_eq!(continue_prompt(None), CONTINUE_PROMPT);
        assert_eq!(
            continue_prompt_after("a note", None),
            format!("a note\n\n{CONTINUE_AFTER_NOTICE}")
        );
    }

    /// Each tree is named by its command, directory and ports, compactly, on one line.
    #[test]
    fn ended_trees_are_named_by_command_directory_and_ports() {
        let ended = processes_ended_by_restart(&[
            tree("pnpm  dev\n--host", Some("/work/app"), &[5187]),
            tree("serve", Some("/work/site"), &[3000, 3001]),
            tree("worker", None, &[]),
            tree("", Some("/work/x"), &[]),
        ])
        .unwrap();
        assert_eq!(
            ended,
            "The restart ended these processes you had started: `pnpm dev --host` in \
             `/work/app` (port 5187); `serve` in `/work/site` (ports 3000, 3001); `worker`; a \
             process whose command could not be read in `/work/x`. Start again any you still \
             need."
        );
    }

    /// A long command is cut, a backtick in it cannot end the quotation, and past a few trees
    /// the rest are counted.
    #[test]
    fn a_long_list_of_long_commands_stays_compact() {
        let long = format!("run `{}`", "x".repeat(400));
        let trees: Vec<ProcessTree> = (0..10).map(|_| tree(&long, Some("/w"), &[])).collect();
        let ended = processes_ended_by_restart(&trees).unwrap();
        assert!(ended.contains("`run 'xxx"), "{ended}");
        assert!(ended.contains("…` in `/w`"), "{ended}");
        assert!(ended.contains("; and 2 more. Start"), "{ended}");
        assert!(ended.len() < 2000, "{}", ended.len());
    }

    /// What the restart ended goes before the words to carry on, whether the notice is
    /// Octoboard's own opening or one the restart brought.
    #[test]
    fn what_a_restart_ended_goes_before_the_words_to_carry_on() {
        let ended = processes_ended_by_restart(&[tree("serve", Some("/w"), &[80])]).unwrap();
        let alone = continue_prompt(Some(&ended));
        assert!(alone.starts_with(&format!(
            "{NOTICE_PREFIX}: your session was restarted. The restart ended"
        )));
        assert!(alone.ends_with(&format!(" {CONTINUE_AFTER_NOTICE}")));
        assert_eq!(
            continue_prompt_after("a note", Some(&ended)),
            format!("a note\n\n{ended}\n\n{CONTINUE_AFTER_NOTICE}")
        );
        assert!(restart_ended_notice(&ended).starts_with(NOTICE_PREFIX));
    }

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

    /// A submission as the map the daemon stores it as.
    fn submission(value: serde_json::Value) -> crate::protocol::Submission {
        value.as_object().expect("an object").clone()
    }

    /// The normal case: a form submits an object of fields, rendered as readable `key: value`
    /// lines, a multiple choice joined by `, ` and a number or a boolean as its JSON text.
    #[test]
    fn a_submission_renders_as_key_value_lines() {
        let rendered = render_page_submission(
            "page-7",
            &submission(serde_json::json!({
                "name": "Ada",
                "age": 30,
                "ratio": 0.5,
                "subscribe": true,
                "topics": ["rust", "sqlite"],
            })),
        );
        assert_eq!(
            rendered,
            "Report panel form submission — page page-7\n\n\
             name: Ada\nage: 30\nratio: 0.5\nsubscribe: true\ntopics: rust, sqlite"
        );
    }

    /// A form's field order is the author's reading order, not alphabetical — `serde_json` is
    /// built with `preserve_order` for exactly this, so a submission's fields must come out in the
    /// order they were sent, not in whatever order a `BTreeMap` would impose.
    #[test]
    fn a_submission_keeps_the_fields_in_their_sent_order() {
        let rendered = render_page_submission(
            "page-1",
            &submission(serde_json::json!({ "who": "Ada", "page": "page-1" })),
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

    /// Anything a checked submission never holds (nested data, an array of non-strings) falls back
    /// to pretty-printed JSON rather than a guessed layout. A submission of no field, from a form
    /// whose fields were all hidden and whose submitter has no name, is the opening line alone.
    #[test]
    fn a_submission_of_nested_data_falls_back_to_pretty_json() {
        let rendered = render_page_submission(
            "page-7",
            &submission(serde_json::json!({ "answers": { "a": 1 }, "n": [1] })),
        );
        assert!(rendered.contains('{'));
        assert!(rendered.contains("\"answers\""));
        let empty = render_page_submission("page-7", &submission(serde_json::json!({})));
        assert_eq!(empty, "Report panel form submission — page page-7");
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
            crate::test_support::project_session("loose", AgentId::CLAUDE, SessionStatus::Idle);
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
