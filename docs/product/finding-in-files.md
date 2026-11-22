# Find in files

**Find in files** searches the contents of a project's files on disk, in the project's directory or in one folder of it,
and lists every matching line. It opens from the project pane's Files panel (see `docs/product/project-pane.md`) in a
modal of its own, and a result opens in the file viewer at its line. It reads the live files, as the Files mode does,
and changes nothing. It is not the file-name filter, which matches names only (see "Filtering by file name" in
`docs/product/project-pane.md`), nor Find in file, which searches the one file the viewer shows (see
`docs/product/finding-in-the-file-viewer.md`). Some languages give it a name of its own, such as 「搜索文件内容」 in
Simplified Chinese, so that it is never named like Find in file.

## Opening it

- **The header's Find in files button**, an icon button in the Files panel's header after the file-name filter toggle,
  in the Files mode only (see "Opening a project's files" in `docs/product/project-pane.md`), searches the whole
  project.
- **The Find in files shortcut**, `Shift+Cmd+F` on a Mac and `Ctrl+Shift+F` elsewhere unless the user has changed it
  (see `docs/product/keyboard-shortcuts.md`), searches the whole project of the Files panel on screen, docked or
  floating in, from anywhere in the window but while a dialog or a menu is open. In the Git mode it switches the panel to the Files mode first. With no Files
  panel on screen it does nothing. The key never reaches the terminal; holding it down opens the modal once.
- **Find in folder**, the first item of a folder's action menu in the Files mode — in the tree and in file-name filter
  results alike, set apart from the items after it (see "A row's actions" in `docs/product/project-pane.md`) — searches
  only that folder and everything beneath it. The modal then names the folder in its scope line (see "The modal"
  below), and offers no way to remove or change it; searching the whole project is opening the modal from the button
  or the shortcut. The Git mode's change list has no Find in folder.

**The query and the options are kept**: opening the modal again shows the last search, run afresh. They last as the
file-name filter's choices do — while the project's pane stays in the right pane, through tab switches and the pane
being hidden — and are not shared across projects (see "Filtering by file name" in `docs/product/project-pane.md`).

## The modal

The modal is the file viewer's size (see `docs/product/file-viewer.md`). Under its title is the **scope line**, always
shown, which says what is searched: the project, by its name, and for a search limited to a folder the folder's path
after it. The project and the folder are each shown as a reference, joined by a chevron as the title bar's breadcrumb
is; assistive technology hears a slash in the chevron's place. The **count** is at the end of the same line. The line is
always one line of text: a scope too long for the room the count leaves fades out at its end and carries its full
wording as its tooltip, as the file viewer's subtitle does, and the count's spinner and warning mark never make the
line taller, so the fields below never move as the count appears and goes.

Below the header are the **query field** and the **file mask field** with the include-ignored toggle beside it, side
by side where they fit and the mask wrapping under the query where they do not, so that the mask is never cut narrower
than its placeholder. Under them is the **result list**, and under the list the **preview** of the selected result. At
a window width of 768 px or less the modal fills the screen and shows no preview: the fields and the list take it
all.

### The query and its options

The query field holds three toggles at its end, the same as the find bar's (see "The find bar" in
`docs/product/finding-in-the-file-viewer.md`): **Match case**, **Whole word** and **Use regular expression**, all off
when the modal is first opened. With them off, the query is literal text matched regardless of case.

- **A regular expression** is in the syntax of the Rust `regex` crate, which differs from JavaScript's: it has no
  look-around and no backreferences. A file is searched line by line, so a pattern that matches a line break finds
  nothing and is refused (see "What the modal says" below), and a match never runs from one line into the next. A line
  ending of `\r\n` ends a line as `\n` does, so `$` matches before either.
- **A match is never empty**: a pattern that can only match nothing, such as `^`, finds no line. An empty query searches
  nothing and shows nothing.

### The file mask

The file mask limits which files are searched: **comma-separated patterns**, spaces around each ignored; empty, every
file is searched. Its placeholder shows an example, `*.ts, !*.test.ts`, and assistive technology hears these rules as the
field's description.

- **A file is searched when it matches one of the patterns** that do not start with `!`, when there are any, and **none
  of those that do**: `!*.test.ts` leaves out the files it matches, and a mask of `!` patterns alone searches every
  other file.
- **A pattern with no `/` matches the file's name**; one with a `/` matches the file's path within the project, even in
  a search limited to a folder, a leading `/` dropped.
- `*` and `?` do not cross a `/`, and `**` does. Matching is case-sensitive.
- A comma always separates patterns, braces included, so `*.{md,ts}` is two broken patterns and is refused, with a hint
  that commas separate patterns; `*.md, *.ts` is the way to write it.

The field reads left to right while it holds text, whatever the interface's language.

### Ignored files

**Include ignored files**, the icon toggle at the end of the file mask's row, is the file-name filter's (see "Filtering
by file name" in `docs/product/project-pane.md`), and is off when the modal is first opened. While it is off, the search
leaves out what Git ignores in a project inside a Git repository: every `.gitignore` level, the repository's
`.git/info/exclude` and the user's global excludes file, with everything inside an ignored folder. Unlike the tree's
dimming (see "Ignored entries" in `docs/product/project-pane.md`), **a file Git tracks is left out too when a pattern
names it**. A `.gitignore` that cannot be read or understood has its rules left out, and the search is not marked
incomplete for it.

A search limited by Find in folder to a folder Git ignores searches that folder all the same, since it was asked for;
below it, the ignore rules still leave out what they name.

### When it searches

Typing in the query or the file mask searches once the last keystroke is **250 ms** old; pressing a toggle searches at
once. A new search replaces the one before, which stops, and the list starts again from empty. A search lost with the
connection to the daemon is made again once the connection is back.

## What is searched

- **The project's directory**, or the folder Find in folder named, read afresh on every search: there is **no index**.
  Code editors such as VS Code search this way; a warm-cache scan of an ordinary project, its ignored folders left out,
  takes a fraction of a second, while an index would have to be built, kept up to date with every change on disk and
  held in memory. The first search after the files were last read can be much slower on an external or network drive,
  which the disk alone decides.
- **Dot files** are searched. The names the file tree leaves out — `.git` and the files an operating system leaves in
  folders — are left out with everything beneath them (see "What is listed" in `docs/product/project-pane.md`).
- **A symbolic link is never followed or searched**, so a search never leaves the project, and a file a link points at
  is found under its own path. Find in folder on a folder with a link anywhere on its path is refused, and the modal
  shows that failure.
- **A binary file** — one in which a NUL byte turns up — yields nothing, whatever it matched before the NUL.
- **A file over 50 MiB is skipped** without the search being marked incomplete. A file with a line longer than 2 MiB
  cannot be searched, and the search is marked incomplete.

## The results

**The count** reads, for example, "12 matches in 3 files": every match counts, so a line holding two counts two. While
the search runs, it grows as results arrive, with a spinner beside it.

**The result list** has one row per matching line, in the order the search finds them, not sorted by file or line.
Results are added at the end as they arrive, and the selection and the scroll position stay where they are. A row
shows:

- **the line**, in a monospace font, reading left to right whatever the interface's language, with its leading
  indentation left out and every match on it highlighted the way the find bar's current match is — a fill, a text colour
  of its own and an underline (see "The matches" in `docs/product/finding-in-the-file-viewer.md`). A line too long
  for the row is cut before its first match, with a leading ellipsis, so that the match stays in view with a little of
  what comes before it; an ellipsis at the end says the line goes on. The search sends at most about 250 characters of
  a line, starting 40 characters before its first match;
- after it, the file's name and the line number, as `server.ts:42`, and, at the widths that show no preview, the file's
  folder too, so results in files of the same name are told apart. A name or folder too long fades out, with the whole
  of it as its tooltip.

Assistive technology hears each row as its line, the file's path and the line number.

**Selecting and opening.** The first result is selected until another is chosen. Down in the query moves keyboard focus
into the list; there the arrow keys move the selection and a click selects. **Enter** or a double click opens a result,
and Enter in the query opens the selected one.

### The preview

Under the list, the **preview** shows the selected result's file: headed by the file's name and its folder, read-only,
as code with line numbers, the result's line scrolled to the middle with its matches marked as the file viewer marks
them (see "Opening at a line" in `docs/product/file-viewer.md`). A Markdown file shows its source. A file that is not
text, or cannot be read, shows what the file viewer would. Its code is selected, copied and quoted as the file viewer's
is, with the same text menu (see "Selecting and copying" in `docs/product/file-viewer.md`), except that Quote writes
the quote at once and the dialog stays open, with its selection kept: nothing is uncovered and keyboard focus is not
moved to the terminal. A result in another file is shown once the selection has
rested on it for **150 ms**, so moving quickly through the results reads only the file it stops on; moving within one
file shows the new line at once.

**The divider** between the list and the preview gives one the other's room: dragged, or from the keyboard with the
arrow keys, Home and End; a double click puts back the even split. Each keeps at least 80 px. The split is kept as a
share of the height, so it holds as the window is resized, and remembered per client, in that client's own browser
storage, across searches and restarts.

### Opening a result

Opening a result closes the modal and opens the file in the file viewer at the result's line, with its matches marked
(see "Opening at a line" in `docs/product/file-viewer.md`). The viewer is shown over the window, as one opened from a
terminal link is: the Files panel is left as it is, no tree row is selected, there is no Previous file or Next file,
and the file is not read again as it changes on disk (see "Opened from the terminal" in
`docs/product/file-viewer.md`). Closing the viewer returns keyboard focus to the header's Find in files button, while the
Files panel still shows it.

## What the modal says

- **Searching**: before the first result, the list's place says the search is running; once results arrive, the
  spinner beside the count. A count of zero is never shown while it runs.
- **No matches**: the list's place says so.
- **Incomplete**: when the search stopped at one of its bounds (see "Bounds" below) or could not read some files or
  folders, an amber warning mark follows the count, named that the search is incomplete. Its popover says why — the
  search took too long, reached the most matches it lists, read as much as it reads at once, found more than can be
  sent, or could not read some files or folders — and that not every match may be listed. It opens as a row's mark does
  (see "Failures, warnings and notes marked at their point" in `docs/product/error-alerts.md`). With no matches, the
  list's place says there are none and that not every file was searched, and the count and its mark stay in the scope
  line.
- **A refused query or file mask**: a regular expression or a pattern of the mask that is not valid is said under its
  own field, in plain words, and the field is marked invalid; nothing is searched. A regular expression's refusal names
  where it fails as the character it is at, counted from 1 as the user sees the characters (an emoji counts as one) —
  for example, that the parenthesis at character 8 is never closed. A look-around or a backreference, and a pattern
  that matches a line break, are each said in words of their own. A mask's refusal names the pattern it refuses.
- **A failure**: in the list's place, an empty-state failure headed that the file contents could not be searched, with
  the reason and **Try again**, which searches again, keyboard focus going to the query (see "Failures shown on the
  screen" in `docs/product/error-alerts.md`).

When a search ends, assistive technology is told the count, or that nothing matched, and whether the search is
incomplete. A refusal is announced as it appears under its field.

## Bounds

| Bound | Value | When it is reached |
|---|---|---|
| Time | 10 seconds | the search stops, marked incomplete |
| Matches | 20,000 | the search stops, marked incomplete; the line that would pass it is not listed |
| Read | 2 GiB of files | the search stops, marked incomplete |
| Sent | 8 MiB of results | the search stops, marked incomplete |
| File size | 50 MiB | the file is skipped; the search is not marked incomplete |
| Line length | 2 MiB | the file cannot be searched; the search is marked incomplete |
| Line shown | about 250 characters, 40 before the first match | the rest of the line is not sent |

A filesystem operation that stalls, such as a read from a volume that stops answering, can hold a search past its time.
