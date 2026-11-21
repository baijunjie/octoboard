> Severity: Minor

# Flat places draw a groove where a hairline belongs

- **Symptom**: rows and blocks on flat surfaces (Settings rows, the accounts table, confirmation sections, dialog row
  lists) are divided by a groove, the carved two-line divider meant only for the window's dimensional parts, instead of
  the flat card's plain hairline.
- **Reproduction steps**:
  1. Run `grep -n groove-` over these files under `packages/ui/src/`: `settings/SettingRow.tsx`,
     `settings/SettingsLead.tsx`, `settings/accounts/AccountsTable.tsx`, `dialogs/confirmations/ConfirmSection.tsx`,
     `about/ThirdPartyLicensesDialog.tsx`, `settings/accounts/AddAgentDialog.tsx`.
  2. Each prints a `groove-t` or `groove-b` class on a row, a table or a section divider.
  3. Open Settings (General section, Agent accounts table), an archiving or project-removal confirmation, the
     third-party licenses dialog and Add agent, and look at the lines between rows or sections: each is a groove.
- **Expected vs. actual**:
  - Expected: a flat card, a table or a list of rows in a dialog or on a Settings page divides its rows and blocks with
    one plain hairline (`hairline-b`), built with `FlatCard` where it is a card (`docs/conventions/ui-standard.md`,
    "Cards, grooves and hairlines").
  - Actual: these places draw `groove-*` dividers.
- **Environment**: any platform and appearance; the current main branch.
- **Scope of impact**: visual only; the six places named above. No workaround is needed, nothing is broken.
- **Leads**:
  - Verified: the `groove-*` classes at `SettingRow.tsx:58`, `SettingsLead.tsx:15`, `AccountsTable.tsx:31` and `:90`,
    `ConfirmSection.tsx:19`, `ThirdPartyLicensesDialog.tsx:24`, `AddAgentDialog.tsx:63` (line numbers as of filing).
  - Inferred: the Settings rows and the accounts table may need `FlatCard` or `hairline-b` on a group that is not a
    card, and the confirmation sections' divider between topic sections may need a decision on whether a section break
    counts as a divider inside a flat card.
- **Acceptance criteria**:
  - [ ] None of the six files draws a `groove-*` class on a flat surface.
  - [ ] Their rows and blocks are divided by `hairline-b` or by `FlatCard`'s own dividers, in both appearances.
  - [ ] The affected gallery scenarios show hairlines, with no other visual change.
