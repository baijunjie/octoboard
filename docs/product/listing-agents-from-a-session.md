# Listing agents from a session

Every session, of any kind, has the `list_agents` tool, so an agent can see which agents a session could be opened on,
what the user noted each one is for and what each of their accounts has left, before it names an agent or an account —
for `open_session` or `request_session`, or `switch_session_account` or `handoff_session`. It takes an optional `agent`
and an optional `refresh` (below), and answers:

```text
{
  "auto_select_account": true,
  "agents": [
    { "agent": "claude", "tags": ["ui"], "accounts": [...] },
    { "agent": "codex", "tags": ["review"], "accounts": [...] }
  ]
}
```

- **`auto_select_account`** is the setting of that name (see "Automatically select an account" in
  `docs/product/opening-sessions.md`), read-only, so that a session, which may not be able to read the settings, knows whether
  an account it leaves out is chosen for it.
- **`agents`** has one entry per agent a session could be opened on: the added agents that are installed, in the order
  the user keeps the added agents in, or every installed agent while that list is undecided (see "Added agents" in
  `docs/product/agent-accounts.md`). A removed agent is not listed, so neither are its tags, and nor is an agent found
  not installed on this machine; one not detected yet is listed, its accounts as not read yet. With `agent` (`claude`,
  `codex` or `grok`) only that agent's entry is given; naming one that is not listed answers `agents: []`.
- **`tags`** are the agent's tags in the user's order, `[]` when it has none (see "Agent tags" in
  `docs/product/per-agent-settings.md`). The tool's description presents them as the user's notes on what the agent is
  for — information to weigh, not a rule; what the sessions are told to do with them is in "The agent and account of a
  session a session starts" in `docs/product/hub-orchestration.md`.
- **`accounts`** lists every account of that agent that the user has not turned off, whether or not a reading is held
  for it (the default account's `account` is `default`), in the order automatic account selection takes them in: the
  user's order of that agent's accounts, then those that order does not name (see "Which account a session uses" in
  `docs/product/opening-sessions.md`). An account the user turned off is not listed, with no exception for the caller's
  own account (see "Turning an account off" in `docs/product/agent-accounts.md`); a session finds the account it runs on
  with `get_session`, which left without a `session` reads the caller itself (see "Session records" in
  `docs/product/what-every-session-reads.md`).

Each account entry gives its `agent`, `account`, `account_name`, `plan`, `fetched_at` (milliseconds since the epoch,
`null` for an account not read yet), `exhausted` (whether the account is used up, by the rule in "A used-up account" in
`docs/product/account-quotas.md`) and `exhausted_until` (when the window that keeps it used up resets, in milliseconds
since the epoch; `null` when it is not used up or the agent gives no reset time), `unavailable` (the reason a reading is
missing or could not be refreshed, else `null`), `can_open` and `cannot_open_reason` (below), and its `windows`, each
with `kind` (`session` for the five-hour window, whichever agent's it is; `weekly` for the account-wide weekly window;
`weekly_model` for a model's own weekly window, the model named by `label`; `credit_period` for a credit pool), `label`,
`used_percent`, `remaining_percent` (100 less the used share, never below 0), `resets_at` and `window_minutes`. Values
are percentages only; no absolute token or money amount is given, and no account's sign-in email.

- **`can_open`** says whether a session could be opened on the account now. When it is `false`, `cannot_open_reason`
  says why in prose: the account is used up (see "A used-up account" in `docs/product/account-quotas.md`), it is not
  logged in (see "An account that is not logged in" in `docs/product/account-quotas.md`), or a change of its sharing
  of the default configuration is running — the reasons, besides being turned off, that automatic account selection
  passes an account over for. It is `null` when `can_open` is `true`.

By default it answers from the readings already held and takes no new one, so an entry is as old as its `fetched_at`
and a call spends nothing. With `refresh` set to `true` it first reads again the accounts it lists — every agent's, or
only `agent`'s when that is given — together with those of an agent that is installed but not added, which Octoboard
reads too, never an account the user turned off, and answers once those readings have finished, which can take several
seconds:

- An account is read only when its last attempt, whatever took it — the background poller, a window's request, a kick's
  follow-up reading or an earlier `refresh` — is at least **60 seconds** old. That limit holds whichever session asks
  and however often, and it is the only rule: a failing account's longer retry interval does not hold it back, and an
  account due for a reading for any other reason is not read sooner by the request. Within the limit the held reading
  is returned as it is, with no error.
- An account whose reading is already running is not read again; the call answers with what is held for it.
- The reading is an ordinary one (see "How a quota is read" in `docs/product/account-quota-readings.md`): it reaches
  every window like any other, and counts towards the four readings, kicks and redeems that may run at once.
- It never kicks (see "When an account is kicked" in `docs/product/starting-usage-windows.md`).
- A `refresh` that is not `true` or `false` is refused.

Each reading runs the agent's CLI, which is why a session's request is limited per account rather than left to the
caller: the tool's description tells the agent to ask only when a reading is too old to decide on.

**A session with a parent is told to report a low quota.** The role description of a worker, and of a coordinator that
reports to a console session, tells it that its account's quota can run out while it works: to find the account it runs
on with `get_session` (no `session`), look it up in `list_agents` — where an account the user turned off, possibly its
own, is not listed — and, when that shows it running low, say so in a `report` with the status `needs_decision`, naming
the account and what is left, rather than carry on until it stops, its parent deciding whether to switch it to another
account or hand it off. It is also told that, where the user has turned automatic switching on, Octoboard switches it by
itself when a turn stops on its account's usage limit, tells it so, and that it then carries on there (see
`docs/product/automatic-account-switch.md`).
