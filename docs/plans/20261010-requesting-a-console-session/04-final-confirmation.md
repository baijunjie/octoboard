# 04 Final confirmation

> Goal: confirm in the running app what the daemon tests could not exercise.
> Completion criteria: every item below confirmed in the app, any defect found fixed on the spot.

- [ ] Reopening a session bound to an archived lead session whose console session is archived too: when the lead
  session's relaunch fails after its console session was brought back, the console session stays reopened and the
  session stays archived (the test harness cannot make a launch succeed).
- [ ] Reopening a session bound to an archived lead session reopens no other session bound to that lead session.
- [ ] With Codex and with Grok Build (both accounts were at their usage limits when 02 was verified, so only the
  daemon side was exercised for them): the call from a running unbound project session shows the dialog and waits;
  approval starts a console session that receives the request as a report with the caller bound to it; refusal, by
  the button and by Escape, leaves everything unchanged.
- [ ] With Codex: what Codex sends when its own MCP tool-call time limit passes while the call waits (believed to be
  60 s by default, not measured). If it sends `notifications/cancelled`, the request is withdrawn at that point and a
  later approval starts nothing, which would break "the call waits" for Codex; fix it on the spot if so.
