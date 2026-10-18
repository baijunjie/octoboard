# Project git status

A project whose directory is a git repository carries its **current branch and how far that branch is from its
upstream** in the sidebar. Octoboard checks those projects against their remotes by itself, on an interval, for the
current console, and checks one project at once when asked to with **Pull current branch**. What a check does beyond
reporting depends on one setting, **Automatically pull repositories**, described in "Automatically pulling
repositories" below, and on whether the check was asked for by hand. Pushing the current branch is not a check
(see "Pushing the current branch" in `docs/product/project-git-actions.md`). Between checks, the branch and the
counts follow changes made in the repository itself, without going to the remote (see "Following changes to the
repository's refs" below).

A project's uncommitted changes are inspected in the project pane's Git mode (see
`docs/product/project-pane-git-mode.md`), which only reads the repository and has no part in these checks. The
separate **Restore file changes** action discards tracked changes after confirmation (see "Restoring file changes"
in `docs/product/project-git-actions.md`).

## The branch badge

At the end of a project's row — after the project's name, before the activity marker a collapsed project carries, a
pinned project's pin button and the row's **+** and action-menu buttons (see "Project rows" in
`docs/product/sidebar.md`) — sits a badge showing that
project's git state. The same badge sits in focus mode's header, after the project's name (see "What both views
show" in `docs/product/focus-mode.md`). It shows nothing at all while no status has arrived for the project yet, and
nothing when the project's directory is not a git repository.

What it holds, in this order:

- **A glyph for what is going on**: a branch glyph normally; a commit glyph when `HEAD` is detached; a spinning refresh
  glyph while the remote is being checked, the branch is being pulled or the current branch is being pushed.
  Idle and in-flight states differ in shape, not in motion alone, and nothing animates where the system asks for reduced
  motion.
- **A warning triangle**, in red, when the last check failed, standing in for the branch or commit glyph; the refresh
  glyph takes its place while a check, a pull or a push runs. The message `git` or the operating system itself
  gave is then part of what the badge says — in its tooltip and in its name, shown as it came inside localized wording
  (see "What follows the language" in `docs/product/language.md`) — and the triangle itself carries nothing of its own.
  A failed check does not blank the rest of the badge: the branch and the counts are still read from the repository and
  shown beside the warning.
- **The branch name** — or the short commit id, when `HEAD` is detached. A repository with no commit yet says so in its
  place, and nothing stands there when the branch could not be read at all, the warning carrying the reason instead.
- **How far the branch is ahead of its upstream**, after an up arrow, and **how far behind**, after a down arrow, each
  shown only while it is not zero and each number written in the current language. A branch with no upstream shows
  neither. During a push, the ahead count's arrow scrolls continuously upward; during a pull, the behind count's
  arrow scrolls continuously downward. Each scroll stays inside a fixed area the size of one count arrow; the other
  arrow and both numbers stay still. A zero count still has no arrow, so the leading refresh glyph remains the in-flight
  indication when the relevant count is zero. Reduced motion keeps the arrows still too.

**While a check is in flight the badge keeps what it was showing** — the branch name (or short commit id), the
counts and a failed check's message — and only its glyph turns into the spinning refresh glyph, the warning triangle
included; the whole badge is replaced once the check finishes. The glyph turns only once the check has been running
for about 0.4 seconds, so a check that finishes sooner goes straight from the old state to the new one without
showing as in flight; a check asked for with **Pull current branch** shows as in flight at once. A project waiting
for its turn (see "When a project is checked" below) is not yet in flight. Meanwhile a local re-read can still update
the branch and the counts (see "Following changes to the repository's refs" below). A project with no status yet has
nothing to keep, so once its first check shows as in flight the badge shows the spinning glyph alone, and nothing
before that.

**While a push is in flight the badge describes the push**, including over a check or pull running at the same
time. All three use the same leading refresh glyph; the tooltip, accessible name and animated count arrow follow the
push state. The branch, counts and any failed check's message remain visible and can update during the push. Every
window showing the project sees the pushing state, including one that connects while the push is running. Once all
pushes of the project end, successfully or otherwise, the badge shows the latest check or pull state, or its
idle glyph if neither is running (see "Pushing the current branch" in `docs/product/project-git-actions.md`).

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
sentence, the same one assistive technology is given: the branch (or that a check, a pull or a push is in
flight, or why there is no branch), the ahead and behind counts, each only while it is not zero, and a failed check's
message. It is one tooltip in both placements, the sidebar row and focus mode's header, and it shows whether or not the
badge's text is cut — so the branch name and the error can still be read once the row has squeezed the text away. The
one state with no tooltip at all is the one saying in visible text that the repository has no commits yet, which a
tooltip would only repeat; that text still runs as a marquee while the pointer is over the text itself (see "Names too
long for their space" in `docs/product/labels-and-tooltips.md`).

**What assistive technology hears.** A project row announces itself as one control, so the badge's facts are folded
into the row's own name instead of being announced separately: the branch (or why there is none), the ahead and behind
counts, each only while it is not zero, and the error. In focus mode's header the badge is not inside such a row and
carries its own name. The badge is deliberately not a live region, so a check or push starting or finishing is not
announced.

## When a project is checked

**Only the projects of the current console** — the one the sidebar shows, see "The console switcher" in
`docs/product/sidebar.md` — are checked, and they are checked:

- as soon as that console becomes the current one;
- every five minutes for as long as it stays current;
- again once the connection to the daemon has been established, including after it was lost and came back (see
  "Losing the daemon connection" in `docs/product/application-lifecycle.md`);
- when a project appears in that console — added, or moved in from another one — which checks it without waiting
  for the next round;
- and when the user chooses **Pull current branch** for it (see "Pulling one project by hand" below).

A console that is not current has none of its projects checked, including one the floating sidebar is only
previewing (see "Previewing a console from the rail" in `docs/product/sidebar.md`); switching to it checks them.
Nothing is checked while no client is connected either: the schedule belongs to the application's window, not to the
daemon. Turning **Automatically pull repositories** on is not a check and is not limited this way; see "Turning the
switch on" below.

**At most four projects are checked at once**, across every window and console; the rest of a round wait their
turn, so a console with many projects is worked through four at a time. The daemon will not start a second check of
a project while one is already running or waiting its turn, and skips a project whose last check finished less than a
minute ago — so several open windows, each on its own interval, cannot multiply the work. A project skipped that way
simply keeps the status it already had. Pull current branch is the exception to all three (see "Pulling one project
by hand" below). **The five-minute interval, the one-minute floor and the four at once are
fixed**, not settings.

A status is never stored: after a daemon restart there is none for any project, and a project's badge shows nothing
again until its next check, or until its refs change (see "Following changes to the repository's refs" below).

### What one check does

- A directory that is not a git repository is reported as such, and the badge shows nothing for it. For this badge,
  for the Git actions in its project menu, a directory is a git repository only when it has a `.git`
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
- Finally the branch may be pulled; see "Automatically pulling repositories" and "Pulling one project by hand"
  below.

## Following changes to the repository's refs

Between checks, the badge follows what happens in the repository itself: a commit made by an agent session or in a
terminal, a branch switched, created or deleted there, a fetch, pull or push run there, or a commit in another
worktree of the same repository. Octoboard watches each git project's `HEAD`, its local branches and its
remote-tracking branches, packed ones included. The working tree and the index are not watched, so editing or staging
files changes nothing here. Once those refs have stayed unchanged for 300 milliseconds, the branch, its upstream and
the ahead/behind counts are read again from the repository — locally, nothing is fetched — and the result reaches
every window showing the project. So a commit raises the ahead count at once, and a push started right after it is
drawn on that count (see "Pushing the current branch" in `docs/product/project-git-actions.md`).

This is not a check: the five-minute interval and the one-minute floor do not apply to it, it never pulls, even with
**Automatically pull repositories** on, and it leaves the project's recorded remote URL alone. Every project of every
console is watched, whether or not its console is current and whether or not a window is connected. A directory is a
git repository here by the badge's rule (see "What one check does" above). Watching starts when the daemon starts,
when a project is added, and, for a directory that becomes a repository later, at the first check that finds it one;
it ends when the project, or its console, is deleted. A project whose refs cannot be watched is left to the checks. A
project whose `.git` has gone is not re-read: its badge keeps what it showed until its next check reports that the
directory is not a repository.

The re-read changes only the branch, its upstream and the counts. It also runs while a check, a pull or a push is in
flight, and leaves the badge's in-flight indication alone. A warning the last check left stays on the badge, since a
local read says nothing about the remote. A re-read that fails puts its own message on the badge as the warning, and
the branch and the counts stay as they were. A project with no status yet — never checked, or not since the daemon
restarted — gets its badge from the re-read.

## The environment a check runs `git` in

Every `git` run for a project's git status — the remotes read, the fetch, the branch read, and a fast-forward, whether
it comes from a check, from Pull current branch, from the pass that turning the switch on starts or from a change to
the repository's refs — is the `git` on the user's `PATH`, started from the user's own login shell environment, but
with that environment **filtered**. Their git configuration files and their credential helpers are used as they
stand.

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
  failure (see "The branch badge" above). A push fails the same way, as an error alert, rather than
  publishing some other repository (see "Pushing the current branch" in `docs/product/project-git-actions.md`).

A git association's clone, and the check the association form makes of a git URL, are **not** filtered this way: they
run with the whole shell environment (see "Associating a project" in `docs/product/consoles-and-projects.md`).

`git` is run so that it can never stop to ask for a credential, a passphrase or an unknown host key — each of those
fails immediately instead — and every `git` call of a check is given at most two minutes, so a check cannot hang; a
call that runs out reports only that it did not finish in time. A push's own bounds are in "Pushing the current
branch" in `docs/product/project-git-actions.md`.

## Automatically pulling repositories

Settings' **Git** section (see "Git" in `docs/product/settings.md`) holds one switch, **Automatically pull
repositories**, **off** to begin with. It is stored by the daemon, so every window and every client sees the same
value and it survives a restart.

Off and on alike, the check above runs and the branch and the counts stay accurate. The switch governs what happens
beyond reporting: with it on, a branch that is **behind its upstream and has no commits of its own** is pulled —
at the end of each check, and once immediately when the switch is turned on (see "Turning the switch on" below) —
and the badge shows the pull in flight. **Pulling** a branch here always means fast-forwarding it to its upstream,
nothing more. With the switch off, only Pull current branch pulls (see "Pulling one project by hand" below), under
the same rules:

- It **never pushes**, and never merges anything that is not a fast-forward. A push happens only when the user
  chooses **Push current branch**, and it is never part of this pull (see "Pushing the current branch"
  in `docs/product/project-git-actions.md`).
- A branch that is ahead of its upstream, whether or not it is also behind, is left alone. So are a branch with no
  upstream and a detached `HEAD`.
- Whether the project has a running agent session makes no difference: the working tree can move under an agent that
  is working in that project.
- A fast-forward `git` refuses records its message for the badge, as a failed fetch does.

Pulling a branch — with the switch on, or by Pull current branch — moves that branch's working tree. Pushing
the current branch does not: it updates the remote and, in the repository, the remote-tracking refs `git` updates
for that push, and when the branch had no upstream it records one. It does not edit the project's files (see
"Pushing the current branch" in `docs/product/project-git-actions.md`). Switching to a local branch moves the
project's own working tree (see "Switching the local branch" in `docs/product/project-git-actions.md`). Restoring
file changes discards tracked changes without moving the branch (see "Restoring file changes" in
`docs/product/project-git-actions.md`). A check's fetch still updates remote refs only and still does not move the
working tree or push (see "What one check does" above). The guarantee these qualify is in "What Octoboard never
modifies" in `docs/product/launching-agents.md`.

### Pulling one project by hand

A project's action menu offers **Pull current branch** under **Git** while the project's status says its directory is a
git repository — so not before its first status has arrived (see "What one check does" above, and "Project rows" in
`docs/product/sidebar.md` for where the menu is). It does not push. Choosing it closes the menu and starts the check
without waiting; a success is not toasted. It runs that project's check right away and then fast-forwards the branch
under exactly the rules above, whether or not **Automatically pull repositories** is on; choosing it is the consent
the switch would otherwise give. The one-minute floor does not apply.

Pull current branch does not wait its turn behind the four at once (see "When a project is checked" above). If a
check of the project is only waiting its turn — in a round of checks, or in the pass that turning **Automatically
pull repositories** on starts (see "Turning the switch on" below) — that check starts at once as this request's
check, with the fast-forward, in its place. If a check of the project is already running, or that pass is already
fast-forwarding it, nothing starts alongside it: as soon as it ends, one more check runs, with the fast-forward, so
the request ends in a fast-forward attempt either way. However many requests arrive meanwhile, they share that one
extra check, and it does not run at all if the project has been removed in the meantime.

The badge shows the check and the pull in flight as for any check, and a failed fetch or a fast-forward `git`
refuses records its message for the badge the same way.

### Turning the switch on

Turning the switch on takes effect at once: the branches already known to be behind their upstream are fast-forwarded
there and then, instead of waiting for the next check. Setting the switch to the value it already has does nothing.

This pass **never goes to the remote** — nothing is fetched. The statuses Octoboard is already holding only pick which
projects are worth visiting, so, unlike a check, it is **not limited to the current console**: every
project that has a status at all is visited — including projects of consoles that were current earlier since the
daemon started, and projects whose status came only from a local re-read (see "Following changes to the repository's
refs" above). A project with no status is not visited — nothing is known to fast-forward it to.

Before anything moves, each project's repository is read again — locally, still no network — and the same rule applied
to what it says now, because an arbitrary amount of time may have passed since the status was filled in. So a project
whose branch has been switched, whose `HEAD` has been detached, which has commits of its own, or whose directory is no
longer a git repository is left alone; and so is one whose local re-read fails, since nothing is fast-forwarded on a
rule that could not be checked.

The one-minute floor between checks (see "When a project is checked" above) does not apply to this pass, which goes
nowhere near the remote; the four at once does, shared with the checks. A project whose check is in flight or waiting
its turn is skipped and left to that check, which reads the switch itself.

The badge shows the pull in flight exactly as during a check, and a fast-forward `git` refuses records its
message for the badge the same way. A warning the last check left stays on the badge: it stands for that check
against the remote, which a local fast-forward says nothing about. An error this pass records for a project of a
console that is current in no window therefore stays on it until that console is current again and its projects are
checked, since a status is dropped only when the project or its console is deleted.

Turning the switch **off** undoes nothing and triggers nothing: branches already fast-forwarded stay where they are.
