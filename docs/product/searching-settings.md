# Searching settings

The Settings dialog (see `docs/product/settings.md`) has a search field that narrows the sections and their rows to the
settings matching what is typed.

## The search field

A search field named **Search settings**, with the placeholder "Search" and a clear button. Above 768 px it sits at
the top of the sections column, above the list of sections; at 768 px and below it is the dialog's top row, above the
row of section tabs, with the dialog's close button at its end. It comes before the section tabs in the Tab order.
Opening Settings still puts keyboard focus on the selected section's tab, not in the field (see "Moving between
sections" in `docs/product/settings.md`).

**Find** — `Cmd+F` on macOS, `Ctrl+F` elsewhere, matched by its physical key — pressed anywhere in Settings moves
keyboard focus to the field and selects the text already in it, keeping the query; pressed in the field itself it
selects its text again. The page takes the key, so a browser's own find does not open. A key an input method is
composing with is left alone, and while a dialog is open in front of Settings the key does nothing here. The key is
fixed (see "What is listed" in `docs/product/keyboard-shortcuts.md`).

`Escape` in the field while it holds text clears it and leaves Settings open; `Escape` in the empty field closes
Settings as everywhere else in it, so a second `Escape` closes after the first has cleared the field. The clear button
also empties the field.

## What a query matches

A query is matched in the current language of the window, ignoring case and the spaces around it; a query of spaces
alone searches for nothing. It is split into words at spaces, and **a setting matches when every word is found
somewhere in its text**, each word on its own and in any order. A setting's text is:

- its name;
- its line of text — **every wording that line can take**: both of a switch's wordings, on and off, and the wording of
  each choice of a select, whichever is shown now. Changing a setting while searching therefore never takes its own row
  away;
- the heading of the group it sits in (see "Group headings" in `docs/product/settings.md`);
- the name of its section, so a section's name finds every setting it shows.

A "?" help tip's text is not searched. A row the section shows only on some clients or under some condition — the rows
only the desktop application has, **Prevent sleep** where the daemon's host can be held awake and **When to prevent
sleep** while it is on, the Git rows shown while sessions work on a branch or in a worktree of their own, the
Permissions rows, the Configuration rows where files can be saved and opened — matches only while it is shown.

**Rows that wait on a read of the system** — **Launch at login** in App behavior and the Screen Recording and
Accessibility rows in Permissions — appear only once Settings has read the system's state, and the search counts and
lists them only from that answer on, so a row and its count appear together and a count never takes in a row that is not
on screen. A launch-at-login read that fails still shows the row, off, and it is counted. When the reads are made is in
"The switch shows the system's state" in `docs/product/launch-at-login.md` and in "A change made outside Octoboard is
picked up when the window comes back" in `docs/product/macos-privacy-permissions.md`.

Some parts of Settings are found differently:

- **Keyboard shortcuts**: each command the client lists is a setting of its own, found by its command's name and its
  group's heading. Its keys are not searched.
- **Agent accounts**: its three app-wide switches are rows like any other. The agents and their accounts — the agent
  tabs with every account under them, or the empty state when no agent is added — are one block, found only by the
  section's name or by "Agents" (in the current language), never by an agent's or an account's name.
- **Trusted folders**: the list of folders is one block, found by the section's name or by the words of the section's
  opening paragraph, never by a folder's path.
- **Permissions, when its read fails**: the alert saying the permissions could not be read, which the section shows in
  place of the two rows (see `docs/product/macos-privacy-permissions.md`), is one block. It is found by its own title
  and text, by the names and descriptions of the Screen Recording and Accessibility rows it stands in for, and by the
  section's name, so a search for a permission finds the alert, counted as one, rather than saying no setting matches.
- **Notifications** and the Updates section's **Version** row are found by their name only, since their line of text says
  where things stand rather than what the setting does. So are **Website**, **GitHub repository** and **Issue
  tracker**, whose addresses are not searched.

The user's own data — account names, directories, folder paths — is never searched.

## While a query is entered

- **The list of sections keeps only the sections with a match**, in their usual order, each tab showing how many of the
  section's settings match in a count badge (a block counting as one). Its name for assistive technology is the
  section's name followed by that count — "Git, 3 matching settings". A section name too long for the column fades out
  before the badge, which stays in view.
- **The section on screen shows only its matching rows and blocks.** A group with no matching row is hidden, heading and
  all. The section's own heading stays.
- **The section on screen stays while it is listed.** A re-match the user did not make — the language changing, a
  setting changed from another window, a read of the system answering — never moves it. Only once it is no longer
  listed does the first listed section take its place, and that section then stays in turn, even when the earlier one
  is listed again. Pressing a section's tab always shows that section. While nothing matches anywhere, the section on
  screen stays as it was, hidden. Clearing the field keeps whichever section is on screen.
- **With nothing matching anywhere**, the tabs and the section give way to an empty state saying no setting matches.
- The dialog tells assistive technology politely how many settings match across every section, or that no setting
  matches the search, as the result changes; the empty state itself is not read, being said by that announcement (see
  "A state another region already announces" in `docs/product/empty-states.md`). Nothing is announced while the field
  is empty.

Tabs and rows that a query takes away or brings back leave, enter and slide with the collection motion (see "Items
moving within a collection" in `docs/product/collection-motion.md`).

A row or block a query hides is only hidden: whatever it holds is kept, so a half-typed value in a hidden field, or in
the section hidden while nothing matches, is still there when a query shows it again.

A request to open Settings on a section, on the Add agent dialog or on an account while Settings is open (see "Opening
and closing Settings" in `docs/product/settings.md`) clears the query, so what it asks for is shown whole; one that
names no section keeps the section on screen.

A control holding keyboard focus that a new match hides — after the language changes, or a setting is changed from
another window — hands focus on as "Moving between sections" in `docs/product/settings.md` says.
