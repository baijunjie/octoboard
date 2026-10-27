> Severity: Moderate

# The Permissions section's Open System Settings buttons fail silently

**Symptom**: when the Screen Recording or Accessibility row's **Open System Settings** button fails to open System
Settings, nothing is shown.

**Reproduction steps**:

1. In a development build of the macOS app, make the pane fail to open: in
   `apps/desktop/src-tauri/src/permissions.rs`, `macos_open_privacy_settings`, change the Screen Recording URL to an
   address no application handles (for example `x-octoboard-none:`), so `NSWorkspace.openURL` returns false.
2. Run the app, open Settings → Permissions, and press **Open System Settings** on the Screen Recording row.
3. Separately, make the IPC call reject (for example remove the command from `invoke_handler` in
   `apps/desktop/src-tauri/src/lib.rs`), and press the button again.

**Expected vs. actual**:

- Expected: each failure opens the error alert dialog — `docs/ui-accessibility.md`, "Where an error is shown", rule 1
  ("An error the user's own action directly causes — … opening — opens the dedicated error alert dialog").
- Actual: nothing happens and nothing is shown.

**Environment**: macOS application (macOS 11 or later), Settings → Permissions, either permission row in a state that
shows the button.

**Scope of impact**: users whose System Settings pane fails to open get no feedback; they can still open System Settings
by hand. The failure is rare.

**Leads**:

- Verified: `macos_open_privacy_settings` in `apps/desktop/src-tauri/src/permissions.rs` ignores the value
  `NSWorkspace.openURL` returns and returns nothing, so a pane that failed to open cannot be told apart from one that
  opened.
- Verified: `openSystemSettings` in `packages/ui/src/settings/useMacosPermissions.ts` catches any rejection and ignores
  it.
- Verified: the Notifications section's own **Open System Settings** button (`NotificationsSection.tsx`,
  `notification_settings.rs`) reports a failed open through the error alert dialog; the two buttons now behave
  differently.

**Acceptance criteria**:

- [ ] When `NSWorkspace.openURL` reports failure for either permission's pane, pressing the button opens the error
  alert dialog.
- [ ] When the IPC call for the button rejects, the error alert dialog opens.
- [ ] A press that opens the pane shows no error, and the row still refreshes its status afterwards.
