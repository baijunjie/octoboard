# Writing text into a session

## Text the daemon writes into a session begins with a constant listed in `OCTOBOARD_OPENINGS`

Applies to any daemon code that adds a new kind of text written into a session's input — a notice, a brief, an
opening prompt, a copy of something shared. An archived session's history (`apps/daemon/src/records/history.rs`)
tells Octoboard's messages from what the user typed only by how they begin, matched against `OCTOBOARD_OPENINGS`
there; nothing at the place that writes the text checks this, so a new kind left out of the list shows in the history
as the user's own words, and no test fails.

So give the new text a fixed opening, defined as a named constant (most are the `*_OPENING` constants in
`apps/daemon/src/messaging/texts.rs`, `REPORT_OPENING` among them), and add that constant to `OCTOBOARD_OPENINGS`. A
notice that begins "Message from Octoboard" is built from `messaging::texts::NOTICE_PREFIX`, or `concat!` on
`messaging::texts::notice_prefix!` for a constant, never a retyped literal; one so built is already covered. Where the
opening alone reads like something a user could type, give its first line a fixed phrase too and match on both, as the
branches prompt does with `messaging::texts::BRANCHES_MARKER`.

Text that one session, or Octoboard on a session's behalf, writes into another also says who sent it, how it relates
to the receiver and what it is (the standard is in `docs/product/message-headers.md`): the user's own typing is the only
text with no header, and that is how a session tells the two apart. Text a session passes along is quoted with
`naming::quote`; a parent session's instruction goes through `texts::render_instruction` and is not quoted, since it is
to be carried out. A new header is also named in what sessions are told about headers (`message_headers` in
`apps/daemon/src/session_tools/role.rs`, and the console session's instruction file, which a test checks).

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
- Never repeat a key blind to make sure the first took: read what it did, on the screen or in the daemon's state, before
  any further press (a run of Ctrl-U presses goes one at a time, each seen on the screen). To learn that a turn ended,
  compare `AppState::turn_ends` with a reading taken before the key, not the status: a message the agent had queued
  takes it back to `Working` within milliseconds of a cancel, so polling for `Idle` misses it.
- Bound the wait for the agent's answer to a key by the agent's pace measured on the slowest host it runs on, not on a
  Mac: on Windows on ARM64, with the x64 daemon under emulation, Claude Code blanks its screen for seconds after the
  trust screen's Enter (the figures are under "What they draw and read on a pseudoconsole" in
  `docs/agent-cli-reference.md`), so a 2 or 3 second window reported presses that had taken as failed. Measure there
  before choosing a bound, and end each wait as soon as what it waits for is seen, so a generous bound only slows the
  report of a real failure.
- Where the step reads or clears the agent's input box, hold `AppState::pause_outbox` for the session from before
  the first key until it is done, so a queued message does not land in or join text left in the box, and read the box
  from `screen_text_undimmed`: Claude Code draws the hints of an empty box faint (a fresh session's `❯ Try "…"`,
  `❯ Press up to edit queued messages`), and the plain screen text reads them as typed text. Do not blank faint cells
  in a reader of your own: over a hint, Claude Code's cursor is an inverse cell outside the faint run, which
  `screen_text_undimmed` discounts and a faint-only filter reads as a one-character draft.
- Text in the box is not yet the user's: for some 20 ms after a message the daemon wrote, Claude Code still shows that
  message in the box (measured under "Writing into a running session" in `docs/agent-cli-reference.md`), and holding
  the outbox does not cover a message written just before. So a step that acts on a non-empty box soon after a write —
  a stash key above all, which would stash the message itself — first waits, bounded, for the box to empty, as
  `LiveSession::settled_composer` does after `note_message_written`.

## A gate on a terminal mode the agent sets reads the mode as it stands, after reading the agent's whole start-up output

Applies when writes into a session, or the release of held ones, wait on a terminal mode the agent turns on (bracketed
paste, focus reporting, the alternate screen) as the sign that it reads its input. An agent can switch such a mode on
and straight off again while it probes the terminal at start, long before it reads anything — Claude Code does so with
bracketed paste (the bytes are under "Writing into a running session" in `docs/agent-cli-reference.md`) — and whether
one PTY read ends between the on and the off is a race. A latch on "seen on once since launch" therefore passes every
unit test and fails live now and then. So:

- Before resting a gate on a mode, read the agent's whole start-up output from a real capture for every place it
  switches that mode, off as well as on, not only the first time it comes on.
- Gate on the mode being on as of the latest output at the point that counts (for Claude Code, at its first hook or
  after it), not on its having been seen once.
- Test the toggle split across two reads, each state held longer than the poll interval of anything that watches the
  mode; a test that feeds the start-up output in one read, or flips the mode within one poll, cannot show the race.

## Diagnose a message left unsent from inside the write, in a throwaway build

Applies when a message the daemon writes into a live session is now and then not sent (left in the input box, echoed,
or missing its Enter) while the unit tests pass. Watching from outside does not find it: the agent's transcript holds no
trace of a prompt it never submitted, and attaching a terminal client or a recorder to the session can shift the timing
enough to hide the failure (in one investigation, 0 failures in 22 resumes with a recorder attached against 2 in 10
without). Instead, in a throwaway worktree that is never committed, make `term::send_message` dump, behind an
environment variable of its own, the PTY's output since launch from its ring buffer, the parsed screen and every
terminal mode the gate reads (such as `bracketed_paste_on()`) just before the write, plus screen samples for a few
seconds after it; run that build against the real agent with no client attached, and read the first failure it
captures.
