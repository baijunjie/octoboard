# UI Convention Alignment Development Plan

## Problem and approach

`packages/ui` is now held to three rules: build from HeroUI 3's own component wherever it has one (every button is
HeroUI's `Button` unless a custom button is needed that HeroUI's cannot be), give every icon-only control a tooltip as
well as its accessible name, and meet WCAG 2.2 AA. An audit of the existing code against HeroUI 3.2.6 and against the
two accessibility rules found elements that break them. This plan brings the existing UI into line with all three; it
adds no behaviour beyond what the rules require.

## Key design decisions

- **Tooltips are HeroUI's `Tooltip`.** The package's labelled-control wrapper currently gives controls a native
  `title` (react-aria drops `title` passed straight to a HeroUI control). It moves onto HeroUI's `Tooltip`, which does
  not take focus on press, so it stays compatible with controls that must not pull keyboard focus off the terminal; it
  also shows on keyboard focus, which the native `title` never did. Every missing tooltip is then added through that
  one wrapper.
- **A tooltip's text matches the control's accessible name**, so what a sighted user reads on hover and what a screen
  reader announces are the same.
- **What stays hand-built, and why** (each keeps or gains a comment at the component stating the reason):
  - the sidebar tree: HeroUI 3 has no Tree or GridList, and each row carries an action menu that a `Disclosure`
    trigger (itself a button) cannot contain;
  - the sidebar resize handle: HeroUI has no splitter; its `Separator` is static and its `Slider` is a value slider;
  - the dimming layer behind a narrow-mode drawer (a native `<button>`), the edge hot zones, and the docked / drawer /
    floating pane system: HeroUI's `Drawer` is modal — it would make the top bar inert, trap focus, take focus off the
    terminal and unmount the report page;
  - the single-line edge fade for clipped labels: `ScrollShadow` turns the element into a real scroll container and
    has no start-edge clip or clipped-only title;
  - the report pager: HeroUI's `Pagination` is a numbered page list, not a previous / next pager with "n / m";
  - the title bar's control row: HeroUI's `Toolbar` would collapse its buttons into one roving tab stop.
- **Expanded state is exposed with `aria-expanded`** on every control that shows or hides something, not only by
  swapping its label or icon.
- **Animation respects `prefers-reduced-motion`**: a decorative spin stops under it, while the status it stands for
  stays readable without the motion.

## Milestones

1. [Tooltips through HeroUI's Tooltip](01-heroui-tooltips.md)
2. [HeroUI components in place of hand-built ones](02-heroui-components.md)
3. [Remaining accessibility fixes](03-accessibility-fixes.md)

## Open

- Whether to convert the cosmetic-only candidates: `Surface` / `Card` for the "Not running" card, `EmptyState` for the
  terminal's "select a session" message and the trusted-folders empty state, `Separator` / `Description` in settings
  rows, `Breadcrumbs` for the top bar's read-only trail. None changes behaviour; decide per item while doing milestone 2.
