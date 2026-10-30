# Removing an account

Removing an account the user owns, with **Remove** in its actions menu in Settings' **Agent accounts** section (see
"The accounts the user owns" in `docs/product/agent-accounts.md`). The default account cannot be removed.

Removing an account clears the reference of every console that refers to it, putting each back on the agent's default
account, and puts every project that refers to it back to inheriting its console's reference (see "The project's
account, model and reasoning effort" in `docs/product/projects.md`); a project on the agent's default account is left
as it is. Removing an account ends a login running for it. **The account's config directory and everything in it are
left as they are**, unless the user chooses to move the directory to the Trash as well (see "Moving the account
directory to the Trash" below).

An account that shares the default configuration, or whose last change of it is incomplete, has its sharing turned off
first, under the same rule as a change of the switch, its idle sessions ended and resumed around it: refused while
sessions of the account are working or it is being signed in, and while a change of its sharing runs (see "Removing or
repointing a sharing account" in `docs/product/sharing-the-default-configuration.md`).

## The confirmation

Removing an account asks first, in a destructive confirmation titled "Remove account" whose first sentence names the
account. Its message says that consoles using the account go back to the agent's default account and projects using it
go back to their console's, and that sessions already open are kept, each going on with the directory it started from.

The confirmation carries an option to **also move the account directory to the Trash**, unticked each time the
confirmation opens, with a hint that the whole directory goes, including its login, settings and conversation history,
and that it can be recovered from the Trash. Ticking it:

- makes the message say that the directory is moved to the Trash, so the account's archived sessions can no longer be
  resumed, their conversations being in it; the line about open sessions goes, since running sessions hold the removal
  up (below);
- asks the daemon whether the directory can go (see "Checking the directory when the option is ticked" below);
- lists the account's sessions that are running or starting, if any (below).

The confirm button reads **Remove** whether the option is ticked or not (see "What a confirmation's confirm button
names" in `docs/product/dialogs.md`). While a removal runs, the button shows as pending and the option cannot be
changed.

**With the option ticked, running sessions hold the removal up.** Every session of the account's agent that is
starting, working, waiting for the user or idle, and that is on the account or runs under its directory, is listed in
the confirmation's consequences, after the option: a line says how many sessions of the account are still running or
starting and that they have to be ended, or switched to another account, first, and the list follows, each session with
its status glyph, its agent's icon and its title, in the sidebar's order (see "Order of projects and sessions" in
`docs/product/order-of-projects-and-sessions.md`). While any is listed, **Remove** is disabled. The list follows
sessions starting or stopping while the confirmation is open. The line is part of the option's description to assistive
technology and is announced as it appears. Archived sessions are not listed and do not hold the removal up. Without the
option, sessions of the account are neither listed nor affected.

When **Remove** becomes disabled while keyboard focus is on it, focus moves to **Cancel**.

## Checking the directory when the option is ticked

Each time the option is ticked, the confirmation asks the daemon what moving the directory to the Trash would come to
now (`inspect_account_trash`), judged as the removal itself judges it (see "Moving the account directory to the Trash"
below) but with only quick looks: each look at a path is given about half a second. Running sessions are not part of
this check; the confirmation lists those itself.

| Outcome | What the confirmation shows |
|---|---|
| Still being checked | **Remove** shows as pending and cannot be pressed; nothing else is shown. |
| The directory can go (`ok`) | **Remove** is available. |
| Nothing is at the directory's path (`missing`) | **Remove** is available. The hint says the account's directory does not exist, so there is nothing to move, and the message stays the one of a removal without the option. |
| A refusal the check can already tell (`protected`, `shared`, `unlookable`) | A warning alert under the option says why the directory cannot be moved to the Trash, stating only the cause, and **Remove** is disabled. The alert is part of the option's description to assistive technology and is announced as it appears. |
| A look that did not answer within its time, or a path still not answering an earlier look (`undecided`) | **Remove** is available, and nothing is said against the directory: the removal judges it with its full time when confirmed, and may still refuse it. |
| The check itself could not be made | A danger alert under the option says that whether the directory can be moved to the Trash could not be checked, with the failure's own sentence, **Details** and **Try again**; **Remove** is disabled. While **Try again** runs, the alert stays with its text and its **Try again** shows as pending. |

The warning alert's causes are those of the refusals in "Moving the account directory to the Trash" below: the
directory is the home folder, the root of a disk or holds Octoboard's own data; it overlaps the directory of another
account, naming its agent and its name; it is, holds or lies inside the agent's default account's directory, naming the
agent; it holds the folder of a project, naming the project; it could not be read, so it could not be checked for files
of other accounts or projects; whether it holds or overlaps another account's or a project's directory could not be
checked; or it could not be accessed.

Unticking the option drops the check and anything it showed; an answer that arrives afterwards is ignored.

## Moving the account directory to the Trash

- The directory is **moved to the system Trash, never deleted outright**, so it can be recovered from there.
- The move comes before the account's record is removed. **If the move fails, the removal fails** with the reason, and
  the account is kept, so the removal can be retried; sharing turned off for the removal stays off.
- **The directory is judged before anything is changed**, so a refusal at that point leaves everything as it was. It is
  judged again once the account is locked (after its sharing is turned off), since the account may have been repointed
  or the file system changed meanwhile; a refusal then leaves sharing turned off. It is judged a last time immediately
  before the move.
- A directory that no longer exists is skipped: nothing is moved, and the account is removed as usual.
- A directory that is a symbolic link is moved to the Trash as the link, not as the directory it points to.
- An account that shares the default configuration has its sharing turned off before the move, so the directory holds
  its own content, the conversations of its sessions taken back included, rather than links into the default directory
  (see "Turning sharing off" in `docs/product/sharing-the-default-configuration.md`).
- **The account's archived sessions can no longer be resumed** once the directory has moved: a session's relaunch is
  refused when the config directory it recorded no longer exists (see "Archiving, interruption and resuming" in
  `docs/product/archiving-and-resuming-sessions.md`). They do not hold the removal up and are not removed with it.

It is refused, with the account kept:

- while a session of the account has a process, or is being launched, resumed or switched to it
  (`account_has_running_sessions`): the directory holds what they run on;
- for the home directory, the root of a disk (`/` or a mount point), or a directory that holds Octoboard's own data
  (`account_directory_protected`);
- while the directory is, holds or lies inside an agent's default configuration directory or another account's
  directory, or is or holds a registered project's folder (`account_directory_shared`, naming that account, agent or
  project). An account directory inside a project's folder is allowed;
- when a path that decides it cannot be looked at (`account_directory_trash_failed`, see below);
- when a login could still write into the directory (`account_directory_trash_failed` with the cause `signing_in`, see
  "Logins and agent reads during the removal" below).

**Directories are compared by what they are on disk** (device and inode), not by how their paths are written, as for a
project's folder (see "Moving the project folder to the Trash" in `docs/product/removing-projects.md`): a symbolic link,
a spelling that differs only in letter case or a macOS firmlink naming one directory makes no difference. Another
account's, an agent's default or a project's path, the home directory's or Octoboard's data folder's, that has nothing
at it or cannot be read for want of permission is compared by its path's text instead.

**A path that cannot be looked at refuses the move**, since what the move would take along cannot be told. Each look is
given five seconds; a path that does not answer in that time, or is still not answering an earlier look, counts as not
looked at. The refusal is `account_directory_trash_failed`, whose `cause` says which:

| Cause | Meaning |
|---|---|
| `unanswered` | The directory itself, the home directory or Octoboard's data folder did not answer in time. |
| `unreadable` | Permission to read the directory itself was refused. |
| `other_unlooked` | Whether the directory holds or overlaps another account's directory, an agent's default directory or a project's folder could not be told: that path did not answer in time or could not be looked at. |
| none | Any other failure to look at the directory, the home directory or the data folder, or the move itself failing. |

## Logins and agent reads during the removal

With the option ticked, once the removal has passed its first judgement and until it ends, however it ends, **a login
of the account cannot start**: it is refused with `account_being_removed` (see "How a login ends" in
`docs/product/logging-accounts-in.md`). Once the removal ends, the account is gone, or it is kept and can be logged in
again. A login that was already running, or started before that point, is ended by the removal and waited for, up to
five seconds, also when the directory turns out to be missing; one that does not end in that time refuses the move with
the cause `signing_in`, and so does a login of another account pointed at the same directory.

Before the move, the removal also holds back the short reads Octoboard runs with the agent's CLI under the directory —
reading the account's quota (see `docs/product/account-quotas.md`), starting its usage windows (see
`docs/product/starting-usage-windows.md`) and listing its models (see "The models offered" in
`docs/product/model-and-reasoning-effort.md`) — since one would create the directory again at its old path, and waits
for those already running to end, without a time limit: starting a usage window can take up to a minute. A read held
back meanwhile looks at the directory again once let through and, finding it gone, runs nothing: the quota reads as
for an account whose directory does not exist, the usage window is not started, and the model list is empty with its
reason.

## When a removal is refused or fails

A refusal or a failure of the removal is shown in an error alert titled "Could not remove the account" over the
confirmation, which stays open as it was. The refusals and failures of moving the directory to the Trash
(`account_has_running_sessions`, `account_directory_protected`, `account_directory_shared` and
`account_directory_trash_failed`) state only the error and its cause, never what to do about it; a cause the system's
own text names, such as access being denied, is said after it as a sentence of its own, and the daemon's own text is
under **Error details** (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`). They say:

- that the account still has that many sessions running or starting, so its directory cannot be moved to the Trash;
- the causes of the warning alert in "Checking the directory when the option is ticked" above, for a protected or
  shared directory and for one that could not be read or whose overlap with another account's or a project's
  directory could not be checked;
- that the directory, the home folder or Octoboard's data folder did not respond in time, so the directory could not
  be checked;
- that the account's login did not end in time, so the directory could not be moved to the Trash;
- otherwise, that the directory could not be moved to the Trash.

After a refusal over the directory itself (`account_directory_protected`, `account_directory_shared` or
`account_directory_trash_failed`, but not one with the cause `signing_in`), the confirmation checks the directory again
once the error alert has been acknowledged and no other error alert is left, so a reason that still holds then stands
under the option with **Remove** disabled, rather than a second confirm being answered with the same alert; a failed
move the check finds nothing wrong with leaves **Remove** available to try again. A refusal over running
sessions is not checked again: the confirmation's list already follows them.
