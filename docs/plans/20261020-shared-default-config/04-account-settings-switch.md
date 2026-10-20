# 04 Account settings switch

> Goal: the user turns sharing on or off for an account in Settings' Agent accounts section.
> Completion criteria: every account except the default one shows the switch; a change takes effect through 03 and
> reports a failure in an error alert; the gallery covers off, on, a change waiting for the account's sessions to stop,
> and a failed change; the product doc describes the behavior.

## Technical design

- [ ] A Share default configuration switch on each account that is not the default account.
- [ ] Help text saying what is shared, what never is, and that conversations written while sharing stay in the default
      directory when sharing is turned off.
- [ ] A line saying that a change applies when the account's next session starts or resumes and none of its sessions is
      running.

## Implementation plan

- [ ] Add the switch to the account's settings, wired to 03's requests.
- [ ] Add gallery scenarios for the new states.

## Notes for the developer

- **Reusable capabilities**: the account settings already in the section (approval mode and the like) and their
  save-and-report-failure pattern; the shared switch component; error alerts.
- **Development notes**: user-facing strings go through the localization resources; follow the UI accessibility
  standard.
- **Reference docs**: `docs/product/agent-accounts.md`, `docs/product/error-alerts.md`, `docs/ui-accessibility.md`.
