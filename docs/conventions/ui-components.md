# UI component conventions

How components in `packages/ui` are built and used: the wrapper layer (`src/primitives/`), HeroUI and React Aria,
shared components, dialogs and fields, scrollers and styling mechanisms. How a component must look and behave for users
is `docs/conventions/ui-standard.md` and `docs/conventions/ui-accessibility.md`; directory placement and the shared
entry points are `docs/structure/ui.md`.

## General principles

- **HeroUI first, through `primitives`.** A component HeroUI 3 provides is used, through its wrapper in
  `src/primitives/` (a missing wrapper is added there in the same change); how a wrapper is written is
  `packages/ui/src/primitives/README.md`.
- **HeroUI's look stays unchanged.** Its supported variants and slots give the look; layout and sizing adaptations may
  stay, appearance overrides may not (`docs/conventions/ui-standard.md`, "Conformance level and exceptions").
- **Styles come from tokens and literal Tailwind classes**, so every surface reads one source and Tailwind sees every
  class.

## The wrapper layer

- **Code outside the layer finds a library part in the DOM through `partSelector`**, never HeroUI's `data-slot` names
  or classes, and **never listens for a react-aria event, sets its attributes or synthesises its key events**; the
  layer offers that behaviour under a project name, so only the layer knows the libraries' internals. The application
  stylesheet, which themes HeroUI's own sheet, is the exception. A wrapper element's state (pressed, hovered, selected,
  disabled, keyboard focus) is styled through the layer's `ui-` variants (`ui-pressed:`, `ui-keyboard-focus:`, defined
  in `style.css`), never the attribute that marks it.
- **A wrapper passes a react-aria prop only when its caller set the project prop behind it, never a value computed from
  the wrapper's own default.** An explicit value, `false` included, beats what a parent's context supplies:
  `excludeFromTabOrder: !tabStop` with `tabStop` defaulting to `true` made a `SearchField`'s clear button a tab stop.
  Write `tabStop === false || undefined`.
- **Pass optional props to a HeroUI part through `definedProps`** (`primitives/props.ts`): some parts spread `...props`
  after their own values, so a key present as `undefined` erases what the part set.

## HeroUI and React Aria

- **Before writing a component, check whether HeroUI 3 provides it and use that one**, never a look-alike assembled from
  other parts or plain elements. The installed `packages/ui/node_modules/@heroui/react/dist/components/` is the
  authoritative list; a component's file there, not its typings, shows what it puts in the DOM. A hand-assembled toast
  lacked `Toast`'s pause on hover, queueing and region semantics.
- **Use the project composite where one exists** (`Dialog`, `ActionMenu`, the shared dialogs). Where what is being
  built is a shape only one case needs and the composite cannot hold it, build on the wrapper's parts and comment at
  the component why, rather than widening the composite with a switch only that case passes; a shape many callers
  share is a parameter or kind of the composite (P2).
- **A hand-built element is acceptable only where HeroUI has no equivalent or cannot meet a stated requirement**; build
  it on the layer's React Aria hooks, not bare DOM events, and comment why HeroUI's is not used.
- **Give a compound component's layout classes to its parts, and put no element of your own between two parts of a
  component whose variants use direct-child selectors (`.tabs--secondary > .tabs__list-container`) or whose parent
  scans its direct children for parts (`Tag` for `Tag.RemoveButton`)**: a wrapper `div` switches the variant off or
  leaves the default part beside yours. Where the parent takes a render-function child, wrap a part that way. Its
  stylesheet under `packages/ui/node_modules/@heroui/styles/dist/components/` shows direct-child selectors, and its
  file under `packages/ui/node_modules/@heroui/react/dist/components/` whether the parent scans its children; a
  component with neither (`ProgressBar`) may hold elements of yours.
- **Never pass a react-aria-owned state attribute (`data-selected`, `data-hovered`, `data-pressed`, `data-expanded`,
  `data-disabled`, `data-focused`) to mark your own state.** The element writes over it; use another name
  (`data-current`) or a class.
- **Every tooltip goes through `Tooltip` (`primitives/Tooltip.tsx`), and `trigger` says what carries it.** The `control`
  kind (the default) goes only around a child that consumes react-aria's `FocusableContext` (a `ToggleButton`, a
  `MenuTrigger`, a `ModalCloseButton`); never around a plain element, a `span` or a collection item. A mark that is no
  control takes `trigger="mark"`; where nothing can be a trigger, `trigger="anchored"`. HeroUI's tooltip parts
  (`primitives/tooltipParts.tsx`) are `Tooltip`'s own, used elsewhere only for `tooltipSurfaceClassName`, the surface a
  `PopoverHint` is drawn on. `Button`, `Switch` and `Checkbox` carry their own tooltip (`tooltip`, and `unavailable`
  for the reason that replaces it) and are never wrapped in a `Tooltip`: a dev-time `console.error` reports it, as the
  outer one would never open. Content with something to press or select is a `PopoverHint`, not a tooltip.
- **Where a tooltip only repeats the control's name and the repeat cannot be seen (a rich `content`, a `mark`), pass
  `aria-describedby=""` on the control**, or the name is announced twice. `Button` and the `control` kind see a
  `content` equal to the control's `label` and set `describedBy=""` themselves. A tooltip that says more than the name
  keeps the link. A `box` tooltip (a `Switch`'s or a `Checkbox`'s) is linked to nothing, so it never needs
  `aria-describedby=""`.
- **A field's explanatory note is HeroUI's `Description`, a direct child of the field and a sibling of the control and
  label part.** HeroUI styles it only there and react-aria wires its id into `aria-describedby`. For a `Select` whose
  note can appear after mount, give it a `useId` and set `aria-describedby` only while it exists, keeping the control
  mounted. A checkbox with a `StatusMark` after its label is `w-fit` with the mark in `MarkLineBox` and the note beneath
  (use `ConfirmOption`'s `mark`).

## Shared components to reuse

The helper for each need is in `docs/structure/ui.md`, "Shared entry points"; the rules that go beyond "use helper X":

- **A confirmation is `ConfirmDialog` (`typeToConfirm` for a typed word), never a dialog assembled for the occasion**,
  so every confirmation shares its focus, busy and error handling.
- **Raw error text shown as a block of its own is `DiagnosticBlock`**, never a `<p className="font-mono">` or a bare
  `CodeBlock`. It sits on a surface of its own and carries the named, keyboard-reachable scroll region and the copy
  control; a label stays outside, named via `labelledBy`, or via `label` where the visible label cannot carry an id (a
  disclosure trigger), and `wrap` breaks long lines (for text a person or an agent wrote). Change its look in the
  component, not in one caller.
- **A dialog whose work cannot be left uses `Dialog`'s `busy` prop.** Wrappers read the state through `useDisabled`,
  custom controls through `useBusy`; a dialog does not disable its own controls for it.
- **Announce outcomes and silent changes through `AttendedStatusAnnouncer`, `ChangeAnnouncer` and
  `ChangedElsewhereAnnouncer`, from the request's outcome, not from broadcast status changes**, mounted beside a
  `role="button"` row, not inside it.
- **An open menu takes window commands through `ActionMenuItem.commandShortcut` / `runMenuCommand`, and components
  never import `shortcuts/`** (lint: the `components/` layer does not).
- **A field with a button or another field beside it in one row stands in a `FieldRow`, and a button there, icon or
  text, takes the wrapper `Button`'s `besideField` (shared by `ToggleButton`, passed on by `IconToggle` and
  `PasteButton`)**, never a gap, a height, corners or an icon-only look of its own: the button takes the field's shape
  and height at every width, 8px from it (`--field-row-gap`), and its variant says how it bears on the field,
  `secondary` for an action that fills or submits it (Paste, Browse, Go), `tertiary` for one that leaves it alone
  (copying the value it shows), by the user's decision. A select whose label stands beside it (`OptionSelect`'s
  `besideLabel`) stands in a `labelled` `FieldRow`. A control drawn inside the field's frame and a compact toolbar cut
  to the row-control height are outside the rule.
- **A pinned row or strip that is not a heading spreads `useStickyHeadingFocus`** (headings use `StickyHeading`); the
  scroll fade's padding would otherwise scroll the list on every focus move onto the pinned element.
- **Settings: a new row or block gets a `searchId` (or `SettingsSearchBlock`) and an entry in `SETTINGS_SEARCH_INDEX` in
  the same change, with the name and every wording of the description** (`settingsSearch.test.tsx` fails when row and
  index drift apart); a group's `title` is a message key; saved on/off settings use `SettingSwitch` /
  `SwitchSettingRow`; a plain daemon on/off setting is a `COPY` entry in `configuration/booleanSettings.ts` rendered by
  `DaemonSwitchSetting`, never a switch of its own; a provider's setting copy is read through `useSettingCopy` /
  `settingText` / `SettingHelp`, and compatibility switches through `AgentCompatibilityField` / `CompatibilityPicker`
  with no per-switch UI code.

## Dialogs and fields

- **A modal whose content is one body is built on `Dialog`; a modal of independently scrolling panels (the settings
  dialog) keeps its own frame from the wrapper layer's `Modal`.** Do not widen `Dialog` with an absent header, a panel
  mode or a size of its own to take such a modal in.
- **A dialog's content goes in `Dialog`'s children only, with no scroll container of your own.** The body scrolls and
  fades inside `Dialog`; an `overflow-y-auto` wrapper scrolls apart and carries no fade. A line under the title goes in
  the `subtitle` slot, never pulled above the body with a negative margin (overflow at a scroller's block start is
  clipped). Something that must scroll apart from the body takes the project's fade (below).
- **A destructive confirmation with options keeps the options apart from the consequences.** A checkbox is a plain block
  whose label says what ticking it deletes, with a short note and the items it covers in a Details disclosure collapsed
  by default; consequence lines sit under the options and each ticked option adds its line; a disclosure is never
  collapsed or remounted by an unrelated change.
- **Consequences take one of two shapes, and a dialog keeps its own**: topic sections (`ConfirmSection` from
  `extraControl`, `ConsequenceLines`, `consequences={null}` when sectioned) or one block (`ConfirmConsequences` as
  `consequences`, which makes it a warning modal). Never fold sections back into one pooled block.
- **A choice the user must make again starts empty, with a placeholder, never on a fallback.** A react-aria `Select`
  calls `onChange` only when the key changes, so a pre-filled fallback can never be "picked". Use `OptionSelect`'s
  placeholder variant (clearable to `null`) and swap placeholder and required variants as a ternary in the same JSX
  position.
- **Every single-line text field goes through `useTrimmedField`** (`TextInput` already does); a form never calls
  `.trim()` and sees the trimmed value. A non-`TextInput` field takes the hook's `value`, `onChange` and `onBlur`. Left
  alone: a free-text area and the tags input.
- **A message about one field's value shows under that field as `TextInput`'s `errorMessage`, from the first submit
  on**, computed each render and passed through `useSubmitValidation` (`shown`, `attempt`); gate the submit on what
  `attempt` answers. A failure after the submit passed, a daemon refusal included, opens the error alert instead. A
  page form (Remote devices, Scheduled tasks) stays open while the user works through it, so its message also shows
  once its field was edited and left.
- **A field marked invalid by hand keeps `validationBehavior="aria"` and renders its message through
  `FieldErrorMessage`** (`role="alert"` on its own element inside `FieldError`; never `role` on `FieldError`, which
  react-aria drops). `native` validation locks the dialog out of submitting. A `SearchField` takes that message through
  its `error` prop, drawn under the whole field, its trailing toggles included; a `TextField` holding a `FieldRow` puts
  the `FieldErrorMessage` after the row, so it sits under the input and the button beside it.
- **A form dialog that edits a console, a project or an account uses `useCheckedEdit` and sends only the fields the user
  changed, with their `base_versions`.** Two clients changing different fields then do not conflict; a field the store
  moved past is put back to the current value and, if the user had edited it, announced through
  `ChangedElsewhereAnnouncer`.
- **A form over a record that changes under it, and that has no `useCheckedEdit`, keeps the user's edits over a base**,
  as the scheduled task form does (`scheduledTasks/taskDraft.ts`): the base is the stored record overlaid with what this
  form's unsettled save sent; an edit equal to the base is not an edit; a refused save puts its parts back as edits; a
  save is settled when the stored record's version is at or past its reply's, so the record carries a version that
  grows with every stored change. Not: deciding what is an edit by comparing the draft with the record's previous and
  new values, or by flags for a save in flight. The model is a pure function, tested over the orders a reply, its
  broadcast and the user's edits can arrive in.
- **A form with a Save of its own that stays on screen (a page form) declares its unsaved changes through
  `useUnsavedChanges`, and every action that would replace it runs through `useLeaveGuard`**, never around it.
- **An option's note may follow its own state and the options above it, never an option below.** Narrow the earlier
  option instead; hiding or disabling an option because of one above is fine.

## Scrollers and the fade

- **Every scroller that hides content behind its edge takes its fade (size and scroll padding) from
  `components/scroll/scrollFade.tsx`**, never `ScrollShadow` or a hand-built fade; `scrollFadeCoverage.test.ts` fails on
  one that does not, and one that cannot is declared with its reason in that test's `EXEMPTIONS`.
- **Pick the API by scroller.** `FadedScroller` (an element wrapped around the content) by default. `useScrollFade`
  where the element must be the scroller: virtualized, carrying its own role and key handling, or with an animating
  ancestor (put it on the inner list or panel, never on a `Popover` or `Tabs.Panel`: the fade's animation never finishes
  and the surface never unmounts). `useStickyScrollFade` or `FadedScroller` with `stickyHeadingHeight` where something
  is pinned above the fade (pass 0 while the pinned element is absent; `stickySurface` off the side-pane colour).
- **On a masked scroller, focus-ring room goes on the scroller itself; a frame, border or background goes on an element
  around it** (`flex min-h-0 flex-col` in a flex column, or the scroller is cut off in a short window). A focusable fade
  region draws `ring-2 ring-app-focus` on the wrapper from `useFadeRegion`'s `focused`, with clear space around it and
  `ring-inset` at the window edge.
- **The fade is a hint over a normal overflow container; do not replace the container with one that scrolls by script.**

## Styling mechanisms

- **Shared theme and geometry live in `src/styles/`; owned stylesheets are registered in `src/style.css`**, not loaded
  from component modules; a wrapper's common correction goes in its adjacent stylesheet in `primitives/`.
- **A Tailwind class name stands in the source as literal text**: no runtime concatenation, and no interpolation glued
  to the end of a class (put the space inside the template literal). Tailwind reads source as text and emits nothing for
  an assembled name, and neither lint nor jsdom tests notice. After adding a utility, grep the built `dist/assets/*.css`
  for it. Not: `` `${side}-0` ``, `` `hover:bg-panel-hover${cond ? " x" : ""}` ``.
- **There is no `tailwind-merge`: to change a property a project component sets itself, add a prop that picks the
  value** (`StatusBadge`'s `single`), not a `className`, which loses to the component's own utility. A background given
  to a HeroUI part through `className` beats its state fills; restore each with the state variant
  (`ui-hovered:bg-default`).
- **Never give one property two `max-` breakpoint utilities whose bands nest** (`max-phone` against `max-tablet`,
  `max-desktop`, `max-sm`, `max-md`): the later-declared one wins, not the narrower. Make the bands disjoint by stacking
  (`phone:max-tablet:max-w-[A] max-phone:max-w-[B]`) or via a custom property; a value also set under `rtl:` gets a
  combined twin (`tablet:rtl:`).
- **Under a local override of a HeroUI token (`.control-fills`, `.sidebar-fills`, the dark floating surfaces), write
  `var(--<name>)`, not `var(--color-<name>)`, in arbitrary values and plain CSS**, and a new local override sets every
  derived token the inner components draw with (`--default-hover`, `--surface-hover`, `--field-*`).
- **Dim a region with a veil** (`INACTIVE_VEIL_CLASS` plus `after:bg-<surface>/55` in the surface's actual colour,
  `after:-m-px` and the rounding to cover a border), **never `opacity` below 1 on an ancestor**: WKWebView paints fading
  and GPU-layer descendants as blank tiles. A panel card is `bg-panel-sidebar`, a popover `bg-overlay`.
- **`dir="auto"` goes only on an element that holds its own text.** The engine stops at every descendant carrying a
  `dir`, so a container whose text sits in such descendants (the paragraphs of a rendered Markdown list or quote) never
  resolves, falls back to `ltr`, and costs WebKit time quadratic in the document. Where blocks wrap their text in
  further elements, write each block's direction as an explicit `dir` from its subtree's first strong character, as
  `packages/ui/src/viewer/markdown/blockDirection.ts` does.
- **A line height given as a ratio must not land a hair under a whole pixel** (`1.3334`, never `calc(4 / 3)` or
  `1.33333`): WebKit rounds it down, Chrome does not.
- **Horizontal padding or margin that places content against the sidebar's or right pane's edge uses `--pane-inset`,
  `--row-fill-inset` and `--row-content-inset`** (`styles/layout.css`), never a literal `px-2/3/4`; layered values are a
  `calc` of the token; vertical padding keeps its own value.
- **A line that divides parts or rows is a groove (`groove-b` and siblings) or a hairline (`hairline-b` and siblings)
  as `docs/conventions/ui-standard.md`, "Cards, grooves and hairlines" assigns them, never a bare `border-*`**.
  `--separator` stays for HeroUI components and composite frames. A card takes no decorative outline: a card in the
  layout takes `shadow-(--panel-shadow)`, a floating non-panel surface keeps `shadow-lg` / `shadow-xl` minus
  the outline, a panel card takes `PANEL_CARD_CLASS`. A surface meant to look like an existing one reuses that surface's
  class or tokens.
- **Every CSS rule written or generated keeps a tag, class or id in its rightmost compound** (`:where()` around
  ancestors or as a suffix, never `.prose :where(pre)`); WebKit tries rules without one against every element. Load
  `@tailwindcss/typography` only through `packages/ui/typography.mjs`.
- **On the macOS translucent window chrome (top bar, rail) put only an icon, an avatar or the breadcrumb's one line**;
  anything else goes in the opaque content panel, since the material's colour shifts with what is behind the window. A
  glyph or tint placed there is measured against the material's colours in the comment on the window's surfaces in
  `packages/ui/src/styles/surfaces.css`.
- **A border that identifies a control or shows a selected state stays when HeroUI's look is kept:** it is not
  decorative, and keeps its 3:1 colour.
