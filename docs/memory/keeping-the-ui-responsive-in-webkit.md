# Keeping the UI responsive in WebKit

## A style change that reaches the whole document costs tens of milliseconds in the app

The app's WebKit recalculates the style of the whole document in roughly 45–65 ms for the window's ~2,800 elements,
where Chrome takes about 8 ms. So anything that happens per interaction (opening or closing an overlay, hovering,
dragging, an update per daemon event) must not change an attribute, class, inline style or custom property on
`<html>`, `<body>` or `#root`, nor change which structural pseudo-classes (`:last-child`, `:first-child`, `:nth-*`)
`#root` or another large subtree's root matches: Tailwind's `space-y-*` and `divide-y` select on `:last-child`, so
such a change restyles everything under it. Put interaction state on the narrowest element that needs it.

That is why each HTML entry of `packages/ui` keeps a permanent hidden sibling after `#root`: overlays are portalled to
the end of `<body>`, and without it the first one mounting and the last one unmounting flip `#root`'s `:last-child`.
A new HTML entry needs the same sibling.

One whole-document restyle per overlay is not ours to remove: every modal overlay — each HeroUI `Dropdown`, `Popover`
and `Modal` in `packages/ui`, `ActionMenu` included — has React Aria set `inert` on everything outside it, `#root`
included, on open and take it off on close (`ariaHideOutside` with `shouldUseInert` hardcoded, so not the cheap
`aria-hidden`). Once nothing of ours restyles the document, that toggle is what is left of a menu's open and close in
WebKit, tens of milliseconds each against a few in Chrome. Do not chase it with `isNonModal`: that skips the `inert`
walk but also stops a press outside from closing the overlay. Nor patch `node_modules` or remove
`HTMLElement.prototype.inert`. The only levers left are fewer elements under `#root` and cheaper style recalculation
per element (fewer and simpler CSS rules on the page).

## Measure UI latency in Playwright's WebKit over the production build, and read only proportions

Chrome understates the app's costs several-fold (one full restyle measured 6 ms there and 39–48 ms in WebKit), so a
latency question is measured in Playwright's `webkit`, against `packages/ui/dist` from `pnpm build` served
statically, never the dev server. Time the user's step end to end, from the real pointer event to the result being in
the DOM (pointer down to the menu's popover).

Headless WebKit is faster than the app's WKWebView in absolute terms — a menu that took about a second to open in the
app took ~126 ms there — so compare before against after in the same harness and judge the fix by the ratio, then
confirm the feel in the real app. To give the page the real daemon's event load, attach it to the running daemon only
with every frame the page sends dropped before it leaves (Playwright's `page.routeWebSocket`, passing on only what the
daemon sends), so nothing reaches the user's sessions.
