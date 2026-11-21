# UI accessibility conventions

How `packages/ui` is reached and understood by keyboard, screen reader, touch and in either reading direction: keyboard
reach, focus, key handling, names and roles, announcements, target size, colour and text direction. The visual and
interaction conventions are `docs/conventions/ui-standard.md`; what each view does is in `docs/product/`.

## General principles

- **Keyboard access, accessible names, focus handling and announcements are required**, unlike the contrast figures,
  which are a reference: a miss is a defect, and a kept shortfall is marked at its code with the user's say.
- **Focus never falls to `<body>`.** When what holds focus goes away (a control, a region, a dialog's subject), code
  moves focus itself to a named element; it is never left to the library, and WebKit drops it silently where Chromium
  catches it.
- **A focus move the user did not ask for follows the last input's ring** (`ringForAutomaticFocus`): a ring after a
  key, none after a press of a mouse, a finger or a pen, none before any input.
- **Never move focus by script inside a mouse press's `pointerdown`, from a layout-effect cleanup or in a microtask.**
  The browser's `mousedown` default takes focus back, react-aria never hears of a cleanup's move, and passive effects of
  a synchronous commit run before a microtask.
- **Nothing is told by colour alone**: a state that differs in colour also differs in glyph, shape or text.
- **Every announcement or automatic focus move is built to a passing case in `docs/voiceover-cases.md`**, matched on
  every item of "What \"matches a case\" means" and named in the plan before building; a status case plus a focus case
  do not cover the pair's timing. If none can be matched, tell the user before building.

## Keyboard

- **Every interactive control is reachable from the keyboard and operable with Enter / Space**; the one exception is a
  report page's chart (hover tooltip, legend toggles, zoom), a named `role="img"` whose numbers the page's own tables
  carry (the user's decision).
- **Inside a dialog's form, a control that holds a choice (a checkbox, a radio, a switch, a closed select, a tab) takes
  Space and its arrows, and Enter submits the dialog from it**, as from a text field; a button that acts and an open
  popover's list keep Enter. The dialog does this for a closed select, a tab and a non-native radio.
- **A confirmation opens with focus on its confirm button, except a destructive one and a question the user did not ask
  for, which open with focus on the dialog itself**, so an Enter pressed before the user moved onto a control does
  nothing; one with a typed word opens in that field. One opened by picking a choice in a select returns focus to that
  select's trigger however it closes, the select showing the old choice until it succeeds. A form dialog opens in its
  first text field (on its submit button where it has none), however it was opened; a field that appears later never
  takes focus. A dialog's form ignores the repeats of a held Enter or Space (except Space in a text field and Enter in a
  multi-line one).
- **A control inside the row of a react-aria collection that manages focus (`GridList`, `Tree`) takes no tab stop of
  its own**: the row is the stop and the arrows move to its controls (the grid and tree pattern). A collection whose
  rows hold tabbable controls takes `useCollectionOptionTab`, so WebKit's Option+Tab leaves it as Tab does. A tag
  group is one stop (`Tag`'s remove button is `excludeFromTabOrder`), Delete or Backspace removing a tag.
- **A control in a collection's section header is reached through the collection's own walk, extended by the
  application** (Up and Down through rows and headings, Right through a heading's controls, mirrored right to left):
  react-aria's delegate walks items only. Such a control takes no tab stop and `focusOnPress={false}`, and a popover
  opened from it hands focus back to its heading.
- **A control revealed only on hover or focus is shown at all times where the primary input cannot hover**
  (`RowControls`, or `[@media(hover:none)]:` from `CANNOT_HOVER` in `input/touchInput.ts`).
- **Where a float decides the source order, every control of that row goes in the float**, so focus never jumps back
  against the reading direction; a flex row that can put the group at the end without a float puts it after the text.
- **A tab panel with nothing tabbable inside is a tab stop; one holding a tabbable control or scroll region is not.**
- **Every new region of the window that holds controls (a pane, a panel, an overlay) is marked `data-region` and added
  to `layout/useRegionCycle.ts`'s order and `shown`**, since the terminal keeps Tab and F6 is the only keyboard way out
  of it; a right pane panel comes in through its `panels` argument. `shown` follows whether the region holds a control,
  a region lands on its first control unless one carries `data-region-landing`, and one whose focus goes through a
  method of its own is a stop only when that method leaves focus inside it.
- **A control xterm's Tab handling puts out of reach has an equivalent reachable another way** (Close terminal in the
  tab-row and tab menus for the exited notice's).
- **A touch gesture is never the only way**: every pane a swipe (`useSwipe`) opens, closes or switches is also reached
  with the top bar's toggles, and the swipe goes through the same open and close paths, so it moves focus as they do.
  Detect a finger gesture from touch events, not pointer events, and never use `touch-action` to keep pointer events
  alive: Chromium sends `pointercancel` once a sideways touch becomes a pan.
- **A menu that opens on right-click also opens with the Menu key or Shift+F10, focus on its first item, and has a
  visible `ActionMenu` trigger offering the same items**; dragging to reorder has a menu counterpart named by reading
  order ("Move earlier" / "Move later"). A text-editing menu (Copy, Paste, Select all) and the rail's console avatars
  need no visible trigger.

## Focus

- **A focus ring is drawn only while the input under way is a keyboard's or an assistive technology's** (react-aria's
  virtual modality); focus placed at launch draws none (`settleUnknownModality`).
- **Never style focus with CSS `:focus-visible`** (lint bars it in class names). Once the last focus came from a click,
  the app's WKWebView does not match it on an element focused by script, so F6 and Tab inside a focus-trapped dialog
  land without a ring; Playwright's `webkit` matches it, so only the packaged app shows the miss.
- **A hand-built focusable element that removes the outline puts a ring back from `useFocusVisibleProps`**
  (`ui-keyboard-focus:ring-2 ui-keyboard-focus:ring-focus`, `group-ui-keyboard-focus:` from a child,
  `has-ui-keyboard-focus:` from a parent), in HeroUI's `--focus`. A container whose focus is held by a library's hidden
  input (xterm's textarea) draws the ring on the container from `useArrivalFocusProps`. Inside a rendering library's
  shadow root the ring is a `data-focus-visible` attribute set by hand and styled from the stylesheet injected there.
- **A handoff because what held focus went away calls `ringForAutomaticFocus()` before it focuses**, not
  `setKeyboardModality()`, which is kept for what a key alone starts (F6, a shortcut, a find key). A keyboard handler
  that moves focus itself calls `setKeyboardModality()` first when it sits on the window's capture phase and stops
  propagation, or handles a chord with ⌘ or Ctrl (Alt outside macOS), and only once it knows it will act. After a tap
  (`interactionIsTouch`) the window never moves focus into a terminal or a text field on its own: it lands on a control
  that takes no text (`focusLandingWithoutText`).
- **A pressable control in the sidebar, toasts, banners and the terminal's surroundings takes `focusOnPress={false}`**
  unless its handler refocuses the terminal; it must still be reachable. Every "⋯" menu is built on `ActionMenu`, which
  returns focus to its trigger.
- **Every path that hides a region able to hold focus first hands focus on when the region contains
  `document.activeElement`** (a toggle, scrim, Escape, a breakpoint change, `inert`: `releaseFocus`,
  `HandoffOnUnmount`), in the close path or a `useLayoutEffect`, never a passive effect, which runs after the browser
  has already moved focus.
- **Move focus to a named element yourself when its holder goes**: `handFocusOff` before removing a control; for an
  element that leaves by a state change, `useFocusHandoff` (the caller stays mounted and selects the value itself), not
  a hand-rolled "forget on blur", since WebKit blurs a control as it is disabled. A handoff as the holder goes runs
  through `HandoffOnUnmount` (which replays `focusin` after commit) or `useHandoffAfterRemoval`.
- **A list whose order can change while a row may hold focus renders through `useListMotion`**; where rows leave a
  react-aria collection, focus moves to a row that stays in the handler, before the state change.
- **A button disabled for the duration of its own action is `pending`, not `disabled`**, which blurs it and takes the
  dialog's Escape and Tab with it; a control disabled for an outside reason while it may hold focus uses
  `useFocusLeavingWhenDisabled` (buttons) or `useFocusHandoff` (fields).
- **Where a dialog replaces content that can hold focus, restore it with `useRefocusIfLost` or `Dialog`'s `resetKey`**,
  never by re-keying the dialog (`key={item.id}`), whose remounted modal records the outgoing element as its restore
  target. A field that sends its value on blur is decided for dialog close, where no blur fires.
- **Nothing focuses at dialog mount except the control the dialog opens on**, through react-aria's `autoFocus` or, for a
  hand-focused control, a mount `useLayoutEffect`; never the DOM element's `autoFocus`, which a dialog over another
  loses to the outer containment. Content replacing other content in an open dialog takes focus through
  `useFocusWhenShown`. A "refocus if lost" hook compares its dependencies with the last ones, never a "skip the first
  run" flag (StrictMode runs it twice).
- **A modal shown from state of its own (an error alert, a pending question) is rendered from the commit after the
  raising one, from a `useLayoutEffect`, and passes `topLayerAtMount`** for that commit only: two react-aria modals
  mounting in one commit each hide the other and focus falls to `<body>`.
- **A modal that can close with nowhere to return focus calls `focusAfterModalCloses` / `cancelFocusAfterModalCloses`**
  rather than a fallback of its own; code focusing into such a modal by hand uses `useFocusWhenShown`, and a caller
  that focuses the modal itself passes `focusesItselfOnOpen`. A restore after a press decides by whether the pressed
  control had focus, never by whether the press was a key, since VoiceOver presses with a click.
- **A rule of the shape "hide, close or hand focus once focus leaves" gates on the window having focus before it answers
  a `focusout` without `relatedTarget`**, and follows window focus only through `components/hooks/windowFocus.ts`, not
  `visibilitychange` or `focusin`/`focusout`.
- **Retry a focus move across frames with `tryEachFrame` / `afterFrames`, send focus into a panel opened on purpose with
  `landInPanel`, and put a panel on screen with `showPanel`**; never a timer.
- **A scroller whose edges fade has a `scroll-padding` of at least the fade's size** (the fade's own capability gives
  it, adding a sticky heading's height or a pinned column's width), so a focused control never lands under the fade; a
  scroller that scrolls rows by script stops `FADE_SIZE` short of the edge (imported, not written out).
- **A scroller nothing else on screen would scroll becomes a focusable, named `region` while it overflows**, so the
  keyboard reaches what its fade says is there; a region that also does something of its own (the file viewer's
  reading surfaces) stays a tab stop whether or not it scrolls.
- **A control inside a rendering library's shadow root is built by hand to every rule here**: `role`, explicit
  `tabindex`, a name, Enter / Space; only the first of the copies the library draws is a tab stop and named, the rest
  `tabindex="-1"` and `aria-hidden` (`viewer/code/rendererDom.ts`).

## Key handling

- **A key handler goes on the plain DOM element a HeroUI or react-aria component renders, not on the component.**
  react-aria wraps a component's `onKeyDown` / `onKeyUp` and stops propagation after it returns, so an Enter-to-save on
  a `TextField` stops the Escape that closes the dialog around it; use the inner `InputGroup.Input`, or call
  `continuePropagation()` on every path.
- **Escape is taken above the React tree; never guard it locally.** While the drawer or a floating pane card is open,
  `layout/usePaneToggles.ts` holds a capture-phase `window` listener: a control in a `data-escape-scope` region that
  needs Escape is accounted for in its test (`holdsText`). A dispatched Escape is never a neutral way to close a
  tooltip in a scenario or a test.
- **A surface that dismisses itself on Escape calls `useEscapeWhileTooltipOpen`** (dialogs already do), since react-aria
  stops every Escape at `document` capture while a tooltip with a trigger is open.
- **Every window shortcut is a command registered in `shortcuts/commands.ts`** (`REBINDABLE`, or `FIXED` for a key the
  user may not change), shown with `ShortcutKbd` and matched with `matchesCommand` inside `useWindowShortcut`; a key
  outside the registry is missing from Settings and from the conflict check.
- **A window shortcut whose default may also be a browser's or the system's key takes a press only when it changes
  something, in a browser client;** otherwise it stops propagation but keeps its default (`KEEP_DEFAULT`). The repeats
  of a held key are taken.
- **Renaming or splitting a rebindable command adds the old id to `RENAMED_IDS` in `shortcuts/store.ts`**, mapped to
  its successor, so stored preferences and imported configuration documents keep the user's key.
- **A matcher that accepts keys beyond its command's binding yields when the pressed keys are another command's binding
  on this client** (`terminal/terminalFontShortcut.ts`): `useWindowShortcut` stops propagation but not immediate
  propagation, so one press could run two commands.
- **Any Cmd/Ctrl test goes through `isPrimaryModifier` from `input/keys.ts`, and showing the key goes by
  `host.modifier`** (`ShortcutKbd`); never derive the platform again.
- **Add a window `keydown` listener in an effect (`useWindowShortcut`), never at module evaluation, and keep it in the
  statically imported graph from `App.tsx`**: `shortcuts/recording.ts` installs its capture listener at module load so
  that it runs first while a shortcut is recorded.

## Names, roles and states

- **Every control has an accessible name**: its visible text, or an `aria-label` when it has none. An icon beside a
  name is `aria-hidden` (lucide adds it; a slot taking a caller's icon types it `LucideIcon`); an icon that alone
  carries a meaning is a named `role="img"`.
- **A control with visible text and a name of its own has the visible text in the name verbatim, numbers included**
  (WCAG 2.5.3), checked against every state the text has; the one accepted exception is a shortcut's key caps as its
  edit button.
- **A name made of two messages is assembled with `aria-labelledby` over the elements that carry them**, never joined
  into one `aria-label`; a control in a table of fields is named by its column and row headers that way, with no
  `aria-label` of its own; a name drawn as several flex boxes is one `sr-only` string with the boxes hidden (WebKit puts
  a space around every flex item); a field whose visible label shows less than its name keeps both inside its
  `<label>` (`TextInput`'s `labelContent`), never an `aria-label` beside it.
- **An element that takes an `aria-label` without being a control gets a role that can be named** (`role="region"`).
- **An icon-only control carries both a tooltip and an accessible name**, not always the same text
  (`docs/product/labels-and-tooltips.md`, "Tooltips on icon-only controls"); a control whose hover opens a panel showing
  what its tooltip would say, a stepper's − and +, and a tree chevron carry only the name. Inside a react-aria
  collection row the name is the bare verb, as react-aria composes it with the row's.
- **What a tooltip says reaches a screen reader by another route**: a tooltip answers the pointer (and a control's
  keyboard focus) only, and an anchored one is linked to nothing.
- **A visual heading is an `<h1>`-`<h6>` that also looks like one and never outranks the one above it**
  (`EmptyPanel`'s `headingLevel`); a menu's group heading is the menu's `<header>`, and Markdown inside a view with its
  own heading is rendered `embedded`.
- **The content panel holds exactly one `main`** (`ContentMain`); a view that is a region rather than the main content
  is a named `<section>`.
- **A control that shows or hides a region carries `aria-expanded`**, plus `aria-controls` for a floating panel (a
  right-pane toggle only while its panel is in the document). A button whose name and icon swap with its state names
  the action and carries no `aria-pressed` or `aria-expanded`. A hand-built list or tree marks its selected row
  `aria-current`; HeroUI's `Tabs` and `ListBox` mark theirs and get none on top.
- **An operation that reports how far it has got shows HeroUI's `ProgressBar`, named by what is in progress**, the
  count as its value text ("3 of 9"); while it cannot count yet it is indeterminate, with `aria-describedby` on the text
  saying what it waits for. It is announced when it starts and at each new phase, not on every step.
- **A hand-built `role="button"` element needs `tabIndex={0}` and Enter / Space, and when it holds a `<button>`,
  `<select>` or button-like `<input>` its first child is its own hidden plain `<button>`** (`TreeRow`'s
  `RowPressTarget`): WebKit sends an accessibility press on it as a click on the first such descendant, which would
  otherwise be one of its actions. Build such a row on `TreeRow`; treat the tab, menu item and list item roles with the
  same care until checked.

## Announcements

- **A change nobody acted for is announced from `role="status"`, an error from `role="alert"`**; HeroUI's `Alert`
  renders no role, so an error appearing in one after the user moved on puts `role="alert"` on an element of its own.
- **Announce through `StatusAnnouncer` and its family, never a region rendered with its text**: a region holding text
  as it is inserted is not reliably read, so the region stays mounted (empty between texts, `sr-only` in a laid-out
  row, never `display:none`) and its first write is deferred.
- **A write near a focus move waits `AFTER_FOCUS_MOVE_DELAY_MS`** (`firstWriteDelayMs`, or `AttendedStatusAnnouncer`):
  VoiceOver drops a status written within about 300 ms of one. A repeat in the same words is said only if the region
  was emptied between.
- **A status about what a modal's own action produced is written after the close and its focus move**, since a modal
  hides everything outside it but a live announcer (`liveAnnouncerProps`) or a top layer.
- **A status whose region can sit in an `inert` subtree is held and written once the area is exposed again**
  (`useHeldWhileHidden`, or `useAnnouncedOnRise`'s `deferred`), unless exposure lands focus on a region already reading
  the same copy.
- **A status about something that can happen while the user is out of the window waits until the window is attended
  (`useWindowAttended`)**, and anything re-read or let go on the user's return or departure goes through
  `onWindowFocusReturn`, `onWindowFocusLoss` or `subscribeWindowFocus`.
- **A change made under the user that nothing on screen marks goes through `ChangeAnnouncer`**, which remounts per
  change and clears after `READ_TIME_MS`; `ChangedElsewhereAnnouncer` words a form's replaced fields.
- **A status region for content a disclosure folds away is mounted outside the folded body, only while it is open.**
- **A dialog step whose title says what happened moves focus to the dialog instead of adding a status**, and a dialog
  whose footer changes with a state the user waits on puts focus on the state's main action.
- **Each failure is announced once, by its owner**: the error alert dialog speaks the failures it shows; a failure on
  the screen announces itself once when it appears after the view first painted (`role="alert"` on its title and first
  description only); a mark has no live region, so its view speaks it through its own `StatusAnnouncer`.

## Pointer target size

- **A control's pointer target is at least 24 by 24 CSS pixels** (WCAG 2.5.8). Compact actions use the wrapper
  `Button`'s `size="xs"` (24px, icon-only a 24px square), never per-call sizing.
- **Row icon buttons share `--row-control-size` and `--row-control-gap`** (`styles/layout.css`) through `RowIconGroup`
  and `RowIconSlot`, never a local offset or the text row's gap.
- **Where the drawing must stay under 24, extend the target, not the drawing**: an absolutely positioned `::after` with
  a negative inset. A mark whose tap opens a touch popover is held to 24 on a coarse pointer through `.touch-target`.
- **A mark that only opens a tooltip with a mouse, and a mark in running text, are not held to the floor**; a control
  sized in units of something else (the diff's separators, in code lines) is checked whenever that unit changes.

## Not by colour alone

- **A two-state view control is one icon button whose icon is its state and whose name says what pressing does**
  (`IconToggle`, which resets HeroUI's selected fill; `aria-pressed` for assistive technology, plus `aria-expanded` when
  it shows a region). A region it takes away while focused hands focus back to it.
- **A selection is a shape: a fill only the selected item has, a bar, or a check mark**, with `aria-current` or
  `aria-selected`; a change in the colour of a fill every item has, or of a label or icon, is not enough.
- **A notice's severity is its glyph (circled `i`, triangle, circled `!`), never its fill alone.**
- **A colour mark that only echoes what words beside it, its tooltip or its row's name say is decorative** and not held
  to 3:1; a mark that is the only carrier of a state is.

## Text direction

- **Position, spacing, borders, corners and alignment use Tailwind's logical utilities** (`start-*` / `end-*`, `ms-*` /
  `me-*`, `ps-*` / `pe-*`, `border-s`, `rounded-s-*`, `text-start`), not `left` / `right` / `ml` / `pr`; `translate-x-*`
  has no logical form, so a slide gets an `rtl:` twin with the opposite sign. Why: under a right-to-left language the
  window mirrors, and nobody sees a miss without switching to Arabic.
- **Prose follows the interface; a value keeps its own order** (WCAG 1.3.2). A value that is not prose (a path, a URL,
  an email address, a branch, a commit id, a command, a port, a shortcut, a `1 / 12` count, a literal to type, a
  diagram, a formula) is drawn with `dir="ltr"` on the element that draws it (on a field's group where it holds an
  affix), with `align-match-parent` where it stands on a line of its own; a free-form name (a session title, a project,
  tag or account name) takes `dir="auto"`.
- **A value substituted into a sentence is isolated**: `<bdi dir="auto">` as React children, `isolate()` from
  `i18n/isolate.ts` in a plain string, at the point every value of a kind passes through (`daemonMessage`). A string
  that is matched, sorted, keyed or a collection's `textValue` is never isolated, and a value in an element that carries
  a `dir` needs none.
- **An LTR value field whose placeholder is prose lays the placeholder out in the interface's direction**, `dir="ltr"`
  only once it holds a value.
- **An icon mirrors only when its meaning is a direction** (`rtl:-scale-x-100`; Back and Forward, a disclosure or
  submenu chevron, a pane toggle, a leading-side layout glyph); an expand / collapse chevron switches between two glyphs
  (forward collapsed, down expanded), never one rotated, since a mirrored forward chevron turned a quarter points up
  (`ExpandChevron` in `project-browser/treeRow.tsx`).
- **A badge, overlay or mask placed with physical offsets gets an `rtl:` twin**, and where a library offers only
  physical placements, the placement is picked from the language (`textDirection(useCurrentLanguage())`).
