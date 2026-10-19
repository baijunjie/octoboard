# Styling the UI

## Keep a HeroUI component's default look through supported variants and slots

A HeroUI component in `packages/ui` keeps its default look, configured through its supported variants and slots.
Do not override its colours, placeholder colours, opacity, borders or shape merely to meet a WCAG contrast threshold;
the priority is settled in the "Conformance level and exceptions" section of `docs/ui-accessibility.md`. Layout and
sizing adaptations (filling a column, truncating) may stay when needed without redrawing the component. So when asked
to strip custom styling, separate those adaptations from appearance overrides rather than removing them wholesale:
dropping a sizing override with the look ones broke the layout.

What the component imposes visually lives in a stylesheet of its own,
`packages/ui/node_modules/@heroui/styles/dist/components/<name>.css`, which `packages/ui/src/style.css` imports
directly, and the JavaScript shows none of it: a component's variants can be nothing but
`tv({ base: "<name>" })`, with every size, colour and spacing left to that file. So read it, rather than the
component's own file, when styling anything the project renders inside a HeroUI component, or when working out why
such a child looks unlike its own classes suggest. A base rule there covers the whole component, so where it sets
text size or colour (`.empty-state` is `@apply p-2 text-sm text-muted`) every child inherits both; HeroUI's own rules
correct only the parts it styles by slot, never a child of the project's. Giving that child its own size or colour is
a correction of what it inherited on project-authored content rather than a change to the HeroUI component's look.
That custom content follows the WCAG 2.2 AA requirements in `docs/ui-accessibility.md`, checked in both appearances.

`style.css` imports only the sheets of the components the app renders, so a component newly taken from
`@heroui/react` renders unstyled until its sheet, and those of the components it is built from, are added there;
`packages/ui/src/style.test.ts` names what is missing. When upgrading `@heroui/styles`, also move the `tw-animate-css`
pin in `packages/ui/package.json` to the version `@heroui/styles` itself depends on; the same test fails until it matches.

## A Tailwind class name has to stand in the source as literal text

Tailwind 4 reads the source as plain text and emits a utility only for a class name it can find spelled out there, so
a name assembled at runtime — `` `${side}-0` ``, a suffix appended to a prefix, anything concatenated — compiles to no
CSS at all and the element silently loses that property. Write each variant out in full and branch between them.

A name spelled out in full still does not count when an interpolation is glued to its end: in
`` className={`... hover:bg-panel-hover${cond ? " x" : ""}`} `` the scanner reads `hover:bg-panel-hover${` as one
invalid candidate and drops it; it still looks fine whenever the same class is spelled out cleanly in some other file.
Put the separating space inside the template literal, not in the interpolated string:
`` `... hover:bg-panel-hover ${cond ? "x" : ""}` ``.

The configured ESLint rules do not check whether a Tailwind utility was emitted, and the UI's tests (`vitest`,
components under jsdom) never evaluate the CSS, so a class whose utility was never emitted is valid TypeScript,
passes lint and tests, builds clean and reads fine in a diff. After adding or changing a utility class, grep the
built `packages/ui/dist/assets/*.css` for it.

## A `docked:` utility loses to an `sm:` or an `rtl:` twin on the same property

`docked` is this project's one custom breakpoint and its token is in `px`, while Tailwind's built-in breakpoints are
in `rem`. Tailwind orders breakpoints by value only where it can compare them, so a px-valued one cannot be placed
against the rem-valued built-ins and its media block is emitted ahead of all of them; a selector variant such as
`rtl:` is emitted after every media variant. Either way the `docked:` rule comes first, so at equal specificity the
twin wins and the `docked:` value never applies — with no build warning and nothing to see in the source.

- Never pair an `sm:` / `md:` / `lg:` utility with a `docked:` one for the same property. Write the narrow side as
  `max-docked:` and leave the wide side to the inherited or the component's own value: `max-docked:` emits
  `@media (width < 1168px)`, the exact complement of the `docked:` band, and being a Tailwind utility it also wins
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

## Horizontal padding in the sidebar and the right pane comes from the inset tokens

In the sidebar and the right pane, any horizontal padding or margin that places content relative to the pane's edge
is written with the tokens in `packages/ui/src/style.css` — `--pane-inset`, `--row-fill-inset`,
`--row-content-inset` — never with a literal such as `px-2`, `px-3` or `px-4`, so both panes keep one inset. A header,
a bar, a notice or a heading outside a row takes `px-(--pane-inset)`; a row's hover or selected fill sits
`--row-fill-inset` from the edge (as `mx-` on the row, or as `px-` on the scroller holding the rows), and the row pads
its content inside the fill by `--row-content-inset`. Anything layered on top — a tree's per-level indent, a bordered
card's border width — is a `calc` of the token, not a literal in its place.

The tokens are horizontal only: vertical padding keeps a value of its own, even on an element whose horizontal padding
is a `calc` of a token (`px-[calc(var(--row-content-inset)-1px)] py-[7px]`, not one `p-[calc(...)]` for both).

## Dividers are grooves, and cards are edged by shadows rather than decorative borders

In `packages/ui`, a line that divides a pane's parts or rows is a groove utility from `packages/ui/src/style.css`
(`groove-b` and its siblings), never `border-b border-separator`, a bare `border-*` or a `divide-*`. `--separator` is
left to HeroUI's own components and to the few lines the "The window chrome and the content panel" section of
`docs/product/window-layout.md` lists as not grooves. A card takes no decorative outline border, and its shadow
depends on whether it floats:

- A card lying in the layout takes the short two-layer `shadow-(--panel-shadow)`, as the "Hand-built rows and cards"
  section of `docs/ui-accessibility.md` sets out — unless it is a panel card, below.
- Whatever floats over the content — the floating cards of both panes, the sidebar's drawer, the notice floating over
  the terminal — keeps its elevation shadow (`shadow-xl`, `shadow-lg`) and drops only its outline border; it never
  takes the short shadow in its place.
- A panel card — the sidebar's floating card or drawer, and every card of the right pane, docked or floating — looks
  exactly like the sidebar's floating card: `panel-edge` for its hairline plus `shadow-xl`, even where it lies in the
  layout. A new one takes `PANEL_CARD_CLASS` (`packages/ui/src/layout/paneOverlay.tsx`) rather than values chosen to
  match it. The same holds for any surface meant to look like an existing one: reuse that surface's own class or
  tokens, never a re-derived near-match — next to the original the difference shows, and the user rejects it.

Borders that identify a control or show a selected state are not decorative and stay, at their 3:1 colour.
