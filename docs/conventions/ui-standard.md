# UI standard

The visual and interaction bar `packages/ui` is held to, as the user confirmed it. Accessibility is
`docs/conventions/ui-accessibility.md`; UI copy is the `knowledge-i18n-copy` skill, and where the two disagree this
standard wins: a name referenced within copy takes an `InlineReference`, never the skill's quotation marks. The
documentation site (`apps/docs/`) is bound by only part of it, as `docs/conventions/docs-site.md` sets out. What each
view shows and does is in `docs/product/`; a component's own design is in its doc comment.

## General principles

- **Reuse the design system and the shared components.** HeroUI's component through its wrapper, the project's shared
  component where one exists (`docs/conventions/ui-components.md`), one token per meaning: two recipes for one look
  drift apart.
- **HeroUI's look is kept.** Its variants and slots give the appearance; the application's own styles aim at WCAG 2.2
  AA in both appearances.
- **Contrast figures are a reference, not a requirement.** A ratio under a threshold is worth weighing but is not on
  its own a defect: a style that reads well and fits the design is kept, and HeroUI's colours, opacity, borders or other
  treatments are never overridden to meet a threshold.
- **A kept exception is marked at its code and needs the user.** Each shortfall or override the user chose carries a
  comment saying so, with its figures; leave a marked one as it is, and add none without the user's say.

## Conformance level and exceptions

- **Thresholds, for reference**: text 4.5:1 against its background (3:1 large); an icon or state indicator 3:1 against
  what it sits on; a boundary 3:1 where it alone shows where a control is. Application-authored text on a custom
  surface is checked against its actual background in both appearances.
- **A shortfall in application styling is fixed at its custom part, surface or shared token**, never by overriding a
  HeroUI component's default look, and an existing override is no licence for another.
- **Do not change how a HeroUI component looks**: its text and placeholder colours, hover opacity, and the fill,
  foreground and shadow of the variant or status chosen. Layout and sizing adaptations do not license replacing its
  visual treatment. The approved departures are marked at their code (an Alert's status fill and shadow, the dark
  floating surfaces, a modal body's foreground text, a disabled menu item's or option's dimming, the Settings section
  menu, the Terminal panel's tabs, the menu filter row, compact key caps, among them).

## Colour

### One definition per semantic colour

Every colour that carries a meaning comes from one definition: HeroUI 3's semantic tokens where HeroUI has one, else a
project token defined once in `packages/ui/src/styles/`. No component hard-codes a semantic colour, keeps a second
recipe for a role that has a token, or overrides a HeroUI component's semantic colours locally.

| Role | Source |
| --- | --- |
| Solid fill and the text on it (a primary button, a count badge, a selected segment, the resize line) | HeroUI `--{colour}` and `--{colour}-foreground` |
| Soft fill (an Alert's background, a chip's and a badge's fill) | HeroUI `--{colour}-soft`; the neutral fill of a chip or badge is the project's neutral tint |
| An icon or glyph of that meaning on a surface (a failure mark, a status dot, a lock, a Git badge); a destructive menu item's icon takes its label's `--danger` | HeroUI `--{colour}-soft-foreground` |
| Text in that colour (a chip's and a badge's text, line counts, the diff's added and removed text) | The project's tint text token: the soft foreground pulled toward the foreground just enough to keep 4.5:1 on every surface |
| A hairline edge (a bordered badge) | The project's tint edge token, mixed from HeroUI's solid colour |
| Focus | HeroUI `--focus`, for HeroUI controls and the application's own rings alike |
| Accent, links included | HeroUI `--accent`; link text in copy takes the accent text token |
| Secondary text | The project's `--app-muted` (`text-app-muted`), HeroUI's `--muted` missing 4.5:1 on a selected sidebar row |
| Help, the project's purple for help and hints | The project's `--help`, with its soft fill, text and edge derived like the others |

- **Native colours that cannot read CSS (the menu bar icon's) are generated from or read back from the same values**,
  and a provider's colour is written once, in its presentation module. The terminal's ANSI palette, syntax
  highlighting, provider and identity colours are not semantic and keep their own palettes.
- **Secondary text is chosen where the text is written, never inherited from a container.** A modal's body text is
  the foreground colour in every modal (`Dialog` sets it back from HeroUI's muted); text in a modal that is secondary
  takes `text-app-muted` on its own element.
- **A solid semantic fill stands behind a line of copy only in the window's full-width status strips** (the update
  notice, the connection banner), which are bands, not Alerts, and hold no primary button; elsewhere a solid warning or
  danger fill is only a glyph-sized mark.
- **Every floating surface in dark is drawn on the project's `--floating`**, and a new kind joins the floating-surface
  rule's list in `packages/ui/src/style.css` (a bar of the app's own takes its `floating-surface` class). A tooltip or
  popover that can hold a control drawn from `--default` carries `control-fills`, being portalled out of the pane's.

### Badge, reference or plain name

- **A status or qualifier after a name is a `StatusBadge`**, never parenthesised text in the name's style and never an
  `InlineReference`: borderless (`soft`) for a state, bordered for a qualifier annotating the name before it. A bare
  count after a row's name is a `CountBadge`; a count in a section heading is plain secondary text in the heading's
  type. A badge sits at the end of a name, row or label line, never in wrapping running copy.
- **A reference mixed into running copy is an `InlineReference`**, where prose would use quotation marks: a name,
  title, control label or technical literal in prose, headings, descriptions, Alerts, field errors, tooltips or toasts.
  A name uses the non-monospace chip with `dir="auto"`; a technical literal (a path, URL, branch, commit, ID, command,
  setting value) its monospace, left-to-right form. Never a raw `code`, the layer's `Code` or `Chip`, or a hand-built
  pill.
- **A name is referenced once per piece of copy**; later mentions use a pronoun or a plain reference.
- **A name shown on its own takes no chip**: a row's title, a list entry, a heading that is just the name, a label, a
  value on its own line, and a run of names joined only by a connector (one chip around the whole run when that run is
  itself referenced).
- **A navigation path through menus or settings is one `InlinePath`** (the sentence takes it as one parameter), never
  separate chips joined by words.
- **Plain-text surfaces (accessible names, native labels, announcements, notifications) carry no chip markup**, and
  collection keys, `textValue`, sorting and submitted values stay raw. Badge and chip read one set of tint tokens
  (`styles/palette.css`), so a chip and a badge of one colour never differ.

### Icon vocabulary

| Meaning | Glyph |
| --- | --- |
| A refresh that re-fetches data (Git status, Files, MCP inventory), and reloading the page | Lucide `RefreshCw`, rotating `motion-safe:animate-spin-slow` |
| Every wait other than a refresh (a session starting, switching account or reconnecting, a pane or list loading, a terminal's cover, signing in, loading menu items or a select's options, a device or the daemon reconnecting, the operation a `pending` button began) | The layer's HeroUI `Spinner`, muted grey; on a surface with a fill of its own (a button, a tinted badge, a status strip) it takes that surface's text colour (`color="current"`) |
| A menu entry that opens a settings dialog | Lucide `Settings2` (the gear `Settings` is only the rail's Settings button) |
| Restart to apply an installed update | Lucide `Power` |

Activity glyphs are 14px in compact markers, the device badge and the connection banner, 16px in regular status slots
and beside a line of text, and 24px in a state that takes a whole pane, and every rotating glyph holds still with
reduced motion.

## Controls

### Buttons

- **Copy explains; a button acts.** An instruction pointing elsewhere ("go to Settings → Agent accounts") is a button
  that takes the user there. One view offers one entry per action (the file viewer's stand-in repeating the header's
  open button is the user's exception).
- **Button order follows the group's alignment.** A trailing-aligned group (a footer, an Alert's buttons, a settings
  row's controls, which stay trailing-aligned when they stack) puts the secondary buttons first and the primary at the
  trailing edge; a start-aligned or centred group (an empty state's row, a status strip) puts the primary first. A
  group has at most one primary, and a centred row at most two buttons.
- **A dialog's footer holds at most two buttons.** With two actions besides dismissing, the dialog's × dismisses it and
  the footer drops Cancel. Quiet status about the dialog's content (a count) goes in `footerStart`, text only.
- **Every button is the wrapper layer's `Button`**; a band or row that is itself the control is `BareButton`; a native
  `<button>` only where neither can serve, with the reason in a comment. Tree chevrons are `TreeExpandButton`.
- **A running action holds its button `pending`**: content hidden in place, a spinner over it, `aria-busy`, no further
  press, focus kept. The label never changes to a progressive form, so no catalog carries a pending variant of a
  button's label.
- **A busy dialog (`busy` on `Dialog`) disables everything in it but the pending action**, Cancel, the × and links
  included, and is never made `inert`; holds that are not the action itself (a file picker waiting, a quick lookup's
  `waiting`) leave it leavable.
- **A control unavailable for a reason the user needs says why in its tooltip, and nowhere else.** `Button`, `Switch`
  and `Checkbox` take the reason as `unavailable`: `aria-disabled`, focusable, faded, no hover tint or press, the reason
  as plain tooltip and description, nothing on a click, Enter or Space, and on touch the reason in the touch popover.
  A control disabled only for the moment an operation takes, or whose cause the screen already shows where it
  happens, carries no reason. A select option or menu item shows its reason as secondary text within the item and
  stays in the arrow-key walk (`disabledBehavior: "selection"`).

### Fields and choices

- **A form control on a surface (a dialog, a popover, a menu, a card) takes HeroUI's `variant="secondary"`; on a pane's
  own background (the sidebar, a right-pane panel, the file viewer, a report page) the default variant.**
- **A date or a time is entered in the application's segmented fields** (`TimeField`, `DatePicker`), never the
  browser's native picker, whose look differs by platform.
- **Every select is an `OptionSelect`**, which owns the option layout; a caller never sets an option's height or
  padding. A select whose options load shows it inside the control (the spinner for the chevron, `aria-busy`, a
  chosen value kept), never as a line below.
- **A field's clear action is `FieldClearButton`**, beside a select's trigger, never inside it.
- **A disclosure folding away a form's rarely set fields (a dialog's Advanced section) uses `QuietDisclosureTrigger`
  and `FlushDisclosureContent`**, never a copy of their classes. The welcome screen's tools not detected keeps them too.
- **Every other disclosure in a dialog's or a page's body is a `TextDisclosure`: its label as text after a
  `DisclosureTriangle`, never a button's look or a chevron**, whatever it folds away (error details, what an option
  covers, a text someone wrote); tree rows, a pane's collapsible section headings and menu items keep their chevrons.
  What it folds away starts under the label, indented past the triangle, unless it is a code block (`DiagnosticBlock`,
  `CodeBlock`, each possibly under a label of its own), which stands flush (`code`). A summary that lays out a row of
  its own (a list's row) draws the same `DisclosureTriangle` and follows the same rule for its content.

### A modal's title is a plain name

A modal's title (a form dialog, a confirmation, a warning, an error alert, a failure modal) is a short plain phrase
naming what it does or what failed, the same whatever it is opened on: no session title, name, path, count, quoted
value or chip. What it is about goes in the body's first sentence; the title is the modal's accessible name, read each
time focus comes back. A setting's risk confirmation may ask a short question ("Bypass permissions?"). A component
showing one piece of content in a modal layer (the file viewer) keeps that content's name.

### Tooltips and hints

- **Nothing uses the native `title` attribute for a tooltip**, except where no tooltip of the application's can be
  rendered (the diff's separators in a shadow root) or the description is the point (a link in a Markdown document).
- **Every tooltip and every floating hint is drawn from one surface definition** (HeroUI's `tooltip` class with
  `control-fills`, `tooltipSurfaceClassName`), so all of them look alike; the component to use is in
  `docs/conventions/ui-components.md`, "HeroUI and React Aria".
- **A tooltip answers the pointer, and a control's keyboard focus**; whether a tap opens a touch popover is the kind's
  parameter (a `mark`'s is on, turned off with a comment where the mark sits in something answering the same tap).
- **An icon-only control's tooltip that must say more than the action is a `HeadedTooltip`** (the action's name, a
  divider, the sentence, which also reaches the control as a hidden description); the reason a control is unavailable
  replaces its tooltip as plain text, with no heading.
- **A mark whose meaning nothing else on screen carries gets a tooltip**, worded from the string that already names the
  mark; a text beside it that names it, or a tooltip already covering that spot, counts as carrying it.
- **A tooltip that is a value in full (a clipped label, a path, a URI) keeps its label's font family** through
  `ValueTooltipText`; a sentence keeps the tooltip's own font.
- **An explanation moved off the screen into a "?" is a `HelpTip`**, beside the label, never in it, and only for a
  principle-level explanation or one item's extra line; a warning, error or note takes its mark or Alert.

### Clipped text

- **Single-line text that may be clipped uses `FadeOverflow`** (an edge fade, a hover marquee and a full-text tooltip
  while clipped). An excerpt of a file's content (a search result's line) may be cut with an ellipsis where it is cut.
- **A clipped label's marquee runs only while the pointer is over its own text region**: independent labels in a row
  have independent regions, and `data-marquee-scope` is only for a single-target control holding one label (a menu
  item, a selector, a picked tag), never a general row, card or header.

### Cards, grooves and hairlines

- **A hand-built card its content identifies has no outline of its own**: the panel shadow (`shadow-(--panel-shadow)`)
  with a transparent border holding the place a selected state's border takes; its selected state takes the accent
  icon colour.
- **A card on a dialog's own surface is a `FlatCard`**: a hairline round it, no shadow, filled header bands with no
  line of their own, so a caller writes no divider, inset or fill class.
- **A groove (`groove-b` and its other sides) is only for the dimensional parts**: panes, panel headers and toolbars,
  the rail and sidebar, a floating panel with a shadow, a sticky heading over a pane's list, a section band in a pane.
  A flat card, a table or a list of rows in a dialog or a Settings page divides with a hairline (`hairline-b` and its
  other sides), and a heading over those rows (a Settings group's sticky heading, a tab strip) takes the same hairline:
  the lines of one list, table or card and the heading over it are of one kind.

### Tab strips

- **Every horizontal tab strip takes its content width, start-aligned, and scrolls when it does not fit**; tabs never
  stretch to fill the row.

### Settings pages

- **A Settings section of more than one block puts each in a `SettingsGroup` with its own heading**, spaced only by the
  group and its rows; groups are ordered by how often each is used, the section's ungrouped rows first.
- **Within a Settings page, tabs are the underline strip** (`AgentTabList`, `TextTabStrip` with `underline="hairline"`); the filled
  strip is the Settings dialog's own sections at narrow widths and tabs inside a dialog's frame.

### Menus

- **Copy actions are top-level items in a group of their own**; a text-editing menu keeps Copy, Paste and Select all
  together. A menu shows at once, and an item awaiting data stays disabled without a reason until it arrives (a failed
  lookup leaves it disabled with one).
- **A submenu list that is incomplete or failed to load puts one status glyph on its trigger**, after the label and
  before the chevron. A truncated list uses a warning triangle and still offers the returned choices. A failed
  list uses a circled error mark and closes the submenu. The glyph's tooltip explains the status on hover and
  whenever the trigger takes keyboard focus. Opening the submenu again reads the list anew.

### External links

- **A link that leaves the app is `ExternalLink`**, with a real destination and its trailing external-link icon, never
  a `target` (the shell would open it twice). Its appearance follows what stands around it: the text appearance in
  prose or on its own, the button appearance beside buttons or among a place's button actions, and the text appearance
  for an address shown as data. Callers never remove the icon or restyle it.
- **A chip that names a separate control is not interactive**; a chip is never itself a link.

### Empty states

- **Every place with nothing to show is an `EmptyPanel`**: a glyph, a heading naming what is missing, optionally a line
  under it and a centred row of at most two buttons, the action that fills it first. Never a second one of centred
  text, and never HeroUI's bare `EmptyState`.
- **A normal absence takes a neutral or purpose glyph, never a warning glyph.**
- **A whole page, pane or list whose subject could not be loaded is an `EmptyPanel` with the danger tone**: a heading
  saying what could not be loaded, the line saying why and how to recover, the further explanation and raw text on a
  surface under it, Try again in its row, and no status mark or Details. What the subject is (not a repository, a gone
  worktree) is an empty state, since no retry would change it.
- **Where another region already speaks the state, the panel takes `silent`** (with no action), and that region says
  the whole state from a `.sentence` message.

## Errors

### Where an error is shown

Where an error or a warning is shown follows from how it arises, not from the screen it arises on:

1. **An error the user's own action directly causes** (a submit, a Save or a confirm, a switch, a menu or row action,
   copying, opening) opens the error alert dialog, over a dialog that keeps its input.
2. **An error located at one input field** (validation at submit, or a lookup the form makes that finds the field's
   value wrong or unusable) stands under its field as its `FieldError`, and no dialog opens.
3. **An important error from automatic behaviour** (one that blocks what follows, or that a component cannot do its job
   without) is an Alert on the screen; in place of a whole page, pane or list, the danger empty state.
4. **An error or a warning that is not important** is a mark at the point it concerns; one with no point is an Alert in
   a shared place (the top of a dialog's body or a section). A warning about the consequence of a choice the user is
   making is a `ChoiceWarning` under that choice while it is selected.
5. **A result view showing a failed result** shows it as an Alert, as the view's own content.
6. **A destructive confirmation** (removing, deleting, archiving) is a warning modal, never an ordinary modal with a
   warning Alert in it.

- **A refusal the window could have foreseen is prevented, not alerted**: the confirm or submit is disabled while the
  state holds, the reason standing where the action is (an Alert under the choice, a `FieldError`, or a consequence
  line), and announced as it appears. The error alert is for what could not be known beforehand: a state that changed
  since, the operation failing, or what only a slow check would find.
- **A value changed elsewhere replaces the form's own**: as soon as the store shows a field at a newer version with a
  different value, the field shows it (`useCheckedEdit`), whether or not the user had edited it, and a replaced edit is
  announced; only the race the window cannot foresee is refused at submit (`write_conflict`).

### The error alert dialog

- **It stays until dismissed on purpose**, with its acknowledgement or its ×, never a toast, a timeout, an outside
  press or Escape. It is HeroUI's alert dialog (the layer's `Modal` with `alert`, `status="danger"`), takes focus on the
  alert itself and announces through `role="alertdialog"` alone. Distinct failures queue; copies of a pending one do
  not multiply.
- **Its body is the application's own prose**; what is not written for the reader (system, Git, daemon or agent text,
  a stack, a diagnostic block) sits behind **Error details**, each block on a surface of its own. Only text of the
  user's own handed back to keep (a cleared draft) stands in the body, as a `DiagnosticBlock` under a label.
- **A recovery action is a button in its footer, never in its body**, secondary beside the acknowledgement; with Report
  issue and a recovery both offered, the acknowledgement goes, the × standing for it, and the recovery is the primary
  in the accent colour. A failure the user can resolve names its remedy; an unexpected one keeps its diagnostics and
  offers the issue entry, never asking for a screenshot or the raw text.
- **A failure that ends a dialog's purpose turns that dialog into a failure modal in the alert's form**, in the same
  modal: danger icon, a title naming the failure, the explanation, Error details, an × and no Escape. A confirmation
  whose subject has gone with nothing left to change closes without a word.

### Alerts and marks

- **An Alert is HeroUI's `Alert` through `ErrorAlert` (or `FailureAlert`, `ChoiceWarning`)**: a title saying what
  happened, then at most two sentences (why, and the one main thing to do), then at most two buttons, **Details**
  (opening the rest: further explanation, then raw text) and the recovery action at the trailing edge, in the Alert's
  status colour. Raw text never shows outside Details, nothing of the Alert stands outside it as loose text, and its
  indicator, typography, spacing and structure stay stock HeroUI (its status fill and shadow are the approved
  exceptions). Try again appears only where trying again can change the answer.
- **A mark is a `StatusMark`: a button opening a `PopoverHint`, never also a tooltip**, named for what it is about,
  its 14px glyph a circled `!` for an error, a triangle for a warning, a circled `i` for a note. A mark that draws a
  word beside its glyph with nothing more to say is plain text.
- **A mark stands right after the name, heading or line it qualifies** (in a `MarkLineBox`, centred on that line), for
  a field after its label or inside its group before its own trailing buttons, for a setting in the row's control area;
  never at a row's trailing edge, so a long name clips and the mark stays visible.
- **Several errors or warnings at one point are one mark**, in the more severe tone, its popover listing each.
- **No sentence of error text is written into a row**, except in a report of outcomes (the import's progress and
  result), where a failed row writes its reason and remedy under it and keeps Details for the rest.

## Motion

- **An animation or transition that is not essential stops under `prefers-reduced-motion: reduce`**
  (`motion-safe:` / `motion-reduce:`); motion driven from script reads `prefersReducedMotion()` before it starts and
  cuts a running effect short through `onReducedMotion()`.
- **Overlays react-aria opens and closes appear and go at once**: one rule in `packages/ui/src/style.css` stops the
  enter and exit animation of every `data-entering` / `data-exiting` element, and `style.test.ts` fails on any other
  unguarded transition an imported HeroUI sheet brings.
- **No transition starts inside content folded with `hidden="until-found"`**: one never reports its end there, and
  react-aria holds every focus move after a press without a pointer until none runs. One rule in `style.css` turns them
  off; a new element there needs nothing unless its transition is set by inline style, `!important` or a more specific
  unlayered rule, and a dialog that must take focus at once places it itself.
- **Items added, removed, reordered or folded in a visible collection move through `useListMotion`** (or
  `useBrowserListMotion`, `MotionListBox`), keyed by stable identities, never array indices or labels, with its timing
  and preference handling left to the shared implementation; a removed item stops taking input at once. A filtered
  menu's items, a replacement page or listing and a fixed result snapshot take no collection motion; a list whose rows
  leave or join when a result arrives in the background, and a live filter, do.
- **A divider hidden after a collection's last item uses `[&:not(:has(~[data-list-motion]))]:border-b-0`**, never
  `last:`, since exit copies are appended after the last real item.
