# Final confirmation

> Goal: the checks that only a person can make are made, together with the ones held back for one packaged build
> rather than one per milestone, and anything they turn up is fixed here.
> Completion criteria: every item below is confirmed in the packaged app, or fixed and then confirmed; nothing is
> left beyond this milestone.

The packaged run everything below was read off: `pnpm build:app` from a detached worktree at the topic's merge base,
launched with `open` on a throwaway `HOME`/`TMPDIR` over a repository built for these cases (a file-to-symbolic-link
and an executable-to-symbolic-link type change, nesting renames in both directions with edited and with unchanged
content, a nesting pair with a real repository inside the repository, a 200-line diff with collapsed runs of 16 / 83 /
73 / 7, a 600-line one, an image, a plain-text file and an added file past the patch budget, and two branches holding
the same states). The window was driven with real pointer and key input and read from captures at the display's own
resolution; every ratio below is computed from the rendered paint.

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

- [x] Copy path in the packaged app: pressing the button with a real mouse and choosing Copy path left the change's
  path on the system clipboard, read back after the toast, so the plugin and the `clipboard-manager:allow-write-text`
  capability entry both reach it.
- [x] A pointer press on the action button opens the menu and does not open the file viewer behind it.
- [ ] A screen reader (VoiceOver) on the button: its name, and that the row's own name is not spoken twice.
- [x] The button's contrast against the row in light mode, hidden and revealed, measured rather than read off a
  screenshot. Hidden, the wrapper is `max-w-0 opacity-0` and draws no ink at all — every pixel in a clip around it is
  the panel's own colour, so no ratio exists there and what has to hold is that hover, focus and selection reveal it,
  which they do. Revealed, the glyph measures 4.93:1 on a selected row, 5.30:1 on a hovered one, 5.16–5.68:1 under
  keyboard focus and 13.96:1 with the pointer on the button itself, against a 3:1 bar. Its own hover fill is
  HeroUI's `bg-default`, 1.04–1.11:1 against the row, which the glyph's margin carries.

## Carried from milestone 07 (a document view for Markdown in the viewer)

- [x] The document's loading state is centred in its frame. Caught by delaying the lazy module's request in WebKit:
  the fallback's content centre sits 0.00 px off the frame's centre vertically and 0.7–0.8 px off horizontally, which
  is the spinner's own sub-pixel jitter as it rotates; the overlay box matches the frame exactly, in both appearances.
- [ ] A screen reader (VoiceOver) on the rendered document: the region's name, the heading and list structure, the
  links, and the footnote heading that is marked for assistive technology only.
- [x] A Tab walk in the real WKWebView rather than Playwright's WebKit build: close button → the source/document
  choice → the document region → each link in reading order (the body link, the footnote reference, the footnote's
  back-reference) → out of the document onto the dialog's Previous file button.
- [x] One measurement of a Markdown file near the viewer's text budget. The premise holds for every realistic shape —
  prose, tables, fence-heavy text, lists of 40, unwrapped paragraphs all cost 0.4–1.1 s of main-thread blocking at the
  budget, against 0.2–1.2 s for highlighting the same text — and fails for one: a single ~10,000-item loose list
  (946 KiB) blocks for 15.5 s as one task on an idle machine, quadratic in document size. The cause is not
  `react-markdown`'s parse (436 ms) but WebKit's handling of an unresolvable `dir="auto"` container (the same document
  costs 0.68 s in Chromium), so the fix is the direction computed in `blockDirection.ts` and given to each block by
  `MarkdownDocument.tsx`, not a smaller budget — a budget of its own was measured and rejected, since 2,000 lines /
  200,000 units would deny the document view to every manual and changelog that renders in well under a second. After
  the fix the same document blocks for 2.2 s and every ordinary shape is unchanged within run-to-run noise.

  The same rule fixed three things a reader sees, which an unresolvable `dir="auto"` had been falling back to `ltr`
  for: a loose list and a blockquote now take the direction of their own text (an Arabic list's markers stay beside
  its items instead of being pushed to the container's far right, and a quote's bar stands on the side it reads
  from), and a document with no strong letter takes the app's direction rather than left to right. A block with no
  strong letter of its own is now given no `dir` at all, so it reads the way the document around it does.

## Carried from milestone 08 (the viewer header's layout and tags)

- [ ] The header's layout in the packaged app, in both appearances. Settled in dark, left-to-right and mirrored: with
  a comparison long enough to fill the details row, the rename's origin takes a row of its own below it, the floated
  controls stay at the row's end, and under a right-to-left language the whole header mirrors with the controls at the
  mirrored end. What is left is the same look in light, and a case where two details drop rather than one — the
  repository's paths are too short to push a third item down at the window's 1148 px minimum, so it needs a change
  whose path is long as well.
- [x] A comparison clipped from the real `ar` catalog. The premise was wrong: `packages/ui/src/i18n/catalog.ts` holds
  `en` and `zh-Hans` only, so `ar` renders the English message in a right-to-left layout and no Arabic catalog exists
  to clip. What the item is really about was checked instead — the product's own `git.compare.source` message, each
  branch in its own `bdi`, clipped under `dir=rtl`: the fade lands on the reading direction's end, the `bdi`s stay
  intact, the `title` carries the full text and the marquee runs and returns on hover. Once an `ar` catalog lands,
  read it again at a width where it clips.
- [x] The 2 px the view controls spill below the floated group. Real at every width below the `md` breakpoint
  (buttons 36 px in a 32 px float, 2 px above and below), gone at 768 px. It touches no ink: the dropped detail row's
  topmost glyph pixel sits 9.0 px below the controls' bottom edge, the same in both reading directions. A dropped row
  carrying a file icon was not measured; by geometry its clearance is about 6 px.
- [x] Every tag, the neutral stage tag included, reading as a tag against the dialog in the packaged app's two
  appearances, and the dialog's close button. Measured from the paint: dark — status chip 1.25:1 against the dialog
  with its text at 8.85:1, neutral stage tag 1.45:1 with its text at 11.92:1, close button 1.45:1 with its glyph at
  4.65:1; light — status chip 1.12:1 with its text at 5.50:1, neutral stage tag 1.27:1, close button 1.27:1 with its
  glyph at 4.70:1.

## Carried from milestone 09 (surfaces and light-mode contrast in the viewer and Git mode)

- [x] The tertiary band around the viewer's frames reads as a boundary from the dialog. Measured 1.18:1 in dark and
  1.20:1 in light — the ratios the gallery recorded, reproduced by the real WKWebView — on all six frames: the plain
  text, code, diff and plain-patch views, the rendered Markdown document and the image checkerboard. Against the
  checkerboard the band is the subtlest, 1.03:1 against the ground's light squares and 1.23:1 against its dark ones.
- [x] The Git mode panel's fields, its section bands and the Files/Git header read as distinct from the panel.
  Measured: light — field 1.22:1, section band 1.15:1 against the panel; dark — field 1.35:1, band 1.10:1. The bands
  stay the lighter of the two against a row's own tint, as intended.
- [ ] The solid accent fill a selected view choice now carries is acceptable to look at in both appearances — the
  viewer's groups, Wrap lines, Settings' appearance choice and Git mode's Group by folder. Read off the packaged
  window it is 5.0:1 against its unselected siblings in dark and 3.26:1 in light, with the glyph on it at 6.22:1 and
  4.04:1 — the same side of 1.4.11's 3:1 as the token-derived 4.58:1 and 3.67:1 recorded beside
  `.toggle-button-group .toggle-button` in `style.css`, the difference being that a capture carries the display's own
  profile. What is left is the judgement of whether it is pleasant, which is the user's.
- [x] Dark mode is unchanged or better throughout: every surface above measures at or above its light counterpart's
  step, and nothing in dark reads flatter than it did.

## Carried from milestone 10 (showing a type change as one diff)

- [x] In the packaged app, against a real repository: a staged file-to-symbolic-link change and a `foo` to `foo/bar`
  rename opened in both the Uncommitted and the Compare view, in both layouts. Each draws one diff with the layout
  choice and no separator, and the daemon's own patch carries what the join keys on — `deleted file mode 100644` (and
  `100755` for the executable case) with `new file mode 120000`, byte-identical between the two views. The marker
  rule was exercised on a file with no trailing newline turning into a link: markers on both sides, only the file
  side's kept, and the viewer says the old version has no newline at its end; the mirrored case keeps the new side's,
  and a symbolic link's own modification, whose only mode evidence is `index … 120000`, drops both.

## Carried from milestone 10a (diffing a rename into or out of a path below itself)

- [x] The separators and their chevrons in the packaged app, in both appearances. A gap between two hunks carries two
  chevrons, the run before the first hunk one. Dark: the band is 1.40:1 against the code area with its chevron and
  text at 4.95:1 against the band, rising to 10:1 under the pointer. Light: band 1.11:1, ink 6.21:1. A real pointer
  press expands in both appearances, and after a keyboard expansion that removes the pressed control, focus lands on
  the next separator's first chevron with its ring drawn — it is not dropped to the document.
- [x] A real staged nesting rename and a real comparison of one, read through the daemon rather than through a
  fixture: both nesting directions give one section and a line-level diff with no rename header, identical content
  gives an empty patch and the viewer says it was renamed with nothing else changed, and the counts the separators
  carry come from the reply's own line counts. The first expansion asks for bodies whose two `source` objects are
  byte-identical to the ones the patch reply reported, later presses of the same diff ask for nothing, and a request
  naming another blob is refused. Moving the repository reports the source as changed only where the repository still
  resolves — a worktree replaced under the project's path answers `source_changed` and the viewer gives the
  expansion up as documented, while a repository that is simply gone answers `source_unavailable` and the separator
  says the lines could not be loaded, which is the failed-read path. Both are the documented behaviours; it is this
  item's wording that was narrower than the product.
- [x] A nesting pair with a submodule side. A real repository inside the repository is listed as one nesting rename
  with both sides `kind: "submodule"`, read as two sections with `deleted file mode 160000` / `new file mode 160000`
  and `Subproject commit` bodies, nothing from inside the nested repository, and joined into one non-expandable diff
  with the layout choice. One caveat worth keeping in mind: `git` pairs gitlinks by exact match, so a listing only
  ever produces such a pair when both sides record the **same** commit; the unit test's differing ids are a shape no
  listing sends, and the note now sits beside that test.

## Notes for the developer

**Development notes**

- The packaged run's setup script is throwaway and lives outside the repository; it builds the repository above,
  creates the console and project through the daemon's own protocol, and needs no trusted-folder seeding — no browse
  request consults folder trust, which was established from the daemon's code and confirmed against a database with
  no trusted directory in it.
- Scripted pointer input into this window needs the pointer moved onto the control and left there before the press,
  and even then a press is dropped often enough that every one has to be confirmed from a capture; a dropdown is
  opened by pointer but chosen from with the keyboard.
