# 04 Account settings switch

> Goal: the user turns sharing on or off for an account in Settings' Agent accounts section.
> Completion criteria: every account except the default one shows the switch; a change takes effect through 03 and
> reports a failure in an error alert; the gallery covers off, on, a change waiting for the account's sessions to stop,
> and a failed change; the product doc describes the behavior.

## Technical design

- [ ] A Share default configuration switch on each account that is not the default account.
- [ ] Help text saying what is shared, what never is, and that when sharing is turned off the account's Octoboard
      sessions take their conversations back while every other conversation stays in the default directory.
- [ ] A line saying that a change applies at once when none of the account's sessions is running, and otherwise when
      its next session starts or resumes and none of its sessions is running.

## Implementation plan

- [ ] Add the switch to the account's settings, wired to 03's requests.
- [ ] Add gallery scenarios for the new states.

## Handoff

- The UI has no messages yet for the daemon's sharing codes: the errors `default_account_not_shareable`,
  `account_sharing_failed` (its `items` say what each item was left as, `sessions` which sessions a conversation
  belongs to) and `account_still_sharing` (deleting or repointing a sharing account while something runs under it,
  a session or the agent's CLI reading its quota or models or signing it in), and the session notices
  `account_sharing_repair_failed` and `unshared_state_database`. Each needs a `daemon.<code>` message in every
  locale, and deleting or repointing an account reports the two refusals like any other failure there.
- A repointed account no longer shares; the account's settings show the switch off afterwards.

## Notes for the developer

- **Reusable capabilities**: the account settings already in the section (approval mode and the like) and their
  save-and-report-failure pattern; the shared switch component; error alerts.
- **Development notes**: user-facing strings go through the localization resources; follow the UI accessibility
  standard.
- **Reference docs**: `docs/product/agent-accounts.md`, `docs/product/error-alerts.md`, `docs/ui-accessibility.md`.
