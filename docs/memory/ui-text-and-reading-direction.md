# UI text and reading direction

## Every string the user reads goes through the catalog, in a shape a translation can follow

Any text a user reads in `packages/ui` — labels, tooltips, accessible names, toasts, notifications, the native menu's
labels — is a message in `packages/ui/src/i18n/messages/en.ts` and in every translated catalog registered in
`CATALOGS` (type checking fails on a translated catalog that lacks it). Look it up like this:

- In a component, through `useT()` from `packages/ui/src/i18n/react.tsx`. The module-level `t()` in
  `packages/ui/src/i18n/language.ts` is for code outside React only: a component that calls it keeps showing the old
  language after the user switches, with no error.
- A helper that returns display text takes the `Translate` function as its first parameter (as `statusLabel(t, status)`
  in `packages/ui/src/sessionLabel.ts` does) instead of calling `t()` itself.
- A module-level table (options, menu items, rows) stores a `PlainMessageKey` and looks it up at render, never text
  resolved when the module loads.
- A sentence with a variable part is one message with a `{placeholder}`, never translated fragments joined in code,
  since word order differs between languages; markup inside a sentence goes through `<Message id params>`. Anything
  that depends on a count is a plural message selected by `count`, never `count === 1 ? … : …`.
- A HeroUI part can carry an English string of its own that no catalog sees: `Tag.RemoveButton` renders
  `aria-label="Remove tag"` ahead of its spread props, which both ships that literal when you pass no `aria-label` and
  shadows the localized name react-aria would otherwise supply through the part's slot. So for every part you give no
  visible text, read its file for a hardcoded string, following through to whatever part that file renders in turn, and
  pass your own message over it: `SearchField.ClearButton`'s own file carries no string, and the `aria-label="Close"` it
  showed in every language comes from the `CloseButton` it delegates to.

Shortening or replacing an existing message drops whatever that message alone said, and the diff shows only that the
string got shorter. So before cutting a clause, grep the catalog for the fact it carries and read the product doc for
that surface: a clause saying where something is made or changed is often the UI's only statement of it, and the
product doc records that line as the guidance — shortening the tag filter's empty state cut the one place naming a
project's settings as where tags are added. Keep such a clause, or move it into copy the surface shows anyway (a
section's description, the line under an empty state's heading); never simply drop it.

Only what the user reads in Octoboard's own UI is translated; text aimed at an agent stays English (see "What follows
the language" in `docs/product/language.md`).

## Lay out by reading direction; keep physical only what never mirrors

Under a right-to-left language the whole window mirrors, except what the "Right-to-left layout" section of
`docs/product/window-layout.md` lists as not mirrored. Nobody sees a regression here without switching to Arabic, so
build it in from the start:

- Position, spacing, borders, corners and alignment use Tailwind's logical utilities (`start-*` / `end-*`, `ms-*` /
  `me-*`, `ps-*` / `pe-*`, `border-s`, `rounded-s-*`, `text-start`), not `left` / `right` / `ml` / `pr` and the like.
  `translate-x-*` has no logical form, so a slide gets an `rtl:` counterpart with the opposite sign.
- An icon that points a direction mirrors with `rtl:-scale-x-100`. An expand / collapse chevron switches between two
  glyphs (forward when collapsed, down when expanded) instead of rotating one: a mirrored forward chevron turned a
  quarter points up.
- Content whose direction is not the UI's sets its own: a path `dir="ltr"`, a name the user typed `dir="auto"`
  (`FadeOverflow` takes the same `dir` and fades along it).
- `dir="auto"` only decides for an element that holds the text itself: the engine reads the element's own text and
  stops at every descendant carrying a `dir` of its own, so a container whose text all sits in such descendants — the
  paragraphs of a rendered Markdown list or quote — can never resolve. An unresolved `auto` falls back to `ltr`
  rather than inheriting, which leaves a right-to-left block's markers, quote bar and alignment on the left, and
  WebKit lays such a container out in time quadratic in the document's size (a document of 10,000 of them held the
  main thread for 15 s as one task). So where blocks wrap their text in further elements, take each block's direction
  from the first strong directional character of its whole subtree and write that out as an explicit `dir`, with no
  `dir` at all on a block that has none so that it inherits (`packages/ui/src/viewer/blockDirection.ts` does this for
  the Markdown document).
