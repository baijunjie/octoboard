# 08 Keep horizontal tab strips at content width

> Goal: keep horizontal tab strips at content width.
> Completion criteria: New/Resume labels and shared agent strips use content width at reading start without unused pill background or equal-width stretching, preserving scrolling and focus clearance. All listed validation passes, and the independently reviewed result is accepted.

Status: **accepted; implementation not started**. Implementation begins only after the audit passes and the preceding required work is closed.

## Scope

G01 in [the gallery audit](audit-gallery.md); source inventory in [the state audit](audit-states.md).

## Implementation contract

- [ ] Constrain both horizontal list paint and tab allocation; cover session conversation, Console Advanced and shared Settings/quota agent strips.
- [ ] Retain the approved fully opaque foreground hover labels and supported HeroUI appearance.
- [ ] Preserve inner ring padding, scroll margin, fade suppression, keyboard selection, panel naming and active tab state.
- [ ] Keep content-sized Files/Changes and narrow Settings navigation working; do not infer that wide vertical navigation must be redesigned.

## Validation requirements

- [ ] Repeat the measured G01 scenarios in light/dark/Chinese/Arabic at wide and narrow widths. Record list width versus summed content/padding; arrow through overflowing tabs and verify the full ring, start alignment and scroll reveal. Run tab/dialog/settings/quota tests, full shared tab gallery and static checks.
- [ ] Complete the project gallery/static/change-check/documentation gates; preserve failed or unresolved checks explicitly. Do not skip scenarios or weaken assertions.

## Dependencies and handoff

Coordinate AgentTabList ownership with collection motion milestone 10; these are separate review targets and must not edit the same shared contract concurrently.

Resume-by-ID is integrated on main at `368a1f1f3`; no separate resume branch is pending. Measure the current
SessionDialog New/Resume strip with the ID-only pending state, the record/record-account fields and Advanced,
and archived/interrupted/live session targets. Preserve selected-panel mounting, remembered New-tab choices,
lookup cancellation when leaving Resume, and the existing focus handoff as the dependent fields change. Use the
current `dialog-new-session-resume-*` fixtures rather than the audit's earlier Resume layout. The original
`3a65c903` audit does not certify these added routes.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

## Notes for the developer

Reuse the existing shared components and mechanisms named in the linked finding record. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery”, `docs/voiceover-cases.md`, and project gallery/change-check workflows. Only gallery fixtures may stage data. Read the exact baseline evidence and recheck caller inventory after rebasing; other owners’ pending changes are not certified by this audit.
