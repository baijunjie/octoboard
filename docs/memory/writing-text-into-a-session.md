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
