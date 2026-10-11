# Final confirmation

> Goal: the checks that only a person can make are made, together with the ones held back for one packaged build
> rather than one per milestone, and anything they turn up is fixed here.
> Completion criteria: every item below is confirmed in the packaged app, or fixed and then confirmed; nothing is
> left beyond this milestone.

## Carried from milestone 02 (expanding collapsed lines in the viewer)

Everything else about the expansion was verified in the packaged app. These two cannot be driven by an agent at
all — they need a person at the machine:

- [ ] A screen reader (VoiceOver) on the separators: their names, the loading and failure announcements, and the
  controls marked unavailable while a read is out.
- [ ] A real input method typing into the viewer while a diff is shown, confirming composition does not reach the
  separator controls' key handling.

## Carried from milestone 05 (filtering the change list by file name)

Implemented and covered by unit tests over synthetic composition events; a real input method cannot be driven by an
agent, as injected keystrokes bypass macOS input methods entirely.

- [ ] A real input method typing into the change list's filter field, confirming composition does not reach the
  list's type-ahead and navigation keys, and that the key ending a composition does not clear the field.
- [ ] A real input method composing in the terminal while a drawer or floating pane is open: the key that ends the
  composition cancels it rather than closing the pane, and an Escape outside a composition still closes the pane.
  The layout's Escape listener now skips an editable target that holds a value, and xterm.js's hidden
  `textarea.xterm-helper-textarea` is excluded from that skip, so the terminal keeps the pane's escape hatch and the
  composition is guarded by the input-method check instead.

## Carried from milestone 06 (a row action button with Copy path)

Implemented and verified in the component gallery under WebKit with real keyboard input, in both list forms, both
themes and under `ar`: the button's reveal and its zero width while hidden, its accessible name, the arrow key
reaching it along the row, and Copy path raising its toast. These are what that could not settle:

- [x] Copy path in the packaged app: pressing the button with a real mouse and choosing Copy path left the change's
  path on the system clipboard, read back after the toast, so the plugin and the `clipboard-manager:allow-write-text`
  capability entry both reach it.
- [x] A pointer press on the action button opens the menu and does not open the file viewer behind it.
- [ ] A screen reader (VoiceOver) on the button: its name, and that the row's own name is not spoken twice.
- [ ] The button's contrast against the row in light mode, hidden and revealed, measured rather than read off a
  screenshot.

## Carried from milestone 07 (a document view for Markdown in the viewer)

Verified in the packaged app: the document renderer's own chunk loads from `tauri://localhost`, a link opens the
system browser with the viewer staying put, Select All then copy yields the file's Markdown source including the code
inside fenced blocks, and an untracked Markdown file opened from the Git mode renders as a document. The rest was
verified in the component gallery under WebKit with real input, in both themes and under `ar`. These are what neither
could settle:

- [ ] The document's loading state is centred in its frame. The change was made but never seen: the renderer's chunk
  loads too fast to catch the Suspense fallback, in the gallery and in the packaged app alike. Throttling, or a file
  large enough to slow the first draw, may make it visible.
- [ ] A screen reader (VoiceOver) on the rendered document: the region's name, the heading and list structure, the
  links, and the footnote heading that is marked for assistive technology only.
- [ ] A Tab walk in the real WKWebView rather than Playwright's WebKit build: the document region, then each link in
  it, then on out of the document.
- [ ] One measurement of a Markdown file near the viewer's text budget. The document is gated on the same budget as
  highlighting, on the stated grounds that a document that size would hold the window as long as highlighting it
  would — but highlighting tokenizes in workers and only pays an insert cost on the main thread, while
  `react-markdown` parses and builds the whole element tree on the main thread. The claim is plausible and unmeasured;
  if it does not hold, the document needs a smaller budget of its own.

## Carried from milestone 08 (the viewer header's layout and tags)

Verified in the component gallery under WebKit with real input, in both appearances and under `ar`: the details
sharing the controls' row and dropping to rows of their own as they grow (32, 64 and 96 px measured), the per-item
fade, tooltip and marquee, a start-clipped path keeping the file's name visible, CJK text at three widths, the tag
spacing, and the Settings sections left unchanged by the path component's split (compared side by side against the
merge base). The fills were measured against the dialog's own surface. These are what that could not settle:

- [ ] The header's layout in the packaged app, in both appearances. This milestone's completion criteria name the
  packaged app, and the layout rests on `float: inline-end` and `flow-root` — the first floats anywhere in
  `packages/ui/src` — so headless WebKit is the only engine that has seen the mechanism. Look at a long comparison
  dropping to its own row, a Compare-view rename dropping two, and the mirrored case under a right-to-left language.
- [ ] A comparison clipped from the real `ar` catalog. The gallery's compare scenario does not clip at its width, so
  only a fixture's left-to-right comparison was seen clipped; the product's comparison is a catalog message with each
  branch in its own `bdi`, which is the case that matters for where the fade lands.
- [ ] The 2 px the view controls spill below the floated group. HeroUI gives a small toggle `h-9` below the `md`
  breakpoint while the float box is `h-8`, so below that width — exactly where the details start dropping to their
  own rows — the controls overhang the float by 2 px. It predates this milestone (the old header row was also `h-8`)
  and very likely never touches a glyph; one glance at a dropped row's top edge settles it.
- [ ] Every tag, the neutral stage tag included, reading as a tag against the dialog in the packaged app's two
  appearances, and the dialog's close button, whose fill the same override changed from 1.19:1 to 1.50:1 in dark.

## Carried from milestone 09 (surfaces and light-mode contrast in the viewer and Git mode)

Every fill was measured from the rendered paint in headless WebKit over the dev gallery, in both appearances, and
every ratio is recorded in the `style.css` comments beside the rule it belongs to. What is left is the judgement a
measurement cannot make, plus the engine the gallery is not: headless WebKit composites without a GPU, so the real
WKWebView has not drawn any of these surfaces. In the packaged app, in both appearances:

- [ ] The tertiary band around the viewer's frames reads as a boundary from the dialog — 1.20:1 against the light
  dialog and 1.18:1 against the dark one, the same order of step as the sidebar's own surface. It frames the plain
  text, code, diff and plain-patch views, the rendered Markdown document and the image checkerboard, so each of the
  six is worth a look; the image frame is the one whose ground is a checkerboard rather than a flat fill.
- [ ] The Git mode panel's fields, its section bands (1.15:1 light, 1.10:1 dark against the panel) and the
  Files/Git header read as distinct from the panel, and the bands do not compete with a row's own hover and selected
  tint, which at 1.17–1.25:1 is deliberately the heavier of the two.
- [ ] The solid accent fill a selected view choice now carries is acceptable to look at in both appearances — the
  viewer's groups, Wrap lines, Settings' appearance choice and Git mode's Group by folder.
- [ ] Dark mode is unchanged or better throughout, which is the milestone's own criterion and the direction every
  measured change went in.

## Carried from milestone 10 (showing a type change as one diff)

Verified in the dev gallery in WebKit with real pointer input, in both views, both layouts and both appearances: a
type change and a rename into and out of a path below itself each draw one diff with the layout toggle, the kind of
change stays in the title's tag, and neither offers an expansion. The joining itself was checked against patches
taken from real git — the worktree, the index and commit-to-commit, for a file to symbolic link change, an
executable file to symbolic link change, and both nesting directions. What the gallery cannot settle is the patch
text itself:

- [ ] In the packaged app, against a real repository: stage a file-to-symbolic-link change and a `foo` to `foo/bar`
  rename, and open each in both the Uncommitted and the Compare view, in both layouts. The patches the gallery used
  are hand-written strings, so what this confirms is that the daemon's own patch carries what the join keys on — the
  `deleted file mode` and `new file mode` lines, and the symbolic link side's `120000` mode, which decides whose
  "no newline" marker is dropped.

## Notes for the developer

**Development notes**

- Every check that an agent can drive was handed to milestone 05 instead, since the only thing in its way was the
  machine's screen locking.
- The filter field's composition guard stops a `keydown` in the capture phase while a composition is in flight, so
  what the real-input check exercises is that guard, in the packaged app's WKWebView.

**Reference docs**

- `docs/memory/building-and-launching-the-app-for-verification.md`, `docs/memory/verifying-the-desktop-ui.md`
