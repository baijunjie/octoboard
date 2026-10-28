> Severity: Minor

# The not-removed alert has a "project's folder" group the daemon never fills

## Symptom

The not-removed alert and its product doc describe a group for a worktree that is a project's folder, but the daemon
never reports such an item, so the group can never appear and the doc contradicts itself.

## Reproduction steps

1. Open `apps/daemon/src/branch_origin/cleanup.rs` and read `to_report`: it keeps an item only when its state is
   `Failed`, when it is named by a forced target, or when its state is `UncommittedChanges`, `Submodules`, `Unmerged` or
   `Locked` and the user did not keep it. An item in state `ProjectFolder` matches none of these.
2. Open `packages/ui/src/daemonMessage.ts`: `LEFT_BRANCH_GROUPS` has an entry with key `projectFolder`, lead
   `branchCleanup.group.projectFolder`, matching state `project_folder`.
3. Open `docs/product/branch-and-worktree-cleanup.md`, section "What is left is reported": its list of groups includes
   "the worktree is a project's folder", while the same section says what is not reported includes "a worktree known to
   be a project's folder or to hold one".

## Expected vs. actual

- Expected: the alert's groups and the doc's list of groups match what the daemon reports; per the same doc section
  ("What is not reported: … a worktree known to be a project's folder or to hold one"), no such group is listed.
- Actual: the UI defines a `projectFolder` group and the doc lists it, though no `project_folder` item ever reaches the
  alert.

## Environment

`main` as of commit `e76e2df84`; all platforms.

## Scope of impact

No user-visible malfunction: the group simply never shows. Readers of the doc and maintainers of the UI are misled
about what the alert can contain; the catalog keeps strings no screen uses.

## Leads

- Verified: `to_report` in `apps/daemon/src/branch_origin/cleanup.rs` filters out `ProjectFolder` items (step 1).
- Verified: `LEFT_BRANCH_GROUPS` in `packages/ui/src/daemonMessage.ts` defines the `projectFolder` group (step 2).
- Inferred: a gallery scenario or test may still render a `project_folder` item in the alert; whoever fixes this should
  check `packages/ui/src/gallery/fixtures/notices.ts` and the `daemonMessage` / `DaemonErrorMessage` tests.

## Acceptance criteria

- [ ] The UI's not-removed groups and the daemon's `to_report` agree: either no `projectFolder` group exists, or the
      daemon reports the items it is for.
- [ ] `docs/product/branch-and-worktree-cleanup.md` "What is left is reported" lists exactly the groups that can appear
      and does not contradict its own "What is not reported".
- [ ] No catalog key, gallery scenario or test remains for a group that cannot appear.
