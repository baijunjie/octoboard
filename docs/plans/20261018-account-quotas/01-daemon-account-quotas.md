# 01 Daemon account quotas

> Goal: the daemon knows every account's quota — default and owned, for every available agent — keeps it fresh, and
> pushes it to clients.
> Completion criteria: unit tests with fake agent scripts cover each probe's success, unrecognised reply, timeout and
> exit-without-reply; a connected client receives the quotas in its snapshot and an update event after a refresh;
> against the real CLIs on this machine each agent's default account reports real windows.

## Technical design

- [ ] A bounded subprocess runner that writes JSON lines to the child's stdin, keeps stdin open, reads stdout lines
      until the reply with the awaited id arrives or a timeout (about 15 s) passes, then kills the process group; same
      output caps as the existing bounded runner.
- [ ] One probe per agent as described in the overview, each run in the cached login-shell snapshot with the agent's
      config directory variable set to the account's directory (unset for the default account) and the home directory
      as working directory.
- [ ] Normalised wire types:
      - `AccountQuota { agent, account: Option<AccountId>, fetched_at, plan: Option<String>, windows: Vec<QuotaWindow>,
        unavailable: Option<{ code, params, message }> }` — `account` is `None` for the default account.
      - `QuotaWindow { kind: session | weekly | weekly_model | credit_period, label: Option<String>, used_percent,
        resets_at: Option<ms>, window_minutes: Option<u32> }` — `label` carries the model name for `weekly_model`.
- [ ] Claude's reply is read from its `limits[]` list generically, falling back to `five_hour`, `seven_day` and
      `model_scoped`; unlabeled internal buckets are ignored.
- [ ] An in-memory cache keyed by agent, account and config directory; invalidated when an account is updated or
      deleted.
- [ ] Protocol: quotas included in `snapshot`; a new `account_quotas_updated` event carrying the whole list; a new
      `refresh_account_quotas { agent?, account? }` request answered with `ack`, refreshing only entries older than the
      polling interval.

## Implementation plan

- [ ] A background poller started beside the existing usage collector: every 5 minutes it probes the accounts of
      available agents, with a small concurrency limit, backing off an account after failures.
- [ ] A refresh request triggers the same probe path for stale entries; each completed probe updates the cache and
      broadcasts the event.
- [ ] Quotas stay out of persistence, configuration export and `get_settings`.
- [ ] Record the three probes' measured commands and reply shapes in the agent CLI reference.

## Notes for the developer

- **Reusable capabilities**: the per-account model-list probe already runs an agent CLI per account in the login-shell
  snapshot with result caching — follow its shape; the shell snapshot already strips the Claude session marker
  variables that would otherwise change the CLI's behaviour; the existing bounded subprocess runner and its fake-agent
  test pattern.
- **Development notes**: the daemon has no HTTP client and needs none for this; update the protocol doc with the new
  request, event and snapshot field.
- **Reference docs**: `docs/agent-cli-reference.md`, `apps/daemon/PROTOCOL.md`, `docs/product/session-usage.md`.
