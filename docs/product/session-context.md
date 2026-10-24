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

A Claude Code session has no window, so its chip shows the compact size with no ring and its card shows no share in
use (see "The context-size chip" below):

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

The sidebar's session rows outside focus mode, focus mode's live session cards, the full archive view, and the header of an archived
session's Session history show the context size as a chip only when it is known and greater than zero; an unknown or
zero size shows no chip at all. The chip is set apart from neighbouring text by spacing alone, with no separator
character. It looks the same in every one of these places, for live and archived sessions alike:

- It is a compact, localized number without a unit label (for example "190.1K"), drawn as the same small grey badge an
  agent picker uses to mark an agent as not installed (see "Which agents the agent pickers list" in
  `docs/product/consoles.md`).
- When a context window is known (for Claude Code, see "Claude Code's context window" above), a small ring sits inside
  the badge before the number, its arc showing the share of the window in use. The arc is in the accent colour up to
  and including 50% used, the warning colour above 50%, and the danger colour above 80%. Without a window the badge
  holds the number alone.

The share in use is a whole percentage and follows the agent's own accounting: Codex's matches what its `/status`
shows, which sets a fixed 12,000-token baseline aside as neither used nor available; Grok Build's is the size over the
window, rounded down; Claude Code's is the size over the window rounded to the nearest whole percent, as its own status
line computes it. All are between 0% and 100%, a size beyond the window showing 100%.

Hovering the chip shows its tooltip, a **Context window** card, whether or not the chip holds a ring.
A tap on the chip beside the Session history's heading opens the same
card in a popover; a tap on a chip in a row is the row's press (see "Tooltips on indicators" in
`docs/product/labels-and-tooltips.md`). With a window known, the card has:

- a top line with the "Context window" title at its start and, at its end, the size against the window in compact,
  localized numbers (for example "14.7K / 200K");
- a bar of the share in use, in the colour the ring's arc would have for that share;
- a bottom line with the share in use ("91% used") at its start and, at its end, how much of the window is left in a
  compact number ("17.6K left"): the window minus the size, never below zero.

Without a window, the card is the title and the compact size alone.

The chip's accessible name is one plain sentence with exact, localized counts: the context size in tokens, then, when a
window is known, the window as the limit, the share in use, and the tokens left (for example "Context 190,123 tokens,
Limit: 475,000, 40% used, 284,877 left"); without a window it is the context size alone. The chip is not a tab stop.

For where the sidebar-row chip appears, see "Session rows" in `docs/product/sidebar-session-rows.md`; for the live-card
chip, see "What both views show" in `docs/product/focus-mode.md`; for the Session history's chip, see "The context
size beside the heading" in `docs/product/session-history.md`. Archive previews show archive times as described in
"The archive view" in `docs/product/archive-view.md`. The history's dates are described in "Archive and creation times"
in `docs/product/session-history.md`.

## When the context size refreshes

The context size refreshes approximately once a second while the agent process is running, subject to when the agent
writes its records; it is not estimated while a reply streams. A final refresh follows the process stopping. The last
known context size remains available after archiving and restarting Octoboard. An archived session gets its final
context size from its records once, when they are still available; missing or unreadable records leave no chip rather
than a zero, and never erase a context size already known. Cumulative token totals saved by earlier versions are
discarded on upgrading, since they measured something else, and the records are read again in their place.

Switching accounts while keeping the same native conversation keeps its context size; moving to a different native
conversation clears it until the new one shows its own.
