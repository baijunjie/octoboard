# Keeping the UI responsive in WebKit

## A style change that reaches the whole document is expensive in the app's WebKit

The app's WKWebView recalculates style many times slower per element than Chrome, so anything that happens per
interaction (opening or closing an overlay, hovering, dragging, an update per daemon event) must not change an
attribute, class, inline style or custom property on `<html>`, `<body>` or `#root`, nor change which structural
pseudo-classes (`:last-child`, `:first-child`, `:nth-*`) `#root` or another large subtree's root matches: Tailwind's
`space-y-*` and `divide-y` select on `:last-child`, so such a change restyles everything under it. Put interaction
state on the narrowest element that needs it.

That is why each HTML entry of `packages/ui` keeps a permanent hidden sibling after `#root`: overlays are portalled to
the end of `<body>`, and without it the first one mounting and the last one unmounting flip `#root`'s `:last-child`.
A new HTML entry needs the same sibling.

A synchronous layout read (`offsetHeight`, `getBoundingClientRect`, `scrollHeight`, `getComputedStyle`) made after a
DOM change runs the pending restyle there and then, so an interaction whose components each change the DOM and then
measure it (a pane switch re-measuring its fades, scrollers and FLIP animations) pays the full restyle once per
read, not once per frame. Judge an interaction's cost as restyle cost times the number of forced passes; make the
restyle cheap first, since that shrinks every pass at once.

One whole-document restyle per overlay is not ours to remove: every modal overlay — each HeroUI `Dropdown`, `Popover`
and `Modal` in `packages/ui`, `ActionMenu` included — has React Aria set `inert` on everything outside it, `#root`
included, on open and take it off on close (`ariaHideOutside` with `shouldUseInert` hardcoded, so not the cheap
`aria-hidden`). Once nothing of ours restyles the document, that toggle is what is left of a menu's open and close in
WebKit. Do not chase it with `isNonModal`: that skips the `inert` walk but also stops a press outside from closing the
overlay. Nor patch `node_modules` or remove `HTMLElement.prototype.inert`. The only levers left are fewer elements
under `#root` and cheaper style recalculation per element.

## Keep a tag, class or id on the rightmost compound of every CSS rule

WebKit files each style rule under the id, class or tag of its rightmost compound selector and, for an element, tries
only the rules filed under what that element has. A rule whose rightmost compound has none of the three — only
`:where()`, `:is()`, `:not()`, another pseudo-class, or `*` — is tried against every element on every style
recalculation, so a hundred such rules made each restyle of a large region an order of magnitude slower in the app
while Chrome and Playwright's WebKit showed only a fraction of it. This applies to any CSS written or generated for
`packages/ui`: stylesheets, Tailwind arbitrary variants, and plugins or packages that emit CSS.

- Keep a tag, class or id in the rightmost compound. `:where()` goes around ancestors or as a suffix on that compound
  — `:where(.prose) pre:where(:not(.not-prose, .not-prose *))`, never `.prose :where(pre)`. Unwrapping a `:where()`
  gives the rule the weight of what was inside it: a tag-only compound still loses to a utility class on the element,
  but one carrying a pseudo-class or an attribute now beats it, so check what the rule is meant to lose to.
- `@tailwindcss/typography` is loaded only through `packages/ui/typography.mjs`, never as the bare plugin: its default
  output is exactly this shape, 133 rules of it.
- After adding a stylesheet, a CSS-emitting plugin or package, or a selector-heavy variant, count the rules in the
  built `packages/ui/dist/assets/*.css` whose rightmost compound starts with `:where(`, `:is(` or `:not(`, or is a
  bare `*`, against the count before. About 150 remain from HeroUI component states, toasts, Tailwind's
  `space-y`/`divide-y` and the typography plugin's sibling and child shapes; a jump of dozens is the thing to fix.

## Measure UI latency in the system WKWebView; Playwright's WebKit and Chrome under-report it

Only the system WKWebView the app runs in gives real style-recalculation cost. Playwright's headless WebKit reports
it several times too low and Chrome lower still, so a cause that dominates in the app can look negligible in both:
never conclude from either that styling is not the bottleneck.

- To judge a change's latency, run `pnpm bench:wkwebview` in `packages/ui` on the base and on the branch, on the same
  machine with nothing else busy, and compare the two tables; `packages/ui/scripts/wkbench/README.md` (sections
  "Running it" and "Reading the tables") has the options and what each column means.
- Its `--engine webkit|chromium` runs are only a coarse before-against-after ratio, and its Chrome CPU profiles
  (`--profile`) only say which code spends the time, never how long the app takes.
- Never point it at the user's own daemon: its scenarios drive that daemon's sessions (the `idle` one renames them),
  and its fixtures create consoles and sessions and trust folders. Leave `--port` out so it starts its own throwaway
  one, or pass it only for a daemon you started yourself on a scratch `HOME`.
