# Message headers

Every text one session writes into another session, and every text Octoboard writes into a session, on a session's
behalf or of its own, **opens with a header** saying who it comes from, how that sender stands to the receiving session,
and what the text is — except the brief a session is started with, whose sender its role description names. What the
user types or pastes into a session's terminal has **no header**: that absence is how a
session tells the user's own words from everything else that reaches it.

A header is framing, not enforcement: it is text in the receiving session's input like the rest, and how a model acts
on it is the model's own call (see "This boundary is framing, not enforcement" in
`docs/product/information-between-sessions.md`).

## The headers

| What reaches the session | How it begins | Where it is specified |
|---|---|---|
| An instruction from its parent session, sent with `instruct_session` or as `resume_session`'s `text` | "Instruction from your parent session", see "A parent session's instruction" below | "The console session's tools" in `docs/product/hub-orchestration.md` |
| A report from a session under it | "Report from your child session", naming the reporting session and its project | "The report as the owner reads it" in `docs/product/reporting-to-owners.md` |
| A report panel form submission | "Report panel form submission", naming the page | "Submitting a form back to the session" in `docs/product/report-panel.md` |
| Information another session shared with it | "Information from your parent session", "Information from your child session" or "Information from another session", by what the sender is to the receiver, naming the sender | "Information as the receiver reads it" in `docs/product/information-between-sessions.md` |
| The copy of information shared with one of its sessions | "Copy of information shared with your child session", naming the receiving session, then the sender and what it is to the owner | "Information as the receiver reads it" in `docs/product/information-between-sessions.md` |
| A notice from Octoboard | "Message from Octoboard:" | with the event it tells of |
| The opening prompt of a session started with `open_session` | the brief's "## Goal" section, or, when the session was handed branches, a sentence naming the session that started it and the branches it handed over, followed by the brief | "Handing out a task: the brief" in `docs/product/hub-orchestration.md`; "Transferring branches to a session the caller starts" in `docs/product/branch-responsible-session.md` |
| The opening prompt of a session started at another session's request | "You were started at the request of the session", naming it | "On approval" in `docs/product/session-requests.md` |
| The first work prompt of a session taking over a handed-off session | "You are taking over the work of the", naming the original | "What the receiving session takes over" in `docs/product/handing-off-sessions.md` |

A header that names a session — a parent session's instruction, a report, information and its copy — names it as the
<kind> session `<id>` ("<title>"), `<kind>` being `console` or `project`, and says what that session is to the reader:
its parent session, its child session, or another session. A title is put on one line.

The text a session passed along — a report's summary, shared information, a note left with a handoff or a release — is
quoted, every line prefixed with `> `, blank ones included, so that it cannot pass for a header of its own; a report's
open items and owner items are each put on one line instead. Lines of Octoboard's own around it, such as a report's
status line, stay unquoted. A parent session's instruction is not quoted (see below), nor is the brief in the opening
prompt of a session started at another session's request: the user approved that request, so the brief is the new
session's work to take on, not information to weigh.

## Headers of earlier versions

Messages that began with the headers of earlier versions — "Report from session ", "Information from session " and
"Copy of information shared with your session " — are still told as Octoboard's wherever messages are told apart by how
they begin, such as in a session's history (see "What the history lists" in `docs/product/session-history.md`), so the
histories of archived sessions read the same. Octoboard no longer writes them.

## A parent session's instruction

The text a parent session sends with `instruct_session`, or as the `text` of `resume_session`, reaches the session as:

```text
Instruction from your parent session, the <kind> session `<id>` ("<title>"). It directs your work: carry it out.

<text>
```

`<kind>` is `console` or `project`, and `<id>` and `<title>` are the parent session's. The text follows as written,
after a blank line, not quoted: it is work the session is to carry out, not information to weigh. When a relaunch also
owes the session a notice of a change of owner, the notice comes first and the instruction, with its header, follows
after a blank line (see "A change of owner told at the next launch" in `docs/product/changing-session-owners.md`).

## What sessions are told about headers

- **A project session** is told in its role description that what other sessions and Octoboard write to it begins with
  a header saying where it comes from and what it is, and that what the user types in its terminal has none. The
  headers it is given are only those it can receive: one with a parent session is also told that its parent session's
  instructions begin "Instruction from your parent session" and are to be carried out as it would carry out the user's,
  and is given "Information from your parent session"; one that can have sessions of its own (a coordinator) is given
  "Information from your child session", "Copy of information shared with your child session" and "Report from your
  child session"; every project session is given "Information from another session" and "Message from Octoboard:".
- **A console session** is told in its instruction file that what sessions and Octoboard write to it begins with such
  a header, naming "Report from your child session", "Information from your child session", "Information from another
  session", "Copy of information shared with your child session" and "Message from Octoboard:", and that what the user
  types in its terminal has none.
- **The tools that send an instruction** say so too: the descriptions of `instruct_session` and of `resume_session`'s
  `text`, and the console session's instruction file, tell the caller the text arrives headed as its parent session's
  instruction, naming the caller, so the session tells it from the user's own typing.
