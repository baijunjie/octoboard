# 04 Final confirmation

> Goal: confirm in the running app what the daemon tests could not exercise.
> Completion criteria: every item below confirmed in the app, any defect found fixed on the spot.

- [x] Reopening a session bound to an archived lead session whose console session is archived too: when the lead
  session's relaunch fails after its console session was brought back, the console session stays reopened and the
  session stays archived (the test harness cannot make a launch succeed).
- [x] Reopening a session bound to an archived lead session reopens no other session bound to that lead session.
- [x] With Codex: the call from a running unbound project session shows the dialog and waits; approval starts a
  console session that receives the request as a report with the caller bound to it; refusal, by the button and by
  Escape, leaves everything unchanged.
- [x] With Codex: what Codex sends when its own MCP tool-call time limit passes while the call waits. Nothing, after
  300 s; "the call waits" holds, and an approval after it is carried out through the message fallback.
- [ ] With Grok Build: the same five cases — the four in the Codex item above, and what Grok Build sends when its own
  time limit on a tool call passes. If it sends `notifications/cancelled`, Octoboard withdraws the request at that point
  and a later approval starts nothing, which would break "the call waits" for Grok Build; fix it on the spot if so.

## Mid-task handoff

Only the Grok Build item is left; everything else is confirmed. That account was at its usage limit both times this
milestone was worked, so only the daemon side has ever been exercised for it.

How a run is set up, so the next one does not have to work it out again: a packaged build
(`pnpm build:app --bundles app` from `apps/desktop/`) launched as
`target/release/bundle/macos/Octoboard.app/Contents/MacOS/octoboard` under a throwaway `HOME`/`TMPDIR`, keeping the
real bundle identifier — a re-identified copy never shows a window at all. The throwaway `HOME` needs a `.zshrc`
exporting the real `PATH`, or the daemon's login-shell snapshot will not find the agent's executable, and the agent's
own credentials copied in read-only (for Codex, `.codex/auth.json` plus a `config.toml` trusting the scratch project
directory). Sessions are made over `GET /ws/control`; the lead session is made by the real route — an unbound project
session's `request_console_session` sent to `POST /mcp/:token` with the token read out of the agent process's own
`--mcp-config` argv, answered with `answer_console_session_request`.

Where an item needs no model turn, a stand-in executable on that run's `PATH` that records its argv and then
`exec cat`s costs nothing and shows which sessions were launched at all, one file of recorded argv per spawned
process. The first two items were run that way: the launch of the lead session was made to fail by renaming the
project's directory away, which `term::launch` refuses on before spawning anything, while the console session's own
working directory stays reachable.

Answering the dialog needs a client. Scripted clicks on the native window require the terminal host to hold Screen
Recording and Accessibility; without them `screencapture` returns wallpaper and System Events reports no windows. The
Codex run answered the dialog from a second UI client instead — the same `packages/ui/dist` the bundle ships, served
on a local port and loaded with `?port=<daemon port>` in headless Chrome driven over CDP, with real mouse and key
events. The daemon treats it as any other client, so the daemon-side outcomes and the agent-side tool results are
fully exercised; the Tauri shell's own event path is not, and by the architecture rule in `apps/desktop/README.md` it
carries no daemon traffic. The user accepted that as confirmation for the Codex item.

Three defects turned up while confirming the first items, all of which predate this topic and have since been fixed.
One affected every Octoboard tool call from a Codex session, and another every approval modal such a session shows. The
third fires for any session, not only Codex: a relaunch that failed to launch re-stamped the session's archived time; it
was filed as its own ticket.
