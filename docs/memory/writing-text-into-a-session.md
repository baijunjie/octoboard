# Writing text into a session

The rules for text and keys the daemon writes into a session (openings, headers, gated single keys, the log) are in
`docs/conventions/daemon-processes-and-io.md`, "Writing text into a session"; the input box's protocol and its
per-message bound are in the module docs of `apps/daemon/src/session/draft_clear.rs` and
`apps/daemon/src/session/composer.rs`. What they leave out:

## Where the per-message bound does not apply

The only waits on a message's path left unbounded by `draft_clear::UNCLEARABLE_DRAFT_WAIT` are the ones
`docs/product/draft-preservation.md`, "When the composer cannot be cleared", names (a dialog, a trust confirmation, a
wait for the user's action, and an unreadable box after a key that may bring in unknown text, which still tells the user
once the bound has passed).

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
