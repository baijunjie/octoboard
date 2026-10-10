# 02 The request tool and its confirmation

> Goal: an unbound project session can ask for a console session with `request_console_session`; on the user's
> approval Octoboard starts one and binds the caller to it as its lead session, its request arriving as its first
> report.
> Completion criteria: daemon tests cover announcing the tool to unbound project sessions and lead sessions only and
> refusing a lead session's call, approval, refusal, the time limit, withdrawal on each signal, an approval after a
> withdrawal, the fallback message after an unseen end of the call, and the first answer winning across clients; in
> the app, the call from a running project session of each agent shows the dialog and waits, approval starts a console
> session that receives the request as a report with the caller bound to it, refusal leaves everything unchanged, and
> the dialog is shown again after a reconnect. Depends on 01.

## Technical design

- [ ] `request_console_session`, announced to unbound project sessions and to lead sessions, which keep that tool
  list (see "Where the rules are written" in the overview). Its arguments carry what the caller needs
  done and what it has found out, shaped like a report. Its description says to call it when sessions are needed in
  other projects, that the user is asked first, and that on approval the caller becomes bound to the new console
  session and reports to it with `report`.
- [ ] A pending request the daemon holds until the user answers, broadcast to clients so a dialog can show it, and
  answered by a client request carrying approve or refuse. The tool call waits for the answer, up to a time limit.
- [ ] On the time limit passing without an answer: the dialog closes, nothing is started, and the call returns a tool
  error saying the user did not respond (see "User confirmation is enforced by Octoboard" in the overview).
- [ ] On approval: start a console session in the caller's console as one the user opens by hand, bind the caller to it
  through the binding operation of 01, and deliver what the caller handed over to it as the lead session's first
  report. The tool's result names the new console session and tells the caller it is now bound to it and must report
  to it with `report`.
- [ ] On refusal: a tool error carrying the reason in prose; nothing is started and the caller stays unbound.
- [ ] Withdrawal and the fallback, as in "User confirmation is enforced by Octoboard" in the overview: the pending
  request is sent again on reconnect; it is withdrawn, its dialog closed, when the caller's process ends, the call's
  connection is dropped or the agent cancels the call; an approval given after a withdrawal starts nothing and the user
  is told in a toast; an approval given after the call has ended unseen is carried out and its outcome also written into
  the caller's session as a message. With several clients, the first answer wins.
- [ ] Once the caller is bound, a further call is refused, with the reason in prose, as `report` is refused for an
  unbound session.

## Implementation plan

- [ ] Daemon: the tool's catalogue entry, the pending-request state and its client protocol messages, and the approve
  and refuse paths.
- [ ] The request's time limit: one value of Octoboard's own, below the fixed 10-minute timeout with which the
  per-session stdio MCP server forwards each call to the daemon over loopback. No agent's own tool-call time limit is
  measured or configured for it.
- [ ] UI: the confirmation dialog showing the requesting session and its request, following the existing pattern of
  the trust confirmation dialog.
- [ ] Update the product docs on orchestration (the unbound project session's tools, the request, the user's
  confirmation) and the daemon's protocol docs.

## Notes for the developer

- **Reusable capabilities**: the delivery of messages into a running session, held while the session cannot take one;
  the trust confirmation's flow of a daemon-held question answered from a dialog, shown
  again after the application reconnects; the console session launch used when the user opens one by hand; the
  reporting path's delivery to an owner, queued while the owner cannot take a message; refusals returned as tool
  errors carrying a reason in prose.
- **Development notes**: the confirmation is the enforcement; nothing in a prompt replaces it. Do not change any role
  description (see "Where the rules are written" in the overview). Anything Octoboard writes into a session goes
  through the existing sanitising and the hold while a trust confirmation may be up, the new console session included.
- **Reference docs**: `docs/product/hub-orchestration.md`, `docs/product/folder-trust.md`,
  `docs/agent-cli-reference.md` (role descriptions recorded once).
