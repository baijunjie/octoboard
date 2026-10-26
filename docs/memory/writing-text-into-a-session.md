# Writing text into a session

## Text the daemon writes into a session begins with a constant listed in `OCTOBOARD_OPENINGS`

Applies to any daemon code that adds a new kind of text written into a session's input — a notice, a brief, an
opening prompt, a copy of something shared. An archived session's history (`apps/daemon/src/session_history.rs`)
tells Octoboard's messages from what the user typed only by how they begin, matched against `OCTOBOARD_OPENINGS`
there; nothing at the place that writes the text checks this, so a new kind left out of the list shows in the history
as the user's own words, and no test fails.

So give the new text a fixed opening, defined as a constant beside its renderer (as `REPORT_OPENING` sits beside the
report's renderer in `apps/daemon/src/reporting.rs`), and add that constant to `OCTOBOARD_OPENINGS`. A notice that
begins "Message from Octoboard" is built from `reporting::NOTICE_PREFIX`, or `concat!` on `reporting::notice_prefix!`
for a constant, never a retyped literal; one so built is already covered. Where the opening alone reads like
something a user could type, give its first line a fixed phrase too and match on both, as the branches prompt does
with `mcp::exec::BRANCHES_MARKER`.

## A key the daemon types into a session on its own is gated, alone and sent once

Applies to daemon code that writes keystrokes of its own into a session's terminal (an interrupt key, a Ctrl-U, an
answer to a screen) rather than a message through the queue. The same key does harm in a different state of the
agent: Ctrl-C at an idle Codex exits it, a second Esc at an idle Codex opens its transcript rewind, and one Ctrl-C at
an idle Claude Code or Grok Build arms its "press again to exit" (the measurements are under each agent's "Keys that
interrupt a turn" in `docs/agent-cli-reference.md`). So:

- Write it through `LiveSession::write_keys_if`, with a gate that re-checks, under the input lock and right before
  each key, the state the key is meant for — for a key meant for a running turn, the hook status read from the store
  being `Working` and `title_activity().shows_running(..)` both. A check made before taking the lock is stale by the
  time the key lands. The bytes are constants; no text from a model or a message reaches such a write.
- One key per write, never joined to a message or to another key: an Esc written together with a bracketed paste
  breaks the paste in Codex, whose text then lands behind a literal `[200~`.
- Never repeat a key blind to make sure the first took: read what it did, on the screen or in the daemon's state,
  before any further press (a run of Ctrl-U presses goes one at a time, each seen on the screen). To learn that a turn ended,
  compare `AppState::turn_ends` with a reading taken before the key, not the status: a message the agent had queued
  takes it back to `Working` within milliseconds of a cancel, so polling for `Idle` misses it.
- Where the step reads or clears the agent's input box, hold `AppState::pause_outbox` for the session from before
  the first key until it is done, so a queued message does not land in or join text left in the box, and read the box
  from `screen_text_undimmed`: Claude Code draws a fresh session's placeholder (`❯ Try "…"`) faint, and the plain
  screen text reads it as typed text.
