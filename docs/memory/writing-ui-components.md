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
  column, truncating); one that only changes its look to taste goes. Even a colour fix never takes the form of a
  border, frame or outline HeroUI does not draw itself (the "Do not change how a HeroUI component looks" section of
  `docs/ui-accessibility.md`). So when asked to strip custom styling, sort each override by that reason rather than
  removing them wholesale: dropping a sizing override with the look ones broke the layout.
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

## A `docked:` utility loses to an `sm:` or an `rtl:` twin on the same property

`docked` is this project's one custom breakpoint and its token is in `px`, while Tailwind's built-in breakpoints are
in `rem`. Tailwind orders breakpoints by value only where it can compare them, so a px-valued one cannot be placed
against the rem-valued built-ins and its media block is emitted ahead of all of them; a selector variant such as
`rtl:` is emitted after every media variant. Either way the `docked:` rule comes first, so at equal specificity the
twin wins and the `docked:` value never applies — with no build warning and nothing to see in the source.

- Never pair an `sm:` / `md:` / `lg:` utility with a `docked:` one for the same property. Write the narrow side as
  `max-docked:` and leave the wide side to the inherited or the component's own value: `max-docked:` emits
  `@media (width < 1148px)`, the exact complement of the `docked:` band, and being a Tailwind utility it also wins
  inside that band over HeroUI's own prebuilt responsive rules, which sit in HeroUI's components layer.
- `rtl:` has no `max-` form, so a `docked:` value for a property also set under `rtl:` needs a `docked:rtl:` twin
  instead.

## Dim a region with a veil, not `opacity` on it

In the app's WKWebView, `opacity` below 1 on an ancestor of anything that fades (an opacity transition, such as a
row's hover controls) or sits on its own GPU layer (HeroUI's buttons are `transform-gpu`) makes WebKit paint those
descendants as blank tiles, and patching the descendants one by one did not hold. To dim a region, lay a veil of the
surface colour over it instead: an `after:` pseudo-element with `after:pointer-events-none after:absolute
after:inset-0` and a translucent fill such as `bg-surface/55` on a `relative` container, as the sidebar's inactive
projects in `packages/ui/src/sidebar/Sidebar.tsx` do.

## An icon-only control also gets a tooltip, through `TitledControl`

Every control in `packages/ui` that shows only an icon — a `CloseButton` or a menu trigger as much as a HeroUI
`Button` with `isIconOnly` — carries a HeroUI tooltip on top of its `aria-label`, and the two are not always the same
text (the "An icon-only control: tooltip and `aria-label`" section of `docs/ui-accessibility.md`). Add the tooltip by
wrapping the control in `TitledControl` from `packages/ui/src/components/TitledControl.tsx`, as the window chrome's
`ChromeButton` in `packages/ui/src/components/ChromeButton.tsx` does. Passing `title` to the HeroUI control itself
does nothing: its react-aria base filters `title` out of the DOM props without a warning.

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

## Only icon-only chrome sits on the window material; text goes on the opaque panel

In the macOS app the top bar and the rail show the translucent window material (see "The window chrome and the
content panel" in `docs/product/window-layout.md`), whose colour shifts with whatever is behind the window. The user
rejected a layout that put the text-heavy sidebar on it, and one that floated the content as a card with gutters of
material around it. So nothing beyond an icon, an avatar or the breadcrumb's one line goes on the chrome; anything
else belongs in the opaque content panel. A glyph or tint that does go there is measured against the material itself,
not only against `--chrome` (the "A glyph or tint on the window material" section of `docs/ui-accessibility.md`).

## Every control meets WCAG 2.2 AA, in both appearances

Every control and view in `packages/ui` is held to WCAG 2.2 level AA and checked in both the light and the dark
appearance; passing in one of them is not passing. The rules to check one against, and the measured facts behind them,
are `docs/ui-accessibility.md`. A few shortfalls are kept on purpose, each marked by a comment at its code saying so:
leave a marked one as it is, and do not add a new exception without the user's say.

Three of those rules settle how the code is written rather than what a later check finds, and the general practice each
one displaces leaves no cue to go looking:

- Never style focus with CSS `:focus-visible` — not the `focus-visible:` or `group-focus-visible:` variant, not a
  `:has(:focus-visible)` selector. A hand-built focusable element that removes the outline takes its ring from
  `useFocusVisibleProps` instead (the "Never style focus with CSS `:focus-visible`" section of
  `docs/ui-accessibility.md`).
- A state that changes without the user acting is announced through `StatusAnnouncer`, never from a `role="status"`
  element rendered together with its text (the "Announcing a change" section of `docs/ui-accessibility.md`).
- A change to a colour token or a surface is a change to every screen: it is audited over every gallery scenario in
  both appearances, and no colour a HeroUI component draws by default is assumed to pass (the "Auditing a change to a
  colour token or a surface" and "HeroUI's own text colours" sections of `docs/ui-accessibility.md`).
