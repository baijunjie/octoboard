# 05 Final confirmation

> Goal: close out the handful of checks that could not be made during development, so the MVP ships with nothing
> outstanding.
> Completion criteria: every item below has been exercised on a real build and has a recorded outcome; anything that
> fails is either fixed or written back into the plan as a deliberate limitation.

This milestone exists because a few confirmations are impossible until the real product exists or until an external
credential is available. They do not block 01 to 04 — each was checked for that — so they are gathered here rather than
holding earlier work up. Keep it last.

## Carried from milestone 00

Only what genuinely cannot be settled yet. Everything else milestone 00 raised was closed there.

- [ ] **Grok's `StopFailure`.** The only hook event of the three agents that was never captured from a live session; its
      payload shape is taken from Grok's own documentation. Provoking it means pointing Grok's chat endpoint at a server
      that returns an error, which redirects an authenticated client's traffic and so needs the user's explicit
      authorisation. Note Grok's "turn finished" mapping names this event, so that branch is unexercised.
- [ ] **The end-to-end terminal latency the user actually perceives.** The daemon-to-WebSocket path measures well under a
      millisecond, but the rendering step on top of it can only be instrumented once the real application exists.

## Notes for developers

- **Development notes**: neither of these is automatable. `StopFailure` needs the user to authorise pointing Grok's chat
  endpoint somewhere it will fail, and the latency figure needs the real application's render loop instrumented. Gatekeeper
  admission is **not** here — it is 04's own completion criterion.
  One technique worth reusing for the first item: pointing an agent at a local HTTP server that returns an error and dumps
  the request body makes its assembled prompt and its error-path behaviour readable at zero token cost. That is exactly
  what makes it credential-sensitive, which is why it needs authorisation rather than being done quietly.
- **Reference docs**: the conditions each of these qualifies are in `docs/mvp.md`, sections 4.4, 5.3 and 6.
