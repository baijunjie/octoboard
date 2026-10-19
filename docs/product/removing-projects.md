# Removing a project

A locked session of the project, archived sessions included, refuses removal before any process is ended or
folder is moved, under "Locking a session" in `docs/product/sessions.md`.

Removing a project removes the association, and with it the records of that project's sessions, archived ones
included. It also clears what Octoboard recorded about which session is working on which branch in the project's
repository, unless another project still points into that repository (see "What the record is" in
`docs/product/branch-responsible-session.md`). **The directory and everything in it are left as they are**, and so are
the repository's branches and worktrees, unless the user chooses to move the folder to the Trash as well (see "Moving
the project folder to the Trash" below).

Removing a project asks for confirmation. **Sessions of the project that are still running do not block it**:

- With none running, the confirmation offers **Remove**.
- With some running, the confirmation lists them — each with its status glyph, its agent's icon and its title, in the
  sidebar's order (see "Order of projects and sessions" in `docs/product/sidebar.md`) — in its one warning, below its
  options and above its buttons, under a line saying that that many sessions are still running and will be ended, and
  its button reads **End and remove**. The list follows
  sessions starting or stopping while the confirmation is open. Confirming ends each running session's process the way
  archiving does (see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`), then removes the project
  with all its sessions and the last output Octoboard kept for each. Sessions bound to another session of the project
  are ended before the session they are bound to, the deepest level first, so ending that one is never refused over
  them. A lead session's console session belongs to no project and is left as it is; the lead session's binding to it
  goes with the lead session's record (see "Lead sessions" in `docs/product/hub-orchestration.md`).

The confirmation also carries an option to **also move the project folder to the Trash**, unticked each time the
confirmation opens, with a hint that the whole folder goes, including anything not committed to Git, and that it can
be recovered from the Trash. Ticking it makes the message say the folder is moved to the Trash; for the confirm
button's label, see "What a confirmation's confirm button names" in `docs/product/window-layout.md`. Ticking it also
lists the worktrees outside the folder, and the branches, that go with it, and offers locked worktrees under a choice
of their own (see "With a project's folder moved to the Trash" in `docs/product/branch-and-worktree-cleanup.md`).
While a removal runs, the button shows as pending and the option cannot be changed.

**The project's shell terminals end with it** (see "Closing a shell terminal" in `docs/product/shell-terminals.md`),
whether their shells are busy or not; they never hold a removal up and the confirmation does not list them. They are
ended once the removal is past its refusals below.

Removal is refused while a session of the project is being launched or resumed at that moment, since that session has
no process yet that could be ended, and, with the Trash option ticked, for the folder reasons below. A refusal is shown
in the confirmation and names the reason; nothing is ended, moved or removed.

## Moving the project folder to the Trash

- The folder is **moved to the system Trash, never deleted outright**, so it can be recovered from there. On macOS,
  Finder's Put Back may not be offered for it; the folder is still in the Trash to be taken back out.
- Running sessions and the project's shell terminals are ended first, then the folder is moved, then the project is
  removed. **If the move fails, the removal fails** with the reason, and the project is kept with its records so the
  removal can be retried; sessions already ended for it stay archived, and shell terminals already ended stay gone.
- A session or shell terminal started in the project while the folder is being moved is ended as well, and goes with
  the project.
- Once the project is removed and its sessions' processes have ended, linked worktrees outside the folder that would
  be left without it, and where the repository outlives the folder the branches of the project's sessions, go too
  (which ones depends on whether the repository goes with the folder); a failure there
  does not undo the removal and is reported (see "With a project's folder moved to the Trash" in
  `docs/product/branch-and-worktree-cleanup.md`).
- A folder that no longer exists is skipped, and the project is removed as usual.
- A project directory that is a symbolic link is moved to the Trash as the link, not as the folder it points to.
- It is refused, before any session is ended and with the project kept, for:
  - the home directory, the root of a disk (`/` or a mount point, such as a mounted volume), or a folder that holds
    Octoboard's own data;
  - a folder shared with another project: another project's directory is the same folder, lies inside it or holds
    it, or another project's Git repository lies inside it, as a linked worktree's does when its main repository is
    this folder. The refusal names that other project, which has to be removed first. Symbolic links are resolved for
    this comparison, so two spellings of one folder count as the same folder.
