# Local branch comparison

> Goal: compare any two local branch tips without checking either branch out.
> Completion criteria: selecting left and right local branches shows their scoped tip-to-tip changes and opens
> correct old/new content, including when neither branch is checked out. Diverged-branch fixtures demonstrate that
> the result is not a merge-base comparison. Moving a branch during review cannot mix commit versions, and the flow
> preserves the browser's source isolation and resource limits.

Depends on [worktree change inspection](04-worktree-changes.md) for the integrated Git review surface and on
[source contracts](01-source-contracts.md) for bounded, version-aware reads.

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
