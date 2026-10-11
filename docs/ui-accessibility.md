# UI accessibility standard

The accessibility bar `packages/ui` is held to, and the rules and measured facts that follow from it. It is a standard
to check a control or a view against: a code comment may cite a rule here by this file's path and the heading it sits
under.

## Conformance level and exceptions

Every control and view in `packages/ui` meets WCAG 2.2 level AA, checked in both the light and the dark appearance.

The user has kept a few shortfalls on purpose, each marked by a comment at its code saying so. Leave one that is marked
as it is, and do not add a new exception without the user's say.

## Keyboard

Every interactive control is reachable from the keyboard and operable with Enter / Space.

### Reaching a control with Tab

Tab is how a control is reached, except for a control inside the row of a react-aria collection that manages focus
itself (a `GridList`, a `Tree`). There the row is the tab stop and Tab moves past the whole list, while the arrow key
that runs along the row (`keyboardNavigationBehavior` defaults to `"arrow"`) moves between the row and its controls.
That is the grid and tree pattern and it meets this rule. Both the change list's row actions and the Files tree's
chevron rely on it, so do not give such a control a tab stop of its own.

`preventFocusOnPress` only keeps a *mouse* press from taking focus; the control still has to be reachable.

### Buttons

Every button is HeroUI's `Button`. A native `<button>` is allowed only where a custom button is needed that HeroUI's
cannot be, with the reason in a comment there. A hand-built `role="button"` element needs `tabIndex={0}` and its own
Enter / Space handling.

### Leaving the terminal with F6

The terminal keeps Tab and Shift+Tab for the agent, so F6 / Shift+F6 region cycling
(`packages/ui/src/layout/useRegionCycle.ts`) is the only keyboard way out of it. Every new region of the window that
holds controls — a pane, a panel, an overlay — is marked `data-region` and added to that hook's `REGIONS` and `shown`,
or F6 skips it and a keyboard user in the terminal has no way to reach it.

## Visible focus

HeroUI controls draw their own focus ring.

### A hand-built focusable element

An element that removes the outline puts a ring back, never `outline-none` alone, and takes it from
`useFocusVisibleProps` (`packages/ui/src/components/useFocusVisibleProps.ts`): spread its props on the element and
style with `data-focus-visible:ring-2 data-focus-visible:ring-focus` (`group-data-focus-visible:` from a child).

### Never style focus with CSS `:focus-visible`

No `focus-visible:` or `group-focus-visible:` variant, no `:has(:focus-visible)` selector. Once the last focus came from
a click, the app's WKWebView does not match `:focus-visible` on an element focused by script, even right after a key
press. So every keyboard move made by script — F6 region cycling, Tab inside a react-aria focus-trapped dialog — lands
without a ring. Playwright's `webkit` matches it in those cases, so only the packaged app shows the miss.

### Focus moved by a change the user did not make

A hand-off that runs because what held focus went away — a pane hidden, a region unmounted by a daemon event — also
calls `setInteractionModality("keyboard")` before it focuses the destination. The user did not ask for the move, so
they have to be able to see where focus went, and the previous input was often a pointer press, which leaves the
destination without a ring. `handFocusOff` takes a `showRing` flag for the hand-off whose trigger is a press, where the
ring follows how the press came; `HandOffOnUnmount` always sets it, its trigger being a render.

### A keyboard handler that moves focus itself

Such a handler calls react-aria's `setInteractionModality("keyboard")` first when the handler sits on the window's
capture phase and stops propagation. react-aria tracks modality from a capture-phase listener on the document, which
the key then never reaches; without the call, what the handler focuses draws no ring when the previous input was a
pointer.

## Accessible names

Every control has an accessible name: its visible text, or an `aria-label` when it has none.

- An icon beside a name is `aria-hidden="true"`.
- An icon that is the only carrier of a meaning gets `role="img"` and an `aria-label`.
- An element that takes an `aria-label` without being a control — a scrolling `div` or `pre` made focusable with
  `tabIndex={0}` — also gets a role that can be named (`role="region"`). ARIA prohibits naming a generic element, so
  the name may never be spoken.

### An icon-only control: tooltip and `aria-label`

Every control that shows only an icon — a HeroUI `Button` with `isIconOnly`, a `CloseButton` or `Modal.CloseTrigger`, a
menu trigger — carries both a tooltip and an accessible name. The name is what assistive technology announces, and it
has to tell the control apart from its neighbours even where the tooltip's text cannot, so the two are not always the
same text. Which control's tooltip repeats its name and which one's is the shorter of the two is settled per control in
the "Tooltips on icon-only controls" section of `docs/product/labels-and-tooltips.md`; `ActionMenu` gives its trigger a
name of that shape by default.

The exception is a control inside a react-aria collection row, such as a tag's remove button. react-aria already
composes its accessible name from the button and the row through `aria-labelledby`, so there the `aria-label` is the
bare verb with no placeholder, or the row's own text is announced twice.

## Roles and states

### Expanded and selected

- A control that shows or hides a region carries `aria-expanded`.
- The selected row of a hand-built list or tree carries `aria-current` (as the sidebar rows do).
- HeroUI's `Tabs` and `ListBox` mark their selection themselves with `aria-selected` and get no `aria-current` on top.

### A visual heading is a heading element

Text drawn to read as a heading is `<h1>`-`<h6>`, never a `<p>` carrying a heavier weight: the structure a screen
reader navigates by is the element, not the look. `EmptyPanel` renders its message as a heading once it is given a
description under it, and as a paragraph when it stands alone.

### Landmarks

The content panel always holds exactly one `main`, whichever of its views is on screen — the terminal pane, or the
welcome screen that replaces the three panes while there is no console. A view that is a region rather than the
window's main content carries `role="region"` with a name (`<section aria-label>`), per the naming rule above.

### Announcing a change

State that changes without the user acting (connection, a terminal problem) is announced from a `role="status"`
element, an error from `role="alert"`.

- A `role="status"` region meant to announce text that comes and goes stays mounted, empty when there is nothing to
  say, and only its contents change. One rendered conditionally, already holding its text when it is inserted, is not
  reliably announced (VoiceOver in WebKit among others).
- Even a region that stays mounted afterwards is not announced for the text it was first rendered with.
  `StatusAnnouncer` defers its first write for that, so announce a state with it.
- VoiceOver also drops a status written within about 300 ms of a focus move (a dialog opening, say), so a surface that
  takes focus as it opens delays its first status write (`StatusAnnouncer`'s `firstWriteDelayMs`).
- A state whose visible form is durable copy rather than a placeholder — the sentence saying where a macOS permission
  stands, in `packages/ui/src/settings/PermissionsSection.tsx` — announces from a `role="status"` on that copy itself
  instead. `StatusAnnouncer` is `sr-only` and expects the visible form to be `aria-hidden`, which this cannot be: it
  is the row's own description, and a reader going through the row statically has to reach it. Only the first text is
  lost that way, and that is the state as the surface opened, which the user is reading anyway; every later change —
  the one that matters here, made in System Settings and picked up on the way back — lands in a region that has been
  mounted all along. Put the subject inside the region when several of them can change independently: `role="status"`
  is atomic, so a bare "Granted." names nothing.
- HeroUI's `Alert` renders no role at all, its name notwithstanding. An error that can appear in one after the user has
  moved on puts `role="alert"` on an element of your own inside it.

## Contrast

### Thresholds

- Text: at least 4.5:1 against its background (3:1 for large text).
- An icon or a state indicator: at least 3:1 against what it sits on.
- A boundary: at least 3:1 too, when it alone shows where a control is.

### Do not change how a HeroUI component looks

Contrast is not a reason to change how a HeroUI component looks. The user had the `style.css` overrides that gave every
field and checkbox a `--muted` border removed, because HeroUI's own are borderless, so never add a border, frame or
outline that HeroUI does not draw itself.

### Form controls on a surface

A form control on a surface — in a dialog, a popover, a card — takes HeroUI's `variant="secondary"` (`TextField`,
`Input`, `Select`, `Checkbox` alike), HeroUI's own variant for that case. The default variant leaves it blending into
the surface in both appearances.

### Hand-built rows and cards

A hand-built row or card that its content already identifies takes HeroUI's separator colour for its outline
(`border-separator`, as the focus-mode session cards do), even though that is under 3:1. Its selected state still has to
reach 3:1 (`border-accent`).

### HeroUI's own text colours

HeroUI's text colours do need checking.

- Its stock light `--muted` (`text-muted`, its secondary text) reached 4.5:1 only on the white surfaces and fell short
  on `--background` and `--default`, where its own components put it. `style.css` therefore darkens the light value
  slightly at the token level.
- Even so, the `Tabs` list dims a hovered tab to 70% opacity, which brings its muted label down to about 3:1, so every
  `Tabs.Tab` in `packages/ui` overrides the hover to darken the label instead (`hover:text-foreground
  hover:opacity-100`), with the reason in a comment at each of the three tab lists.

So measure each text and indicator colour a HeroUI component draws by default against what it actually sits on, in both
appearances, rather than assuming it passes.

### Where to fix a shortfall

- Fix a shortfall in that colour alone with a utility class on that part. Tailwind's utilities layer overrides HeroUI's
  components layer without `!`.
- Fix it at the token in `style.css` when the token itself falls short.

### Finding every rule that draws a colour

A fix at a token reaches only the rules that read that token, and HeroUI draws the same-looking text from more than one
rule. So grep its compiled stylesheet (`packages/ui/node_modules/@heroui/styles/dist/heroui.min.css`) for the token and
for the CSS property both, and look for the elements no rule of either kind matches.

Those elements fall through to Tailwind's preflight, which derives the colour from the element's own `currentcolor` — a
placeholder on anything but HeroUI's four input classes is half the foreground over the field's own fill. Such text
moves with whatever fill the element sits on, and a change at the token never reaches it.

### Surface-filled components in a dialog or popover

HeroUI's `--surface` equals its `--overlay` in both appearances. A surface-filled component (`Alert`, `Card`) inside a
dialog or popover is therefore set apart only by its shadow, which does not show in the dark appearance. Give it a fill
of its own there (a tint such as `bg-warning/10 shadow-none`).

### Auditing a change to a colour token or a surface

A change to a colour token or a surface touches every screen, so its audit covers every gallery scenario in both
appearances (the user rejected a re-theme whose contrast had been spot-checked). It also measures what a pass over text
and glyphs misses:

- Every fill — a chip, a tertiary button, a ghost button's hover — against the surface it sits on.
- An animated state at its faintest frame. An `animate-pulse` glyph sank to about 2:1; animate such a glyph without
  lowering its opacity.
- A translucent fill (HeroUI's soft chip is 15% of its colour) as drawn: composite it over each surface it actually
  lands on — a selected and a hovered row, a dialog's `--overlay` — and measure the label against that. Measuring
  against the bare surface or the fill's own colour overstated one chip's contrast and missed a 4.5:1 failure on
  selected rows.

### A region with a surface colour of its own

A region given a surface colour of its own needs HeroUI's `--default`-filled controls checked on it in particular: the
dark sidebar landed on `--default`'s own lightness and every such control vanished.

- Override the whole `--default` family on that subtree, as `.sidebar-fills` and `.control-fills` in
  `packages/ui/src/style.css` do. `--default-hover` and `--default-soft` are computed on `<html>`, and `--default` alone
  does not reach them.
- Such an override moves the ground under the text HeroUI draws on those fills too, so its audit covers every control
  the family reaches in that subtree, label and glyph, resting and hovered.
- The label's colour is not necessarily the foreground, because an enclosing HeroUI component may have imposed one
  already: `.modal__body` applies `text-muted`, which everything inside a dialog's body inherits, controls portalled in
  from elsewhere included. So a label that needs 4.5:1 on the new fill is given `text-foreground` there explicitly
  rather than assumed to have it.
- Those texts also pin how far such a fill may go. Settle its lightness from them first — the darkest label and the
  most muted one HeroUI puts on the family, in both appearances — rather than picking the fill for how it looks and
  auditing afterwards, or each nudge for looks frees one label and leaves another under 4.5:1.

### A glyph or tint on the window material

In the macOS app the top bar and the rail show a translucent window material whose colour shifts with whatever is behind
the window (see "The window chrome and the content panel" in `docs/product/window-layout.md`). A glyph or tint added to
that chrome is measured against the material's measured colours, given in the comment on the window's surfaces in
`packages/ui/src/style.css`, not only against `--chrome`, which is just the browser's stand-in for the material. The
dark material is a grey far lighter than dark `--chrome`.

## Pointer target size

A control's pointer target is at least 24 by 24 CSS pixels (WCAG 2.2 AA 2.5.8). HeroUI's own sizes already clear it —
its smallest icon-only button is 32 — so this only bites where a control is drawn smaller than HeroUI would draw it.

Neither exception in the criterion is easy to rely on here. *Spacing* needs a 24-pixel circle centred on the target to
touch no other target, which a small control placed over a larger one (a delete badge on a corner) fails by
construction. *Equivalent* needs the same action to be available from another control on the same view, which a lone
affordance does not have.

Where the drawn size has to stay under 24, extend the target rather than the drawing, as HeroUI does for its own
select clear button (`.select__clear-button:after{inset:-2px}`): an absolutely positioned `::after` with a negative
inset enlarges what the pointer hits and changes nothing visible. The avatar field's delete button
(`packages/ui/src/dialogs/AvatarField.tsx`) is drawn at 20 and hit at 24 that way.

## Not by colour alone

A status or state that differs in colour also differs in glyph, shape or text.

**A two-state view control carries its state in its icon.** The shape is one icon button whose icon is the state it is
in and whose accessible name and tooltip say what pressing will do, so the name alternates with the state (see
"Two-state icon toggles" in `docs/product/labels-and-tooltips.md`). Nothing distinguishes the pressed button from the
unpressed one by fill: `.icon-toggle` in `packages/ui/src/style.css` resets HeroUI's selected tokens back to the
resting ones, and `aria-pressed` carries the state for assistive technology. The window chrome's two pane toggles were
already built this way; the Git mode's group-by-folder choice and the viewer's three view controls now are too.

**One kept shortfall is left, and it is a single control**: the settings dialog's three-way appearance choice, which
is a toggle group where the selected button differs by fill colour only. Which button is selected is also carried by
its position and by `aria-pressed`, and the fill difference meets 1.4.11's 3:1 for a state. The reasoning and the
measured ratios sit in the comment above `.toggle-button-group .toggle-button` in `packages/ui/src/style.css`; a
two-state choice does not belong in that group, because the icon form above has no shortfall to accept.

## Motion

An animation or transition that is not essential stops under `prefers-reduced-motion: reduce` (Tailwind's
`motion-safe:` / `motion-reduce:` variants).
