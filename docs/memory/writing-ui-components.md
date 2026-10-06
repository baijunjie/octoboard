# Writing UI components

## Build from HeroUI 3's own component wherever it has one

In `packages/ui`, before writing a component, check whether HeroUI 3 already provides it, and if so use that one
rather than assembling a look-alike from other HeroUI parts or plain elements. The installed package's
`packages/ui/node_modules/@heroui/react/dist/components/` is the authoritative list for the version in use (it
includes less obvious ones such as `toast`, `drawer`, `disclosure`, `toolbar`, `empty-state`, `kbd`, `skeleton`);
HeroUI's own docs are the reference for how to use each. A hand-assembled stand-in looks and behaves unlike the rest
of the UI and lacks what HeroUI's carries: the toast stack built from `Alert` + `CloseButton` with its own timers and
stacking had none of `Toast`'s pause on hover and focus, queueing or ARIA region semantics.

- Style it through its own variants, slots and the theme tokens, not by overriding it into something else.
- The project's existing wrappers over a HeroUI component (`Dialog` over `Modal`, `ActionMenu` over `Dropdown`) are
  that component; use the wrapper where one exists.
- A hand-built element is acceptable only when HeroUI has no equivalent, or its equivalent cannot meet a stated
  requirement. Then build it on react-aria / react-aria-components hooks, as HeroUI itself is, not on bare DOM event
  handling, and put a comment at the component saying why HeroUI's is not used (a design-intent comment under the
  "Comment conventions" section of `.claude/skills/agent-docs/SKILL.md`).

## Focus dropped to `<body>` cuts the terminal off: a HeroUI control's press, a hidden pane

Clicking around the terminal must not move keyboard focus off it (see the "The console → project → session menu"
section of `docs/product/sessions.md`). In `packages/ui` every pressable HeroUI 3 control (`Button`,
`Dropdown.Trigger` and the rest, all built on react-aria's press handling) focuses itself when pressed with the
mouse. For a control that unmounts right after the press, such as a list item's × or a toast's Dismiss, focus then
falls to `<body>` and the terminal silently stops receiving keystrokes.

So give `preventFocusOnPress` to every pressable control in the sidebar, the toasts, the banners and the terminal's
surroundings, unless its handler puts focus back on the terminal itself. A menu also needs it on its trigger, and on
top of that react-aria moves focus back to the trigger when the menu closes. Build every "⋯"-style menu on
`packages/ui/src/components/ActionMenu.tsx`, which handles both, rather than on a bare `Dropdown`.

Nothing in the markup gives this away — a plain `<button>` never takes focus on click in WKWebView — so the defect
only appears in the running page.

Hiding a region with CSS (`hidden`, a `docked:hidden` variant) while focus is inside it drops focus to `<body>` the
same way, with no event, and `preventFocusOnPress` does not help there: focus reaches a pane by Tab or by a click into
an iframe. So every code path that hides a region able to hold focus — a toggle, a scrim, Escape, a breakpoint change
— first hands focus to the terminal if the region contains `document.activeElement`. The docked panes already do this
through `releaseFocus` in `packages/ui/src/layout/usePaneToggles.ts`; hide a new pane through that hook rather than
beside it.

## Inside a dialog, a focused control that unmounts takes Escape and Tab with it

When the focused element is removed, WebKit and Chrome fire no `focusout` and focus falls to `<body>`. A dialog's
Escape handling and Tab containment hang off focus being inside it, so both stop working. Whenever a dialog replaces
content that can hold focus — a listing swapped for the next one, a queue moving on to its next item — put focus back
with `useRefocusIfLost` from `packages/ui/src/dialogs/Dialog.tsx`, or pass the shared `Dialog` a `resetKey` naming the
current item, which does it for you.

Do not switch a dialog to its next subject by re-keying it (`key={item.id}`): the remounted modal records the
outgoing one's soon-detached element as its focus-restore target, so focus is lost again when it closes. Keep it
mounted and reset the per-item state from `resetKey` instead (`useDialogAction(resetKey)` does that for the error and
busy state).

## A Tailwind class name has to stand in the source as literal text

Tailwind 4 reads the source as plain text and emits a utility only for a class name it can find spelled out there, so
a name assembled at runtime — `` `${side}-0` ``, a suffix appended to a prefix, anything concatenated — compiles to no
CSS at all and the element silently loses that property. Write each variant out in full and branch between them.

Nothing on the way past catches it: `packages/ui` has neither a linter nor a test suite, so `tsc --noEmit` is its only
automated gate, and a class whose utility was never emitted is valid TypeScript, builds clean and reads fine in a
diff. After adding or changing a utility class, grep the built `packages/ui/dist/assets/*.css` for it.

## An icon-only control also gets a tooltip, through `TitledControl`

Every control that shows only an icon — a HeroUI `Button` with `isIconOnly`, a `CloseButton` or `Modal.CloseTrigger`,
a menu trigger — has a HeroUI tooltip in addition to its `aria-label`, the same text. Give it one by wrapping the
control in `TitledControl` from `packages/ui/src/components/TitledControl.tsx`, as the title bar's `BarButton` in
`packages/ui/src/components/TitleBar.tsx` does. Passing `title` to the HeroUI control itself does nothing: its
react-aria base filters `title` out of the DOM props without a warning.

Any tooltip on a control takes this shape — the control directly inside HeroUI's `Tooltip`, as `TitledControl` does —
never `Tooltip.Trigger`, which renders a focusable `role="button"` `div` around the control: an extra tab stop that
also takes focus on a mouse press, undoing `preventFocusOnPress`. The shape works only when the child is itself a
react-aria-components control (any HeroUI button or trigger), which picks the tooltip's hover and focus handling up
from context; any other child — a plain element, a `span` around the control — gets no tooltip, with no warning.

## The UI meets WCAG 2.2 AA

Every control and view in `packages/ui` meets WCAG 2.2 level AA, checked in both the light and the dark appearance.
Concretely:

- **Keyboard**: every interactive control is reachable with Tab and operable with Enter / Space. `preventFocusOnPress`
  only keeps a *mouse* press from taking focus; the control still has to be a tab stop. Every button is HeroUI's
  `Button`; a native `<button>` is allowed only where a custom button is needed that HeroUI's cannot be, with the
  reason in a comment there. A hand-built `role="button"` element needs `tabIndex={0}` and its own Enter / Space
  handling. The terminal keeps Tab and Shift+Tab for the agent, so F6 / Shift+F6 region cycling
  (`packages/ui/src/layout/useRegionCycle.ts`) is the only keyboard way out of it: every new region of the window
  that holds controls — a pane, a panel, an overlay — is marked `data-region` and added to that hook's `REGIONS` and
  `shown`, or F6 skips it and a keyboard user in the terminal has no way to reach it.
- **Visible focus**: HeroUI controls draw their own focus ring. A hand-built control that removes the outline puts a
  ring back (`outline-none focus-visible:ring-2 focus-visible:ring-focus`, as the sidebar rows do), never
  `outline-none` alone. A keyboard handler that moves focus itself calls react-aria's
  `setInteractionModality("keyboard")` first when the handler sits on the window's capture phase and stops
  propagation (react-aria tracks modality from a capture-phase listener on the document, which the key then never
  reaches), or what it focuses draws no ring when the previous input was a pointer.
- **Name**: every control has an accessible name — its visible text, or an `aria-label` when it has none. An icon
  beside a name is `aria-hidden="true"`; an icon that is the only carrier of a meaning gets `role="img"` and an
  `aria-label`.
- **Roles and states**: a control that shows or hides a region carries `aria-expanded`; the selected row of a
  hand-built list or tree carries `aria-current` (as the sidebar rows do), while HeroUI's `Tabs` and `ListBox` mark
  their selection themselves with `aria-selected` and get no `aria-current` on top; state that changes without the
  user acting (connection, a terminal problem) is announced from a `role="status"` element, an error from
  `role="alert"`.
- **Contrast**: text at least 4.5:1 against its background (3:1 for large text); an icon, a state indicator or a
  boundary the user has to see at least 3:1 against what it sits on. HeroUI 3's default `--border`,
  `--border-secondary` and `--border-tertiary` all fall short of 3:1 in both appearances (1.2:1 to 2.7:1): they are
  for separators between content on one surface. A line that has to read as a boundary — a field, a checkbox, a
  pressable surface — takes `--muted` (at least 4.6:1 light on every surface, 6.7:1 dark). `style.css` already
  routes HeroUI's own field and checkbox borders through `--muted`, so a HeroUI form control needs nothing on top.
  HeroUI's text colours are no safer: its stock light `--muted` (`text-muted`, its secondary text) reached 4.5:1 only
  on the white surfaces and fell short on `--background` and `--default`, where its own components put it, so
  `style.css` overrides the light value at the token level. Even so, the `Tabs` list dims a hovered tab to 70%
  opacity, which brings it down to about 3:1. So measure each text and indicator colour a HeroUI component draws by
  default against what it actually sits on, in both appearances, rather than assuming it passes, and fix a shortfall
  with a utility class on that part (Tailwind's utilities layer overrides HeroUI's components layer without `!`), or
  at the token in `style.css` when the token itself falls short. HeroUI's `--surface` equals its `--overlay` in both
  appearances, so a surface-filled component (`Alert`, `Card`) inside a dialog or popover is set apart only by its
  shadow, which does not show in the dark appearance; give it a fill of its own there (a tint such as
  `bg-warning/10 shadow-none`).
- **Not by colour alone**: a status or state that differs in colour also differs in glyph, shape or text.
- **Motion**: an animation or transition that is not essential stops under `prefers-reduced-motion: reduce`
  (Tailwind's `motion-safe:` / `motion-reduce:` variants).
