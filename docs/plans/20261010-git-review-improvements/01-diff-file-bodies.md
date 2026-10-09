# Reading a diff's full file bodies

> Goal: the daemon can return, for a change it has sent a patch for, the full old and new file bodies that patch was
> made from.
> Completion criteria: daemon tests show the bodies returned for each kind of diff side (index, worktree, `HEAD`, a
> compared commit), a request whose version no longer matches the patch's rejected, and a body over the limit refused;
> the protocol document describes the request.

## Technical design

- [ ] A request naming a change the same way a patch request does (project, worktree or compared commits, path,
  section), plus the version information of the patch being expanded.
- [ ] A reply carrying the old and new bodies, each either present or absent (an added or deleted file has one side),
  or a typed refusal: version mismatch, over the limit, not readable.
- [ ] The same limits as the existing bounded reads apply to each body and to the reply as a whole.

## Implementation plan

- [ ] Resolve each side of the change to the same source the patch was computed from, then check the version
  information before reading; read each body through the existing bounded read.
- [ ] Document the request and its refusals in the protocol document.

## Notes for the developer

**Reusable capabilities**

- The source identities, version information and bounded reads established for the project file browser.
- The daemon's existing patch requests for the Uncommitted and Compare views, whose way of naming a change this
  request mirrors.

**Development notes**

- The Git mode never writes to the repository, takes no lock and never refreshes the index; reading the bodies keeps
  to that.

**Reference docs**

- `apps/daemon/README.md`, `apps/daemon/PROTOCOL.md`, `docs/product/project-pane-git-mode.md`
