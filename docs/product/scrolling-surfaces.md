# Scrolling surfaces

What the window does wherever a surface holds more than it can show: how it says so, which of those surfaces a
keyboard can scroll, and what is left out. A single line of text too long for its space is a different thing and
fades under its own rules (see "Names too long for their space" in `docs/product/labels-and-tooltips.md`).

## The edge that hides content fades

Wherever a surface scrolls content out of sight, **the edge it hides that content behind fades out**, so that there is
something to see rather than only something to try. The fade is **24 px** deep everywhere the application draws it; a
strip of tabs and Settings' column of sections are the exceptions, and fade more deeply. Which surfaces this holds for, and the scrolling it leaves
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
- In focus mode's lists and the Git mode's change lists, the top fade begins below the section heading; see "What both
  views show" in `docs/product/focus-mode.md` and "Folding a section" in `docs/product/project-pane-git-change-list.md`
  for the headings' scrolling behavior. In Settings' Agent accounts section it begins below the agent tab strip once the
  strip is pinned at the top (see "The agent strip" in `docs/product/agent-accounts.md`), and in its Git and Keyboard
  shortcuts sections below a group's heading once that heading is pinned at the top (see "Group headings" in
  `docs/product/settings.md`).
- Only the **top and bottom** edges fade, apart from the strips that scroll sideways and fade at their sideways
  edges: focus mode's strip of console session chips (see "The switch strip" in `docs/product/focus-mode.md`), a
  strip of tabs, the Files panel's header controls (see "A panel's header" in
  `docs/product/right-pane-panels.md`) and a report page's blocks that scroll sideways. The file viewer's long lines,
  read by scrolling sideways while wrapping is off, fade at neither side (see "Wrapping long lines" in
  `docs/product/file-viewer.md`).

A heading or strip pinned at the top of a vertically scrolling surface stays clear while it is pinned or approaching
its pinned position. When its section pushes it upward past that position, its contents fade from the upper edge as
it leaves, over a band that grows with the displacement up to 24 px. Its background stays opaque, covering the content
scrolling beneath it. This applies to nested headings as well: each fades when pushed above its own pinned position.

**Some surfaces draw no scrollbar at all**, so the fade is what says there is more: the rail's column of console
avatars, too narrow for a scrollbar (see "The rail" in `docs/product/window-layout.md`), focus mode's strip of chips,
a strip of tabs, which also shows a small chevron at each end that has
more beyond it, pressed to scroll the strip that way (the chevrons are not keyboard stops), and Settings' column of
sections, which shows no chevrons.

The **New conversation / Resume conversation**, console **Advanced**, Settings **agent**, rail **quota agent** and
narrow Settings **section navigation** tab lists fit their tabs' combined content and inner padding, bounded by the
available width, and begin at the reading direction's start. Each tab keeps its content's width without stretching to
fill the row; a filled tab list's background ends with the list. Overflow scrolls sideways, and arrow-key navigation
reveals the focused tab with clearance for its focus ring. Settings' agent header spans the section with its opaque
pinned surface, divider and actions at the end; its transparent tab list occupies only the space its contents need
within it. The divider under the rail quota panel's agent tabs likewise spans the whole panel.

## The surfaces that fade

Once it holds more than it can show, each of these fades at its edge:

- the **sidebar's** list of console sessions and projects, a **focus mode's** cards and archived rows, and the
  **archive view's** rows (see `docs/product/sidebar.md`, `docs/product/archive-view.md` and
  `docs/product/focus-mode.md`);
- the **rail's** column of console avatars;
- the **welcome screen's** content (see "What the welcome screen shows" in `docs/product/welcome-screen.md`);
- **every dialog's body**, and in Settings the panel holding the section on screen (see `docs/product/settings.md`);
- the **project pane's** file tree (see "The file tree" in `docs/product/project-pane.md`), and the Git mode's change
  list (see `docs/product/project-pane-git-change-list.md`);
- the **Files panel's header controls** other than Close panel (see "A panel's header" in
  `docs/product/right-pane-panels.md`);
- the **file viewer's** reading surfaces — a file shown as plain text, a highlighted file, a diff and a rendered
  Markdown document (see "What it shows" in `docs/product/file-viewer.md` and `docs/product/markdown-documents.md`);
- a **report page** in the report panel, and inside it the blocks that scroll sideways rather than wrap — a code block,
  a diff, monospaced copyable text and a table wider than the page — which fade at their sideways edges (see
  `docs/product/report-page-components.md`);
- the **lists inside a popover**: the list of branches of the Git mode's branch selector (see "The branch
  selector" in `docs/product/project-pane-git-mode.md`), and the tags offered by the project filter (see "Filtering
  the project list" in `docs/product/sidebar.md`);
- the list of accounts in the **rail's account quota panel** (see "What the panel shows" in
  `docs/product/account-quota-panel.md`);
- the **rail's Processes popover** (see "The rows" in `docs/product/processes-on-the-rail.md`);
- the **directory browser's** list of directories (see "Browsing directories" in
  `docs/product/projects.md`);
- a **strip of tabs** whose tabs do not fit, which scrolls sideways: the row Settings' list of sections becomes
  in a narrow window (see "In a narrow window" in `docs/product/settings.md`). **A tab strip fades over 64 px,
  not 24**. While a tab has visible keyboard focus, the strip's fade is suppressed so the tab and its focus ring stay
  fully visible, including after the arrow keys scroll a tab into view. The fade's size and the chevrons'
  appearance remain the component library's own; the application supplies the chevrons' accessible names in the
  current language. Settings' sections are such a strip only in the narrow window;
- **Settings' column of sections** above 768 px, when the dialog is too short to show every section (see "In a narrow
  window" in `docs/product/settings.md`): the list of sections scrolls up and down beneath the search field, which
  stays in place. It fades over 64 px, as a strip of tabs does, draws neither a scrollbar nor chevrons, and likewise
  suppresses its fade while a tab has visible keyboard focus;
- the **screens shown before there is anything to show** (see "The window chrome and the content panel" in
  `docs/product/window-layout.md`): the column of content of each of them;
- an **empty-state failure** (see "Failures shown on the screen" in `docs/product/error-alerts.md`) taller than the
  pane, panel or dialog body it fills, and each block of raw text held to a height — on an empty-state failure's
  details surface, the stack trace on the screen for a UI that failed and the daemon's message on the screen for a
  daemon that could not start among them, and under **Error details** in an error alert — once it is taller than that
  height (see "Names and diagnostic details" in `docs/product/error-alerts.md`).

Three kinds of scrolling show no fade:

- **What scrolls as a document of its own**: the terminal's scrollback (see "Output, scrollback and size" in
  `docs/product/session-terminal.md`).
- **Settings' accounts table**, which scrolls sideways in a narrow window with its actions column pinned at its end
  edge, where a fade would dim that column (see "The accounts table" in `docs/product/agent-accounts.md`).
- **A menu's or a selector's own list of choices** — an action menu, the account and option selects — which scrolls
  with its scrollbar alone. The Git mode's branch selector is the exception: the list of branches inside it fades, as
  above.

## Scrolling a surface from the keyboard

A surface whose own content takes keyboard focus is scrolled by moving through that content: a list by its rows, a tab
strip by its tabs. The file viewer's reading surfaces take focus themselves, whether or not they scroll, because
selecting and copying is theirs as well (see "Selecting and copying" in `docs/product/file-viewer.md`).

**Where nothing else on screen would scroll it, the surface itself becomes a tab stop while it scrolls** — a region
named for what it holds, which the arrow keys, Page Up and Page Down then scroll. It is **no stop once its content
fits**, so there is never one that would scroll nothing. Reached with Tab it shows a focus ring, which the fade never
covers. These are:

- **a dialog's body**, named by the dialog's own title. A dialog whose controls are all in its footer — a long
  message, a page of text — has nothing in its body to take focus otherwise.
- **the welcome screen's content area** (see "Keyboard focus" in `docs/product/welcome-screen.md`).
- **the column of content of a screen shown before there is anything to show**, named as the screen's message. Its
  buttons sit at the end of that content and the window itself never scrolls, so a long message — the instructions
  of the screen with no daemon address or key, say — would otherwise leave nothing between those buttons and the top of the
  screen to take focus. In Tab order it comes before the buttons it holds.
- **a diagnostic block's text**, whenever it overflows horizontally or vertically, named by its disclosure or heading
  where there is one, and otherwise "Error details". Its floating Copy button does not scroll the text, nor does the
  surrounding surface's scrolling reach the text within the block (see "Names and diagnostic details" in
  `docs/product/error-alerts.md`).
- **an empty-state failure taller than the pane, panel or dialog body it fills**, named by its heading. Nothing around
  it scrolls, and its lines and details hold little or nothing that takes focus, so without it what is scrolled out of
  view there would be out of the keyboard's reach.
- **a report page**, named by the page's title, or "Report" for a page with none, and inside it **a block that scrolls
  sideways** — a code block, a diff and monospaced copyable text, named by its label or, without one, as code, a diff
  or text to copy, and a table wider than the page, named by its caption or as a table (see
  `docs/product/report-page-components.md`).
- **the list of accounts in the rail's account quota panel**, named "Accounts". Its rows hold nothing that takes focus,
  so nothing else would scroll it. In Tab order it comes after the agent tabs and before Manage accounts (see "What the
  panel shows" in `docs/product/account-quota-panel.md`).

**Keyboard focus does not land under a fade**: a row, a control or a region scrolled into view stops 24 px short of the
edge, clear of the fade, rather than arriving underneath. In focus mode's lists and the Git mode's change lists that
space is measured below the section heading (see "What both views show" in `docs/product/focus-mode.md` and "Folding a
section" in `docs/product/project-pane-git-change-list.md`), in Settings' Agent accounts section below the agent tab
strip, and in its Git and Keyboard shortcuts sections below a group's heading. A strip of tabs and Settings' column of
sections instead suppress their fade while a tab has visible keyboard focus, as above. For focus visibility in the Files panel's header controls, see "A
panel's header" in `docs/product/right-pane-panels.md`.

**A control in a section heading pinned at the top is not scrolled to**: in the sidebar, in focus mode and in
Settings' Agent accounts section, a heading that stays at the top of its list is in view already, so moving keyboard
focus among its controls — the project filter's button, Collapse all projects, focus mode's switch, the agent tabs —
leaves the list where it is. Moving on from the heading to a row still scrolls that row into view, below the heading
and the fade. A heading not yet at the top, further down the list, is scrolled into view as anything else is.
