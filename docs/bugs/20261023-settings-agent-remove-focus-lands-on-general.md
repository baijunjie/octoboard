> Severity: Moderate

# Removing an agent in Settings puts focus on the General section tab

- **Symptom**: After an agent is removed from Settings → Agent accounts with the keyboard, focus lands on the
  "General" section tab while "Agent accounts" stays the selected section.
- **Reproduction steps**:
  1. Open the UI state gallery scenario `settings-accounts-agent-remove` (the Remove agent confirmation is open over
     Settings, "Agent accounts" selected).
  2. Focus the confirmation's "Remove" button and press Enter.
  3. Wait for the confirmation to close (about 800 ms) and look at `document.activeElement`.
- **Expected vs. actual**:
  - Expected: focus stays inside the section the user is working in ("Agent accounts" section tab or the agent tab
    strip), visibly ringed — `docs/ui-accessibility.md`, "Dynamic collections": keyboard focus stays on surviving
    controls and the surface keeps its focus hand-off when the focused item is removed; and "Focus moved by a change
    the user did not make": the user has to be able to see where focus went.
  - Actual: focus is on the "General" section tab (`aria-selected="false"`); the selected section tab is still
    "Agent accounts".
- **Environment**: local `main` at 4dd7aaccb, UI state gallery in headless Chromium (Playwright chromium headless
  shell), viewport 1400×900, light theme, `lang=en`. Same result with reduced motion emulated.
- **Scope of impact**: Keyboard and screen-reader users removing an agent are moved out of the section they were in;
  a further Tab/arrow sequence gets them back. Pointer users are unaffected.
- **Leads**:
  - Inferred: the section's lost-focus hand-off (`useSectionRefocus` in `packages/ui/src/settings/`, called from
    `AgentAccountsSection.tsx` with `removedCount` among its dependencies) or the `SettingsFocusContext` getter it uses
    resolves the first section tab instead of the selected one.
- **Acceptance criteria**:
  - [ ] After step 2, focus is on the "Agent accounts" section tab or on a tab of the agent strip, not on "General".
  - [ ] The focused element shows the focus ring.
  - [ ] Same with reduced motion on, and in RTL (`lang=ar`).
