# Standing session notices

A **standing notice** tells the user that a running session lacks something Octoboard would have given it — it
started without Octoboard's additions, its hooks never arrived, its account was last read as logged out — for as long
as that holds. It is shown in a banner over the session's terminal, in every window, whenever the window connects, and
goes when its condition ends or the session's process does. It needs no answer and blocks nothing: the terminal works
as usual underneath. Why sessions run with less than Octoboard's additions is in `docs/product/launching-without-octoboards-additions.md`.

A standing notice is never a toast and never an error alert, in a window that knows it as standing (see "A window
that predates standing notices" below for one that does not).

## The banner

- **Where**: in the selected session's terminal, floating over the terminal's top rows, centred, at most the
  terminal's width less its margins; it covers none of the terminal around it, and the output under it stays as it
  is. It leads with a warning mark, so its kind is not told by colour alone. The upload notice of dropped files, which
  shares the top edge, stacks under it (see "The upload notice" in
  `docs/product/dropping-files-onto-a-terminal.md`).
- **What it says**: the newest notice's sentence, on one line however narrow the terminal; a sentence too long for it
  fades where it is clipped, and the whole sentence shows as its tooltip.
- **Details**, or **All notices** while the session has more than one, opens a popover listing every notice of the
  session, newest first: each with its sentence, then, where it has them, its remedy, the failure it nests and that
  failure's remedy, and its details (the agent's own output, a path), laid out as a failure's details are elsewhere
  (see "Names and diagnostic details" in `docs/product/error-alerts.md`), with the link a remedy offers where it has
  one. A notice whose sentence already carries its step (an account read as logged out says to log in in the
  terminal) gets no further remedy line.
- **The close button** (named "Dismiss notice") hides the notice the banner shows, in this window only; the next newest
  then shows, and the banner goes with the last. Nothing is sent to the daemon: other windows keep showing it, and it
  shows again in this window after a reload. A notice raised again — by a later launch, or with a new reading — is a
  new notice and shows even where the earlier one was dismissed.
- **Not shown** for an archived session, whose history covers its terminal (see `docs/product/session-history.md`),
  nor for a session with no process, which has no standing notices.
- **A notice of a kind the window has no words for**, such as one a newer daemon sends, is shown by the daemon's own
  English text, in that text's own direction, and listed the same way with nothing else.

Its two controls are compact and take no keyboard focus when pressed, so typing goes on reaching the terminal. From
the keyboard the banner is a stop of the `F6` cycle while it shows (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`), landing on **Details** or **All notices**; when the banner goes while
one of its controls has focus, focus goes back to the terminal.

**Assistive technology** hears each notice once, as it arrives for the session on screen, through a status
announcement; the banner's own sentence is hidden from it, so nothing is said twice. Notices the session already has
when it is selected, or when the window connects, are read with the terminal and not announced apart. An arrival is
held while the terminal is out of reach (a maximized panel, or one floating over it at 768 px and below) or while its
session's process cannot be reached, and said once it is back unless it was said already; one dismissed or withdrawn
before it is said is not said. Of several arriving before one is said, only the newest is.

## How long a notice stands

- **It belongs to the launch.** The daemon keeps a session's standing notices with its running process, in the order
  they were raised, and serves them with the session's record, so a window that connects later shows the same banner.
  They are kept in memory only; when the process ends, its notices go with it, and the next launch raises its own.
- **It is withdrawn when its condition ends**, where it has one (see the table below): it leaves the record and every
  window's banner.
- **One notice of a kind at a time.** A notice raised again with the same code, and the same detail where it has one,
  replaces the earlier one in its place in the list.
- **Across a daemon restart**: a daemon that takes over a running session restores the notices its launch had when it
  started, less those withdrawn since: a withdrawal is kept beside the running launch, so a notice withdrawn under one
  daemon stays withdrawn under the next. Only a daemon that ends at the very moment it withdraws a notice can leave
  that notice to come back. A notice raised later in the launch comes back only once the new daemon finds its
  condition again.

## The notices

Each is a `session_notice` marked `standing`, with its `code` and `params`, and the daemon's English `message`. The
session's record lists those that stand as `standing_notices`, each with its `code`, `params`, `message` and `at`, the
time it was raised in milliseconds since the epoch; a withdrawal is a `session_notice_withdrawn` naming the code. These
are Octoboard's own:

| Code | Raised when | Withdrawn |
|---|---|---|
| `launch_enhancements_unavailable` | Octoboard's additions could not be prepared, so the session started as in a plain terminal. It says that the approval chosen in Octoboard is not applied, the agent's own configuration deciding what it may do, and nests the failure as `reason`, `reason_code` and the failure's own parameters, whose words and remedy the banner's list shows | never |
| `launch_retried_plain` | The agent's CLI exited as it started, and the plain launch that followed stayed up. It says the same of the approval; `detail` is what the CLI wrote as it exited | never |
| `hooks_not_received` | A hook was due and none arrived within 30 seconds | when a hook arrives |
| `conversation_id_unknown` | The launch has no conversation id and nothing will report one, so its next start opens a new conversation | when an id is recorded for the session |
| `conversation_record_not_found` | A resume went ahead though the conversation's record was not found, the agent's own resume saying in the terminal what it finds | never |
| `account_sign_in_unconfirmed` | The user opened the session on an account last read as logged out; `agent`, `agent_id`, `account`, `config_dir` | when a reading finds the account logged in |
| `shell_environment_unavailable` | The login shell's environment could not be read, so the launch ran on a fallback; `cause` is `earlier_capture` or `daemon`, `detail` the capture's failure | never |
| `session_git_hooks_unavailable` | A project session could not be given Octoboard's git hooks; `cause` is `git_too_old` (with the git `version`), `user_configuration` or `other` | never |

"Never" means for the rest of the launch. When each is raised is in
`docs/product/launching-without-octoboards-additions.md`, "The launch environment" in
`docs/product/launching-agents.md`, "When the agent or the account cannot open a session" in
`docs/product/opening-sessions.md`, and "Archiving, interruption and resuming" in
`docs/product/archiving-and-resuming-sessions.md`. An agent's own standing notices — what a launch with less left
out, an addition its configuration turns off, a version not measured — are in `docs/product/per-agent-launch-specifics.md`.

## A window that predates standing notices

A window of an earlier build that speaks the same protocol generation receives each standing notice as an ordinary
`session_notice`, as it arrives: it shows it as a toast (see `docs/product/toasts.md`), or, where the agent's provider
marks the notice as one to acknowledge, in the error alert (see "Notices to acknowledge" in
`docs/product/error-alerts.md`), and it shows nothing of a notice raised before it connected.
