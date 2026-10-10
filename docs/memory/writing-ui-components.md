# Writing UI components

## Build from HeroUI 3's own component wherever it has one

In `packages/ui`, before writing a component, check whether HeroUI 3 already provides it, and if so use that one
rather than assembling a look-alike from other HeroUI parts or plain elements. The installed package's
`packages/ui/node_modules/@heroui/react/dist/components/` is the authoritative list for the version in use (it
includes less obvious ones such as `toast`, `drawer`, `disclosure`, `toolbar`, `empty-state`, `kbd`, `skeleton`);
HeroUI's own docs are the reference for how to use each, but only a component's own file in that directory shows what
it puts in the DOM — a hardcoded attribute, which child it treats as a given part, which props it drops — and a
decision about wrapping or naming one usually turns on that, so read it rather than its typings. A hand-assembled
stand-in looks and behaves unlike the rest of the UI and lacks what HeroUI's carries: the toast stack built from
`Alert` + `CloseButton` with its own timers and stacking had none of `Toast`'s pause on hover and focus, queueing or
ARIA region semantics.

- Style it through its own variants, slots and the theme tokens, not by overriding it into something else. An
  override on a HeroUI component stays only for a colour fix accessibility needs or for layout and sizing (filling a
  column, truncating); one that only changes its look to taste goes. So when asked to strip custom styling, sort each
  override by that reason rather than removing them wholesale: dropping a sizing override with the look ones broke
  the layout.
- Give a compound component's layout classes to its parts themselves and never put an element of your own between
  two parts: some of HeroUI's variant styles use direct-child selectors (`.tabs--secondary > .tabs__list-container`,
  the `switch--sm` / `switch--lg` sizes), so a wrapper `div` silently switches the variant off. A parent may also pick
  a part out of its children in JavaScript, and only among its *direct* ones (`Tag` scans for `Tag.RemoveButton` that
  way), so a part wrapped in an element of yours — the project's own `TitledControl` among them — leaves the parent
  rendering its default part beside yours. Where the parent takes a render-function child, as `Tag` does, that form
  skips the scan and is how to wrap a part.
- The project's existing wrappers over a HeroUI component (`Dialog` over `Modal`, `ActionMenu` over `Dropdown`) are
  that component; use the wrapper where one exists. The same goes for the shared dialogs built on them: a
  confirmation, including one where the user types a word to confirm a deletion, is `ConfirmDialog`
  (`packages/ui/src/dialogs/ConfirmDialog.tsx`, its `typeToConfirm`), never a dialog of its own.
- A hand-built element is acceptable only when HeroUI has no equivalent, or its equivalent cannot meet a stated
  requirement. Then build it on react-aria / react-aria-components hooks, as HeroUI itself is, not on bare DOM event
  handling, and put a comment at the component saying why HeroUI's is not used (a design-intent comment under the
  "Comment conventions" section of `.claude/skills/agent-docs/SKILL.md`).

## Focus dropped to `<body>`: a HeroUI control's press, a hidden pane, a reordered list

Clicking around the terminal must not move keyboard focus off it (see the "Rows, names and keyboard focus" section of
`docs/product/sidebar.md`). In `packages/ui` every pressable HeroUI 3 control (`Button`,
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

Reordering does it too: React reorders keyed children by moving their DOM nodes, and moving the node that holds focus
blurs it to `<body>`, so a keyboard user on a row loses their place whenever a live status update re-sorts the list.
Render any list whose order can change while one of its rows may hold focus through `useFlip`
(`packages/ui/src/sidebar/useFlip.ts`), which puts focus back after the move, rather than beside it.

A wrapper that renders nothing once its last child is gone takes focus down with it, and a library's own rescue does
not reach that case: react-aria's `useTagGroup` focuses its list element when the last tag is removed, but a list
that unmounts in the same commit is gone before the effect runs, and one laid out with `display: contents` (as both
tag groups in `packages/ui` are, to flow their tags into a row of the parent's) cannot be counted on to take focus. So
move focus to a named element yourself, in the removal handler, rather than leave it to the library: `ProjectFilterTags`
in `packages/ui/src/sidebar/ProjectFilter.tsx` hands it to `returnFocusTo` through `handFocusOff`
(`packages/ui/src/components/handFocusOff.ts`) on the removal that empties the group, and `TagsInput` in
`packages/ui/src/dialogs/TagsInput.tsx` hands it to its text input on every removal, a dialog being where focus on
`<body>` costs Escape and Tab as well.

## Inside a dialog, a focused control that unmounts or turns disabled takes Escape and Tab with it

When the focused element is removed, WebKit and Chrome fire no `focusout` and focus falls to `<body>`. A dialog's
Escape handling and Tab containment hang off focus being inside it, so both stop working. Whenever a dialog replaces
content that can hold focus — a listing swapped for the next one, a queue moving on to its next item — put focus back
with `useRefocusIfLost` from `packages/ui/src/dialogs/Dialog.tsx`, or pass the shared `Dialog` a `resetKey` naming the
current item, which does it for you.

Do not switch a dialog to its next subject by re-keying it (`key={item.id}`): the remounted modal records the
outgoing one's soon-detached element as its focus-restore target, so focus is lost again when it closes. Keep it
mounted and reset the per-item state from `resetKey` instead (`useDialogAction(resetKey)` does that for the error and
busy state).

A control that stays mounted costs the same when it turns disabled: react-aria's `isDisabled` puts the native
`disabled` attribute on the element, which blurs the control the user has just pressed. So a button disabled for
the duration of its own action — the obvious way to keep a slow request from being sent twice — drops focus to
`<body>` and takes the dialog's Escape and Tab with it until the action ends. Mark it `isPending` instead: that blocks
press and hover, keeps the control focusable, and announces the pending state. Keep `isDisabled` for a control
that is unavailable for a reason outside the action itself (a field not filled in yet), where focus is not on it.

## A control inside a pane never sees Escape first: the window's listener has it

While a drawer or a floating pane is open, `packages/ui/src/layout/usePaneToggles.ts` holds a capture-phase `keydown`
listener on `window` that dismisses the overlay. It runs before React's own dispatch from the root container, so
nothing inside the React tree can keep Escape from it — not even `onKeyDownCapture` on a wrapper around the control.
So a control inside a region marked `data-escape-scope` (the sidebar, the aside, the terminal pane, the top bar, the
rail, the scrim) that needs Escape for itself is accounted for in that listener's own test instead (`holdsText`, which
leaves the key to a text field that holds text), never guarded locally. A control in a popover or a dialog portalled
to `<body>` sits outside every scope and is unaffected, which is the only reason the sidebar's project filter and the
dialogs' fields never needed this.

Check it where it shows: the listener is installed only while an overlay is up, so the symptom appears only below the
layout breakpoint, with the pane as a drawer, or with a docked pane hidden and floated in on hover. A gallery scenario
at a normal width, both panes docked, never reaches it.

## A single-line field hands its form the trimmed value

Leading and trailing whitespace is never meant, so a form never holds it: every single-line text field in
`packages/ui` goes through `useTrimmedField` (`packages/ui/src/dialogs/useTrimmedField.ts`), which `TextInput` already
does. The form's state, its validation, its submit (Enter included) and any request fired from it see the trimmed value
at once, while the field keeps what was typed until it loses focus, so a space between two words is not stripped on its
way to the next one. A new field that is not a `TextInput` takes the hook's result as its `value`, `onChange` and
`onBlur`, and its form does not call `.trim()` on the value; the project filter's search field does the same. Left
alone on purpose: a free-text area and the tags field's input, which trims what it adds as a tag itself.

## What is wrong with one field shows under that field, from the first submit on

In a `packages/ui` dialog, a message about one field's value — empty, malformed, not allowed — belongs to that
field: `TextInput` (`packages/ui/src/dialogs/TextInput.tsx`) takes it as `errorMessage`, which marks the field
invalid and renders the message right under it. `useDialogAction().setError` and `DialogError` carry only the
failure of the request itself, at the foot of the dialog, and never a verdict on a single field.

Compute each field's message on every render from the current value and pass it through `useSubmitValidation`
(`packages/ui/src/dialogs/Dialog.tsx`): `shown(message)` holds it back until `attempt(...messages)` has gated a
submit, so no field is marked before the user has tried to submit, and from then on the message follows what is
typed. Gate the submit on what `attempt` answers rather than re-checking the values there.

Marking a field invalid yourself carries two conditions wherever that field is built — `TextInput` already meets
both, a field assembled anywhere else has to:

- Pass `validationBehavior="aria"` to the react-aria-components field. It defaults to `"native"`, which feeds a
  controlled `isInvalid` to the browser's own constraint validation (`setCustomValidity`); the browser then refuses
  the `<form>` submit before the dialog's `onSubmit` runs, and since the mark only ever appears from that handler,
  the dialog locks itself out of submitting at all.
- Put the `role="alert"` that announces the message on an element of your own inside `FieldError`, not on
  `FieldError`. react-aria passes a component's props through `filterDOMProps`, which keeps only `id`, the
  `aria-label` / `aria-labelledby` / `aria-describedby` / `aria-details` family, link props, `dir` / `lang` /
  `hidden` / `inert` / `translate`, the global events and `data-*`; anything else — `role` among them — is dropped
  without a warning, and a message that appears only on submit is then never announced at all.

## A react-aria-components element owns its state `data-*` attributes: it writes over yours, and may never set its own

A react-aria-components element spreads the props it is given first and then sets the state attributes it owns —
`data-selected`, `data-focused`, `data-hovered`, `data-pressed`, `data-expanded`, `data-disabled` and the like — so a
value you pass under one of those names is replaced without a warning, and a style keyed on it follows react-aria's
state rather than yours. HeroUI's components render react-aria-components elements and behave the same. To mark a
state of your own on one (a tree row that is the current file while the tree has no react-aria selection, say), use a
`data-*` name that the component's own file under `packages/ui/node_modules/react-aria-components/dist/private/` does
not write, such as `data-current` on a `TreeItem`, or a class.

It sets one of its own only while the hook behind it is enabled for that element's configuration, so a style keyed on
it (`data-hovered:`, `group-data-hovered/<name>:`) can silently never apply: a `Tag` tracks hover only when its group
allows selection or has an action, so a tag that is only removable never gets `data-hovered`. Before keying a style
on a state attribute, check in that same file that the hook setting it is not disabled for your configuration; where
it is, use the CSS pseudo-class through Tailwind (`hover:`, `group-hover/<name>:`). The jsdom tests never hover, so
only a real pointer in the running page shows the miss.

## A HeroUI `ListBox` or a `Tree`: where its padding and its fade go

For a list rendered inside react-aria's `Virtualizer` with `ListLayout` (a HeroUI `ListBox`, one in a `Select`'s
popover included, or a react-aria `Tree` or `GridList`):

- `ListLayout` makes every row as wide as the scroll container's inner width and then places it inside the
  container's padding, so any side padding on the list — HeroUI's own `p-1` on `ListBox`, `p-1.5` on a `Select`
  popover's list — makes it overflow sideways (a horizontal scrollbar) and clips the rows' focus rings. Clear it with
  `px-0` on the list and give the inset to the rows and headings instead (`mx-1` on an item, with `w-auto` where the
  item's own styles set a width as HeroUI's `ListBox.Item` does, and padding on a `Header`). HeroUI corrects this only
  inside its `Autocomplete`.
- The list has to be the element that scrolls, so a HeroUI `ScrollShadow` wrapped around it fades nothing. Put the
  fade on the list itself through `useScrollFade` (`packages/ui/src/components/useScrollFade.ts`).

Virtualized or not, a `Select` whose items show `ListBox.ItemIndicator` needs `pe-7` on each item: the `Select`
popover's styles give its items `px-2.5`, which overrides the `pe-7` `ListBox.Item` reserves for the check mark, so a
long label otherwise runs under the mark.

## A Tailwind class name has to stand in the source as literal text

Tailwind 4 reads the source as plain text and emits a utility only for a class name it can find spelled out there, so
a name assembled at runtime — `` `${side}-0` ``, a suffix appended to a prefix, anything concatenated — compiles to no
CSS at all and the element silently loses that property. Write each variant out in full and branch between them.

Nothing on the way past catches it: `packages/ui` has no linter, and its tests (`vitest`, components under jsdom) never
evaluate the CSS, so a class whose utility was never emitted is valid TypeScript, passes the tests, builds clean and
reads fine in a diff. After adding or changing a utility class, grep the built `packages/ui/dist/assets/*.css` for it.

## Dim a region with a veil, not `opacity` on it

In the app's WKWebView, `opacity` below 1 on an ancestor of anything that fades (an opacity transition, such as a
row's hover controls) or sits on its own GPU layer (HeroUI's buttons are `transform-gpu`) makes WebKit paint those
descendants as blank tiles, and patching the descendants one by one did not hold. To dim a region, lay a veil of the
surface colour over it instead: an `after:` pseudo-element with `after:pointer-events-none after:absolute
after:inset-0` and a translucent fill such as `bg-surface/55` on a `relative` container, as the sidebar's inactive
projects in `packages/ui/src/sidebar/Sidebar.tsx` do.

## An icon-only control also gets a tooltip, through `TitledControl`

Every control that shows only an icon — a HeroUI `Button` with `isIconOnly`, a `CloseButton` or `Modal.CloseTrigger`,
a menu trigger — has a HeroUI tooltip in addition to its `aria-label`. The user wants tooltips short: the tooltip says
what the control does ("More actions"), while the `aria-label` carries whatever tells it apart from its neighbours for
assistive technology ("Actions for session <title>" on each row's menu), so the two differ wherever the accessible
name needs that context; `ActionMenu` already does this by default. The exception is a control inside a react-aria
collection row, such as a tag's remove button, whose accessible name react-aria already composes from the button and
the row through `aria-labelledby`: there the `aria-label` is the bare verb with no placeholder, or the row's own text
is announced twice. Give it one by wrapping the control in `TitledControl` from
`packages/ui/src/components/TitledControl.tsx`, as the window chrome's `ChromeButton` in
`packages/ui/src/components/ChromeButton.tsx` does. Passing `title` to the HeroUI control itself does nothing: its
react-aria base filters `title` out of the DOM props without a warning.

Any tooltip on a control takes this shape — the control directly inside HeroUI's `Tooltip`, as `TitledControl` does —
never `Tooltip.Trigger`, which renders a focusable `role="button"` `div` around the control: an extra tab stop that
also takes focus on a mouse press, undoing `preventFocusOnPress`. The shape works only when the child is itself a
react-aria-components control (any HeroUI button or trigger), which picks the tooltip's hover and focus handling up
from context; any other child — a plain element, a `span` around the control — gets no tooltip, with no warning.

The one sanctioned exception is a tooltip on a non-control indicator (a badge, a status mark), which has no
react-aria-components control to hang the shape on: `IndicatorTooltip` in
`packages/ui/src/components/IndicatorTooltip.tsx`. It is hover-only by design and fixes what a bare `Tooltip.Trigger`
gets wrong: the trigger hardcodes `role="button"` (so one left without a `role` exposes an unnamed button) and is
focusable, so it always sets `tabIndex={-1}`, cancels `mousedown` so a press does not take focus off the terminal, and
takes either a `label` (`role="img"` and `aria-label`) or `role="presentation"` (browsers ignore that role on a
focusable element, so it only keeps the unnamed button role off). A deliberately focusable indicator would use
`Tooltip.Trigger` itself.

## Every string the user reads goes through the catalog, in a shape a translation can follow

Any text a user reads in `packages/ui` — labels, tooltips, accessible names, toasts, notifications, the native menu's
labels — is a message in `packages/ui/src/i18n/messages/en.ts` and in every translated catalog registered in
`CATALOGS` (type checking fails on a translated catalog that lacks it). Look it up like this:

- In a component, through `useT()` from `packages/ui/src/i18n/react.tsx`. The module-level `t()` in
  `packages/ui/src/i18n/language.ts` is for code outside React only: a component that calls it keeps showing the old
  language after the user switches, with no error.
- A helper that returns display text takes the `Translate` function as its first parameter (as `statusLabel(t, status)`
  in `packages/ui/src/sessionLabel.ts` does) instead of calling `t()` itself.
- A module-level table (options, menu items, rows) stores a `PlainMessageKey` and looks it up at render, never text
  resolved when the module loads.
- A sentence with a variable part is one message with a `{placeholder}`, never translated fragments joined in code,
  since word order differs between languages; markup inside a sentence goes through `<Message id params>`. Anything
  that depends on a count is a plural message selected by `count`, never `count === 1 ? … : …`.
- A HeroUI part can carry an English string of its own that no catalog sees: `Tag.RemoveButton` renders
  `aria-label="Remove tag"` ahead of its spread props, which both ships that literal when you pass no `aria-label` and
  shadows the localized name react-aria would otherwise supply through the part's slot. So for every part you give no
  visible text, read its file for a hardcoded string and pass your own message over it.

Only what the user reads in Octoboard's own UI is translated; text aimed at an agent stays English (see "What follows
the language" in `docs/product/language.md`).

## Lay out by reading direction; keep physical only what never mirrors

Under a right-to-left language the whole window mirrors, except what the "Right-to-left layout" section of
`docs/product/window-layout.md` lists as not mirrored. Nobody sees a regression here without switching to Arabic, so
build it in from the start:

- Position, spacing, borders, corners and alignment use Tailwind's logical utilities (`start-*` / `end-*`, `ms-*` /
  `me-*`, `ps-*` / `pe-*`, `border-s`, `rounded-s-*`, `text-start`), not `left` / `right` / `ml` / `pr` and the like.
  `translate-x-*` has no logical form, so a slide gets an `rtl:` counterpart with the opposite sign.
- An icon that points a direction mirrors with `rtl:-scale-x-100`. An expand / collapse chevron switches between two
  glyphs (forward when collapsed, down when expanded) instead of rotating one: a mirrored forward chevron turned a
  quarter points up.
- Content whose direction is not the UI's sets its own: a path `dir="ltr"`, a name the user typed `dir="auto"`
  (`FadeOverflow` takes the same `dir` and fades along it).
- An `rtl:` utility wins over a `docked:` one on the same property, so a `docked:` value for a property also set under
  `rtl:` needs a `docked:rtl:` twin, or it silently does not apply under right-to-left.

## Only icon-only chrome sits on the window material; text goes on the opaque panel

In the macOS app the top bar and the rail show the translucent window material (see "The window chrome and the
content panel" in `docs/product/window-layout.md`), whose colour shifts with whatever is behind the window. The user
rejected a layout that put the text-heavy sidebar on it, and one that floated the content as a card with gutters of
material around it. So nothing beyond an icon, an avatar or the breadcrumb's one line goes on the chrome; anything
else belongs in the opaque content panel. A glyph or tint added to the chrome is measured against the material's
measured colours (in the comment on the window's surfaces in `packages/ui/src/style.css`), not only against
`--chrome`, which is just the browser's stand-in for it: the dark material is a grey far lighter than dark `--chrome`.

## The UI meets WCAG 2.2 AA

Every control and view in `packages/ui` meets WCAG 2.2 level AA, checked in both the light and the dark appearance.
The user has kept a few shortfalls on purpose, each marked by a comment at its code saying so; leave one that is
marked as it is, and do not add a new exception without the user's say. Concretely:

- **Keyboard**: every interactive control is reachable from the keyboard and operable with Enter / Space. Tab is how it
  is reached, except for a control inside the row of a react-aria collection that manages focus itself (a `GridList`, a
  `Tree`): there the row is the tab stop and Tab moves past the whole list, while the arrow key that runs along the row
  (`keyboardNavigationBehavior` defaults to `"arrow"`) moves between the row and its controls. That is the grid and tree
  pattern, it meets this rule, and both the change list's row actions and the Files tree's chevron rely on it, so do not
  give such a control a tab stop of its own. `preventFocusOnPress` only keeps a *mouse* press from taking focus; the
  control still has to be reachable. Every button is HeroUI's `Button`; a native `<button>` is allowed only where a
  custom button is needed that HeroUI's cannot be, with the reason in a comment there. A hand-built `role="button"`
  element needs `tabIndex={0}` and its own Enter / Space handling. The terminal keeps Tab and Shift+Tab for the agent,
  so F6 / Shift+F6 region cycling (`packages/ui/src/layout/useRegionCycle.ts`) is the only keyboard way out of it: every
  new region of the window that holds controls — a pane, a panel, an overlay — is marked `data-region` and added to that
  hook's `REGIONS` and `shown`, or F6 skips it and a keyboard user in the terminal has no way to reach it.
- **Visible focus**: HeroUI controls draw their own focus ring. A hand-built focusable element that removes the
  outline puts a ring back, never `outline-none` alone, and takes it from `useFocusVisibleProps`
  (`packages/ui/src/components/useFocusVisibleProps.ts`): spread its props on the element and style with
  `data-focus-visible:ring-2 data-focus-visible:ring-focus` (`group-data-focus-visible:` from a child). Never style
  focus with CSS `:focus-visible` — no `focus-visible:` or `group-focus-visible:` variant, no
  `:has(:focus-visible)` selector. Once the last focus came from a click, the app's WKWebView does not match it on an
  element focused by script, even right after a key press, so every keyboard move made by script — F6 region
  cycling, Tab inside a react-aria focus-trapped dialog — lands without a ring. Playwright's `webkit` matches it in
  those cases, so only the packaged app shows the miss. A keyboard handler that moves focus itself calls react-aria's
  `setInteractionModality("keyboard")` first when the handler sits on the window's capture phase and stops
  propagation (react-aria tracks modality from a capture-phase listener on the document, which the key then never
  reaches), or what it focuses draws no ring when the previous input was a pointer.
- **Name**: every control has an accessible name — its visible text, or an `aria-label` when it has none. An icon
  beside a name is `aria-hidden="true"`; an icon that is the only carrier of a meaning gets `role="img"` and an
  `aria-label`. An element that takes an `aria-label` without being a control — a scrolling `div` or `pre` made
  focusable with `tabIndex={0}` — also gets a role that can be named (`role="region"`): ARIA prohibits naming a
  generic element, so the name may never be spoken.
- **Roles and states**: a control that shows or hides a region carries `aria-expanded`; the selected row of a
  hand-built list or tree carries `aria-current` (as the sidebar rows do), while HeroUI's `Tabs` and `ListBox` mark
  their selection themselves with `aria-selected` and get no `aria-current` on top; state that changes without the
  user acting (connection, a terminal problem) is announced from a `role="status"` element, an error from
  `role="alert"`. A `role="status"` region meant to announce text that comes and goes stays mounted, empty when there
  is nothing to say, and only its contents change: one rendered conditionally, already holding its text when it is
  inserted, is not reliably announced (VoiceOver in WebKit among others). Even a region that stays mounted afterwards
  is not announced for the text it was first rendered with; `StatusAnnouncer` defers its first write for that, so
  announce a state with it. VoiceOver also drops a status written within about 300 ms of a focus move (a dialog
  opening, say), so a surface that takes focus as it opens delays its first status write
  (`StatusAnnouncer`'s `firstWriteDelayMs`). HeroUI's `Alert` renders no role at all, its name notwithstanding, so
  an error that can appear in one after the user has moved on puts `role="alert"` on an element of your own inside
  it.
- **Contrast**: text at least 4.5:1 against its background (3:1 for large text); an icon or a state indicator at
  least 3:1 against what it sits on, and a boundary too when it alone shows where a control is. Contrast is not a
  reason to change how a HeroUI component looks: the user had the `style.css` overrides that gave every field and
  checkbox a `--muted` border removed, because HeroUI's own are borderless, so never add a border, frame or outline
  that HeroUI does not draw itself. A form control on a surface — in a dialog, a popover, a card — takes HeroUI's
  `variant="secondary"` (`TextField`, `Input`, `Select`, `Checkbox` alike), HeroUI's own variant for that case: the
  default variant leaves it blending into the surface in both appearances. A hand-built row or card that its content already identifies takes HeroUI's
  separator colour for its outline (`border-separator`, as the focus-mode session cards do), even though that is
  under 3:1; its selected state still has to reach 3:1 (`border-accent`).
  HeroUI's text colours, on the other hand, do need checking: its stock light `--muted` (`text-muted`, its secondary
  text) reached 4.5:1 only on the white surfaces and fell short on `--background` and `--default`, where its own
  components put it, so `style.css` darkens the light value slightly at the token level. Even so, the `Tabs` list
  dims a hovered tab to 70% opacity, which brings it down to about 3:1. So measure each text and indicator colour a
  HeroUI component draws by default against what it actually sits on, in both appearances, rather than assuming it
  passes, and fix a shortfall in that colour alone with a utility class on that part (Tailwind's utilities layer
  overrides HeroUI's components layer without `!`), or at the token in `style.css` when the token itself falls
  short. HeroUI's `--surface` equals its `--overlay` in both
  appearances, so a surface-filled component (`Alert`, `Card`) inside a dialog or popover is set apart only by its
  shadow, which does not show in the dark appearance; give it a fill of its own there (a tint such as
  `bg-warning/10 shadow-none`).
  A change to a colour token or a surface touches every screen, so its audit covers every gallery scenario in both
  appearances (the user rejected a re-theme whose contrast had been spot-checked), and it measures what a pass over
  text and glyphs misses: every fill — a chip, a tertiary button, a ghost button's hover — against the surface it sits
  on, and an animated state at its faintest frame (an `animate-pulse` glyph sank to about 2:1; animate such a glyph
  without lowering its opacity). A translucent fill (HeroUI's soft chip is 15% of its colour) is measured as drawn:
  composite it over each surface it actually lands on — a selected and a hovered row, a dialog's `--overlay` — and
  measure the label against that; measuring against the bare surface or the fill's own colour overstated one chip's
  contrast and missed a 4.5:1 failure on selected rows. A region given a surface colour of its own needs HeroUI's `--default`-filled
  controls checked on it in particular: the dark sidebar landed on `--default`'s own lightness and every such control
  vanished. Override the whole `--default` family on that subtree, as `.sidebar-fills` in `packages/ui/src/style.css`
  does, since `--default-hover` and `--default-soft` are computed on `<html>` and `--default` alone does not reach
  them.
- **Not by colour alone**: a status or state that differs in colour also differs in glyph, shape or text.
- **Motion**: an animation or transition that is not essential stops under `prefers-reduced-motion: reduce`
  (Tailwind's `motion-safe:` / `motion-reduce:` variants).
