# Folder trust

Each of the three agents stops the first time it runs in a folder it has not been told to trust, on a
confirmation of its own, and waits for a person. Octoboard never makes that trust decision itself: it
presses the agent's own confirmation on the agent's own screen, with the keystrokes a person would type —
for a project session only once the user has given Octoboard permission, for a console session without
asking. The agent then records the trust in its own configuration, as it does when a person answers, so
its later launches in that folder do not ask again. Grok Build is the one agent whose record cannot reach
the user's own configuration by itself; Octoboard carries the entry Grok wrote over, unchanged (see
"Carrying Grok Build's trust record" below).

## The agents' trust confirmations

| Agent | When it asks | The confirmation | Where the agent records the trust |
|---|---|---|---|
| Claude Code | The first time it runs in a directory it has not been trusted with. | A two-option list, "No, exit" and "Yes, I trust this folder", with the cursor starting on "No, exit". | Its own configuration. |
| Codex | Only when the working directory is inside a git repository Codex has not been told to trust; outside one it never asks. | "Trust this folder?", with "1. Trust and continue", where the cursor starts, and "2. Quit". | `config.toml` in the Codex home that launch uses, for the **root of the git repository**: from a subfolder it records the repository's root, and from a linked worktree the main repository's root, so the record can cover more than the project asked about. |
| Grok Build | Only when the folder holds content Grok's trust gates, such as an `AGENTS.md`, git repository or not. | "Do you trust the contents of this directory?", with "Yes, proceed" (`y`) and "No, quit" (`n`), and no cursor. A decline exits Grok. | Its trust store, `trusted_folders.toml`, in the session's own Grok home, from which Octoboard carries it to the user's own. |

Until its confirmation is answered the session does nothing else and reports nothing (see "What the statuses are derived
from" in `docs/product/session-statuses.md`). Until the folder is trusted, Claude Code ignores the project's own `allow`
permission rules (see "Claude Code" in `docs/product/per-agent-launch-specifics.md`), and Grok Build loads none of the
project's instructions, hooks or MCP servers.

## What happens when a confirmation comes up

| Session | What happens |
|---|---|
| Console session | Pressed at once, without a dialog and without recording anything: its working directory is the console's own, which holds the instruction file Octoboard writes there. |
| Project session whose project has the trust permission, of its own or through a trusted folder | Pressed at once, without a dialog. |
| Any other project session | The user is asked in a dialog; nothing is sent until they agree. |

Once Octoboard recognises a confirmation that needs the user, the session reads as *waiting for the user* and raises its
hand everywhere that status is shown or counted (see "The raised hand" in `docs/product/session-statuses.md`). Closing
the dialog with **Not now**, moving the terminal's cursor or a failed attempt to press the confirmation leaves the hand
up. It comes down when Octoboard successfully presses the confirmation, the agent sends its first hook report, or, for
Codex answered in the terminal, Octoboard sees that the confirmation has been replaced by the next screen — or, for Grok
Build, which may run no hook at all (see "Messages held until a session can take them" in
`docs/product/message-delivery.md`), sees that the session's screen no longer shows the confirmation, however it was
answered. The session then shows the agent's last reported status, or its launch status if no report has arrived. An
interrupted or archived session keeps that status instead of a trust hand.

A confirmation pressed automatically for a console session or under an existing trust permission raises no hand
unless that press fails and the confirmation still needs the user.

## The trust permission

What lets Octoboard press a confirmation without asking is one permission, **shared by all three agents**. It is
Octoboard's own record, not any agent's trust record, and it comes in two forms; either one is enough:

- **A project's permission** belongs to one project, is stored with it, and covers every later trust confirmation in
  that project, a resume included. It is set only by "Trust and continue" with the dialog's folder checkbox left
  unticked, and cannot be withdrawn from the application; editing the project leaves it as it is, and removing the
  project removes it with the association. A project starts without it.
- **A trusted folder** covers every project whose directory is that folder or lies anywhere below it, in any console,
  including projects associated after the folder was trusted — by the user or by a console session. It is set only by
  "Trust and continue" with that checkbox ticked, is stored on its own rather than with any project, and can be
  removed in Settings. Its rules are in "Trusted folders" below.

Because it is shared, agreeing when one agent asks covers a later confirmation from either of the other two in the same
project. Pressing one agent's confirmation records that agent's trust only; each of the others records its own when its
own confirmation is pressed. Either form is recorded only after Octoboard's own press of the confirmation has
succeeded. A confirmation the person answers in the terminal records no permission.

## The trust dialog

The dialog, titled "Trust this folder?", opens by naming the agent that is asking whether to trust this folder — for
the named session, when the session is known — with the project's path set apart below in a monospace block. Then:

- what agreeing does: Octoboard presses that agent's own confirmation and the agent records the trust, so it does not
  ask again there; Octoboard also records a permission every agent shares, so a later trust confirmation for this
  project is pressed without asking;
- for Codex only, that Codex records the trust for the root of the git repository the folder is in, which covers more
  than this project when the project is a subfolder or a linked worktree;
- last, a caution, a plain paragraph of the message like the rest, with no alert or title of its own, saying what
  trusting means for the agent that is asking: for Claude Code, that trusting lets
  the project's own permission rules and hooks take effect and its `.claude/settings.json` may pre-approve tool
  permissions; for Codex, that trusting lets Codex load the project's own configuration, hooks and command rules, which
  can run code even without a model request, and that the confirmation does not change Codex's sandbox or approval
  policy, though the project's own configuration can; for Grok Build, that trusting lets it load the project's
  instructions, hooks and MCP servers.

The project's path and the folder offered are shown with the home directory written as `~`, as in "How paths are
shown" in `docs/product/settings.md`, so the folder reads the same as the row it would add to Settings.

**The folder-wide choice** is a checkbox under the message, shown only when the daemon offers a folder for this
project (see "Trusted folders" below for which folder that is); a project with none to offer gets no checkbox. It
starts unticked. Its label names that folder, as the choice to trust every project under it, and a note below the
label says that this records the folder instead of this project and covers every project under it, at any depth,
including ones added there later, for every agent. The label is where the folder's path is read: it wraps rather than
being cut, and no tooltip carries it.

Its buttons:

- **Trust and continue** — Octoboard presses this session's confirmation. Only a press that succeeded records
  anything, and what it records is what the checkbox says:
  - *left unticked* — the project's permission is recorded; Octoboard then also presses the confirmations already
    waiting in other sessions of the same project, whichever agent shows them, and their prompts leave the queue;
  - *ticked* — that folder is added to the trusted folders instead, and the project's own permission is not recorded.
    If the folder can no longer be offered by the time the go-ahead is given (see "Trusted folders" below), it is
    refused before anything is pressed: an error alert explains why and the trust dialog stays open with the box still
    ticked, so the user can untick it and go on without the folder.
- **Not now** — also what Escape and the dialog's close button do. Nothing is sent and nothing is
  recorded; the confirmation stays for the user to answer in the session's terminal. The same session is asked about
  again only if the application reloads its state (a reconnect, or catching up after falling behind) while the
  confirmation is still up, and a later session of the project is asked about again, since the project still has no
  permission.

Keyboard focus starts on the dialog itself rather than on a button, for each prompt it shows, so Enter answers nothing
until the user moves focus onto a control (see "Where keyboard focus starts in a confirmation" in
`docs/product/dialogs.md`).

The box belongs to the one showing of the dialog: it is cleared when the dialog is dismissed with "Not now" and when a
go-ahead is sent, so the next prompt in the queue starts unticked, and so does the same prompt asked about again after
the application reloads its state.

Once the go-ahead has been given the dialog closes, whether or not the confirmation could be pressed — a confirmation is
pressed at most once, so trying again from the dialog could not succeed. A failure is then shown in an error alert about
the session. Two outcomes differ: the refusal of a folder that can no longer be offered keeps the dialog open, and a
go-ahead for a confirmation that is no longer waiting — already answered, whether from another client, in the terminal
or by Octoboard itself — closes the dialog without any message when the box was unticked, since nothing went wrong; with
the box ticked it shows a message saying the folder was not trusted, because that choice was not carried out.

Prompts are shown one at a time, oldest first; closing one brings up the next. A prompt still waiting is dropped,
without being answered, when its session stops running or is deleted. When a project gains the permission, or a folder
becomes trusted, the prompts waiting for that project or for projects under that folder are dropped as well, because
Octoboard presses those confirmations itself; prompts for other projects stay queued. Whenever the application reloads
its state (a connect, a reconnect, or catching up after falling behind), the daemon asks again about every confirmation
still waiting, so a prompt the user never saw comes back; with no client connected the confirmation simply waits for the
user to answer it in the terminal. Once one client has answered, a go-ahead from another changes nothing.

## How a confirmation is pressed

Octoboard sends only the keys that accept the confirmation, checking before each one:

| Agent | Keys | Checked before the keys |
|---|---|---|
| Claude Code | A Down, then an Enter | Before the Down, that the confirmation as it was recognised is still showing and the cursor is on "No, exit"; before the Enter, that the cursor has moved to "Yes, I trust this folder" and the terminal has gone quiet. |
| Codex | An Enter | That the confirmation as it was recognised is still showing with the cursor on "1. Trust and continue", and the terminal has gone quiet. |
| Grok Build | `y` | That the session has sent no hook report and its process is still running. Grok keeps animating its logo while it waits, so neither its first drawing still showing nor a quiet terminal can be checked for. |

Before any key, nothing else may have written into the session's input — since the attempt began for Claude Code,
since the confirmation was recognised for Codex and Grok Build: the user typing in the terminal, or a message Octoboard
delivers to the session. What an attached terminal sends by itself is not counted: focus reports, answers to the
agent's terminal queries, and mouse movement, wheel turns and button releases. A mouse button press is counted.

For Codex and Grok Build, anything typed into the session from a terminal after the confirmation was recognised is
taken as the person answering it there: its prompt is not put to a client again, a go-ahead for it is treated as one
for a confirmation that is no longer waiting, and an attempt of Octoboard's to press it ends without a notice.

When any check fails the attempt stops without sending the next key and the confirmation is left for the user. A
session's confirmation is pressed at most once per run, so a failed attempt is not retried; the user is told in a notice
on the session that Octoboard could not press that agent's confirmation, with the reason, and that they should answer it
in the terminal. The window shows it as an error alert titled "Could not answer the trust confirmation"; a reason that
carries the system's text, such as a failed write to the terminal, keeps that text under **Error details**, and one the
window has no words for is given as something unexpected having gone wrong. The confirmation counts as pressed once it
is seen to go away — for Grok Build, once Grok has sent its first hook report, within about 12 seconds of the key, and
the entry it wrote has reached the user's own trust store (see "Carrying Grok Build's trust record" below). When no hook
report comes to carry the entry — a policy that runs none of Octoboard's hooks — the press counts once the session's
screen is seen no longer to show the confirmation, and the permission a go-ahead from the trust dialog asked for is
recorded then; nothing has been carried to the user's trust store at that point, and a hook report that comes later
still carries the entry.

**Recognition is limited to the start of a session.** A confirmation is looked for only in the session's own terminal
output, and only for as long as it can still come up, so the same words appearing later in a session are never taken
for it:

- for Claude Code and Grok Build, until the session's first hook report, which each sends only once it is past the
  confirmation. There is no limit on output or time before that, so a confirmation that comes after a long stretch of
  other screens is still recognised — Claude Code's onboarding on an account's first launch, with its sign-in through
  the browser, comes before its confirmation;
- for Codex, which sends no hook report until its first prompt, until the first terminal title it sets or its first hook
  report, whichever comes first. Codex sets its terminal title only once its session has started — past the
  confirmation, and on an account's first launch past its sign-in screen, which comes before the confirmation — so there
  is no limit on output or time before that either. Every launch sets the items its title shows (see "Codex" in
  `docs/product/per-agent-launch-specifics.md`), so no configuration switches the title off.

What Octoboard writes into a session while its confirmation may be up is held; see "Messages held until a session can
take them" in `docs/product/message-delivery.md`.

## Carrying Grok Build's trust record

A Grok Build session runs against a per-session Grok home (see "The per-session Grok home" in
`docs/product/per-agent-launch-specifics.md`), so the entry Grok writes when its confirmation is accepted lands only in
that session's copy of the trust store and would be lost with the session. Once the confirmation has been accepted —
pressed by Octoboard, or answered by the person in the terminal — Octoboard copies exactly that one entry, as Grok wrote
it, into `trusted_folders.toml` in the session's source home, adding it or replacing that folder's entry there, and
leaving every other entry as it is. It holds the store's lock file, `trusted_folders.toml.lock`, exclusively while it
reads and writes; Grok takes the same lock before it saves the store, so the two never write at once. Octoboard writes
only an entry Grok itself wrote, never one of its own. An answer that writes no entry — a decline — changes nothing and
is not reported. A person's answer in the terminal is carried the same way, so Grok keeps it as Claude Code and Codex
keep an answer given in their own terminal, but it records no permission of Octoboard's.

When the entry does not reach the user's store, nothing is written there, no permission is recorded, and a notice on
the session says that Octoboard could not copy Grok's trust for this folder into that file, so Grok will ask again the
next time it opens the folder. The window shows it as a toast (see `docs/product/toasts.md`), except where it carries
what the system said — a failed read or write of the file, or a reason this window has no words for, which the
sentence then gives as something unexpected having gone wrong. That is an error alert titled "Trust not carried
over", its first line naming the session, with the system's text under **Error details** and no **Report issue** (see
"The error alert" in `docs/product/error-alerts.md`). The reason given is one of:

- no entry for the folder appeared in the session's copy of the store within a few seconds of Grok's hook report;
- the user's store stayed locked by another program;
- the user's store could not be read as a trust store, or the entry could not be added without changing anything else
  in it, so it was left as it is;
- reading or writing the file failed; the system's own message is not part of the sentence.

When Octoboard pressed the confirmation itself, such a failure is also the failure of that press, so "Trust and
continue" records nothing, the folder checkbox ticked or not.

## Trusted folders

**What trusting a folder grants.** A trusted folder is not limited to the projects present when it was trusted. Every
project associated under it later is covered as well, by whatever means it was associated — **including repositories a
console session clones or adds into that folder on its own** — and Octoboard presses every agent's trust confirmation
for each of them without asking. Once a confirmation is pressed, that agent records the trust and applies the project's
own configuration without asking either: Claude Code the permission rules and hooks in the project's
`.claude/settings.json`, Codex the project's own configuration, hooks and command rules, Grok Build the project's
instructions, hooks and MCP servers. Trusting a folder therefore means trusting whatever ends up inside it, for as long
as the folder stays trusted.

**Which folder.** The folder offered, and the only one that can be trusted from a session's dialog, is the parent
directory of that session's project; the daemon derives it from the project's path, and no client can name a folder of
its own. None is offered — and the dialog shows no folder checkbox — when:

- the project's path is not absolute;
- the parent is the filesystem root, which includes a project directly inside `/`;
- the parent is the user's home directory or any directory that contains it, which includes a project directly inside
  the home directory. This is checked on the paths as written and also on the directories themselves, so a symbolic
  link to the home directory or to a directory above it, or a spelling that differs only in letter case on a volume
  that does not tell case apart, is caught as well;
- the home directory cannot be determined.

Such a project can still be trusted on its own with "Trust and continue". The same check is made again when a go-ahead
with the folder ticked arrives, and a folder that fails it then is refused before anything is pressed — with the error
code `trust_directory_too_broad` when it is the root, the home directory or a directory containing it,
`trust_path_not_absolute` when the project's path is not absolute, and `trust_home_unknown` when the home directory
cannot be determined.

**Which projects it covers.** A project is under a trusted folder when its directory is that folder or lies below it at
any depth. Paths are compared component by component after a purely lexical clean-up (`.` dropped, `..` folded into the
component before it, trailing slashes ignored):

- `/work` covers `/work/app` and `/work/a/b/c`, but not `/work2` or `/workspace/app` — never a text prefix;
- symbolic links are not resolved: a project is covered when the path it was associated under lies below the folder as
  written. A project associated through a symbolic link inside a trusted folder is therefore covered wherever that link
  points, so a link inside a trusted folder extends the trust to its target; a project associated under a path outside
  the folder is not covered, however it is linked from inside;
- names are compared exactly, so a spelling of the same folder that differs only in letter case is not covered;
- only absolute paths are ever covered: a project recorded with a relative path is never under a trusted folder.
  Octoboard records every project it associates with an absolute path (see "Associating a project" in
  `docs/product/projects.md`).

**Trusting a folder presses what is already waiting.** At the moment a folder is added, every trust confirmation still
waiting in a project under it is pressed, whichever agent shows it, including one the user put off with "Not now";
confirmations of projects outside it stay as they were and are still asked about. Each of these is pressed under the
same checks, and reported the same way when a check fails, as any other confirmation Octoboard presses.

**The list in Settings.** The Trusted folders section of Settings (see `docs/product/settings.md`) opens with a
paragraph explaining that under these folders Octoboard presses every agent's trust confirmation without asking, for
every project including ones added later, that each agent then records the trust in its own configuration and applies
the project's own permission rules and hooks, where a folder is trusted from — a session's
trust dialog — and, last, that a folder should be trusted only if everything that ends up inside it is trusted. The
section lists every trusted folder, sorted by path, each path shown as in "How paths are shown" in
`docs/product/settings.md`, with no line of text under it. With no folder trusted, an empty row in the list's place says
there are none yet (see "The compact row" in `docs/product/empty-states.md`); where a folder is trusted from is in the
paragraph above it, not in that row.

**Removing a folder asks first.** Each folder has a Remove button, whose accessible name gives the folder's full path
and whose tooltip says that it stops trusting the folder. Pressing it opens a confirmation, titled to remove a trusted
folder, whose first sentence says that the folder will be removed from the trusted folders, its path written with the
daemon host's home directory as `~` as in the list. Its message then says that later trust confirmations for projects
under the folder are asked about again unless the project's own permission or another trusted folder covers them, and
that projects' own permissions, running sessions and the trust each agent has already recorded are kept. Cancelling
changes nothing; confirming with **Remove** stops trusting the folder, and the confirmation closes once the daemon has
done so. A removal that fails is reported in an error alert and the confirmation stays open. Removing a folder leaves
every project's own permission as it is, leaves running sessions alone, a confirmation already pressed included, and
does not undo the trust an agent has already recorded; a trust confirmation that comes up afterwards in a project under
it, without a permission of its own and not under another trusted folder, is asked about again.
