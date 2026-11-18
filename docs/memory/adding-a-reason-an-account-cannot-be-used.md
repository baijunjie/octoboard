# Adding a reason an account cannot be used

## Where each place that picks or accepts an account checks it

The rule, a new reason checked at every place that picks or accepts an account in the same change, is
`docs/conventions/daemon.md`, "Agent accounts". The places, by file:

- Daemon refusals: opening a session (`resolve_open` in `apps/daemon/src/coordinator/open.rs`) and switching a
  session's account (`switch_session_account_with` in `apps/daemon/src/coordinator/account_switch.rs`). If the refusal
  has a new error code, the UI classifies that code too.
- Daemon choices made without asking: automatic account selection (`account_can_open` and `cannot_open` in
  `apps/daemon/src/accounts/choice/standing.rs`, where a new reason becomes a `CannotOpen` variant judged by
  `blocked_by`), the MCP fallback to the calling session's account (`choice_for_started_session` in
  `apps/daemon/src/accounts/choice/started.rs`), and the descriptions of that fallback in the MCP tool
  descriptions and schemas (`apps/daemon/src/session_tools/catalogue/`) and in `apps/daemon/PROTOCOL.md`.
- What agents are told before they pick: `list_agents`'s `can_open` and `cannot_open_reason` (`agent_accounts` in
  `apps/daemon/src/mcp/tools/agents.rs`), which word each `CannotOpen` the choice module reports, except a turned-off or
  removed account, which it leaves out (a new variant is a new arm there), and the list of reasons in its
  description (`apps/daemon/src/session_tools/catalogue/reads.rs`).
- Daemon work that spends an account: starting usage windows automatically (`AccountQuotas::kick_allowed` in
  `apps/daemon/src/accounts/quotas/poll.rs`, which both the kick itself and the due mark an on-demand refresh leaves for
  the poller consult).
- UI rules in `packages/ui/src/accounts/`: the choice groups and `isUsable` (`choiceEntries.ts`),
  `avoidUnavailableAccount` and `settledInitialChoice` (`openingChoice.ts`) and the switch-menu entries
  (`switchEntries.ts`).
- UI pickers and flows that use those rules: the new-session dialog (`useLaunchChoice`), quick create
  (`packages/ui/src/dialogs/openSessionRequest.ts`), the switch-account dialog and menu, and the default-account
  pickers of the console dialog (`ConsoleAccountsField.tsx`) and the project dialog. The last two were missed at
  first. Each picker marks the account as its product doc says: unavailable, or still pickable with a note.
- The gallery's fixture daemon, in each domain module under `packages/ui/src/fixtures/` that answers a request the
  new reason refuses (`fixtureAccounts.ts`, `fixtureSessions.ts` and the like), so the gallery shows it being refused.

## Re-measure a reading in scratch homes before it refuses work

The rule (re-measure against every sign-in kind, gate only on a reply that tells them apart, an unmatched reply is
unknown) is `docs/conventions/daemon.md`, "Agent accounts". Measure in scratch homes holding each kind of sign-in the
agent supports: fake keys and fake credential files cover most of them, with no real login. Gate on the agent's own
account state rather than its quota: a refused or empty quota request is what API-key, cloud-provider and
custom-provider accounts answer too. What each agent answered, and what could not be measured, is in
`docs/agent-cli-reference.md`, "Signing in a config directory without a terminal" (its paragraph "Before a login, on the
quota path").
