# 04 Every Alert has a title and holds its own details

> Goal: no Alert renders without a title, and no Alert's own details sit outside it, now or later.
> Completion criteria: the standard states both rules; the Alert wrappers require a title in their types and nothing
> renders HeroUI's `Alert` outside them; every title-less caller has a title or has moved to the form that fits it;
> every Alert whose own details (such as the list of affected items) sit after it holds them inside; the indicator
> and the first text line are aligned in every Alert scenario of the gallery; checks pass, and the user has seen the
> changed screens in the preview.

## Technical design

- [ ] `ErrorAlert` and `FailureAlert` take `title` as required.
- [ ] `TintedAlert` takes the title (required), the description and an optional details slot itself instead of callers
  composing HeroUI's parts.
- [ ] A lint restriction keeps HeroUI's `Alert` from being used outside the wrappers.
- [ ] Details inside the stock structure: a list of affected items (or another detail block) is a child of
  `Alert.Content` after `Alert.Title` / `Alert.Description` — never inside `Alert.Description`, which is a single text
  run — drawn in the description's own text style, so the Alert keeps HeroUI's stock look. Only the remedy (the
  recovery sentence and its actions) goes after the Alert; the error-only screens' diagnostic block (startup failure,
  crash screen) keeps standing beside its Alert, as the standard already allows.
- [ ] Title-less callers to resolve: `PermissionsSection` (permissions unreadable), `StartupScreen` (daemon failed, no
  address), `ResumeConversationFields` (lookup problem), `ErrorBoundary` (crash), `ProjectDialog` (detection error),
  `LaunchSettingsFields` (model list), `HandoffDialog` (moving branches failed), `TrustPromptDialog` and
  `SessionRequestDialog` (warnings), and the confirmation warning's branch with no title (below). For each, add a title
  in the application's own words, or, where an Alert is the wrong form, change to the form the standard gives that
  case.
- [ ] Details moved inside: the shared confirmation warning (`ConfirmWarning`) used by Delete session, Archive
  session, Remove project and Delete all archived — the list of sessions deleted, archived or ended along with the
  subject goes inside its Alert. The account-switch failure alert's per-session list is checked the same way.
- [ ] The consequence lines of ticked options ("Also permanently deletes …"): proposal — they move inside the same
  warning Alert after the list, as its description lines, because they are what the warning warns about; with no
  bound sessions but a ticked option, the Alert's title names the consequence ("This also permanently deletes:"-style
  title, wording settled at implementation) and the lines follow. The checkboxes above do not move when a line is
  added, as now, because the Alert sits below them. (Alternative the user may prefer: keep the lines outside the
  Alert, separated from it by a divider or heading.)

## Implementation plan

- [ ] Write the rules into `docs/ui-accessibility.md` ("Errors shown among other content" and the Alert paragraph under
  "Confirmed visual choices"): an Alert has a title (the stock indicator aligns with the title's line); an Alert holds
  its title, its description and its own details, and only the remedy sits after it; where details go in the stock
  structure. Withdraw "a short error … needs no generic title".
- [ ] Bring the product docs that prescribe the list outside the Alert into line (`docs/product/branch-and-worktree-
  cleanup.md`, `docs/product/deleting-sessions.md`, and any other found) through the docs workflow.
- [ ] Change the wrapper types and add the lint restriction; fix every caller the type check reports.
- [ ] Restructure the confirmation warning and its four dialogs; check the account-switch failure alert.
- [ ] Write the new titles in English and Simplified Chinese.
- [ ] Have the gallery scenarios showing these Alerts updated (gallery-writer), including the delete-session
  scenarios with bound sessions, all options ticked, and no records.

## Notes for the developer

- **Reusable capabilities**: the existing Alert wrappers (`ErrorAlert` already puts its remedy in a second
  description and its raw text behind Details); i18n catalogs; the gallery's archive, startup, settings, dialog,
  hand-off and notice scenarios.
- **Development notes**: titles follow the copy conventions (`knowledge-i18n-copy` skill); the title must not just
  repeat the description — move the "what happened" into the title and keep the description for why and the remedy.
  The confirmation's options stay plain blocks outside any warning (`docs/product/branch-and-worktree-cleanup.md`).
- **Reference docs**: `docs/ui-accessibility.md` ("Where an error is shown", "Errors shown among other content");
  `docs/product/labels-and-tooltips.md`; `docs/product/branch-and-worktree-cleanup.md`;
  `docs/product/deleting-sessions.md`.
