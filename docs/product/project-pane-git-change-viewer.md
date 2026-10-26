# Project pane Git change viewer

A change listed in the project pane's Git mode (see `docs/product/project-pane-git-mode.md`) opens in the project
pane's read-only file viewer (see `docs/product/file-viewer.md`). This doc is what the viewer
shows of a change, how its diff expands, how the viewer moves through the list, and what happens when the change
changes or cannot be read.

## Opening a change

A change opens in the project pane's file viewer (see "The header" in `docs/product/file-viewer.md`). Its title is the
file's name, with the kind of change after it as a tag — Added, Deleted, Modified, Renamed or Type changed — coloured
as the change's letter in the list is (green, red, amber, blue; the same colours for the same kind). A staged or
unstaged change has its stage — Staged or Unstaged — as a neutral tag before the kind, since a neutral qualifier goes
before the coloured status: the two things a change says, whether it is staged and what kind it is, are kept apart,
the first neutral and the second coloured. Both tags come after the name, and read with it as "server.ts: Unstaged,
Modified". An untracked file is always new, so its only tag is Untracked, in the neutral grey the list's U has, which
is no kind of change and shares no colour with Added or Deleted; a path in conflict has only a Conflicted tag,
coloured as its C is. The Conflicted tag's tooltip names the kind of conflict (see "What it shows" below) — "Both
modified." — opened by a resting mouse and, on a touch screen, by a tap; assistive technology hears the same sentence
as part of the title, after the tag — "merge.md: Conflicted. Both modified.". Neither has a stage tag, its status tag
already saying where it is from; a committed change has none either. The tags read as one group, spaced closer to each
other than the group is to the name, and each of them, the neutral stage tag included, is visibly a tag against the
viewer's background in either appearance. The tags never shrink; the name fades out when there is no room for it.

A committed change carries the commits it is read from in the header's **subtitle**, the line directly under the
title, each commit by its first seven characters:

- **From the Committed section listed together**: the branch, the base branch and the commit the two forked at, then the
  commit the branch is at — "feature forked from main at 4f2a9c1, now at 9e8d7c6", say, or, with a remote branch as the
  base, "main forked from origin/main at 4f2a9c1, now at 9e8d7c6", in the same words as the list's own line about the
  fork. For a branch listed with its whole history (see "Committed changes" in
  `docs/product/project-pane-git-change-list.md`), the branch and the commit it was at, with all of its history.
- **From under a commit's row**, grouped by commit (see "Grouped by commit" in
  `docs/product/project-pane-git-change-list.md`): the commit and
  the parent it is compared with — "Commit 9e8d7c6, compared with its parent 4f2a9c1" — or, for a commit with no
  parent, that it has none.

The full wording is that line's tooltip where it is cut off (see "The header" in `docs/product/file-viewer.md`), and
each branch name in it is laid out in the direction of its own text (see `docs/product/right-to-left-layout.md`). An
uncommitted change has no such line.

Under that are the change's details, laid out as the viewer lays out any subject's (see "The header" in
`docs/product/file-viewer.md`): the path within the project and, for a rename, the path it was renamed from.

A committed change is read from two commits, and is shown as a staged change is below; it is never untracked or in
conflict. Listed together, its old side is read from the commit the branch and its base forked at — the empty tree
for a branch listed with its whole history, every change of which is an addition — and its new side from the branch's
tip, both as the comparison on screen has them; from under a commit's row, its old side from
the commit's first parent — the empty tree for a commit with none — and its new side from the commit.

## What it shows

- **A text change** is a diff, drawn from the patch Git makes for it: unified at first, or split into its two sides
  side by side, chosen among the view controls in the viewer's header. The choice is offered only for a diff with
  lines on both sides: an added or a deleted file reads the same either way, so it has none. It is
  remembered: later diffs open in the layout last chosen, across changes and after the app restarts. Only the patch's
  hunks and the lines around them are shown, each run of unchanged lines between them collapsed into a separator that
  can be expanded (see "Expanding the collapsed lines" below). A change whose patch has no lines — an empty file added
  or removed, say — says it has no diff to show; one whose two sides hold the same content, under one name or two,
  shows the file whole instead, as the bullets below say. Whether its long lines wrap is the viewer's own choice, which
  a diff shares with every file (see "Wrapping long lines" in `docs/product/file-viewer.md`). A patch of more than
  1,000,000 characters or 10,000 lines is shown as the plain patch, with a notice saying it is shown that way because
  the change is large; a patch the diff could not be drawn from is shown the same way, with a failure notice saying
  so (see "Notices and stand-ins" in `docs/product/file-viewer.md`). A patch that is not valid UTF-8 — of a file in
  another encoding — is shown with replacement characters.
- **A missing newline at the end** of either version is said in a notice — the old version, the new
  one, or neither has one — rather than as a line of it. A symbolic link's change never says so, a link's target
  having no newline at its end anyway. A plain patch keeps Git's own marker lines.
- **An image change** — of a binary image format the file viewer displays (see "What it shows" in
  `docs/product/file-viewer.md`); an SVG, being text, is a text change — shows the two images, Before and After, side by
  side where the viewer is wide enough (Before on the left, mirrored under a right-to-left language, see
  `docs/product/right-to-left-layout.md`) and one above the other otherwise. A side with no file says so. Each side is
  headed Before or After, and what is said about one side's own content stands beside its heading rather than in the
  viewer's header (see "Notices and stand-ins" in `docs/product/file-viewer.md`).
- **Any other binary change**, or an image changed to or from text, says it cannot be shown as a diff, with the size
  before and after.
- **An untracked file** is shown as new content: the file itself, as the file viewer shows a file; its Untracked tag
  says there is no earlier version to compare it with. An untracked symbolic link, or an untracked repository of its
  own, says what it is, having no content to show.
- **A rename across the project's boundary** shows only the version inside the project, as a file rather than a diff,
  under a warning that the previous or the new version is outside the project, with its path from the repository's
  root. Nothing of the version outside is read.
- **A type change** — a file that became a symbolic link, or the reverse — is one diff, with the choice of unified or
  split and the layout remembered, as a modified file's is. Git writes such a change as the old side's removal and the
  new side's addition, and the diff holds the two together: the whole old side as removed lines, then the whole new
  side as added ones. **The two sides are never compared line by line** — a file's lines and a link's target have
  nothing to pair up — so each side is shown whole, with no run of unchanged lines: the diff collapses nothing and has
  no separator to expand. It offers the layout choice by the rule above, so an empty file that became a link, having
  added lines alone, offers none. The kind of change stays in the title's tag. A type change into or out of a path
  below itself (`lib` to `lib/index.ts`) is shown this way as well, not as the rename below it is.
- **A rename into or out of a path below itself** (`lib` to `lib/index.ts`, or back) is compared line by line like any
  other change: its diff is the one between exactly its two versions. Identical content therefore says the change was
  renamed and nothing more, as the bullet below it says, and edited content gives an ordinary diff — hunks with context,
  the runs between them collapsed into separators that can be expanded, the layout choice, Show whole file and the run
  beyond the last hunk, all as above. It shows under Renamed, with the path it came from among the details, and nothing
  below the outer path is read, named in its diff or counted against the limits the read is held to. This holds in the
  Staged section and for a committed change, in both directions of the nesting, and it takes both sides being a file or
  a symbolic link: with a side that is a repository of its own the change is shown instead as the old path's removal
  followed by the new path's addition in one diff, collapsing nothing and with no separator to expand, as a type change
  is; each side shows only the commit it records, nothing from inside the nested repository. Two submodules are listed
  as one renamed change only when both record the same commit (see "What is listed" in
  `docs/product/project-pane-git-change-list.md`), so that is the only way a nesting pair of them is ever opened.
- **A rename with nothing else changed** says it was renamed and its content is unchanged, and shows that content as
  below. It is the wording of every rename whose patch shows nothing but the path changed — an empty patch, or one
  holding the rename's header alone — wherever it was listed and whether or not its two paths nest. A rename whose
  patch shows that something else did change, its executable bit or a side Git treats as binary, is not worded that
  way: it says it has no diff to show, as any change with no lines does.
- **No differences**, when the two sides of a change that is not a rename now hold the same content: the change was
  staged, committed or undone since it was listed. Its content is shown as below as well.
- **A path in conflict** is not a two-sided change: it shows the file as it is on disk, with its conflict markers where
  both sides modified or added it, and the kind of conflict — both modified, both added, both deleted, added by us,
  added by them, deleted by us, deleted by them — is said by the title's Conflicted tag (see "Opening a change" above),
  with no notice of its own. A file in conflict whose content cannot be read shows the viewer's error alone (see
  "Notices and stand-ins" in `docs/product/file-viewer.md`). A path both sides deleted has no file to show and says it
  does not exist.

**A change with no lines to draw shows the file itself.** For the two cases above — the same content under one name or
two — there is nothing to compare, so the whole content is read instead, the read a diff's collapsed lines use (see
"Expanding the collapsed lines" below), once as the change opens; while it is in flight the viewer shows its usual
loading, announced as a file's read is (see "What it shows" in `docs/product/file-viewer.md`). It is then shown whole
as a file rather than as a diff — so a Markdown file is offered as a document with its source a choice away, exactly as
an untracked file is — with a note saying which of the two cases the change is, in one sentence. Where the
content cannot be had after all, the change falls back to the centred stand-in and shows none of the file, saying there
the same state the notice would have: as a heading, with the line under it where it has one, rather than as that one
sentence (see "The same state said in one sentence" in `docs/product/empty-states.md`). Where the read itself failed —
a lost connection to the daemon among the reasons — a warning notice, not a failure, since the stand-in is still true
and only the content is lost, says the file's content could not be shown and goes on with why, in Octoboard's words;
a failure that is unexpected gives no why at all. What the system or Git said is not in that sentence but under it,
in the mark's popover. Where the content simply cannot be had — a side the daemon will not deliver
whole, by the same rule as a diff's collapsed lines, or the change having moved on since it was read, or the two sides
turning out to differ from what the change says they are — a note says which of those it was. Only a
change with nothing to read the content from leaves the stand-in alone: one whose two sides are not both files whose
content can be read, a rename between two symbolic links among them. Every one of these notices and the stand-in are as
"Notices and stand-ins" in `docs/product/file-viewer.md` describes them.

A whole file body shown here instead of a diff — an untracked file, one added with no patch, the version inside the
project of a rename across the boundary, or a change with no lines to draw whose content was read — is a Markdown
document when the file is Markdown, as it is for a file read for itself (see `docs/product/markdown-documents.md`).
Following a link in that document to another project file leaves Git mode, as that section describes. A diff and the
file behind a path in conflict are shown as source whatever the file is.

How a diff is coloured in each appearance is in "What follows the choice" in `docs/product/appearance.md`. A diff's
code is focused, selected and copied as a file's is (see "Selecting and copying" in `docs/product/file-viewer.md`).

## Expanding the collapsed lines

Each run of unchanged lines a diff leaves out stands in it as a **separator** saying how many lines it holds — "6
unmodified lines" — in the current language (see "What follows the language" in `docs/product/language.md`); once part
of a run has been revealed, its separator counts the lines of it that are still hidden. Where the lines can be shown,
the separator reveals them, in every section of the change list and in both diff layouts:

- **One expansion reveals 20 lines.** A run between two hunks longer than that offers two controls: one reveals the 20
  lines at the run's top, which appear above the separator, the other the 20 at its bottom, which appear below it. A
  shorter run, and the run before the first hunk or after the last, offers one. The **third expansion of the same run**
  reveals whatever is left of it, whichever of its controls is used.
- **Show whole file**, at the end of a separator, reveals the file's every line at once, leaving no separator behind.
  Each run offers it once — in the split layout on one side only. With wrapping off it stays at the visible end of the
  bar as the diff is scrolled sideways, as the separator's text and controls do.
- **A separator's text is a control of its own**, which **reveals the whole of its run at once**, however long the run
  is and however many of its lines the other controls have revealed already. It is named for that, with the count it
  shows — "Show all 53 unmodified lines" — so what it offers, and not only how many lines are hidden, is announced;
  on a separator whose count came from the file's length rather than from the patch the name carries no count. It is
  a tab stop and is used with Enter or Space like the other controls. The text of a separator that offers no
  expansion is inert.
- **The run beyond the last hunk** is a separator of its own, with the one control such a run has. A patch never says
  how long a file is, so the run's length comes from the length of the file the change was read at: where the last hunk
  reaches the file's end there is nothing beyond it and **no separator is drawn at all**; where lines do follow, the
  separator counts them as every other one does. Where the length is not known, it says instead that more unmodified
  lines may follow.
- **The lines are read only when the user expands.** Nothing of them comes with the patch, with a refresh, or when the
  connection to the daemon comes back. The first expansion of a diff reads both of its sides whole, and every later
  expansion of the same diff uses what that read returned. The read writes nothing to the repository and takes no
  lock, as nothing in the Git mode does (see "Reading only" in `docs/product/project-pane-git-mode.md`).
- **An expansion lasts as long as the diff is on screen.** A change opened again, or read again at other versions, is
  drawn with its lines collapsed again.

While the lines are being read the separator says so, and no other expansion starts until that read is answered: every
control of the diff reads as unavailable meanwhile. That, and each notice below, is announced to assistive technology.
Then:

- **A failed read** — but for one that finds the change's worktree gone, which closes the viewer (see "When the open
  change changes" below) — leaves the diff as it is: the separator says the lines could not be loaded and to try again,
  a failure notice says the unmodified lines could not be read and why, in Octoboard's words — that the connection to
  the daemon was lost, among them, and that something unexpected went wrong for a failure that is unexpected — with what
  the system or Git said under it in the mark's popover, and the next expansion reads again. A read of the change itself
  that follows — on a listing with news of it, or once the connection to the daemon is back — clears the failure without
  another expansion.
- **A change that has moved on** since its patch was read, its file or its index having changed underneath, keeps every
  line the patch shows, with a notice saying its unmodified lines cannot be shown, and gives the
  expansion up as below; it is offered again only once the change is read again (see "When the open change changes"
  below). A committed change, read from two fixed commits, cannot move on this way.
- **Moving on to another change** while the lines are being read gives that read up, saying nothing.

**A diff that offers no expansion** keeps its separators with their counts and nothing else: no controls, and no
separator for the run beyond the last hunk, which only an expandable diff has. That is so from the start for a diff
whose two sides are not both files whose content can be read — an added or a deleted file, which has one side only, a
symbolic link or a submodule, hence a type change, and a version outside the project. A type change has no separator
at all, its diff holding both of its sides whole so that nothing in it is collapsed (see "What it shows" above). The
rest is found only on the first expansion, and the diff is then drawn again in that form, so the separator for the run
beyond its last hunk goes on that press: a side the daemon will not deliver whole — one larger than the 4 MiB a read
opens, a file type it does not read, a side read from the worktree at a path with a clean filter in effect (see
below), a change it cannot make sense of — a side past the viewer's own budget of 10,000 lines or 1,000,000
characters, and the change having moved on. Each of the three adds a notice, and each is announced:
that the whole file is not delivered for this change, that the file is too large to be shown in full here, or, as the
bullet above says, that the change has moved on since it was read.

**A path with a clean filter** is one Git filters on its way into the repository — a `filter` attribute naming a
driver configured with a `clean` or a `process` command, as Git LFS sets up for every path it tracks. The patch's
hunks are numbered from what that filter produces rather than from the bytes the file on disk holds, so the daemon
delivers no side read from the worktree at such a path. Only an unstaged change has such a side: a staged change and
both sides of a committed change are read from the repository, which holds the filtered content already, and
expand as any other change does, and an untracked file is shown as itself with no diff to expand (see "What it shows"
above). A `filter` attribute whose driver has neither command configured filters nothing, so that path expands
normally too. Where such a change has also moved on since it was read, the notice is the one for having moved on.

From the keyboard, **Tab** from the code region moves into the separators' controls and then along them, run by run
from the top of the diff and, within a run, in the order they are read — the control or pair that reveals part of the
run, the separator's own text, then Show whole file — and after the last one leaves the diff; **Shift+Tab** from the
first goes back to the code region. **Enter** or **Space** uses a control. Whenever the control used had keyboard focus
— pressed with Enter or Space, or pressed by a screen reader such as VoiceOver on the control it is on, which arrives
as a click — keyboard focus is given back once the diff is drawn again: on that control, or on the nearest control left
where that one is gone, or on the code region where none is left at all — after Show whole file, and after a diff
turns out to offer no expansion. A control reached from the keyboard shows a focus ring, in either appearance, and so
does the code region when focus falls back to it. A mouse press never moves focus onto a control, so pressing one that
did not have focus leaves keyboard focus where it was.

## Moving between changes

**Previous file** and **Next file**, and the **Left** and **Right** arrow keys, move through the change list the change
was opened from as it is on screen — uncommitted and committed changes alike — top to bottom, from one section into the
next, and stop at either end rather than going round. They follow the rows as shown: a change the filter hides is
skipped (see "Filtering by file name" in `docs/product/project-pane-git-change-list.md`), and so are the changes in a
folded section (see "Folding a section" in `docs/product/project-pane-git-change-list.md`), those under a folded commit
row (see "Grouped by commit" in the same doc) and, grouped by folder, those under a folded folder. A file in both Staged
and Unstaged is two stops, as is a file both uncommitted and committed, and, grouped by commit, a file several commits
changed is a stop under each of their commit rows. A change with no row of its own — one the list no longer has — keeps
its place in the order, and the keys move on to the changes that surround it. When the keys do nothing, and how they
mirror under a right-to-left language, is as in "Moving between files" in `docs/product/file-viewer.md`.

## Closing it

**Escape** or the viewer's **Close** button closes it. Keyboard focus then goes to the row of the change it showed
last.

## When the open change changes

For an uncommitted change, the listings that keep the change list up to date (see "Keeping the list up to date" in
`docs/product/project-pane-git-change-list.md`) also decide when the open change is read again:

- a listing with news of it — the change listed at other versions (a file overwritten, staged again or committed), or
  gone from a list that is not cut short — reads it again, and it shows as it is now;
- a change that kept changing while it was read, after two more tries, says so, and is read again at each new listing
  that still has it, until a read succeeds;
- a change whose worktree is found gone, by its list or by the worktrees read again, is read again from that
  worktree;
- a change from a worktree the branch is no longer found checked out in counts as gone from the list, and is read
  again;
- **a read that finds the change's worktree gone closes the viewer**, saying nothing and opening no error alert, as a
  committed change the list no longer has closes (see below) — whether it read the change, a path in conflict, or the
  lines a diff is expanded from, and even where the branch is checked out in another worktree by then: another
  worktree's change is never shown in its place.

A committed change, its two commits being fixed, is not read again when the branches are read. When the comparison is
made again at other commits while a change from the Committed section listed together is open (see "Keeping the list up
to date" in `docs/product/project-pane-git-change-list.md`), the change is read again from the new commits if the new
comparison lists it; if it does not, there is no such change between those commits: the viewer closes, no change is
selected, and keyboard focus, unless it is somewhere outside the list and the viewer, goes to the first row of the list,
or to the branch selector when the list has no rows. A change from under a commit's row stays that commit's whatever the
branch does; when the commits are listed again for a new comparison and no longer have it, the viewer closes the same
way. The viewer never shows a change from other commits than the list's.

When the connection to the daemon is lost, what is shown stays until the connection is back, and then the change is
read again; a change that had nothing on screen yet says, in the neutral stand-in rather than as an error (see
"Notices and stand-ins" in `docs/product/file-viewer.md`), that the connection was lost and that it is read again
once the connection is back.

## Errors

A change that cannot be read keeps what the list said of it — its section, its status and its paths — with the reason
in place of the diff, in the current language. The cases beyond those of a file (see "Errors" in
`docs/product/project-pane.md`, which also apply to a path in conflict):

| Situation | What the user is told |
|---|---|
| Its patch is larger than 4 MiB | it is too large to show as a diff, its patch being more than can be opened |
| A file it has to show whole (an untracked file, the inside version of a rename across the boundary, a binary side) is larger than 4 MiB | its size, and the most that can be opened |
| It kept changing while it was being read, after two more tries | it kept changing, and is read again at the next refresh |
| A committed change's commit is one the repository no longer has (a deleted branch's, once Git has pruned it) | the repository has no such commit |
| A committed change's content is missing from the repository | Git could not read it, with Git's own message behind Details |

A change whose worktree is gone is none of these: the viewer closes instead (see "When the open change changes"
above).
