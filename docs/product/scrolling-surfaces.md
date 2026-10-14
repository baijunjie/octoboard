# Scrolling surfaces

What the window does wherever a surface holds more than it can show: how it says so, which of those surfaces a
keyboard can scroll, and what is left out. A single line of text too long for its space is a different thing and
fades under its own rules (see "Names too long for their space" in `docs/product/labels-and-tooltips.md`).

## The edge that hides content fades

Wherever a surface scrolls content out of sight, **the edge it hides that content behind fades out**, so that there is
something to see rather than only something to try. The fade is **24 px** deep everywhere the application draws it; a
strip of tabs is the one exception, and fades more deeply. Which surfaces this holds for, and the scrolling it leaves
alone, are in "The surfaces that fade" below.

- The fade is on **the edge that has more beyond it**: the bottom of a surface at its start, the top of one scrolled
  to its end, and both at once anywhere in between. A surface whose content fits shows none, and an edge stops fading
  as the last of the content past it comes into view.
- It is **a mark alone**. It takes no pointer input, and the surface under it scrolls exactly as any other would: by
  the wheel or a trackpad, by dragging its scrollbar where one is drawn, from the keyboard, and when focus moves to
  something out of view. Content under the fade is still there — still selectable, still pressable — and nothing is
  cut away or made unreachable. It falls over **the content alone**: a scrollbar the surface draws is left out of the
  fade and reads at full strength, as it would with no fade at all — the one along the bottom of a surface that
  scrolls both ways included, which lies inside the fade's band for its whole length.
- In focus mode's lists and the Git mode's change lists, the top fade begins below the section heading; see
  "What both views show" in `docs/product/focus-mode.md` and "Folding a section" in
  `docs/product/project-pane-git-mode.md` for the headings' scrolling behavior.
- Only the **top and bottom** edges fade, apart from the strips that scroll sideways and fade at their sideways
  edges: focus mode's strip of console session chips (see "The switch strip" in `docs/product/focus-mode.md`) and a
  strip of tabs. The file viewer's long lines, read by scrolling sideways while wrapping is off, fade at neither side
  (see "Wrapping long lines" in `docs/product/project-pane.md`).

**Some surfaces draw no scrollbar at all**, so the fade is what says there is more: the rail's column of console
avatars, too narrow for a scrollbar (see "The rail" in `docs/product/window-layout.md`), focus mode's strip of chips,
and a strip of tabs, which also shows a small chevron at each end that has more beyond it, pressed to scroll the
strip that way; the chevrons are not keyboard stops.

## The surfaces that fade

Once it holds more than it can show, each of these fades at its edge:

- the **sidebar's** list of console sessions and projects, a **focus mode's** cards and archived rows, and the
  **archive view's** rows (see `docs/product/sidebar.md` and `docs/product/focus-mode.md`);
- the **rail's** column of console avatars;
- the **welcome screen's** content (see "What the welcome screen shows" in `docs/product/welcome-screen.md`);
- **every dialog's body**, and in Settings the panel holding the section on screen (see `docs/product/settings.md`);
- the **project pane's** file tree (see "The file tree" in `docs/product/project-pane.md`), and the Git mode's change
  list and change tree (see "The change list" in `docs/product/project-pane-git-mode.md`);
- the **file viewer's** reading surfaces — a file shown as plain text, a highlighted file, a diff and a rendered
  Markdown document (see "The file viewer" in `docs/product/project-pane.md`);
- the **lists inside a popover**: the list of branches of each branch selector of the Compare view (see "The branch
  selectors" in `docs/product/project-pane-git-mode.md`), and the tags offered by the project filter (see "Filtering
  the project list" in `docs/product/sidebar.md`);
- the **directory browser's** list of directories (see "Browsing directories" in
  `docs/product/consoles-and-projects.md`);
- a **strip of tabs** whose tabs do not fit, which scrolls sideways: the Git mode's Uncommitted and Compare row (see
  "Uncommitted and Compare" in `docs/product/project-pane-git-mode.md`), and the row Settings' list of sections
  becomes in a narrow window (see "In a narrow window" in `docs/product/settings.md`). **A tab strip fades over 64 px,
  not 24**, and is the one surface where the fade can cover what the keyboard moved to: a tab reached with the arrow
  keys is scrolled only as far as the strip's edge, which is where its fade sits. The fade's size and the chevrons'
  appearance remain the component library's own; the application supplies the chevrons' accessible names in the
  current language. Settings' sections are such a strip only in the narrow window: in the docked layout they stand
  as a column as tall as the sections it holds, however many there are, which therefore never scrolls and carries
  no fade;
- the **screens shown before there is anything to show** (see "The window chrome and the content panel" in
  `docs/product/window-layout.md`): the column of content of the screen for a daemon that could not start or for no
  daemon address being known (see "Starting up" in `docs/product/application-lifecycle.md`), and on the screen for a
  UI that failed both that column and the stack trace in it.

Two kinds of scrolling show no fade:

- **What scrolls as a document of its own**: the terminal's scrollback (see "The terminal" in
  `docs/product/sessions.md`) and a report page, which the console session's model wrote (see
  `docs/product/report-panel.md`).
- **A menu's or a selector's own list of choices** — an action menu, the worktree selector, the account and option
  selects — which scrolls with its scrollbar alone. The Compare view's branch selectors are the exception: the list
  of branches inside each one fades, as above.

## Scrolling a surface from the keyboard

A surface whose own content takes keyboard focus is scrolled by moving through that content: a list by its rows, a tab
strip by its tabs. The file viewer's reading surfaces take focus themselves, whether or not they scroll, because
selecting and copying is theirs as well (see "Selecting and copying" in `docs/product/project-pane.md`).

**Where nothing else on screen would scroll it, the surface itself becomes a tab stop while it scrolls** — a region
named for what it holds, which the arrow keys, Page Up and Page Down then scroll. It is **no stop once its content
fits**, so there is never one that would scroll nothing. Reached with Tab it shows a focus ring, which the fade never
covers. These are:

- **a dialog's body**, named by the dialog's own title. A dialog whose controls are all in its footer — a long
  message, a page of text — has nothing in its body to take focus otherwise.
- **the welcome screen's content area** (see "Keyboard focus" in `docs/product/welcome-screen.md`).
- **the column of content of a screen shown before there is anything to show**, named as the screen's message. Its
  buttons sit at the end of that content and the window itself never scrolls, so a long message — the reasons a
  daemon could not start, say — would otherwise leave nothing between those buttons and the top of the screen to take
  focus. In Tab order it comes before the buttons it holds.
- **the stack trace on the screen for a UI that failed**, named as the stack trace. Nothing else on that screen
  scrolls the stack, so without it the fade at the stack's edge would be saying there is more where a keyboard could
  not go. In Tab order it comes before Try again, and after the screen's column where that is a stop too.

**Keyboard focus does not land under a fade**: a row, a control or a region scrolled into view stops 24 px short of
the edge, clear of the fade, rather than arriving underneath. In focus mode's lists and the Git mode's change lists
that space is measured below the section heading (see "What both views show" in `docs/product/focus-mode.md` and
"Folding a section" in `docs/product/project-pane-git-mode.md`). A strip of tabs is the exception, as above.
