# Fading the edge of a scroller

## Every faded scroller takes the fade from one module

A surface in `packages/ui` that hides content behind its own edge fades that edge, and no site works out how: the fade
comes from `packages/ui/src/components/scrollFade.tsx`, which supplies the fade's size and the matching scroll padding
(the "Focus scrolled to the edge of a scroller that fades there" section of `docs/ui-accessibility.md`), so no site
states either. Never reach for HeroUI's `ScrollShadow` directly and never hand-build a fade: HeroUI's own default size
is a larger one (40px) that nothing here uses, and a bare `ScrollShadow` carries no scroll padding at all. The library
fades exactly one thing of its own — a `Tabs` tab strip, over a size nothing outside it can configure — and that one is
left as it is; every other faded surface here is the project's own and goes through the module.

Both halves of that are enforced by `pnpm --filter ui test`, which fails on a scroll container in `packages/ui/src`
that does not take its fade from the module and on `ScrollShadow` named anywhere outside it. A surface that genuinely
cannot take the fade is declared, with its reason, in the `EXEMPTIONS` list of
`packages/ui/src/components/scrollFadeCoverage.test.ts` — that list is the whole account of what the rule leaves
alone, so a scroller the check cannot see (one whose overflow comes from a library's stylesheet or another package's
DOM) is declared there as well, rather than passing unmentioned. Declaring it is the only way out: never slip past the
check instead, by assembling the overflow class from pieces or moving the scroller out of this package's source.

Which API a site takes follows from its scroller:

- `FadedScroller` is an element wrapped around the content, and is the one to reach for wherever the scroller can be
  that wrapper.
- `useScrollFade` fades an element that has to **be** the scroll container. Three things make a scroller that, they are
  not interchangeable, and a site has one of them: the content is virtualized (a list inside react-aria's `Virtualizer`
  stays the scroll container of its own rows, so a wrapper around it scrolls nothing); the element carries its own role,
  name and key handling that a wrapper cannot take over (a tree, a list box, a reading surface); or its ancestor
  animates away — a react-aria element with an exit transition unmounts only once `getAnimations()` **on that element**
  has settled, and the fade's `animation-timeline: scroll()` animation never finishes, so a fade on a `Popover` or a
  `Tabs.Panel` leaves it on screen for good. `getAnimations()` does not look at the subtree, so that fade goes on the
  scrolling list or panel inside, never on the popover or the panel itself.
- `useStickyScrollFade` is for a virtualized scroller whose section headings must remain above the fade. It leaves
  the scroller on the collection element and supplies an overlay wrapper; a mask on that element would dim the
  headings regardless of their z-index. A wrapper scroller with the same need uses `FadedScroller` with
  `stickyHeadingHeight` instead. Both take the heading height; the module supplies the fade size and matching scroll
  padding.
  That padding covers the heading too, so a control inside a pinned heading scrolls the list on every focus move
  unless exempted. Under a `FadedScroller` with `stickyHeadingHeight`, the sticky element takes
  `useStickyHeadingFocus` (`packages/ui/src/components/stickyHeadingFocus.ts`); `StickySectionHeading` in
  `packages/ui/src/sidebar/rows.tsx` already does, so a heading built from it needs nothing more.

Sticky-heading surfaces use overlay fades (the "What both views show" section of `docs/product/focus-mode.md` and
the "Folding a section" section of `docs/product/project-pane-git-mode.md`); other surfaces apply the fade as a
`mask-image` on the scroller. For a masked scroller, the mask clips by that element's border box, so:

- Padding that gives a control inside the scroller room — the room its focus ring needs — goes on the scroller itself,
  inside the clip, never on an element around it.
- Where the scroller is focusable (`useFadeRegion`, whose own conditions are the "A scroller nothing else on screen
  scrolls takes focus itself" section of `docs/ui-accessibility.md`), its ring is drawn on the element around it — the
  project's custom `ring-2 ring-app-focus`, from the `focused` that hook hands back — because a ring on the scroller
  itself is cut away and an inset one lands inside the fade band and is eaten by it. That ring paints outside the
  wrapping element's border box, so the room it needs is clear space *around* that element, which whatever lays it out
  has to leave; padding inside it gives the ring nothing. Add `ring-inset` where it reaches the window's own edge,
  there being no space outside to paint into.

The fade is a hint, never the only cue: it covers an ordinary overflow container, so the wheel, the keyboard and
a control focused out of view scroll as they would anyway. Keep it that way, and do not replace the container with one
that scrolls by script. Where a site scrolls the container itself to reveal a row, it stops that row the fade's own size
short of the edge, taken from the module's `FADE_SIZE` rather than written out: scroll padding governs only the
scrolling the browser and react-aria do.

jsdom has no `ResizeObserver`, which the fade needs to mount, so `packages/ui/src/test/setup.ts` installs a stub for
every test file, and a test that has to measure replaces it with `vi.stubGlobal`.
