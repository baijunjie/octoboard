# 04 Every Alert has a title

> Goal: no Alert renders without a title, now or later.
> Completion criteria: the standard states the rule; the Alert wrappers require a title in their types and nothing
> renders HeroUI's `Alert` outside them; every title-less caller has a title or has moved to the form that fits it;
> the indicator and the first text line are aligned in every Alert scenario of the gallery; checks pass, and the user
> has seen the changed screens in the preview.

## Technical design

- [ ] `ErrorAlert` and `FailureAlert` take `title` as required.
- [ ] `TintedAlert` takes the title (required) and description itself instead of callers composing HeroUI's parts.
- [ ] A lint restriction keeps HeroUI's `Alert` from being used outside the wrappers.
- [ ] Title-less callers to resolve: `PermissionsSection` (permissions unreadable), `StartupScreen` (daemon failed, no
  address), `ResumeConversationFields` (lookup problem), `ErrorBoundary` (crash), `ProjectDialog` (detection error),
  `LaunchSettingsFields` (model list), `HandoffDialog` (moving branches failed), `TrustPromptDialog` and
  `SessionRequestDialog` (warnings), `RequestedDialog` (consequences branch with no title). For each, add a title in the
  application's own words, or, where an Alert is the wrong form, change to the form the standard gives that case.

## Implementation plan

- [ ] Write the rule into `docs/ui-accessibility.md` ("Errors shown among other content" and the Alert paragraph under
  "Confirmed visual choices"): an Alert has a title; withdraw "a short error … needs no generic title"; give the reason
  (the stock indicator aligns with the title's line).
- [ ] Change the wrapper types and add the lint restriction; fix every caller the type check reports.
- [ ] Write the new titles in English and Simplified Chinese.
- [ ] Have the gallery scenarios showing these Alerts updated (gallery-writer).

## Notes for the developer

- **Reusable capabilities**: the existing Alert wrappers; i18n catalogs; the gallery's startup, settings, dialog,
  hand-off and notice scenarios.
- **Development notes**: titles follow the copy conventions (`knowledge-i18n-copy` skill); the title must not just
  repeat the description — move the "what happened" into the title and keep the description for why and the remedy.
- **Reference docs**: `docs/ui-accessibility.md` ("Where an error is shown", "Errors shown among other content");
  `docs/product/labels-and-tooltips.md`.
