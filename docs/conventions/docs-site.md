# Documentation site conventions

The rules for the Starlight documentation site (`apps/docs/`, `@octoboard/docs`): its pages, snippets, quoted values,
sidebar and styling. Where a file lives, what it imports and the shared entry points are in `docs/structure/docs.md`;
how copy is worded and translated is in the `knowledge-i18n-copy` skill; the Nuxt website, which links into the docs,
follows `docs/conventions/website.md`. Paths below are relative to `apps/docs/` unless they start with `apps/`,
`packages/`, `scripts/` or `docs/`.

## Quoted values and names

- **A command, value or address a page quotes comes only from `src/commands.ts`**, never written out in the page, so one
  change reaches every page and language. An entry no page uses is removed.
- **The product name in a page body is `{productName}`**, never the literal name.
- **A UI name cited from the app is the catalog's text in that language, set in bold.** A catalog message with a
  platform placeholder (`{systemSettings}`, `{settingsApp}`, `{trash}`, `{menuBarIcon}`, ...) reads differently on each
  system, so it is resolved for each operating system the sentence describes: the term each system takes is in
  `packages/ui/src/platform/host.ts`, each language's word for it in `packages/ui/src/i18n/platformTerms.ts`. A menu or
  settings path is written `A › B` from the catalog's own names.
- **A technical value (an address, path, command, port, `127.0.0.1`) is `<code dir="ltr">`, not a backtick span**,
  inside Asides, lists and tables too, so it keeps its order inside a right-to-left paragraph of the Arabic pages.

## Pages, snippets and links

- **A sidebar entry takes its page's title, and a group's label is translated in `sidebar.ts` per language tag.** No
  per-entry label override is kept: it would drift from the page's own title.
- **Copy several guides share goes in `src/snippets/<language>/`**, imported by each guide of that language. A language
  that translates a guide adds its own copy of the snippets that guide imports; it never imports another language's.
- **Where prose refers to another guide by its title, the title links to that guide.** Where a link does not fit (a
  heading, a code block), the prose names the method in plain words instead of the bare title.
- **In a translated page, an anchor uses the target heading's slug in that language**: Starlight derives a heading's id
  from its text, so an English fragment copied into a translation is a broken link (it is right only where that language
  has no translation of the target page). Retitling a heading changes the anchors to it.

## Look and feel

- **The site keeps Starlight's own components and look.** The accent comes only from the `--sl-color-accent*` tokens in
  `src/styles/theme.css`, which may also carry narrow readability fixes (a minimum width for table cells, say), and the
  header changes only through the `SiteTitle` override (`src/components/SiteTitle.astro`).
- **`docs/conventions/ui-standard.md` binds the site in part:** its contrast thresholds ("Contrast", held as references,
  per "Conformance level and exceptions"), pointer target size (24 by 24 CSS pixels, "Pointer target size") and text
  direction rules ("Text direction") apply to the site's own styling and markup; its component rules are for
  `packages/ui`.

## Copies of the website's decisions

- **A copy the docs keep of a website decision is compared with the website's module by `webParity.test.ts`**, and the
  website modules and files that test reads are declared in `scripts/verify/scope.mjs` as `docs/conventions/tooling.md`,
  "Scripts", requires.
