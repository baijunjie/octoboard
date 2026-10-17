# 02 Account management with quotas

> Goal: Settings' agent accounts section shows, for each account (default and owned), its plan and the quota windows
> described in the overview, alongside the existing name, directory and approval controls.
> Completion criteria: component tests cover loaded, loading, unavailable and no-data states; gallery scenarios show
> them; opening the section triggers a refresh of stale entries; the UI preview shows real quotas from the daemon.

## Technical design

- [ ] Protocol types and a store slice for account quotas, fed by the snapshot and the update event.
- [ ] A quota meter component: window label ("5-hour" / "Session" / "Weekly" / "<Model> weekly" / the Grok period),
      "N% remaining", "resets in <countdown>" updating live, and a meter bar showing what remains.
- [ ] An account quota summary component rendering one account's plan and meters, with loading and unavailable states;
      shared with milestone 3.

## Implementation plan

- [ ] Rework each account row in the accounts section so the quota summary sits with the account, for the default
      account and owned accounts alike.
- [ ] Opening the section sends the refresh request.
- [ ] Strings in every i18n catalog; gallery scenarios through the gallery skill.

## Notes for the developer

- **Reusable capabilities**: HeroUI's `Meter` (semantically "remaining"); the existing live-clock hook and relative
  time formatting for countdowns; the agent icon component.
- **Development notes**: the approval selector and the add / edit account dialog keep working as they are.
- **Reference docs**: `docs/product/settings.md` ("Agent accounts"), `docs/ui-accessibility.md`,
  `docs/product/labels-and-tooltips.md`.
