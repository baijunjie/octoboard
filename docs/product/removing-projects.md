# Removing a project

A locked session of the project, archived sessions included, refuses removal before any process is ended or
folder is moved, under "Locking a session" in `docs/product/sessions.md`.

Removing a project removes the association, and with it the records of that project's sessions, archived ones included,
and the report panel pages they pushed (see `docs/product/report-panel.md`). It also clears what Octoboard recorded
about which session is working on which branch in the project's repository, unless another project still points into
that repository (see "What the record is" in `docs/product/branch-responsible-session.md`). **The directory and
everything in it are left as they are**, and so are the repository's branches and worktrees, unless the user chooses to
move the folder to the Trash as well (see "Moving the project folder to the Trash" below).

Removing a project asks for confirmation. **Sessions of the project that are still running do not block it**:

- With none running, the confirmation offers **Remove**.
- With some running, the confirmation lists them — each with its status glyph, its agent's icon and its title, in the
  sidebar's order (see "Order of projects and sessions" in `docs/product/order-of-projects-and-sessions.md`) — in its
  consequences, the body text after its options and before its buttons: a line says that that many sessions are still
  running and will be ended, and the list follows it (see "The confirmation" in
  `docs/product/branch-and-worktree-cleanup.md` for the lines a ticked option adds). Its button reads **End and
  remove**. The list follows sessions starting or stopping while the confirmation is open. Confirming ends each running
  session's process the way archiving does (see "Archiving, interruption and resuming" in
  `docs/product/archiving-and-resuming-sessions.md`), then removes the project with all its sessions and the last output
  Octoboard kept for each. Sessions bound to another session of the project are ended before the session they are bound
  to, the deepest level first, so ending that one is never refused over them. A lead session's console session belongs
  to no project and is left as it is; the lead session's binding to it goes with the lead session's record (see "Lead
  sessions" in `docs/product/hub-orchestration.md`).

The confirmation also carries an option to **also move the project folder to the Trash**, unticked each time the
confirmation opens, with a hint that the whole folder goes, including anything not committed to Git, and that it can be
recovered from the Trash. Ticking it makes the message say the folder is moved to the Trash; for the confirm button's
label, see "What a confirmation's confirm button names" in `docs/product/dialogs.md`. Ticking it also lists the
worktrees outside the folder, and the branches, that go with it, and offers locked worktrees under a choice of their own
(see "With a project's folder moved to the Trash" in `docs/product/branch-and-worktree-cleanup.md`). While a removal
runs, the button shows as pending and the option cannot be changed.

**The project's shell terminals end with it** (see "Closing a shell terminal" in `docs/product/shell-terminals.md`),
whether their shells are busy or not; they never hold a removal up and the confirmation does not list them. They are
ended once the removal is past its refusals below.

Removal is refused while a session of the project is being launched or resumed at that moment, since that session has
no process yet that could be ended, and, with the Trash option ticked, for the folder reasons below. A refusal is shown
in an error alert over the confirmation and names the reason; nothing is ended, moved or removed.

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
- A folder that no longer exists is skipped: nothing is moved, and the project is removed as usual.
- A project directory that is a symbolic link is moved to the Trash as the link, not as the folder it points to. A
  project path that is a file, a link to a file, or a link that leads nowhere is moved as itself. It holds nothing, so
  the protected folders below never apply to it; it is refused only when another project's directory holds it, when
  another project is at that same file or at a link to it, or when another project could not be looked at.
- It is refused, before any session is ended and with the project kept, for:
  - the home directory, the root of a disk (`/` or a mount point, such as a mounted volume), or a folder that holds
    Octoboard's own data (`project_directory_protected`);
  - a folder shared with another project: another project's directory is the same folder, lies inside it or holds
    it, or another project's Git repository lies inside it, as a linked worktree's does when its main repository is
    this folder (`project_directory_shared`). The refusal names that other project, which has to be removed first.
- **Folders are compared by what they are on disk** (device and inode), not by how their paths are written, both for
  the reasons above and for telling whether another project is in the way: a symbolic link, a spelling that differs
  only in letter case, a bind mount, or a macOS firmlink (`/System/Volumes/Data/Users/...` for `/Users/...`) naming
  one folder makes no difference.
  - Another project's path, the home directory's or Octoboard's data folder's, that has nothing at it or cannot be
    read (no permission), is compared by its path's text instead, as written and, where it resolves, as resolved.
    This fallback is only for those paths: the project's own path that cannot be read refuses the move, as below.
  - Another project whose path is a file rather than a folder is in the way only when the folder holding that file,
    or holding the file a link at that path leads to, is this folder or lies inside it.
  - Where both projects' paths are files, they are compared by the file's identity on disk: another project at the
    same file, or at a link to it, is in the way. Only where the file system reports no identity for them are their
    paths' text compared.
- **A path that cannot be looked at refuses the move** at the same point, with the project kept, since what the move
  would take along cannot be told: the project's own path when it cannot be read, any path that fails with another
  error or lies on a file system that reports no inode, and any path that does not answer within about 5 seconds (as
  on a mount that hangs) or is still not answering an earlier look.
  - For the project's own folder, the home directory or Octoboard's data folder, it is refused as a failed move
    (`project_directory_trash_failed`), with the daemon's own text under **Error details** and in no sentence:
    - where one of them did not answer in time, the alert says that the folder, the home folder or Octoboard's data
      folder did not respond in time, so the folder could not be checked and the project was kept, and advises trying
      again once the volume responds again, or removing the project without moving its folder to the Trash;
    - where the project's own folder, or what a link there leads to, could not be read for want of permission, the
      alert says that the folder could not be read, so whether another project's files are in it could not be
      checked, and the project was kept, and advises checking the folder's permissions and granting Octoboard access
      in the system's privacy settings if needed, then trying again, or removing the project without moving its
      folder to the Trash;
    - any other case (another error, no inode) is worded as any failed move.
  - For another project, it is refused as shared with that project (`project_directory_shared`). A project known to
    be in the way is named before one that only could not be looked at.
  - The root of a disk is refused as protected ahead of any refusal over the home directory, the data folder or another
    project that could not be looked at.
- **A refusal over a path that could not be looked at is repeated by the removal confirmed after that lookup.** The
  confirmation looks up what goes along with the folder (see "With a project's folder moved to the Trash" in
  `docs/product/branch-and-worktree-cleanup.md`). Where a lookup is refused because a path could not be looked at
  (either refusal above), confirming the removal with the Trash option ticked is refused the same way, even if the
  path can be looked at by then, so the removal takes along nothing the confirmation did not show.
  - The repeated refusal belongs to that one lookup, not to the project: the confirmation sends back the token the
    lookup's refusal carried (the `inspection` param), and the daemon repeats that refusal once and then drops it. The
    confirmation sends the token of its latest lookup; a lookup in another window does not affect it.
  - A removal with no such token, or with one the daemon no longer keeps — already used, its project or console
    removed, pushed out once four newer such refusals of the same project are kept, or a restart of the daemon in
    between — looks for itself when it is confirmed.
