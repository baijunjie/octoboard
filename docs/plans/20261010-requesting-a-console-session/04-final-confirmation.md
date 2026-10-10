# 04 Final confirmation

> Goal: confirm in the running app what the daemon tests could not exercise.
> Completion criteria: every item below confirmed in the app, any defect found fixed on the spot.

- [x] Reopening a session bound to an archived lead session whose console session is archived too: when the lead
  session's relaunch fails after its console session was brought back, the console session stays reopened and the
  session stays archived (the test harness cannot make a launch succeed).
- [x] Reopening a session bound to an archived lead session reopens no other session bound to that lead session.
- [ ] With Codex and with Grok Build (both accounts were at their usage limits when 02 was verified, so only the
  daemon side was exercised for them): the call from a running unbound project session shows the dialog and waits;
  approval starts a console session that receives the request as a report with the caller bound to it; refusal, by
  the button and by Escape, leaves everything unchanged.
- [ ] With Codex: what Codex sends when its own MCP tool-call time limit passes while the call waits (believed to be
  60 s by default, not measured). If it sends `notifications/cancelled`, the request is withdrawn at that point and a
  later approval starts nothing, which would break "the call waits" for Codex; fix it on the spot if so.

## Mid-task handoff

The two items above are confirmed; the two below are not started, and the user set them aside for now (both
accounts were at their usage limits when 02 was verified, and they are again). Nothing else is left.

How the two confirmed items were run, so the next run does not have to work it out again: a packaged build
(`pnpm build:app`) under a throwaway `HOME`/`TMPDIR`, with a stand-in `claude` on that run's `PATH` that records its
argv and then `exec cat`s, so no model turn is spent and no other agent can be reached. The lead session was made by
the real route — an unbound project session's `request_console_session` sent to `POST /mcp/:token` with the token
read out of the agent process's own `--mcp-config` argv, answered with `answer_console_session_request`. One file of
recorded argv per spawned process is what shows which sessions were launched at all. The launch of the lead session
was made to fail by renaming the project's directory away, which `term::launch` refuses on before spawning anything;
the console session's own working directory stays reachable, so only the project sessions below it fail. Give the
verification copy of the bundle its own identifier if the user's own Octoboard is running, so a scripted click
cannot land on their window.

A defect found while confirming them, filed separately as it predates this topic and fires for any session: a
relaunch that failed to launch re-stamped the session's archived time. Since fixed.
