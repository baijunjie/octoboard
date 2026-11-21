# Building on HeroUI and react-aria

The rules for building on HeroUI 3 and react-aria are in `docs/conventions/ui-components.md`. These are the traps
behind them, most of which a jsdom test that only asserts the DOM passes.

## Pin what a react-aria element puts in the DOM on a render, not from its file

An ancestor's context may override a part's default: a `Dialog` sets its title slot to `level: 2`, so `Modal.Heading`
is an `h2`, not react-aria's default `h3`. Assert it in a jsdom test or the running page.

## Search a HeroUI part for `children ??` before passing it children

Where a part's default children follow the parent's variant or state (`Alert.Indicator`, `ProgressBar.Output`), any
children replace that distinction silently: pass none. Pass children only where the default is a fixed stand-in you
mean to replace.

## HeroUI sizes controls by the viewport, not the pane

`Button` and `ToggleButton` grow one step below 48rem whatever pane they sit in, so a fixed-height row holding one gets
the height of the larger form, and is looked at in a window narrower than 768px.

## Check that react-aria sets a state attribute for your configuration before styling on it

Look in the component's file under `react-aria-components/dist/private/` that the hook setting the attribute is enabled
for the way you use it (a removable-only `Tag` never gets `data-hovered`); otherwise style with the CSS pseudo-class
(`hover:`).

## Override a HeroUI state look on every selector its sheet keys that state on

HeroUI keys hover on `:hover` and `[data-hovered]`, and `data-hovered` clears a frame after `:hover`: an override of one
alone flickers (in a masked scroller in WKWebView, a repaint of the whole tab strip). The shared tab rule in
`style.css` matches both.

## Hover inside a list item's `Label` or `Description` needs pointer events back

HeroUI sets `pointer-events: none` on a `ListBox` or menu item's `Label` and `Description`, and it inherits, so hover
content there (a tooltip, `FadeOverflow`, `hover:`) needs `pointer-events-auto` on that element, `cursor-[inherit]` on
the `Label`, and `ui-disabled:hover:` / `ui-disabled:active:` cancellation on disabled rows. Before relying on hover in
any other HeroUI part, search its sheet for `pointer-events-none`.

## A virtualized `ListLayout` list takes its inset on rows, not the list

In `ListBox`, `Tree` and `GridList` under `ListLayout`, side padding on the list overflows sideways and clips focus
rings: clear it with `px-0` and give the inset to rows and headings (`mx-1` with `w-auto`, header padding). A `Select`
whose items show `ListBox.ItemIndicator` needs `pe-7` per item.

## A `Tabs.Panel` mounts only while its tab is selected, and a collection renders twice

- State, refs and effects in a panel are discarded on every switch. `shouldForceMount` is only for hidden panels that
  must keep taking layout (the rail quota panel), hidden yourself (`ui-inert:invisible`) with HeroUI's panel spacing
  cleared; it is not a way to keep state.
- A react-aria collection, `Tabs` among them, renders what is inside it twice, once in a hidden copy. Anything whose
  effect must happen once (a connection, a subscription, timers, document listeners) goes outside `<Tabs>`'s children
  or inside a panel's content; a hook watching an element inside a panel attaches in a ref callback (`useListMotion`
  does); a tab's handler reaches what it needs through the DOM from its own element, not a ref on `Tabs.List`.
- State that later makes an element inside a panel tabbable (a `useFadeRegion` scroller) is held by the component that
  renders the `Tabs.Panel`, so the panel drops its own `tabindex`: react-aria's observer misses a descendant's
  `tabindex` change, and in WebKit Tab onto such a panel drops focus to `<body>`.

## Check the traps where they can show, not in a DOM-only test

- Hover: jsdom never hovers, and the gallery's hover steps dispatch events to an element without hit-testing, so a style
  keyed on a state attribute react-aria never sets, or hover content under HeroUI's `pointer-events: none`, passes
  both. Only a real pointer in the running page shows it.
- State held in a `Tabs.Panel`: a unit test mounts the panel alone and never switches tabs, and neither does the obvious
  click-through. Switch to another section and back (and close and reopen a dialog) before trusting it.
- Something placed inside `<Tabs>` outside a panel's content runs its effects twice; a jsdom test that counts a mocked
  child's effect runs catches it.
- A panel that stays a tab stop after a child turns tabbable late: a jsdom test makes the late change and asserts the
  `tabpanel` has no `tabindex` afterwards.
