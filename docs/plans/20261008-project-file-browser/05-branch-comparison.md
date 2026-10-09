# Local branch comparison

> Goal: compare any two local branch tips without checking either branch out.
> Completion criteria: selecting left and right local branches shows their scoped tip-to-tip changes and opens
> correct old/new content, including when neither branch is checked out. Diverged-branch fixtures demonstrate that
> the result is not a merge-base comparison. Moving a branch during review cannot mix commit versions, and the flow
> preserves the browser's source isolation and resource limits.

It extends the shipped Git mode of the project pane (`docs/product/project-pane-git-mode.md`; code in
`packages/ui/src/browser/`, see `packages/ui/README.md`); bounded, version-aware reads follow
`apps/daemon/PROTOCOL.md`'s "Browsing a project".

## Technical design

- [ ] The daemon enumerates local branches for the selected project's repository. Left and right selectors are
  independent; either endpoint may be any available local branch, with no requirement for a checked-out worktree.
- [ ] Starting or refreshing a comparison resolves both branch references to concrete commit identities. Its file
  list, patches and old/new bodies use that pair until another comparison is accepted.
- [ ] Left is the old side and right is the new side. Comparison uses the two tips directly; changing their order
  changes the direction. Selecting the same commit on both sides yields an explicit empty result.
- [ ] The comparison respects the project's relative scope and the boundary-crossing rename behavior defined by
  change inspection. Historical paths need not exist in the current working directory.
- [ ] Selecting another endpoint or project invalidates prior comparison requests. Moving/deleting a branch after
  resolution does not silently repoint an open comparison; refreshing resolves the new pair or reports an
  unavailable endpoint. The view identifies the commits whose content it is showing.
- [ ] Branch changes use the existing read-only diff adapter and unsupported/oversized-content presentation.
  Worktree selection affects uncommitted review only, not the meaning of a branch pair.

## Implementation plan

- [ ] Add bounded branch discovery and commit-pair comparison through the controlled Git read contract.
- [ ] Connect branch selectors and comparison results to the project Git mode, keeping branch-comparison state
  distinct from selected-worktree uncommitted state.
- [ ] Verify arbitrary local endpoints, neither endpoint checked out, reversed endpoints, equal commits and diverged
  branches whose tip-to-tip and merge-base results differ. No verification path may change the checkout or index.
- [ ] Verify committed additions, deletions, renames, supported image changes and unsupported binary changes;
  reuse the previously verified diff behavior while confirming both sides come from the selected commits. Include
  modified renames across the project boundary, confirming the restricted preview does not expose the other body.
- [ ] Verify branch movement/deletion, endpoint switching with delayed replies, reconnect and unavailable objects.
  All parts of an accepted comparison must retain the same resolved commit pair.
- [ ] Verify large lists and patches against the established budgets, complete browser and packaged-WebView checks,
  and update the shipped product, protocol and module documentation.

## Handoff

- Serve and verify the comparison reply recorded as a contract in `apps/daemon/PROTOCOL.md`'s "Changes and
  comparisons" (`left` / `right` each `{branch, commit}`, and the versions used for its patch and bodies), together
  with the Git version-race cases.
- The worktree change requests (`list_project_changes`, `read_project_change`) and their types (`ChangeEntry`,
  `ChangeSide`, `SideKind`, `SideRef`, `SideRead`) are the shapes to extend; `ChangeGroup` is a closed enum of the three
  worktree groups, so a comparison adds its own group (e.g. `committed`) and request. In the daemon,
  `apps/daemon/src/browse/changes.rs`'s `Reader` pieces (`tree_entry` per side, `patch()` with two commit ids,
  `carries_bodies`, the out-of-scope rule, exact pathspecs through `GitEnv::run_exact` with nested-path handling) can
  be reused; both sides of a commit pair are immutable, so no re-check is needed. Size the new reservation against
  the existing ones (`budget.rs`; a change list's and a diff's worst cases already exceed one connection's share).
- In the UI, the Git mode's state (`browserState.ts`), the window-wide Git request bound (`gitRequests.ts`, 2) and the
  listings' window bound (11) were sized so that 11 + 1 viewer read + 2 Git requests + 1 replacement stays under the
  daemon's 16 per connection; a branch comparison's requests must fit that sum. The viewer's change presentations
  (`changePresentation`) already handle every side shape.
- The worktree selector cuts a long name with an ellipsis, where the app fades long names out (`FadeOverflow`, see
  `docs/product/labels-and-tooltips.md`); align it together with the branch selectors this milestone adds.

## Notes for the developer

**Reusable capabilities**

- The daemon source foundation supplies repository identities, isolated Git invocation and bounded byte-preserving
  reads. The project Git surface supplies change presentation, the modal and error states.

**Development notes**

- Keep endpoint parsing on the daemon side: a client branch label is not an arbitrary Git revision expression.
- Branch comparison must not reuse live Files content or the currently selected worktree's index for either side.
- Fetching remotes, creating worktrees and merge-base review are not needed for local tip-to-tip comparison.

**Reference docs**

- `apps/daemon/README.md`, `apps/daemon/PROTOCOL.md`, `packages/ui/README.md`
- `docs/product/project-git-status.md`
- [Git diff semantics](https://git-scm.com/docs/git-diff)
