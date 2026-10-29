# Styling the UI

## Keep a HeroUI component's default look through supported variants and slots

A HeroUI component in `packages/ui` keeps its default look (the priority is the "Conformance level and exceptions"
section of `docs/ui-accessibility.md`), but layout and sizing adaptations (filling a column, truncating) may stay
without redrawing the component. So when asked to strip custom styling, separate those adaptations from appearance
overrides rather than removing them wholesale: dropping a sizing override with the look ones broke the layout.
Equally, what a composite the project builds on top of a HeroUI component adds around it to mark out its own region —
such as a frame around a picker's scrolling list — is the composite's own design, not a redrawn HeroUI default, so
work that restores HeroUI's look leaves it in place; only styling that changes how a HeroUI part itself looks counts
as an override.

What the component imposes visually lives in a stylesheet of its own,
`packages/ui/node_modules/@heroui/styles/dist/components/<name>.css`, which `packages/ui/src/style.css` imports
directly, and the JavaScript shows none of it: a component's variants can be nothing but
`tv({ base: "<name>" })`, with every size, colour and spacing left to that file. So read it, rather than the
component's own file, when styling anything the project renders inside a HeroUI component, or when working out why
such a child looks unlike its own classes suggest. A base rule there covers the whole component, so where it sets
text size or colour (`.empty-state` is `@apply p-2 text-sm text-muted`) every child inherits both; HeroUI's own rules
correct only the parts it styles by slot, never a child of the project's. Giving that child its own size or colour is
a correction of what it inherited on project-authored content rather than a change to the HeroUI component's look.

`style.css` imports only the sheets of the components the app renders, so a component newly taken from `@heroui/react`
renders unstyled until its sheet, and those of the components it is built from, are added there;
`packages/ui/src/style.test.ts` names what is missing. When upgrading `@heroui/styles`, also move the `tw-animate-css`
pin in `packages/ui/package.json` to the version `@heroui/styles` itself depends on; the same test fails until it
matches.

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

## A `className` loses to a project component's own utility, and beats a HeroUI part's state fills

`packages/ui` merges no classes (there is no `tailwind-merge`). A component of the project's own that appends the
caller's `className` to its own utilities emits both, and where both set the same property the one Tailwind emits later
in its stylesheet wins, whatever their order in the class string: `px-0` passed to `StatusBadge`
(`packages/ui/src/components/StatusBadge.tsx`), whose own class is `px-2`, loses with nothing to show it. The habit
comes from HeroUI, where a `className` does win, only because `packages/ui/src/style.css` imports HeroUI's sheets into
the `components` layer, which Tailwind's `utilities` layer outranks. So to change a property such a component sets
itself, add a prop to it that picks the value (as `StatusBadge`'s `single` drops the side padding), rather than passing
a `className`.

The same layering cuts the other way on a HeroUI part: a background given to it through `className` also beats the
fills HeroUI's sheet gives its states, such as a `ListBox.Item`'s hover `bg-default`, and the part silently stops
showing them (the jsdom tests never hover). When giving a HeroUI part a background of its own — an opaque one behind a
sticky row, say — restore each state fill on the same element with its state attribute's variant
(`data-[hovered=true]:bg-default`).

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

## Under a local override of a HeroUI token, use the token itself, not its `--color-*` alias or a token derived from it

Several rules in `packages/ui/src/style.css` give a subtree its own value of a HeroUI token: `.control-fills` and
`.sidebar-fills` the `--default` family, and in dark every floating surface (a dialog, a menu, a popover, a tooltip, a
toast) its own `--overlay`, `--surface`, `--default`, `--border` and `--separator`. A custom property whose value is a
`var()` is resolved on the element that declares it and inherited as that result, so inside such a subtree only the
overridden token itself carries the local value:

- HeroUI's theme (`packages/ui/node_modules/@heroui/styles/dist/themes/shared/theme.css`) declares its colours in
  `@theme inline` as `--color-<name>: var(--<name>)`, which Tailwind emits on `:root`. A utility (`bg-overlay`,
  `bg-overlay/80`) is compiled with `var(--<name>)` inlined and follows the local value, but `var(--color-<name>)`
  written in an arbitrary value or plain CSS resolves on the root and draws the page's colour inside the surface:
  `shadow-[0_-4px_0_0_var(--color-overlay)]` on a sticky row in a dialog painted a dark strip once dialogs got their
  own `--overlay`. Write `var(--<name>)` there.
- HeroUI's tokens computed from another (`--default-hover`, `--default-soft`, `--surface-hover`, `--field-focus`,
  `--field-border-hover`, `--field-border-focus`, in `.../dist/themes/default/variables.css`) are resolved on the root
  too, and neighbours such as `--surface-secondary` are values of their own, so overriding `--default` or `--surface`
  on an element moves none of them. A new local override sets, on the same element, every token the components inside
  it draw with, as the existing rules do.

## Dim a region with a veil, not `opacity` on it

In the app's WKWebView, `opacity` below 1 on an ancestor of anything that fades (an opacity transition, such as a
row's hover controls) or sits on its own GPU layer (HeroUI's buttons are `transform-gpu`) makes WebKit paint those
descendants as blank tiles, and patching the descendants one by one did not hold. To dim a region, lay a veil of the
surface colour over it instead: an `after:` pseudo-element with `after:pointer-events-none after:absolute
after:inset-0` and a translucent fill such as `bg-surface/55` on a `relative` container, as the sidebar's inactive
projects in `packages/ui/src/sidebar/Sidebar.tsx` do.

## Nothing may start a transition inside a collapsed disclosure: the browser never ends it, and focus waits on it

Applies to `packages/ui`, to any element with a CSS transition that sits in the body of a react-aria / HeroUI
`Disclosure` (the dialogs' Advanced section, an error-details disclosure). react-aria holds several focus moves until
no transition is running anywhere on the page, counting them from `transitionrun` against `transitionend` /
`transitioncancel` (`runAfterTransition`): `focusSafely` under virtual interaction modality (a screen reader, a
scripted press) and `useDialog`'s half-second take-back among them. A collapsed body carries `hidden="until-found"`,
content the browser skips, and neither Chrome nor WebKit sends an end event for a transition started there until the
content is revealed. One such transition stalls those focus
moves for as long as the section stays folded; a mouse never shows it.

A transitioned property changing while folded is enough: state that changes under a collapsed section, or a library
writing inline styles at mount — the shared-element transition behind HeroUI's `Tabs.Indicator` slides a newly
mounted indicator from where the previous one was, which React StrictMode's double mount (dev and the gallery) does on
every open. So one unlayered rule in `packages/ui/src/style.css` sets `transition-property: none` on everything under
`[hidden="until-found"]`, which beats HeroUI's and Tailwind's layered transitions; revealing the content then changes no
value and starts nothing. A new element in a disclosure needs nothing of its own, unless its transition is set by
something the rule cannot beat: an inline style, an `!important` declaration, or a more specific unlayered rule in
`style.css`. The rule is a longhand so that a jsdom test can read it (see below).

The rule cannot stop a transition already running when the section collapses. react-aria sets `hidden` once the
panel's own collapse animation (200 ms, none under reduced motion) has finished, without waiting for the content's
transitions, so one with longer left to run is still running then. WebKit cancels it under the rule; Chrome, after
the animated collapse, sends no end for it until the section is revealed again (seen in both by starting a 1 s
transition in the Advanced body just before folding it in the gallery). So where a focus move must not wait on
`runAfterTransition` while a section may just have folded, the code places focus itself, as `ConfirmDialog`'s mount
effect and the callers of `Dialog`'s `focusesItselfOnOpen` do.

When a deferred focus move never lands under virtual modality, find the stuck transition in a browser by logging
`transitionrun` / `transitionend` / `transitioncancel` document-wide against the gallery. jsdom runs no transitions,
ignores rules inside `@layer` and does not expand the `transition` shorthand in computed style, so a test asserts the
computed `transition-property` with the section collapsed and expanded, as
`packages/ui/src/dialogs/AdvancedSection.test.tsx` does.

## A line height given as a ratio must not come a hair under a whole pixel

When a line height is set as a ratio of the font size — a theme `--text-*--line-height` token, a `leading-[…]` or
`text-*/[…]` utility, a `line-height` in plain CSS — check that font size × ratio does not land a hair under a whole
pixel. WebKit (the app's WKWebView and Safari) drops such a line height to the pixel below, while Chrome keeps the whole
pixel, so each line comes out a pixel short and text centred in a box sits a pixel lower in the app — with nothing to
see while developing in Chrome. Tailwind's stock `text-xs` (`calc(1 / 0.75)`, 15.99996px at 12px) and `text-2xl` are
cases of it; the project already overrides them in `packages/ui/src/style.css`'s `@theme`. Write a ratio a hair over
the exact one instead (`1.3334`, never `calc(4 / 3)` or `1.33333`).

When text in the app or Safari sits a pixel off from Chrome, compare the line box's height in the two engines first: a
half-pixel text position on its own is not the cause, since both engines draw it on the same pixel rows.

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
`docs/product/window-layout.md` lists as not grooves, and to a frame a composite puts around a region of its own (the
list in `packages/ui/src/dialogs/DirectoryPicker.tsx`, the panel in `packages/ui/src/dialogs/ConsoleLaunchFields.tsx`):
neither a divider nor a card outline, it keeps `border border-separator`. A card takes no decorative outline border, and
its shadow depends on whether it floats:

- A card lying in the layout takes the short two-layer `shadow-(--panel-shadow)`, as the "Hand-built rows and cards"
  section of `docs/ui-accessibility.md` sets out — unless it is a panel card, below.
- Anything else floating over the content — the notice floating over the terminal, a popover — keeps its elevation
  shadow (`shadow-lg`, `shadow-xl`) and drops only its outline border; it never takes the short shadow or the panel
  card's shadow in its place.
- A panel card — the sidebar's floating card or drawer, and every card of the right pane, docked or floating — looks
  exactly like the sidebar's floating card: `panel-edge` for its hairline plus the light `shadow-(--panel-card-shadow)`,
  whether it lies in the layout or floats. A new one takes `PANEL_CARD_CLASS` (`packages/ui/src/layout/paneOverlay.tsx`)
  rather than values chosen to match it. The same holds for any surface meant to look like an existing one: reuse that
  surface's own class or tokens, never a re-derived near-match — next to the original the difference shows, and the user
  rejects it.

Borders that identify a control or show a selected state are not decorative and stay, at their 3:1 colour.
