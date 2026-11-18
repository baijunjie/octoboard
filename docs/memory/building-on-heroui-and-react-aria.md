# Building on HeroUI and react-aria

## Check the HeroUI and react-aria traps where they can show, not in a DOM-only test

The rules for building on HeroUI 3 and react-aria (the wrapper layer, compound parts, state attributes, pointer events
in list items, context props, `Tabs` panels and the collection's hidden copy, tooltip wiring, overlays closing on
scroll) are in `docs/conventions/ui-components.md`, "The wrapper layer" and "HeroUI and React Aria". Most of their
failures pass a jsdom test that only asserts the DOM, so check each where it can show:

- Hover: jsdom never hovers, and the gallery's hover steps dispatch events to an element without hit-testing, so a style
  keyed on a state attribute react-aria never sets for that configuration, or hover-driven content under HeroUI's
  `pointer-events: none`, passes both. Only a real pointer in the running page shows it.
- State held in a `Tabs.Panel`: a unit test mounts the panel alone and never switches tabs, and neither does the obvious
  click-through. Switch to another section and back (and close and reopen a dialog) before trusting it.
- Something placed inside `<Tabs>` outside a panel's content runs its effects twice; a jsdom test that counts a mocked
  child's effect runs catches it.
- A panel that stays a tab stop after a child turns tabbable late: a jsdom test makes the late change and asserts the
  `tabpanel` has no `tabindex` afterwards. In WebKit, Tab onto such a panel drops focus to `<body>`.
