# Adding a reason an account cannot be used

## Where each place that picks or accepts an account checks it

The rule, a new reason checked at every place that picks or accepts an account in the same change, is
`docs/conventions/daemon.md`, "Agent accounts". The places, by file:

- Daemon refusals: `refuse_unless_account_usable` in `apps/daemon/src/coordinator/accounts.rs`, whose `AccountGate`
  says which reasons each kind of opening refuses: the full gate for switching a session's account
  (`switch_session_account_with` in `apps/daemon/src/coordinator/account_switch.rs`) and a handoff's receiving session
  (`check_handoff_open` in `apps/daemon/src/coordinator/open.rs`), the new-session gate for every new session
  (`resolve_open` and `check_open` there: the dialog, defaults, the MCP open and request tools, resume by ID), and a
  turned-off account only for a handoff whose account the user still picks (`check_handoff_open_asking`), refused in
  full at approval. A new reason decides which gates refuse it. If the refusal has a new error code, the UI classifies
  that code too.
- Daemon choices made without asking: automatic account selection (`account_can_open` and `cannot_open` in
  `apps/daemon/src/accounts/choice/standing.rs`, where a new reason becomes a `CannotOpen` variant judged by
  `blocked_by`; decide whether the choice may still fall back to it, as it does to the soonest-resetting used-up
  account), the MCP fallback to the calling session's account (`choice_for_started_session` in
  `apps/daemon/src/accounts/choice/started.rs`), and the descriptions of that fallback in the MCP tool
  descriptions and schemas (`apps/daemon/src/session_tools/catalogue/`) and in `apps/daemon/PROTOCOL.md`.
- What agents are told before they pick: `list_agents`'s `can_open` and `cannot_open_reason` (`agent_accounts` in
  `apps/daemon/src/mcp/tools/agents.rs`), which word each `CannotOpen` the choice module reports, except a turned-off or
  removed account, which it leaves out (a new variant is a new arm there), and the list of reasons in its
  description (`apps/daemon/src/session_tools/catalogue/reads.rs`). `can_open` means usable (automatic choice may
  pick it, a switch or handoff accepts it), not that opening a new session on it succeeds.
- Daemon work that spends an account: starting usage windows automatically (`AccountQuotas::kick_allowed` in
  `apps/daemon/src/accounts/quotas/poll.rs`, which both the kick itself and the due mark an on-demand refresh leaves for
  the poller consult).
- UI rules in `packages/ui/src/accounts/`: the choice groups, `isUsable` and `isOnlyUsedUp` (`choiceEntries.ts`),
  `avoidUnavailableAccount` and `settledInitialChoice` (`openingChoice.ts`) and the switch-menu entries
  (`switchEntries.ts`). `isUsable` is what a choice prefers, not what a new session may open on.
- UI pickers and flows that use those rules: the new-session dialog (`useLaunchChoice` and `accountOption`, whose
  `refuseUsedUp` disables a used-up account only in the handoff dialog and the handoff request form; a new reason
  refused in some flows and not others needs the same per-flow handling), quick create
  (`packages/ui/src/dialogs/openSessionRequest.ts`), the switch-account dialog and menu, and the default-account
  pickers of the console dialog (`ConsoleAccountsField.tsx`) and the project dialog. The last two were missed at
  first. Each picker marks the account as its product doc says: unavailable, or still pickable with a note.
- The gallery's fixture daemon, in each domain module under `packages/ui/src/fixtures/` that answers a request the
  new reason refuses (`fixtureAccounts.ts`, `fixtureSessions.ts` and the like), so the gallery shows it being refused.

## Re-measure a reading in scratch homes before it refuses work

A reading that only labelled a state (a quota probe's reply, say) and is to become a reason an account cannot be used is
re-measured first against every sign-in kind the agent supports. Gate only on a reply that tells the kinds apart, and
count a reply that matches no measured marker as unknown, not as the refusing reason: an API-key or custom-provider
account answers a quota probe like a signed-out one and still works. Measure in scratch homes holding each kind of
sign-in the agent supports: fake keys and fake credential files cover most of them, with no real login. Gate on the
agent's own account state rather than its quota: a refused or empty quota request is what API-key, cloud-provider and
custom-provider accounts answer too. What each agent answered, and what could not be measured, is in
`docs/agent-cli-reference.md`, "Signing in a config directory without a terminal" (its paragraph "Before a login, on the
quota path").
