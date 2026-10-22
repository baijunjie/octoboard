# Session context size

## Session context size

A session's **context size** is how many tokens the main conversation's latest request to the model carried, as the
agent's own records show it. Every further turn sends that context again, so it shows whether continuing the session is
getting expensive. It is not cumulative token usage, a price estimate or an elapsed duration. Only the main
conversation counts: the agent's subagents and its side calls (title generation, summaries, classifiers) never do.

The figure follows the conversation down as well as up: it drops after a compaction, a revert or a rewind. Where the
agent's records say nothing yet, the context size is unknown, which is not the same as zero:

- **Claude Code**: after a compaction it is unknown until the next request of the main conversation.
- **Codex**: a thread just reverted shows the figure where it picks up the conversation until its own first turn; a
  forked thread is unknown until its first turn.
- **Grok Build**: a forked session starts with its parent's figure.

Where each agent's figure comes from:

| Agent | Context size | Context window |
|---|---|---|
| Claude Code | The input of the main conversation's latest request, cache reads and cache writes included and output left out, as Claude Code's own status line counts it. | The model's context window as Claude Code gives it to a status line, reported by the one Octoboard injects (see "Claude Code's context window" below). |
| Codex | The latest request's token count, as Codex's own `/status` shows it. | The model's context window, as in `/status`. |
| Grok Build | The context figure Grok Build shows in its footer. | The window Grok Build records beside it. |

## Claude Code's context window

Claude Code's own records hold no context window; it gives one only to a status line. Octoboard's status line
command (see "Claude Code's status line" in `docs/product/launching-agents.md`) reports it each time Claude Code runs
it, while the context size itself still comes from the transcript as in the table above. The latest report wins.

A Claude Code session has no window, and its tooltip no **Limit** row:

- until the first report since its launch, when none was stored for it before; Claude Code runs the status line as
  the conversation changes, so the first report comes with the session's first reply;
- when its Claude Code is older than 2.0.0, or its version cannot be told, since no status line is injected then;
- when a managed (policy) setting sets the status line, since that outranks Octoboard's and Octoboard's never runs;
- whenever Claude Code runs no status line at all: in a workspace it has not been trusted with, or with
  `disableAllHooks` set.

Once known, the window is kept: across a compaction, a restart of Octoboard, and into an archived session. After the
model is changed with `/model`, the new window shows from the next report. After an account switch the previous
window shows until the relaunched process reports its own.

## The context-size chip

The sidebar's session rows outside focus mode, focus mode's live session cards, its archive preview rows, the items
of the "View archive" and "Archived console sessions" submenus, and the full archive view show the context size as a
chip only when it is known and greater than zero; an unknown or zero size shows no chip at all. The chip holds a compact, localized number without a unit label, drawn as the same small grey
badge an agent picker uses to mark an agent as not installed (see "Which agents the agent pickers list" in
`docs/product/consoles.md`), set apart from neighbouring text by spacing alone, with no separator character.

Its tooltip has a heading line naming it as the context with the exact token count. When a context window is
known (for Claude Code, see "Claude Code's context window" above), a divider follows, then a **Limit** row with the
window's size and the share of it in use as a whole percentage, the value aligned at the row's end. The share follows
the agent's own accounting: Codex's matches what its `/status` shows, which sets a fixed 12,000-token baseline aside
as neither used nor available; Grok Build's is the size over the window, rounded down; Claude Code's is the size over
the window rounded to the nearest whole percent, as its own status line computes it. All are between 0% and 100%,
a size beyond the window showing 100%. The chip's accessible name is one plain sentence carrying the same information
in the same order.

For where the sidebar-row chip appears, see "Session rows" in `docs/product/sidebar.md`; for the live-card chip, see
"What both views show" in `docs/product/focus-mode.md`; for the archive submenus' chip, which has no tooltip there, see
"The archive view" in `docs/product/sidebar.md`. Archive dates and the archive metadata block are described in
"The archive view" in `docs/product/sidebar.md`.

## When the context size refreshes

The context size refreshes approximately once a second while the agent process is running, subject to when the agent
writes its records; it is not estimated while a reply streams. A final refresh follows the process stopping. The last
known context size remains available after archiving and restarting Octoboard. An archived session gets its final
context size from its records once, when they are still available; missing or unreadable records leave no chip rather
than a zero, and never erase a context size already known. Cumulative token totals saved by earlier versions are
discarded on upgrading, since they measured something else, and the records are read again in their place.

Switching accounts while keeping the same native conversation keeps its context size; moving to a different native
conversation clears it until the new one shows its own.
