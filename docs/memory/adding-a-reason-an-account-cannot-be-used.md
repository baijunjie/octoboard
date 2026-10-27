# Adding a reason an account cannot be used

## Every place that picks or accepts an account checks a new reason, not only where a session opens

A used-up quota, a running sharing change and an account that is turned off each keep an account from new work. No
single check covers them. Each place that picks an account, or accepts one it was given, tests each reason on its own.
So a new reason is checked at every one of these places in the same change. If you check only some of them, the
other places still put work on the account or offer it with nothing marked. The daemon then refuses that work later,
or starts the work anyway where nothing refuses it.

- Daemon refusals: opening a session (`resolve_open`) and switching a session's account
  (`switch_session_account_with`), both in `apps/daemon/src/coordinator.rs`. If the refusal has a new error code, the
  UI classifies that code too.
- Daemon choices made without asking: automatic account selection (`account_can_open`), the MCP fallback to the
  calling session's account (`apps/daemon/src/mcp/exec.rs`), and the descriptions of that fallback in the MCP tool
  descriptions and schemas (`apps/daemon/src/mcp/mod.rs`) and in `apps/daemon/PROTOCOL.md`.
- Daemon work that spends an account: starting usage windows automatically (`AccountQuotas::kick_allowed` in
  `apps/daemon/src/account_quotas/mod.rs`, which both the kick itself and the due mark an on-demand refresh leaves for
  the poller consult).
- UI rules in `packages/ui/src/accountChoices.ts`: the choice groups, `avoidUnavailableAccount`, `settledInitialChoice`
  and the switch-menu entries.
- UI pickers and flows that use those rules: the new-session dialog (`useLaunchChoice`), quick create
  (`packages/ui/src/dialogs/openSessionRequest.ts`), the switch-account dialog and menu, and the default-account
  pickers of the console dialog (`ConsoleAccountsField.tsx`) and the project dialog. The last two were missed at
  first. Each picker marks the account as its product doc says: unavailable, or still pickable with a note.
- The gallery's fixture daemon (`packages/ui/src/gallery/fixtureDaemon.ts`), so the gallery shows the new reason
  being refused.

## Before a reading of an account refuses work, re-measure it against every way the agent can be signed in

Applies when something read off an account that so far only shaped what is shown — a quota probe's reply, a sign-in
marker — becomes a reason the account cannot be used: refusing an open, a handoff or a switch, or disabling the account
in a picker. A reading that only labelled a state was measured against the case it labels, usually a home signed in to
nothing, and the other sign-ins an agent supports — an API key, a cloud provider, a custom provider — can answer the
same way and still work. A refused or empty quota request is such a reading: it is what those accounts answer too, so
it is no sign of being signed out, and gating on it locks out working accounts.

So before the change, re-measure the reading in scratch homes holding each kind of sign-in the agent supports (fake
keys and fake credential files cover most of them, with no real login), and gate only on a reply that tells them
apart, such as the agent's own account state rather than its quota. Where a kind could not be measured, treat a reply
that matches no measured marker as unknown, not as the refusing reason. What each agent answered, and what could not be
measured, is in `docs/agent-cli-reference.md`, section "Signing in a config directory without a terminal" (its
paragraph "Before a login, on the quota path").
