# UI component conventions

How components in `packages/ui` are written and used: the wrapper layer (`src/primitives/`), HeroUI and React Aria,
shared components to reuse, dialogs and fields, scrollers, accessibility mechanics, and the styling mechanisms (tokens,
CSS files, Tailwind). React and TypeScript code in general is `docs/conventions/ui-code.md`; how a component must look,
behave and be named for users is `docs/conventions/ui-standard.md`; localized text is
`docs/conventions/i18n-in-code.md`. Directory placement and the table of shared entry points are in
`docs/structure/ui.md`.

Each rule is a bullet: the rule, then why. Where nothing names an enforcing check, review is the check.

## The wrapper layer

- **HeroUI and React Aria reach the application only through `src/primitives/`**, imported from its barrel `primitives`,
  never a file inside it; lint enforces both (`WRAPPED_LIBRARIES`, `PRIMITIVES_FILE_MESSAGE` in
  `packages/ui/eslint.layers.mjs`; tests, test support, the gallery, fixtures and the website preview are exempt). A
  component without a wrapper gets one in `primitives/` first, in the same change. How a wrapper is written
  (project-named props, no third-party types in its API, treatments the standard approves living in it, hooks over
  react-aria's) is `packages/ui/src/primitives/README.md`; the rule and its reasons are
  `docs/conventions/ui-standard.md`, "Third-party UI components come through the wrapper layer".
- **Code outside the layer finds a library part in the DOM through `partSelector` and never listens for a react-aria
  event, sets its attributes or synthesises its key events**; the rule is `docs/conventions/ui-standard.md`,
  "Third-party UI components come through the wrapper layer".
- **A wrapper passes a react-aria prop only when its caller set the project prop behind it, never a value computed from
  the wrapper's own default.** An explicit value, `false` included, beats what a parent's context supplies:
  `excludeFromTabOrder: !tabStop` with `tabStop` defaulting to `true` made a `SearchField`'s clear button a tab stop.
  Write `tabStop === false || undefined`.
- **Pass optional props to a HeroUI part through `definedProps`** (`primitives/props.ts`): some parts spread `...props`
  after their own values, so a key present as `undefined` erases what the part set.
- **Pin what a react-aria element puts in the DOM on a render** (a jsdom test or the running page), not on the default
  read from its file: an ancestor's context may override it (a `Dialog` sets its title slot to `level: 2`, so
  `Modal.Heading` is an `h2`, not react-aria's default `h3`).
- **A HeroUI part given no visible text gets a catalog message of your own** (`Tag.RemoveButton`, `CloseButton`); the
  rule is `docs/conventions/i18n-in-code.md`, "What is translated, and where it lives".

## HeroUI and React Aria

- **Before writing a component, check whether HeroUI 3 provides it and use that one**, never a look-alike assembled from
  other parts or plain elements. The installed `packages/ui/node_modules/@heroui/react/dist/components/` is the
  authoritative list; a component's file there, not its typings, shows what it puts in the DOM. A hand-assembled toast
  lacked `Toast`'s pause on hover, queueing and region semantics.
- **Use the project composite where one exists** (`Dialog`, `ActionMenu`, the shared dialogs). Where its shape cannot
  hold what is being built, build on the wrapper's parts and comment at the component why; do not widen the composite
  with a switch only one case passes.
- **A need many callers share that looks or acts like an existing shared component becomes a parameter or a kind of
  it, never a second component beside it.** Examples: `Tooltip`'s `trigger` kinds and `Button`/`Switch`/`Checkbox`'s
  `unavailable` (a tooltip, a control unavailable for a reason). The bullet above still holds for a shape only one case
  needs.
- **A hand-built element is acceptable only where HeroUI has no equivalent or cannot meet a stated requirement**; build
  it on the layer's React Aria hooks, not bare DOM events, and comment why HeroUI's is not used.
- **Give a compound component's layout classes to its parts and put no element of your own between two parts.** HeroUI
  variants use direct-child selectors (`.tabs--secondary > .tabs__list-container`) and parents scan their direct
  children for parts (`Tag` for `Tag.RemoveButton`); a wrapper `div` switches
  the variant off or leaves the default part beside yours. Where the parent takes a render-function child, wrap a part
  that way.
- **Before passing children to a HeroUI part, search its component file for `children ??` / `children ||`.** Where the
  default follows the parent's variant or state (`Alert.Indicator`, `ProgressBar.Output`), pass none; children replace
  the distinction silently. Pass children only where the default is a fixed stand-in you mean to replace.
- **Give a fixed-height row holding a HeroUI `Button` or `ToggleButton` the height of its larger form, and look at it in
  a window narrower than 768px.** HeroUI sizes by the viewport, one step larger below 48rem, whatever pane the row is
  in.
- **Never pass a react-aria-owned state attribute (`data-selected`, `data-hovered`, `data-pressed`, `data-expanded`,
  `data-disabled`, `data-focused`) to mark your own state.** The element writes over it; use another name
  (`data-current`) or a class.
- **Before keying a style on a react-aria state attribute, check in the component's file under
  `react-aria-components/dist/private/` that the hook setting it is enabled for your configuration**; otherwise use the
  CSS pseudo-class (`hover:`). A removable-only `Tag` never gets `data-hovered`; jsdom never hovers.
- **When overriding a look HeroUI gives a state, match every selector its sheet keys that state on** (`:hover` and
  `[data-hovered]`), as the shared tab rule in `style.css` does: `data-hovered` clears a frame after `:hover`, and the
  gap is a flicker (a repaint of the whole tab strip in a masked scroller in WKWebView).
- **Hover-driven content (a tooltip, `FadeOverflow`, `hover:`) inside a `ListBox` or menu item's `Label` or
  `Description` needs `pointer-events-auto` on that element**, plus `cursor-[inherit]` on the `Label` and
  `ui-disabled:hover:` / `ui-disabled:active:` cancellation on disabled rows. HeroUI sets `pointer-events: none` there,
  which inherits. Before relying on hover in any other HeroUI part, search its sheet for `pointer-events-none`.
- **In a virtualized `ListLayout` list (`ListBox`, `Tree`, `GridList`), clear side padding on the list with `px-0` and
  give the inset to rows and headings** (`mx-1` with `w-auto`, header padding); a `Select` whose items show
  `ListBox.ItemIndicator` needs `pe-7` per item. Side padding on the list overflows sideways and clips focus rings.
- **A `Tabs.Panel` mounts only while its tab is selected**: state, refs and effects in it are discarded per switch. Use
  `shouldForceMount` only where hidden panels must keep taking layout (the rail quota panel), hide the force-mounted one
  yourself (`ui-inert:invisible`) and clear HeroUI's panel spacing. It is not a way to keep a panel's state.
- **A react-aria collection (`Tabs` among them) renders what is inside it twice**, once in a hidden copy. Anything whose
  effect must happen once (a connection, a subscription, timers, document listeners) goes outside `<Tabs>`'s children or
  inside a `Tabs.Panel`'s content; a hook that watches an element inside a panel attaches in a ref callback
  (`useListMotion` does); a tab's handler reaches what it needs through the DOM from its own element, not through a ref
  on a sibling such as `Tabs.List`.
- **State that later makes an element inside a `Tabs.Panel` tabbable (a `useFadeRegion` scroller) is held by the
  component that renders the `Tabs.Panel`**, not a child, so the panel drops its own `tabindex`; react-aria's observer
  misses a descendant's `tabindex` change.
- **Give a `Tab` its `Tabs.Indicator` only where the strip's tab set does not change while shown.** A strip that adds
  and selects a tab at once draws the fill on the tab (`ui-selected:` classes), as the Terminal panel's strip does; the
  indicator's shared-element transition sticks otherwise.
- **Every tooltip goes through `Tooltip` (`primitives/Tooltip.tsx`), and `trigger` says what carries it.** The `control`
  kind (the default) goes only around a child that consumes react-aria's `FocusableContext` (a `ToggleButton`, a
  `MenuTrigger`, a `ModalCloseButton`); never around a plain element, a `span` or a collection item. HeroUI's tooltip
  parts (`primitives/tooltipParts.tsx`) are `Tooltip`'s own and are not used elsewhere, but for `tooltipSurfaceClassName`, the surface a `PopoverHint` is drawn on. A mark that is no control takes `trigger="mark"`; where nothing can be
  a trigger, `trigger="anchored"`. `Button`, `Switch` and `Checkbox` carry their own tooltip (`tooltip`, and `unavailable`
  for the reason that replaces it) and are never wrapped in a `Tooltip`: a dev-time `console.error` reports it, as the
  outer one would never open. Content with something to press or select is a `PopoverHint`, not a tooltip.
- **Never pass `title` to a HeroUI control to get a tooltip**; react-aria filters it out.
- **Where a tooltip only repeats the control's name and the repeat cannot be seen (a rich `content`, a `mark`), pass
  `aria-describedby=""` on the control**, or the name is announced twice. `Button` and the `control` kind see a `content`
  equal to the control's `label` and set `describedBy=""` themselves. A tooltip that says more than the name keeps the
  link. A `box` tooltip (a `Switch`'s or a `Checkbox`'s) is linked to nothing, so it never needs `aria-describedby=""`.
- **Never name as the anchor of a `Tooltip` of `trigger="anchored"` an element the UI itself scrolls (a marquee).** A non-modal react-aria
  overlay closes when anything holding its trigger scrolls.
- **A press that opens a list and focuses something partly out of view brings the field into view before react-aria
  handles the press** (`useRevealOnTriggerPress` in `components/form/TagsInput.tsx`). Never add a gallery or test step
  that scrolls the control into view first: it hides the defect from every later check.
- **Never put HeroUI typography (`Paragraph`, `Heading`, `Typography`, `Code`, the dialogs' `Path` helper) inside a
  react-aria field's label or subtree.** Its slotted `TextContext` makes `Text` throw; use a plain element with the same
  classes or wrap in `<TextContext.Provider value={{}}>`.
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
  `CodeBlock`. It sits on a surface of its own and carries the named, keyboard-reachable scroll region (capped at 40% of
  the viewport) and the copy control; a label stays outside via `labelledBy`, and `wrap` breaks long lines (for text the
  user wrote). Change its look in the component, not in one caller.
- **A strip of text tabs uses the `primitives/Tabs.tsx` wrappers with `fit="content"` and `Tab size="content"`**
  (`compact` only in a pane header beside the title), so every strip aligns and sizes the same.
- **A list that animates reordering or hides mounted rows uses `useListMotion`, not a row-level FLIP.**
- **A spinner beside a label that already says what runs is the layer's `Spinner` without a `label`.** One with a
  `label` is announced as a status.
- **A dialog whose work cannot be left uses `Dialog`'s `busy` prop.** Wrappers read the state through `useDisabled`,
  custom controls through `useBusy`; a dialog does not disable its own controls for it.
- **Announce outcomes and silent changes through `AttendedStatusAnnouncer`, `ChangeAnnouncer` and
  `ChangedElsewhereAnnouncer`, from the request's outcome, not from broadcast status changes**, mounted beside a
  `role="button"` row, not inside it.
- **A menu with a filter field takes no single-letter shortcuts; an open menu takes window commands through
  `ActionMenuItem.commandShortcut` / `runMenuCommand`, and components never import `shortcuts/`** (lint: the
  `components/` layer does not).
- **A search field takes no placeholder of its own**; see `docs/conventions/ui-standard.md`, "Input placeholders".
- **A field with a button or another field beside it in one row stands in a `FieldRow`, and a button there, icon or
  text, takes the wrapper `Button`'s `besideField` (shared by `ToggleButton`, passed on by `IconToggle` and
  `PasteButton`)**, never a gap, a height, corners or an icon-only look of its own; see
  `docs/conventions/ui-standard.md`, "Pointer target size" ("A field and what stands beside it in one row").
- **Pinned section headings use `StickyHeading`; a pinned row or strip that is not a heading spreads
  `useStickyHeadingFocus`.** The scroll fade's padding would otherwise scroll the list on every focus move onto the
  pinned element.
- **Settings: a new row or block gets a `searchId` (or `SettingsSearchBlock`) and an entry in `SETTINGS_SEARCH_INDEX` in
  the same change, with the name and every wording of the description** (`settingsSearch.test.tsx` fails when row and
  index drift apart); a group's `title` is a message key; saved on/off settings use `SettingSwitch` /
  `SwitchSettingRow`; a plain daemon on/off setting is a `COPY` entry in `configuration/booleanSettings.ts` rendered by
  `DaemonSwitchSetting`, never a switch of its own; a provider's setting copy is read through `useSettingCopy` /
  `settingText` / `SettingHelp`, and compatibility switches through `AgentCompatibilityField` / `CompatibilityPicker`
  with no per-switch UI code.
- **Sessions open in the agent's desktop application through `useOpenSessionInApp`**, which sends `open_session_in_app`
  and handles the not-installed and not-seen cases, and the agent's name, install page, mark and colour come from the
  provider readers (`docs/conventions/provider-neutral-core.md`).

Rules for these are in the UI standard: buttons ("Buttons"), tooltips and hints ("An icon-only control: tooltip and
`aria-label`", "A tooltip is the application's own, never the browser's `title`", "A help tip: an explanation behind a
\"?\""), accessible names ("Accessible names"), error placement ("Where an error is shown"), badges and colour roles
("Confirmed visual choices"), row and card building ("Hand-built rows and cards", "Row action menu buttons"), target
sizes ("Pointer target size"), direction ("Text direction"), motion and list motion ("Motion", "Dynamic collections"),
reaching controls by keyboard ("Reaching a control with Tab", "Leaving the terminal with F6"), expanded and selected
states ("Expanded and selected"), progress ("Progress of a long operation") and references in copy ("References in copy
use `InlineReference`").

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
  `attempt` answers. A failure after the submit passed, a daemon refusal included, opens the error alert instead.
- **A field marked invalid by hand keeps `validationBehavior="aria"` and renders its message through
  `FieldErrorMessage`** (`role="alert"` on its own element inside `FieldError`; never `role` on `FieldError`, which
  react-aria drops). `native` validation locks the dialog out of submitting. A `SearchField` takes that message through
  its `error` prop, drawn under the whole field, its trailing toggles included; a `TextField` holding a `FieldRow` puts
  the `FieldErrorMessage` after the row, so it sits under the input and the button beside it.
- **A form dialog that edits a console, a project or an account uses `useCheckedEdit` and sends only the fields the user
  changed, with their `base_versions`.** Two clients changing different fields then do not conflict; a field the store
  moved past is put back to the current value and, if the user had edited it, announced through
  `ChangedElsewhereAnnouncer`.
- **An option's note may follow its own state and the options above it, never an option below.** Narrow the earlier
  option instead; hiding or disabling an option because of one above is fine.
- **An input's placeholder follows `docs/conventions/ui-standard.md`, "Input placeholders"**, which beats the
  `knowledge-i18n-copy` skill's generic placeholder guidance.

## Scrollers and the fade

- **Every scroller that hides content behind its edge takes its fade (size and scroll padding) from
  `components/scroll/scrollFade.tsx`**; never use `ScrollShadow` directly and never hand-build a fade.
  `pnpm --filter @octoboard/ui test` fails on a scroller in `src/` that does not take it (`scrollFadeCoverage.test.ts`).
- **A scroller that cannot take the fade is declared with its reason in `EXEMPTIONS` of `scrollFadeCoverage.test.ts`**;
  never slip past the check by assembling the overflow class from pieces or moving the scroller out of the package.
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

- **A HeroUI component keeps its default look through supported variants and slots**; layout and sizing adaptations may
  stay, appearance overrides may not (`docs/conventions/ui-standard.md`, "Conformance level and exceptions", "Do not
  change how a HeroUI component looks", "Where to fix a shortfall"). When asked to strip custom styling, separate the
  two kinds; a composite's own region marking is its design.
- **Read HeroUI's own sheet** (`@heroui/styles/dist/components/<name>.css`), not the component file, when styling
  project content inside a HeroUI component: a base rule covers every child (`.empty-state` sets text size and colour).
  A newly used HeroUI component needs its sheet (and its dependencies') imported in `src/style.css` (`style.test.ts`
  names what is missing), and moving `@heroui/styles` also moves the `tw-animate-css` pin.
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
- **Nothing starts a transition inside a collapsed `Disclosure` body**; the rule and the one `style.css` rule that
  enforces it are `docs/conventions/ui-standard.md`, "Motion". A new element there needs nothing of its own unless its
  transition is set by inline style, `!important` or a more specific unlayered rule.
- **Position, spacing, borders, corners and alignment use Tailwind's logical utilities** (`start-*` / `end-*`, `ms-*` /
  `me-*`, `ps-*` / `pe-*`, `border-s`, `rounded-s-*`, `text-start`), not `left` / `right` / `ml` / `pr`; `translate-x-*`
  has no logical form, so a slide gets an `rtl:` twin with the opposite sign. Why: under a right-to-left language the
  window mirrors, and nobody sees a miss without switching to Arabic (mirroring rules:
  `docs/conventions/ui-standard.md`, "Text direction").
- **An expand / collapse chevron switches between two glyphs** (forward when collapsed, down when expanded), never one
  rotated: a mirrored forward chevron turned a quarter points up (`ExpandChevron` in
  `packages/ui/src/project-browser/treeRow.tsx`).
- **A free-form name (a session title, a project, tag or account name) is drawn with `dir="auto"`;** an identifier takes
  `dir="ltr"` (`docs/conventions/ui-standard.md`, "Text direction").
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
- **A line that divides a pane's parts or rows is a groove utility (`groove-b` and siblings), never
  `border-b border-separator`, a bare `border-*` or `divide-*`**; a flat card or a table divides with `hairline-b`
  instead (`docs/conventions/ui-standard.md`, "Where a groove goes"). `--separator` stays for HeroUI components and
  composite frames. A card takes no decorative outline: a card in the layout takes `shadow-(--panel-shadow)`, a floating
  non-panel surface keeps `shadow-lg` / `shadow-xl` minus the outline, a panel card takes `PANEL_CARD_CLASS`. A surface
  meant to look like an existing one reuses that surface's class or tokens.
- **Every CSS rule written or generated keeps a tag, class or id in its rightmost compound** (`:where()` around
  ancestors or as a suffix, never `.prose :where(pre)`); WebKit tries rules without one against every element. Load
  `@tailwindcss/typography` only through `packages/ui/typography.mjs`. After adding CSS-emitting code, count the built
  rules whose rightmost compound starts with `:where(`, `:is(`, `:not(` or `*` against the count before (about 150
  remain; a jump of dozens is the thing to fix).
- **On the macOS translucent window chrome (top bar, rail) put only an icon, an avatar or the breadcrumb's one line**;
  anything else goes in the opaque content panel, since the material's colour shifts with what is behind the window. A
  glyph or tint placed there is measured against the material (`docs/conventions/ui-standard.md`, "A glyph or tint on
  the window material").
- **A border that identifies a control or shows a selected state stays when HeroUI's look is kept:** it is not
  decorative, and keeps its 3:1 colour.
