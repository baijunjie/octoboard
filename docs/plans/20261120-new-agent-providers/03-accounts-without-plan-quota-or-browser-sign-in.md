# Accounts without a plan quota or a browser sign-in

> Goal: let a provider say that Octoboard reads no plan quota for it, read its accounts' sign-in state apart from any
> quota, and offer Log in only where the provider can drive the agent's sign-in, so that an account of such a provider
> is shown, chosen and refused correctly instead of reading "Reading usage…" for ever.
> Completion criteria: with the test-only provider declaring no quota window, no login flow and a sign-in reading, its
> account shows a login badge and no usage anywhere a quota is drawn, opening a session on it is refused with
> `account_not_signed_in` once it reads as signed out and accepted once it reads as signed in, and Log in is not
> offered; a gallery scenario shows each of these states; the three existing agents' accounts read, show and refuse
> exactly as before; the bindings and schema are regenerated; `pnpm test:rust`, `pnpm clippy:rust` and the UI tests
> pass.

The overview is `README.md` in this directory. This milestone follows `02-descriptor-facts-for-new-agent-shapes.md`.
It adds no quota reading and no login flow for any real agent.

## Technical design

### A provider Octoboard reads no plan quota for

- [ ] A descriptor whose `quota_windows.kinds` is empty declares that Octoboard reads no plan quota for the provider.
  The descriptor test that requires at least one kind goes; a new one holds that a provider hands out a quota probe
  (`AccountOps::quota_probe`) exactly when it declares a kind.
- [ ] The fact is already served (`ProviderInfo::quota_windows`); clients read it there. No capability is added.
- [ ] Nothing about usage is shown for such a provider's accounts. In the window: the accounts table's Usage cell and
  the account details dialog's Usage part are empty rather than waiting; the rail's Account usage panel has no tab for
  it, and its empty states count only providers that have a quota; the account pickers and the Switch account submenu
  draw no usage bar for its accounts; the selected session's quota indicator shows nothing for its sessions. In the
  desktop application, the menu bar icon has no quota submenu for it. For sessions, `list_agents` lists its accounts
  with no quota.
- [ ] The rules that rank or pass over accounts by quota (automatic account selection, the automatic switch at a
  usage limit, starting a usage window) treat such an account as having no quota to weigh: it is never used up, and
  it keeps its place in the saved order.

Why the existing field: "which windows the provider reports" already has a natural empty value, and the plan-quota
surfaces already read it. A provider that has a plan but no reading yet declares the same, and gains its kinds when its
reading is built.

### A sign-in reading of its own

- [ ] `AccountOps` gains a sign-in reading: given the account's directory (none for the default account) and the
  login-shell environment, whether the account is signed in, signed out, or cannot be told, with the email when the
  provider's sign-in record gives one. The default cannot tell. It may read the agent's files or run a short query of
  its CLI under the bounds every agent query has; it never uses a credential.
- [ ] For a provider with no quota probe, the account poll takes this reading in the quota reading's place, on the
  same occasions and for the same accounts, and publishes it in the reading every client already receives: no plan, no
  windows, the email, and the existing not-signed-in or missing-directory reason when the account is signed out. A
  reading that cannot be told carries no reason, like a failed quota reading that says nothing about the sign-in.
- [ ] Everything that already follows a reading's sign-in state does so unchanged: the Not logged in badge, the
  refusal to open, hand off or switch onto the account (`account_not_signed_in`), automatic selection passing it
  over, and the pickers listing it as unavailable.
- [ ] A provider with a quota probe keeps taking its sign-in state from that reading, as today; its sign-in reading is
  not asked.

Why reuse the reading: every consumer of the sign-in state reads it from the account's reading, and a second channel
would have to be merged with it in each of them.

### Log in only where it can be driven

- [ ] `Capabilities` gains `login_flow: bool`, served to clients: the provider can sign an account in without a
  terminal. A registry test holds that it agrees with what the provider hands out (`AccountOps::login`).
- [ ] The daemon refuses a login request for a provider without one through `CodedError::require_offered`, instead of
  starting a login that ends at once as failed.
- [ ] The window shows Log in, in an account's actions menu and after its Not logged in badge, only for a provider
  that declares it. For any other, the places that tell a signed-out account to log in say how instead, with the
  provider's own message.
- [ ] `ProviderMessageName` gains `signInHint`: the command of the agent's own CLI that signs an account of the
  provider in, set apart as a reference, in the provider's presentation module. A provider without one gets a generic
  sentence that names no agent and no command.
- [ ] Octoboard has no field for a key or any other credential: for an account that signs in with an API key, the
  hint and the command it names are all the window offers.
- [ ] Claude Code, Codex and Grok Build declare `login_flow: true`.

## Implementation plan

- [ ] Allow an empty list of quota kinds and add the probe-agreement test; change the test-only provider to declare
  none. Verified by the registry tests.
- [ ] Add the sign-in reading to `AccountOps` and to the poll for providers without a probe. Verified with the
  test-only provider answering signed in, signed out and cannot tell: the published reading, the refusal to open, and
  automatic selection passing the account over.
- [ ] Add `login_flow`, the agreement test and the refusal. Verified by a login request for the test-only provider
  being refused with `provider_unsupported`.
- [ ] Make every quota surface of the window read the provider's declaration. Verified per surface by a component
  test over the test-only provider and by gallery scenarios of the accounts table, the account details dialog, the
  Account usage panel with only such providers added, and an account picker.
- [ ] Make the menu bar icon's menu and the `list_agents` tool leave such a provider's quota out. Verified by their
  existing tests extended to the test-only provider.
- [ ] Replace Log in with the sign-in hint where the provider has no flow, and add the generic sentence. Verified by
  component tests of the accounts table and the not-logged-in states, and by the registry test that a declared
  `signInHint` exists in every language.
- [ ] Regenerate the client bindings and the protocol schema, and mirror the changes in the UI's served-info
  fixtures.

## Notes for the developer

**Reusable capabilities**

- The account poll and its judgement of a reading (signed out, missing directory, failed), which already decide the
  refusals and what the pickers show.
- Running an agent's CLI as one of its accounts for a short query, with its bounds and the admission that keeps
  configuration sharing from moving the directory under it.
- The UI's one place that turns a reading into a login state, and the one that names a provider's quota windows.
- `ProviderMessage` and the registry's lookup of a provider's named messages, with their generic fallbacks.

**Development notes**

- The reading's sign-in email is read whether or not a quota could be; the sign-in reading keeps that.
- Readings live in the daemon's memory only and are never exported; the sign-in reading is the same kind of state.
- The copy of the sign-in hint follows `docs/conventions/ui-standard.md` first and the `knowledge-i18n-copy` skill
  second; a command named in it is a reference, not quoted text.
- These are changes to what the product shows. Each of these states today's rule for "every account":
  `docs/product/account-quotas.md`, `docs/product/account-quota-readings.md`,
  `docs/product/account-quota-panel.md`, `docs/product/agent-accounts.md`, `docs/product/logging-accounts-in.md`,
  `docs/product/menu-bar-icon.md` and `docs/product/listing-agents-from-a-session.md`.
- The user decided that Octoboard takes no API keys (`README.md`, "An agent without a plan quota says so, and its
  sign-in is read apart from quota"). Do not add a key field, and do not pass a key to an agent's CLI.

**Reference docs**

- `docs/conventions/provider-neutral-core.md`
- `docs/conventions/daemon.md`, "Agent accounts"
- `docs/conventions/protocol-and-contracts.md`
- `docs/conventions/ui-code.md` and `docs/conventions/ui-components.md`
- `docs/conventions/tests.md`, "The UI state gallery and its fixtures"
- `docs/product/account-quotas.md`, "Quota states" and "An account that is not logged in"
- `docs/product/account-quota-readings.md`, "How a quota is read" and "When quotas are read"
- `docs/product/logging-accounts-in.md`
