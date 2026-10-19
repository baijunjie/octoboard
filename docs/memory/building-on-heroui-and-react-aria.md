# Building on HeroUI and react-aria

## Build from HeroUI 3's own component wherever it has one

In `packages/ui`, before writing a component, check whether HeroUI 3 already provides it, and if so use that one
rather than assembling a look-alike from other HeroUI parts or plain elements. The installed package's
`packages/ui/node_modules/@heroui/react/dist/components/` is the authoritative list for the version in use (it
includes less obvious ones such as `toast`, `drawer`, `disclosure`, `toolbar`, `empty-state`, `kbd`, `skeleton`);
HeroUI's own docs are the reference for how to use each, but only a component's own file in that directory shows what
it puts in the DOM — a hardcoded attribute, which child it treats as a given part, which props it drops, an element
of its own around your children — and a decision about wrapping, naming or placing one usually turns on that, so read
it rather than its typings: `Popover.Trigger` wraps what it is given in a `div role="button"` of its own, and that
`div`, not your child, is what react-aria places the popover against. A hand-assembled
stand-in looks and behaves unlike the rest of the UI and lacks what HeroUI's carries: the toast stack built from
`Alert` + `CloseButton` with its own timers and stacking had none of `Toast`'s pause on hover and focus, queueing or
ARIA region semantics.

- Give a compound component's layout classes to its parts themselves and never put an element of your own between
  two parts: some of HeroUI's variant styles use direct-child selectors (`.tabs--secondary > .tabs__list-container`,
  the `switch--sm` / `switch--lg` sizes), so a wrapper `div` silently switches the variant off. A parent may also pick
  a part out of its children in JavaScript, and only among its *direct* ones (`Tag` scans for `Tag.RemoveButton` that
  way), so a part wrapped in an element of yours — the project's own `TitledControl` among them — leaves the parent
  rendering its default part beside yours. Where the parent takes a render-function child, as `Tag` does, that form
  skips the scan and is how to wrap a part.
- A part given no children fills itself in, and for some of them what it fills in follows the parent's variant or
  state: `Alert.Indicator` and `Toast.Indicator` draw the glyph that goes with the `status`, `ProgressBar.Output`
  writes the value's own text. Children of your own replace that, putting one fixed thing where the variant's choice
  was, so a distinction the component was drawing is quietly gone with nothing wrong to see in the source or the
  result. Before passing children to a part, search its component's file under
  `packages/ui/node_modules/@heroui/react/dist/components/` for `children ??` or `children ||`: where the default
  follows the parent, pass none and let the variant pick; where it is a fixed stand-in (`CloseButton`'s cross, a
  `MenuItem`'s submenu chevron), pass children only to put a different one there.
- The project's existing wrappers over a HeroUI component (`Dialog` over `Modal`, `ActionMenu` over `Dropdown`), and
  the shared dialogs built on those, are that component; use the wrapper where one exists. Where the wrapper's shape
  cannot hold what is being built, assemble the HeroUI component's own parts instead, rather than widening the wrapper
  with a switch that only the one case would ever pass, and put a comment at the component saying why it is not built
  on the wrapper.
- A hand-built element is acceptable only when HeroUI has no equivalent, or its equivalent cannot meet a stated
  requirement. Then build it on react-aria / react-aria-components hooks, as HeroUI itself is, not on bare DOM event
  handling, and put a comment at the component saying why HeroUI's is not used (a design-intent comment under the
  "Comment conventions" section of `.claude/skills/agent-docs/SKILL.md`).

## A HeroUI control's size follows the window's width, one step larger below 48rem

HeroUI 3 sizes a `Button` or a `ToggleButton` by the **viewport's** width, not its container's: every size variant is
one step taller, and an icon-only one that much wider, below a 48rem (768px) window — `sm` icon-only is 36px square
there against 32px at and above, the default 40px against 36px (the `@media (min-width: 48rem)` rules in
`packages/ui/node_modules/@heroui/styles/dist/heroui.min.css`). So a control measured at a comfortable window width
has a second, larger height nothing in the source, the types or a screenshot shows, and a row sized to the smaller one
is overflowed by it as soon as the user narrows the window.

Give any fixed-height row that holds such a control the height of its larger form, and look at the row in a window
narrower than 768px as well as a wide one. The pane the row sits in tells nothing: a narrow pane in a wide window
still gets the small form.

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

## An ancestor's context decides a react-aria element's props, so the default in its own file is not what renders

A react-aria-components element merges a context value over the props it was given (`useContextProps`), and a parent
that treats one as a known part supplies that value per slot, so a default written in the element's own file can be
replaced from above with nothing at the call site to show it: react-aria's `Heading` defaults to `level = 3`, but a
`Dialog` sets its `title` slot to `level: 2`, so HeroUI's `Modal.Heading` and `AlertDialog.Heading` — each a `Heading
slot="title"`, and rendered in this project as `Frame.Heading` (`packages/ui/src/dialogs/Dialog.tsx`, where `Frame` is
one of those two) — render `h2`, and a heading inside a panel that can appear in a dialog is pitched against that
rather than against `h3`.

So settle what such an element puts in the DOM on a render — a jsdom test asserting the tag, or the running page —
rather than on the default read off its own file under `packages/ui/node_modules/`, which an ancestor's context may
have overridden; where the two disagree, the file loses.

## A HeroUI `ListBox` or a `Tree`: where its padding goes

For a list rendered inside react-aria's `Virtualizer` with `ListLayout` (a HeroUI `ListBox`, one in a `Select`'s
popover included, or a react-aria `Tree` or `GridList`), `ListLayout` makes every row as wide as the scroll container's
inner width and then places it inside the container's padding, so any side padding on the list — HeroUI's own `p-1` on
`ListBox`, `p-1.5` on a `Select` popover's list — makes it overflow sideways (a horizontal scrollbar) and clips the
rows' focus rings. Clear it with `px-0` on the list and give the inset to the rows and headings instead (`mx-1` on an
item, with `w-auto` where the item's own styles set a width as HeroUI's `ListBox.Item` does, and padding on a
`Header`). HeroUI corrects this only inside its `Autocomplete`.

Virtualized or not, a `Select` whose items show `ListBox.ItemIndicator` needs `pe-7` on each item: the `Select`
popover's styles give its items `px-2.5`, which overrides the `pe-7` `ListBox.Item` reserves for the check mark, so a
long label otherwise runs under the mark.

## A `Tabs` panel is mounted only while its own tab is selected

A tab panel in `packages/ui` exists only while its tab is the selected one: react-aria-components' `TabPanel`
returns `null` otherwise, and HeroUI's `Tabs.Panel` passes its props straight through to it. A panel's state, its
refs and the effects hanging off them are thrown away on every tab switch, and a dialog's panels again on every
close.

So state held inside a panel is worth only "for as long as this section is on screen" — a scroll position, a
half-typed field, an expanded row. Anything the section must not lose is derived afresh instead, from the shell or
the daemon, or from state hoisted above the `Tabs`, and never remembered in the panel. Nothing catches the miss: a
unit test mounts the panel on its own and never switches tabs, and neither does the obvious click-through. The
Permissions section's "restart for this to take effect" line was first built from React state remembering the
grant it had watched arrive, and died on the first click to another section.

The panel also mounts, unmounts and is replaced in a commit that does not render the component holding it: react-aria
renders the panel from the collection it built when the selection changes. So a hook whose owner renders the `Tabs`
and hands a ref to an element inside a panel never sees that element arrive or go from its own effects — they run only
on the owner's next render, which already carries that render's change. A hook that watches such an element attaches
and detaches in a ref callback, as `useListMotion` (`packages/ui/src/components/useListMotion.ts`) does.

`shouldForceMount` keeps the unselected panels mounted, but it is not the way to keep a panel's state. Pass it only
where the hidden panels must go on taking up layout, as the rail's quota panel
(`packages/ui/src/quota/RailQuotaEntry.tsx`) is the one place that does: it stacks every agent's panel in one grid cell so that, anchored by its bottom edge, it keeps the tallest tab's height
and its tab strip does not move out from under the pointer. A force-mounted unselected panel is rendered `inert` with
`data-inert="true"` and without its `role`, `id` or `tabIndex`, but it is still shown, so hide it yourself
(`data-[inert=true]:invisible`). HeroUI's `.tabs__panel` gives every panel padding and a top margin and takes an
exiting one out of the flow (`data-[exiting=true]`, `position: absolute`), so where the panels share one box, clear
those on the panels and put the spacing on the container.

## A `Tabs` panel stays a tab stop until it renders with something tabbable inside

react-aria's `useTabPanel` makes the selected panel itself a tab stop (`tabIndex={0}`) while `useHasTabbableChild`
finds nothing tabbable inside it. That check runs again when the panel's own component renders and when nodes are
added or removed inside it, but never when a descendant's `tabindex` attribute changes: its `MutationObserver`
filters on `tabIndex` in camelCase, which does not match the attribute's name in the DOM.

So state that later makes an existing element inside a panel tabbable — a `useFadeRegion` scroller that becomes a
focusable region once it reports overflow — is held by the component that renders `Tabs.Panel`, so the change
re-renders the panel; held in a child of the panel, it leaves the panel a stop in front of the element. In WebKit,
Tab onto such a panel then drops focus to `<body>`, since the render on that focus finds the element and takes the
panel's `tabindex` away. A jsdom test catches it by making the late change and asserting the `tabpanel` has no
`tabindex` afterwards.

## `Tabs.Indicator` only on a strip whose tabs are fixed

HeroUI's `Tabs.Indicator` is react-aria-components' `SelectionIndicator`, which slides from tab to tab with a
shared-element transition. When the newly selected tab renders again within the frame it was selected in, that
transition is left stuck part-way, and a tab created and selected in one go always renders again then. So in
`packages/ui`, use `Tabs.Indicator` only where the set of tabs does not change while the strip is shown; a strip that
adds a tab and selects it at once draws the selected tab's fill on the tab itself (`data-[selected=true]:` classes
carrying the indicator's fill and shadow), as the Terminal panel's strip in
`packages/ui/src/terminal/ShellTerminalPanel.tsx` does.

## An icon-only control also gets a tooltip, through `TitledControl`

Every control in `packages/ui` that shows only an icon — a `CloseButton` or a menu trigger as much as a HeroUI
`Button` with `isIconOnly` — carries a HeroUI tooltip on top of its `aria-label`, and the two are not always the same
text (the "An icon-only control: tooltip and `aria-label`" section of `docs/ui-accessibility.md`). Add the tooltip by
wrapping the control in `TitledControl` from `packages/ui/src/components/TitledControl.tsx`, as the window chrome's
`ChromeButton` in `packages/ui/src/components/ChromeButton.tsx` does. Passing `title` to the HeroUI control itself
does nothing: its react-aria base filters `title` out of the DOM props without a warning.

Except for the `Switch` case below, a tooltip on a control takes this shape — the control directly inside HeroUI's
`Tooltip`, as `TitledControl` does — never `Tooltip.Trigger`, which renders a focusable `role="button"` `div` around
the control: an extra tab stop that
also takes focus on a mouse press, undoing `preventFocusOnPress`. The shape works only when the child itself consumes
react-aria's `FocusableContext` (through `useFocusable` or `Focusable`), which is where HeroUI's `Tooltip` leaves the
trigger props and the trigger ref; every HeroUI button and trigger does. Any other child gets no tooltip at all and
nothing warns — a plain element or a `span` around the control, and a react-aria collection item as well
(`ListBox.Item`, built on `useOption` / `useSelectableItem`), which reads as a react-aria-components control and is
not one for this purpose.

Choose the tooltip shape by what carries it:

- A control: `TitledControl`, as above. It opens on hover and on keyboard focus, or on keyboard focus alone where
  its `focusOnly` prop is passed, for a control whose hover already does something else. While it is open react-aria
  links it to the control as its description, except where the tooltip only repeats the control's `aria-label`:
  `TitledControl` unlinks that one on purpose, so the same words are not announced twice.
- A HeroUI `Switch` needing both hover and keyboard-focus tooltips outside a collection row: wrap the whole
  `Switch` in `Tooltip.Trigger role="presentation" tabIndex={-1}` inside `Tooltip`, and give `Tooltip.Content`
  `pointer-events-none`, as in `packages/ui/src/sidebar/FocusView.tsx`. Keep the accessible name on `Switch`; when
  the tooltip repeats that name, pass `aria-describedby=""` on `Switch` to avoid a duplicate announcement. Do not
  cancel the wrapper's mouse press: the switch must still take focus. `TitledControl` directly around `Switch`
  sends its hover props to the visually hidden input through `useSwitch` / `useToggle`, so the visible control
  does not open it on hover; directly around `Switch.Content` reaches neither hover nor focus, since its
  `SwitchButton` does not consume `FocusableContext`. Verify hover and keyboard focus in the running page when
  changing this composition.
- A non-control indicator that can be its own trigger — a badge, a status mark: `IndicatorTooltip` in
  `packages/ui/src/components/IndicatorTooltip.tsx`. It answers the pointer, and fixes what a bare
  `Tooltip.Trigger` gets wrong: the trigger hardcodes `role="button"` (so one left without a `role` exposes an
  unnamed button) and is focusable, so it always sets `tabIndex={-1}`, cancels `mousedown` so a press does not take
  focus off the terminal, and takes either a `label` (`role="img"` and `aria-label`) or `role="presentation"`
  (browsers ignore that role on a focusable element, so it only keeps the unnamed button role off). That
  `tabIndex={-1}` keeps the trigger out of the Tab order, not out of every keyboard route: a row that walks its
  focusable descendants with the arrow keys — a react-aria `GridList` or `Tree` row — reaches the trigger as a stop
  of its own and opens the tooltip on focus, which is a stop the row should not have, so a mark inside such a row
  takes the anchored shape below instead. A mark inside a collection item whose row has no such walk (a
  `ListBox.Item`) carries this shape as usual. A deliberately focusable indicator would use `Tooltip.Trigger`
  itself.
- Text or a mark that cannot be a trigger where it sits — a run of running text, or a mark in a row that walks its
  focusable descendants as above: `AnchoredTooltip` in `packages/ui/src/components/AnchoredTooltip.tsx`. The caller
  reports the hover itself and names the element the tooltip is placed against, which may be an ancestor of what the
  tooltip belongs to. Never name as that anchor an element the UI itself scrolls: react-aria closes a tooltip as soon
  as anything containing its anchor scrolls, so a label that runs as a marquee and anchors to itself loses its
  tooltip to its own marquee.

Only a control's tooltip opens on keyboard focus, so neither of the other two shapes may be the only route to what
it says. An `IndicatorTooltip`'s own `label` is that route where the mark means something on its own. An
`AnchoredTooltip` adds no element at all and is not linked to what it belongs to for assistive technology either, so
its text has to reach a screen reader some other way — the text itself, which is in the document whether or not it is
clipped, or the name of the row it sits in. Do not reach for that shape where no such route exists; the only exception
is where nothing around the text reaches assistive technology either, kept so far for one case only, in the "A tooltip
is the application's own, never the browser's `title`" section of `docs/ui-accessibility.md`.
