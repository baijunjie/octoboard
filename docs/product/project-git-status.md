# Project git status

A project whose directory is a git repository carries its **current branch and how far that branch is from its
upstream** in the sidebar. Octoboard checks those projects against their remotes by itself, on an interval, for the
current console, and checks one project at once when asked to with **Sync repository**. What a check does beyond
reporting depends on one setting, **Automatically sync repositories**, described in "Automatically syncing
repositories" below, and on whether the check was asked for by hand. Pushing the current branch is not a check
(see "Pushing the current branch" below).

A project's uncommitted changes are not part of this: they are in the project pane's Git mode (see
`docs/product/project-pane-git-mode.md`), which only reads the repository and has no part in these checks.

## The branch badge

At the end of a project's row — after the project's name, before the activity marker a collapsed project carries and
the row's **+** and action-menu buttons (see "Project rows" in `docs/product/sidebar.md`) — sits a badge showing that
project's git state. The same badge sits in focus mode's header, after the project's name (see "What both views
show" in `docs/product/focus-mode.md`). It shows nothing at all while no status has arrived for the project yet, and
nothing when the project's directory is not a git repository.

What it holds, in this order:

- **A glyph for what is going on**: a branch glyph normally; a commit glyph when `HEAD` is detached; a spinning
  refresh glyph while the remote is being checked; a download glyph bobbing up and down while the branch is being
  fast-forwarded.
  The four differ in shape, not in motion alone, and nothing animates where the system asks for reduced motion.
- **A warning triangle**, in red, when the last check failed, standing in for the branch or commit glyph; the
  refresh and download glyphs still take its place while a check or a fast-forward runs. The message `git` or the
  operating system itself gave is then part of what the badge says — in its tooltip and in its name, shown as it came
  inside localized wording (see "What follows the language" in `docs/product/language.md`) — and the triangle itself
  carries nothing of its own. A failed check does not blank the rest of the badge: the branch and the counts are
  still read from the repository and shown beside the warning.
- **The branch name** — or the short commit id, when `HEAD` is detached. A repository with no commit yet says so in its
  place, and nothing stands there when the branch could not be read at all, the warning carrying the reason instead.
- **How far the branch is ahead of its upstream**, after an up arrow, and **how far behind**, after a down arrow, each
  shown only while it is not zero and each number written in the current language. A branch with no upstream shows
  neither.

**While a check is in flight the badge keeps what the last check found** — the branch name (or short commit id), the
counts and a failed check's message — and only its glyph turns into the spinning refresh glyph, the warning triangle
included; the whole badge is replaced once the check finishes. A project's first check has nothing to keep, so until it
finishes the badge shows the spinning glyph alone.

**As the row gets tight the branch and counts give way together**: the branch name (or short commit id) and both
upstream counts form one line, showing its beginning and fading at its right edge. This line reads left to right
even under a right-to-left language. The glyph stays in place while the whole line runs as a marquee, through the
branch and the last count (see "Names too long for their space" in `docs/product/labels-and-tooltips.md`). The text
can disappear entirely, leaving the glyph (the warning triangle after a failed check). The project's name keeps a
floor of its own and never vanishes. When the line only says that the repository has no commits yet, it follows the
window's reading direction instead.

**One tooltip for the whole badge.** Resting the mouse anywhere on the badge — the state glyph, the branch name or
either count — shows one tooltip, after the window's usual wait and on pointer hover only, since the badge is no tab
stop (see "How a tooltip behaves" in `docs/product/labels-and-tooltips.md`). It says the badge's whole state in one
sentence, the same one assistive technology is given: the branch (or that a check or a fast-forward is in flight, or
why there is no branch), the ahead and behind counts, each only while it is not zero, and a failed check's message.
It is one tooltip in both placements, the sidebar row and focus mode's header, and it shows whether or not the badge's
text is cut — so the branch name and the error can still be read once the row has squeezed the text away. The one
state with no tooltip at all is the one saying in visible text that the repository has no commits yet, which a tooltip
would only repeat; that text still runs as a marquee while the pointer is over the text itself (see "Names too long
for their space" in `docs/product/labels-and-tooltips.md`).

**What assistive technology hears.** A project row announces itself as one control, so the badge's facts are folded
into the row's own name instead of being announced separately: the branch (or why there is none), the ahead and behind
counts, each only while it is not zero, and the error. In focus mode's header the badge is not inside such a row and
carries its own name. The badge is deliberately not a live region, so a check starting or finishing is not announced.

## When a project is checked

**Only the projects of the current console** — the one the sidebar shows, see "The console switcher" in
`docs/product/sidebar.md` — are checked, and they are checked:

- as soon as that console becomes the current one;
- every five minutes for as long as it stays current;
- again once the connection to the daemon has been established, including after it was lost and came back (see
  "Losing the daemon connection" in `docs/product/application-lifecycle.md`);
- when a project appears in that console — added, or moved in from another one — which checks it without waiting
  for the next round;
- and when the user chooses **Sync repository** for it (see "Syncing one project by hand" below).

A console that is not current has none of its projects checked, including one the floating sidebar is only
previewing (see "Previewing a console from the rail" in `docs/product/sidebar.md`); switching to it checks them.
Nothing is checked while no client is connected either: the schedule belongs to the application's window, not to the
daemon. Turning **Automatically sync repositories** on is not a check and is not limited this way; see "Turning the
switch on" below.

The daemon will not start a second check of a project while one is already running, and skips a project whose last
check finished less than a minute ago — so several open windows, each on its own interval, cannot multiply the work. A
project skipped that way simply keeps the status it already had. Sync repository is the exception to both (see
"Syncing one project by hand" below). **The five-minute interval and the one-minute floor are fixed**, not settings.

A status is never stored: after a daemon restart there is none for any project, and a project's badge shows nothing
again until its next check.

### What one check does

- A directory that is not a git repository is reported as such, and the badge shows nothing for it. For this badge,
  for **Sync repository** and for **Push current branch**, a directory is a git repository only when it has a `.git`
  entry of its own; a subdirectory of a repository is not one. The project pane's Git mode looks further up the
  folders (see "The repository and its worktrees" in `docs/product/project-pane-git-mode.md`); the badge does not.
  The project's recorded remote URL is cleared, unless the directory cannot be seen at all (see "The project's remote
  URL" in `docs/product/consoles-and-projects.md`).
- Otherwise the repository's remotes are read, and the project's recorded remote URL is made the one that read gives,
  which is how that value is kept true to a repository whose remote changed outside Octoboard (same section). A read
  that could not be made leaves the recorded value alone. A changed value reaches the open windows at once; nothing
  about the badge changes with it.
- The remote is then fetched — **remote refs only**: nothing in the working tree moves and no branch changes.
  The remote fetched is the current branch's upstream remote; with no upstream and exactly one remote configured, that
  one; with no upstream and several, `git`'s own default. A repository with no remote at all is not fetched, and that
  is not an error.
- Then the branch, its upstream and the ahead/behind counts are read from the repository. A fetch that failed —
  offline, authentication, a host key that has never been accepted — records its message for the badge but does not
  stop this read, so the branch and the counts still show.
- Finally the branch may be fast-forwarded; see "Automatically syncing repositories" and "Syncing one project by
  hand" below.

## The environment a check runs `git` in

Every `git` run for a project's git status — the remotes read, the fetch, the branch read, and a fast-forward, whether
it comes from a check, from Sync repository or from the pass that turning the switch on starts — is the `git` on the
user's `PATH`, started from the user's own login shell environment, but with that environment **filtered**. Their git
configuration files and their credential helpers are used as they stand.

What the filter takes out are the `GIT_*` variables the user exports in their own shell configuration. Kept are only
those that say which of the user's own material to use and how to reach a remote with it: where git's configuration
files live, the TLS certificates and keys a connection uses along with their formats, the TLS version and cipher list,
and the git protocol proxy. Everything else is dropped — in particular anything that would point a command at another
repository, worktree or index, anything that sets configuration items outright, and the variable that turns TLS
certificate verification off.

Two consequences are worth knowing:

- Certificates and keys an enterprise network needs still reach the fetch, and a push, when they are given through
  those variables alone rather than through a configuration file.
- A setting only a dropped variable carries does not apply. The check then fails, rather than quietly reporting the
  branch and counts of whatever repository such a variable names, and its reason reaches the badge like any other
  failure (see "The branch badge" above). A push fails the same way, as an error toast, rather than
  publishing some other repository (see "Pushing the current branch" below).

A git association's clone, and the check the association form makes of a git URL, are **not** filtered this way: they
run with the whole shell environment (see "Associating a project" in `docs/product/consoles-and-projects.md`).

`git` is run so that it can never stop to ask for a credential, a passphrase or an unknown host key — each of those
fails immediately instead — and every `git` call of a check is given at most two minutes, so a check cannot hang; a
call that runs out reports only that it did not finish in time. A push's own bounds are in "Pushing the current
branch" below.

## Automatically syncing repositories

Settings' **Git** section (see "Git" in `docs/product/settings.md`) holds one switch, **Automatically sync
repositories**, **off** to begin with. It is stored by the daemon, so every window and every client sees the same
value and it survives a restart.

Off and on alike, the check above runs and the branch and the counts stay accurate. The switch governs what happens
beyond reporting: with it on, a branch that is **behind its upstream and has no commits of its own** is
fast-forwarded — at the end of each check, and once immediately when the switch is turned on (see "Turning the switch
on" below) — and the badge shows the fast-forward in flight. With it off, only Sync repository fast-forwards (see
"Syncing one project by hand" below), under the same rules:

- It **never pushes**, and never merges anything that is not a fast-forward. A push happens only when the user
  chooses **Push current branch**, and it is never part of this fast-forward (see "Pushing the current branch"
  below).
- A branch that is ahead of its upstream, whether or not it is also behind, is left alone. So are a branch with no
  upstream and a detached `HEAD`.
- Whether the project has a running agent session makes no difference: the working tree can move under an agent that
  is working in that project.
- A fast-forward `git` refuses records its message for the badge, as a failed fetch does.

Fast-forwarding a branch — with the switch on, or by Sync repository — moves that branch's working tree. Pushing
the current branch does not: it updates the remote and, in the repository, the remote-tracking refs `git` updates
for that push, and when the branch had no upstream it records one. It does not edit the project's files (see
"Pushing the current branch" below). A check's fetch still updates remote refs only and still does not move the
working tree or push (see "What one check does" above). The guarantee these qualify is in "What Octoboard never
modifies" in `docs/product/launching-agents.md`.

### Syncing one project by hand

A project's action menu offers **Sync repository** under **Git** while the project's status says its directory is a
git repository — so not before its first status has arrived (see "What one check does" above, and "Project rows" in
`docs/product/sidebar.md` for where the menu is). It does not push. Choosing it closes the menu and starts the check
without waiting; a success is not toasted. It runs that project's check right away and then fast-forwards the branch
under exactly the rules above, whether or not **Automatically sync repositories** is on; choosing it is the consent
the switch would otherwise give. The one-minute floor does not apply.

If a check of the project is already running, nothing starts alongside it: as soon as it ends, one more check runs,
with the fast-forward, so the request ends in a fast-forward attempt. However many requests arrive meanwhile, they
share that one extra check, and it does not run at all if the project has been removed in the meantime.

The one exception is a request that arrives while the project is being visited by the pass that turning
**Automatically sync repositories** on starts (see "Turning the switch on" below). The request is left to that pass:
no check follows it and nothing is fetched, so the branch moves only if the pass's own local re-read finds it behind
its upstream with no commits of its own.

The badge shows the check and the fast-forward in flight as for any check, and a failed fetch or a fast-forward `git`
refuses records its message for the badge the same way.

### Turning the switch on

Turning the switch on takes effect at once: the branches already known to be behind their upstream are fast-forwarded
there and then, instead of waiting for the next check. Setting the switch to the value it already has does nothing.

This pass **never goes to the remote** — nothing is fetched. The statuses Octoboard is already holding only pick which
projects are worth visiting, so, unlike a check, it is **not limited to the current console**: every
project that has a status at all is visited, including projects of consoles that were current earlier since Octoboard
was started. A project that has never been checked is not visited — nothing is known to fast-forward it to.

Before anything moves, each project's repository is read again — locally, still no network — and the same rule applied
to what it says now, because an arbitrary amount of time may have passed since the status was filled in. So a project
whose branch has been switched, whose `HEAD` has been detached, which has commits of its own, or whose directory is no
longer a git repository is left alone; and so is one whose local re-read fails, since nothing is fast-forwarded on a
rule that could not be checked.

The one-minute floor between checks (see "When a project is checked" above) does not apply to this pass, which goes
nowhere near the remote. A project whose check is in flight is skipped and left to that check, which reads the switch
itself.

The badge shows the fast-forward in flight exactly as during a check, and a fast-forward `git` refuses records its
message for the badge the same way. A warning the last check left stays on the badge: it stands for that check
against the remote, which a local fast-forward says nothing about. An error this pass records for a project of a
console that is current in no window therefore stays on it until that console is current again and its projects are
checked, since a status is dropped only when the project or its console is deleted.

Turning the switch **off** undoes nothing and triggers nothing: branches already fast-forwarded stay where they are.

## Pushing the current branch

**Push current branch**, under **Git** in the project's action menu, pushes that project's current branch and only
that branch. It is offered only while the project's status says the directory is a git repository (see "Project
rows" in `docs/product/sidebar.md`). Choosing it closes the menu at once. The push is waited on until `git push` has
exited. When the push is accepted, the window that asked shows a notice toast saying the current branch was pushed.
A refusal is an error toast worded from the code below. Pushing does not fetch and does not fast-forward, and a
fast-forward never pushes (see "Automatically syncing repositories" above). A local branch that is behind its
upstream stays behind; if the remote rejects the update, the push fails and the branch stays where it was.

The push never uses `--force`, and it is never a bare `git push`, which would follow `push.default` and can publish
every branch whose name exists on the remote. The one branch is named explicitly, so only that branch is published.
The user's git configuration and credential helpers are used as they stand. `git` cannot stop to ask for a
credential, a passphrase or a host key — the same refusal as a fetch (see "The environment a check runs `git` in"
above) — and the push itself is given at most two minutes.

Once the directory is a repository, the branch is read, then the repository's remotes. That remotes read is
filtered and non-interactive in the same way, and is given at most 30 seconds. If either read fails, the push
fails and `git push` is not run, so a detached `HEAD` whose remotes cannot be read is reported as that failed read
rather than as not being on a branch.

What is pushed, when both reads succeed:

- A branch that has an upstream is pushed to that upstream's remote and remote branch. The remote is the longest
  configured remote name that is a prefix of the upstream followed by `/`, and the rest of the upstream — which
  must be non-empty — is the remote branch. A remote named `fork/origin` wins over `fork` for an upstream
  `fork/origin/main`; a remote named `origin` does not match an upstream `origin-fork/main`. The remote branch
  need not have the same name as the local branch. This push does not change the upstream.
- A branch with no upstream is pushed to `origin` when the repository has that remote, otherwise to its only
  remote, and that push records the upstream under the local branch's own name.

Nothing is pushed when the push is refused. A refused push publishes no status of its own:

- The project directory is not a git repository when the push runs: `not_a_git_repository`. The rule is the badge's
  (see "What one check does" above): `.git` must be in the project directory itself. A subdirectory of a repository
  is not one.
- `HEAD` is detached: `not_on_a_branch`.
- The branch has no commit yet: `no_commits_to_push`.
- The upstream names no configured remote of this repository: `upstream_not_a_remote`.
- There is no upstream, and the repository has neither an `origin` nor exactly one remote: `no_push_remote`. No
  remotes at all, and several remotes with no `origin`, are both this refusal.
- A branch or remote name is empty, starts with `-`, or contains `:`, a NUL or a newline: `git_push_failed`. `git`
  is not run. The reason says the name cannot be used in a refspec. The local branch's name is judged before the
  upstream. A remote or remote-branch name is judged only after the upstream has matched a remote, so an upstream
  that matches none is `upstream_not_a_remote` even when its text would not be a usable name.
- The branch cannot be read, or the remotes cannot be read: `git_push_failed`. The branch's reason is `git`'s own
  message, or that the read did not finish in time. The remotes' reason says they could not be read.
- `git push` itself fails, including a remote that rejects the update, a missing credential, or a push that does
  not finish within two minutes: `git_push_failed`, with `git`'s own message, or that it did not finish in time.

A project that no longer exists is `unknown_project`.

The push does not take the claim a check uses, so the two are not ordered. **Sync repository**, or a scheduled
check, that arrives during a push is handled as it would be with no push in flight: it is not queued behind the
push. A push that finishes while a check is in flight publishes an idle status, which can replace the check's
in-flight badge until the check publishes again. A second push of the same project is not refused while one is
still running.

A successful push does not edit the project's files and does not move the working tree. It updates the remote, and
in the repository it updates the remote-tracking refs `git` updates for that push. When the branch had no upstream,
it also records one. The daemon then re-reads the local branch header and publishes the project's git status, with
no fetch, so the badge's ahead count can drop. Every window showing the project sees that status. The badge does
not show a check or a fast-forward in flight for the push itself. An error the last check left on the badge stays,
unless that re-read fails, in which case the warning becomes the re-read's message and the branch and the counts
stay as they were. A re-read that fails does not turn the notice into an error toast: the push was accepted.
Turning **Automatically sync repositories** on never pushes.
