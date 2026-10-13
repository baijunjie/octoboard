# Styling the UI

## Style a HeroUI component through its own variants, slots and the theme tokens

A HeroUI component in `packages/ui` takes its look from its own variants, slots and the theme tokens, not from being
overridden into something else. An override on a HeroUI component stays only for a colour fix accessibility needs or
for layout and sizing (filling a column, truncating); one that only changes its look to taste goes. Even a colour fix
never takes the form of a border, frame or outline HeroUI does not draw itself (the "Do not change how a HeroUI
component looks" section of `docs/ui-accessibility.md`). So when asked to strip custom styling, sort each override by
that reason rather than removing them wholesale: dropping a sizing override with the look ones broke the layout.

What the component imposes visually lives in a stylesheet of its own,
`packages/ui/node_modules/@heroui/styles/dist/components/<name>.css` — the readable source of what ends up in
`heroui.min.css` — and the JavaScript shows none of it: a component's variants can be nothing but
`tv({ base: "<name>" })`, with every size, colour and spacing left to that file. So read it, rather than the
component's own file, when styling anything the project renders inside a HeroUI component, or when working out why
such a child looks unlike its own classes suggest. A base rule there covers the whole component, so where it sets
text size or colour (`.empty-state` is `@apply p-2 text-sm text-muted`) every child inherits both; HeroUI's own rules
correct only the parts it styles by slot, never a child of the project's. Giving that child its own size or colour is
a correction of what it inherited rather than a change of look, so it stands as an override.

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

## A style that follows a measurement is taken again on every commit, not only when an observer fires

Where a component in `packages/ui` styles itself from a measurement of layout — whether a scroller overflows, which
edge of a line is cut off — take that measurement in an effect with **no dependency array**, so it runs on every commit
of the component, and keep a `ResizeObserver` alongside it for what moves between commits (a lazy render finishing, the
window resizing). Use a layout effect where the first paint already has to carry the style, an ordinary one otherwise.
Measuring that often costs nothing when the answer has not moved: write it back through `useState` and React bails out
of a set to the value already there.

An observer on its own is not enough, in two ways that each look from the outside like nothing happening:

- A box stays exactly the same size while what it holds changes — a row renamed, a `pre` handed a new file, the
  word-wrap class swapped — and a `ResizeObserver` reports nothing at all.
- React puts a new element in the measured one's place while the component stays mounted, which a surface keyed on its
  subject does on a new subject. An effect keyed on the ref never runs again for it, so the observer is left on the
  element that went away.

A `MutationObserver` is not the answer either: those changes reach the DOM as a text node's new value or a swapped
class, which only an observer watching every attribute and character of the subtree reports, and on a virtualized list
that observation has to be rebuilt as rows come and go. The scroll fade and `FadeOverflow` are both built the way above.
