# Website conventions

The rules for the Nuxt website (`apps/web/`). Placement and the shared entry points are in `docs/structure/web.md`; how
its copy and catalogs are written is in `docs/conventions/i18n-in-code.md`, "Website copy". Paths below are relative to
`apps/web/` unless they start with `apps/` or `packages/`.

## General principles

- **Import from `@octoboard/ui` by package name only what its `package.json` `exports` names** (`agent-marks`,
  `languages`, `match-language`, `preview-messages`), and host the React preview in an iframe; never import its
  components. The website is a separate build with a different framework, and the `exports` map is the contract between
  the two.
- **A fact the Vue app and the Markdown renderer both show is defined once in `seo/`**, so the two stay in step:
  download targets, tabs and the commands the copy quotes in `seo/downloads.ts`; homepage sections, feature groups and
  workflow items in `seo/page-outline.ts`; FAQ answer keys through `homeQuestionAnswerKey`; the privacy revision date
  through `privacyUpdatedDate` and `formatPrivacyDate`; the locale table, canonical paths and JSON-LD in
  `seo/site-seo.ts`. Not: a section list repeated in a component and again in the Markdown renderer.
- **Use the composable instead of the lower level it wraps**: `useSiteCopy` for text that names the product or its
  operator, `useSiteSeo` for head tags (a page may override its own fields with `useSeoMeta`), `useVisibleMotion` for
  gating an animation on visibility and reduced motion, `ProductPreview.vue` or `FeatureAppView.vue` (not
  `usePreviewFrame.ts` directly) for a React preview, and `AgentMark.vue` for an agent's mark.

## Imports and dependencies

- **Anything both `nuxt.config.ts` and the Vue app need goes in `seo/` as a plain `.ts` module**, never in Nuxt's
  `shared/`: a file there is externalized and `nuxt generate` fails.
- **Product identity and contacts come from `config/app.json`**, mapped into runtime configuration by
  `nuxt.config.ts`.

## Where content is defined

- **Links to the documentation site are built with `docsPath` from `docsPages` in `seo/site-seo.ts`**, never
  hard-coded. A docs page the website links to is added to `docsPages`, and `test/site-seo.test.ts` checks that it
  exists in every language.
- **Which pages are kept out of the index (robots `noindex`, no canonical or hreflang, sitemap exclude, no Markdown
  alternate) is defined once in `seo/site-seo.ts`** (`unindexedPageKinds`, `isIndexedPage`,
  `unindexedSitemapExcludes`); `nuxt.config.ts` and `useSiteSeo` derive from it.
- **A feature app view (`packages/ui/src/website-preview/features/`) that reuses an application message also shows
  what that message refers to** (a row "below", a chosen option), or uses a message that points at nothing outside
  itself: the view is a still picture, and a reference to a missing part reads as a broken one in every language.
- **A new language comes from `@octoboard/ui/languages` through `seo/site-seo.ts`**, with its catalog and a committed
  share image.

## Icons

- **An icon is a local SVG in `app/assets/icons/`, drawn by `SiteIcon.vue`.** A new one comes from Font Awesome Free
  (platform and brand icons) or Phosphor (UI icons); diagram glyphs are Lucide (`FeatureGlyph.vue`); agent marks are
  LobeHub's. The licence or attribution file of each source is in `public/licenses/`, and a new source adds its file
  there.
