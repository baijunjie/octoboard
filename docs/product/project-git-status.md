# Project git status

A project whose directory is a git repository carries its **current branch and how far that branch is from its
upstream** in the sidebar. Octoboard checks those projects against their remotes by itself, on an interval, for the
current console. What a check does beyond reporting depends on one setting, **Automatically sync
repositories**, described last.

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
- **A warning triangle**, only when the last check failed. Its tooltip carries the message `git` or the operating
  system itself gave, shown as it came inside localized wording (see "What follows the language" in
  `docs/product/language.md`). A failed check does not blank the rest of the badge: the branch and the counts are
  still read from the repository and shown beside the warning.
- **The branch name** — or the short commit id, when `HEAD` is detached. A repository with no commit yet says so in its
  place, and nothing stands there when the branch could not be read at all, the warning carrying the reason instead.
- **How far the branch is ahead of its upstream**, after an up arrow, and **how far behind**, after a down arrow, each
  shown only while it is not zero and each number written in the current language. A branch with no upstream shows
  neither.

**While a check is in flight the badge keeps what the last check found** — the branch name (or short commit id), the
counts and the warning triangle — and only its glyph turns into the spinning refresh glyph; the whole badge is replaced
once the check finishes. A project's first check has nothing to keep, so until it finishes the badge shows the
spinning glyph alone.

**As the row gets tight the branch name is what gives way**: it fades out and can disappear entirely, leaving the
glyph, the counts and the warning, which carry the same facts. The counts go next, and the warning triangle is the
last thing to go. The project's name keeps a floor of its own and never vanishes.

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
- and when a project appears in that console — added, or moved in from another one — which checks it without waiting
  for the next round.

A console that is not current has none of its projects checked, including one the floating sidebar is only
previewing (see "Previewing a console from the rail" in `docs/product/sidebar.md`); switching to it checks them.
Nothing is checked while no client is connected either: the schedule belongs to the application's window, not to the
daemon. Turning **Automatically sync repositories** on is not a check and is not limited this way; see "Turning the
switch on" below.

The daemon will not start a second check of a project while one is already running, and skips a project whose last
check finished less than a minute ago — so several open windows, each on its own interval, cannot multiply the work. A
project skipped that way simply keeps the status it already had. **The five-minute interval and the one-minute floor
are fixed**, not settings.

A status is never stored: after a daemon restart there is none for any project, and a project's badge shows nothing
again until its next check.

### What one check does

- A directory that is not a git repository is reported as such, and the badge shows nothing for it.
- Otherwise the remote is fetched — **remote refs only**: nothing in the working tree moves and no branch changes.
  The remote fetched is the current branch's upstream remote; with no upstream and exactly one remote configured, that
  one; with no upstream and several, `git`'s own default. A repository with no remote at all is not fetched, and that
  is not an error.
- Then the branch, its upstream and the ahead/behind counts are read from the repository. A fetch that failed —
  offline, authentication, a host key that has never been accepted — records its message for the warning triangle but
  does not stop this read, so the branch and the counts still show.
- Finally the branch may be fast-forwarded; see "Automatically syncing repositories" below.

`git` runs with the user's own shell environment, so it is the `git` on the user's `PATH` and their git credentials
that are used, as for a git association's clone (see "Associating a project" in
`docs/product/consoles-and-projects.md`). It is run so that it can never stop to ask for a credential, a passphrase or
an unknown host key — each of those fails immediately instead — and every `git` call is given at most two minutes, so
a check cannot hang; a call that runs out reports only that it did not finish in time.

## Automatically syncing repositories

Settings' **Git** section (see "Git" in `docs/product/settings.md`) holds one switch, **Automatically sync
repositories**, **off** to begin with. It is stored by the daemon, so every window and every client sees the same
value and it survives a restart.

Off and on alike, the check above runs and the branch and the counts stay accurate. The switch governs what happens
beyond reporting: with it on, a branch that is **behind its upstream and has no commits of its own** is
fast-forwarded — at the end of each check, and once immediately when the switch is turned on (see "Turning the switch
on" below) — and the badge shows the fast-forward in flight.

- It **never pushes**, and never merges anything that is not a fast-forward.
- A branch that is ahead of its upstream, whether or not it is also behind, is left alone. So are a branch with no
  upstream and a detached `HEAD`.
- Whether the project has a running agent session makes no difference: the working tree can move under an agent that
  is working in that project.
- A fast-forward `git` refuses records its message for the warning triangle, as a failed fetch does.

This is the one case in which Octoboard changes anything inside a project's directory — the guarantee it qualifies is
in "What Octoboard never modifies" in `docs/product/launching-agents.md`.

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
message for the warning triangle the same way. A warning the last check left stays on the badge: it stands for that
check against the remote, which a local fast-forward says nothing about. An error this pass records for a project of a
console that is current in no window therefore stays on it until that console is current again and its projects are
checked, since a status is dropped only when the project or its console is deleted.

Turning the switch **off** undoes nothing and triggers nothing: branches already fast-forwarded stay where they are.
