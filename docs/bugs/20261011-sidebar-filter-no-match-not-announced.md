> Severity: Moderate

# The sidebar project filter does not announce that no project matches

## Symptom

When typing into the sidebar's project filter empties the list, a screen-reader user hears nothing: the "No matching projects" message appears silently.

## Reproduction steps

1. Open the application with at least one project in the sidebar, with a screen reader running (VoiceOver is the one the project's accessibility notes target).
2. Open the sidebar's project filter (the filter button beside the "Projects" heading) and focus its keyword field.
3. Type text that matches no project.

The list is replaced by the "No matching projects" empty state, and the screen reader says nothing about it. Not yet listened to with a screen reader; the absence of any status region was confirmed in the code (see Leads).

## Expected vs. actual

- **Expected**: the change from a list to "No matching projects" is announced as a status message, without moving focus. `docs/memory/writing-ui-components.md` sets WCAG 4.1.3 (Status Messages, AA) as the bar, and records the idiom for it: a `role="status"` region kept mounted so that what appears in it is announced.
- **Actual**: the empty state is rendered inside a plain element and nothing is announced.

## Environment

- Any platform and build; the sidebar's project list with one or more projects (the empty state shows only when `projects.length > 0` and the filtered list is empty).
- Screen reader behaviour was not observed.

## Scope of impact

Screen-reader users filtering the sidebar's projects: they cannot tell an empty result from a list that has not updated yet. The workaround is to move into the list and find it empty, or to clear the field.

## Leads

- **Verified in the code**: `packages/ui/src/sidebar/Sidebar.tsx` renders `<EmptyPanel compact icon={SearchX} message={t("sidebar.filter.noMatch")} />` when `projects.length > 0 && ordered.length === 0`.
- **Verified in the code**: neither `packages/ui/src/components/EmptyPanel.tsx`, `packages/ui/src/sidebar/Sidebar.tsx` nor `packages/ui/src/sidebar/ProjectFilter.tsx` has a `role="status"` element, an `aria-live`, or a `StatusAnnouncer`.
- **Verified in the code**: the project's idiom is already in place elsewhere: `packages/ui/src/browser/BranchComparison.tsx` keeps a `role="status"` div mounted for as long as the comparison is, and `packages/ui/src/components/StatusAnnouncer.tsx` is the shared piece for a state whose visible form comes and goes.
- **Related, being fixed separately**: the Git mode's new change-list filter (milestone 05 of `docs/plans/20261010-git-review-improvements/`, branch `feat/change-filter`) had the identical gap, found in a change review, and is being given a mounted `role="status"` region for it. That is a worked example in the codebase to follow. The sidebar's filter is outside that topic, so it has this ticket of its own.
- **Inferred**: a region inserted together with its text is not reliably announced (VoiceOver in WebKit), per `docs/memory/writing-ui-components.md`, so a region that appears with the empty state would not fix this on its own.

## Acceptance criteria

- [ ] With a screen reader running, typing into the sidebar's project filter until nothing matches announces "No matching projects" without moving focus.
- [ ] The announcement is made once per change to the empty result, not on every keystroke that leaves the list empty, and not again when the sidebar is merely re-rendered.
- [ ] Clearing the filter or matching a project again leaves the region quiet, or announces nothing stale.
- [ ] The empty state is not announced twice (its visible form and the region do not both speak).
- [ ] A test covers that the message reaches a status region.
