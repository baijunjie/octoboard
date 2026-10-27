> Severity: Moderate

# The Permissions section's Grant buttons fail silently

**Symptom**: when pressing a permission row's **Grant…** button fails to raise the system's request, nothing is shown.

**Reproduction steps**:

1. In a development build of the macOS app, make the request call reject: remove `macos_request_permission` from
   `invoke_handler` in `apps/desktop/src-tauri/src/lib.rs`.
2. Run the app, open Settings → Permissions, and press **Grant…** on the Screen Recording or Accessibility row.

**Expected vs. actual**:

- Expected: the error alert dialog opens — `docs/ui-accessibility.md`, "Where an error is shown", rule 1 ("An error the
  user's own action directly causes — … a switch, a menu or row action … — opens the dedicated error alert dialog").
- Actual: the button stops being pending, the row's status is read again, and nothing tells the user the request
  failed.

**Environment**: macOS application (macOS 11 or later), Settings → Permissions, a row that shows **Grant…**.

**Scope of impact**: users whose permission request fails get no feedback; they can still grant the permission in System
Settings by hand. The failure is rare: only the IPC call itself can fail, since the shell command always answers.

**Leads**:

- Verified: `request` in `packages/ui/src/settings/useMacosPermissions.ts` catches any rejection of
  `macosPermissions.request(kind)` and ignores it.
- Verified: `macos_request_permission` in `apps/desktop/src-tauri/src/permissions.rs` returns a `bool` and has no failure
  of its own.
- Verified: the same section's **Open System Settings** button reports a failed open through the error alert dialog,
  so the two buttons in one row behave differently on failure.

**Acceptance criteria**:

- [ ] When the request call for either permission rejects, pressing **Grant…** opens the error alert dialog.
- [ ] After a failed request the button is no longer pending and the row reads its status again.
- [ ] A request that succeeds or is refused shows no error alert, and a refusal still offers **Open System Settings**.
