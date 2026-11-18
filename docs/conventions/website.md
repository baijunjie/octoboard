# Website conventions

The rules for the Nuxt website (`apps/web/`, `@octoboard/web`). Where a file lives, what it imports and the shared entry
points are in `docs/structure/web.md`; how the website's copy and catalogs are written is in
`docs/conventions/i18n-in-code.md`, "Website copy"; the React UI that the website embeds follows the UI docs.
Paths below are relative to `apps/web/` unless they start with `apps/` or `packages/`.

## Imports and dependencies

- **Import from `@octoboard/ui` by package name only what its `package.json` `exports` names** (`agent-marks`,
  `languages`, `match-language`, `preview-messages`), and host the React preview in an iframe; never import its
  components. The website is a separate build with a different framework, and the `exports` map is the contract between
  the two.
- **Anything both `nuxt.config.ts` and the Vue app need goes in `seo/` as a plain `.ts` module**, never in Nuxt's
  `shared/`: a file there is externalized and `nuxt generate` fails.
- **`seo/agent-documents.ts` reads the catalogs with `node:fs` and is imported only by `nuxt.config.ts` and Node
  tests**, never by Vue code, since it does not run in the browser.
- **Product identity and contacts come from `config/app.json`**, mapped into runtime configuration by `nuxt.config.ts`
  (how copy names the product is in `docs/conventions/i18n-in-code.md`, "Website copy"). The default port is read by
  `seo/downloads.ts` and pinned for `public/install.sh` by `test/install-script.test.ts`.
- **`seo/downloads.ts` reads the stable manifests of the `updater` release by the names `release-manifest.mjs`
  gives**, so a change to those names in `apps/desktop/scripts/` changes this module in the same change.

## Where content is defined

- **A fact the Vue app and the Markdown renderer both show is defined once in `seo/`**, so the two stay in step:
  download targets, tabs and the commands the copy quotes in `seo/downloads.ts`; homepage sections, feature groups and
  workflow items in `seo/page-outline.ts`; FAQ answer keys through `homeQuestionAnswerKey`; the privacy revision date
  through `privacyUpdatedDate` and `formatPrivacyDate`; the locale table, canonical paths and JSON-LD in
  `seo/site-seo.ts`. Not: a section list repeated in a component and again in the Markdown renderer.
- **Use the composable instead of the lower level it wraps**: `useSiteCopy` for text that names the product or its
  operator, `useSiteSeo` for head tags (a page may override its own fields with `useSeoMeta`), `useVisibleMotion` for
  gating an animation on visibility and reduced motion, `ProductPreview.vue` or `FeatureAppView.vue` (not
  `usePreviewFrame.ts` directly) for a React preview, and `AgentMark.vue` for an agent's mark.
- **A new language comes from `@octoboard/ui/languages` through `seo/site-seo.ts`**, with its catalog and a committed
  share image; `nuxt.config.ts` throws at load when the image is missing.

## Icons

- **An icon is a local SVG in `app/assets/icons/`, drawn by `SiteIcon.vue`.** A new one comes from Font Awesome Free
  (platform and brand icons) or Phosphor (UI icons); diagram glyphs are Lucide (`FeatureGlyph.vue`); agent marks are
  LobeHub's. The licence or attribution file of each source is in `public/licenses/`, and a new source adds its file
  there.

## Vue code style

- **Vue files follow the ESLint configuration** (`eslint.config.mjs`: `vue/flat/essential`, two-space template indent,
  double quotes, end tags, no spaces around `=` in attributes); `vue/multi-word-component-names` is off only for
  `app/pages/**` and `app/error.vue`, whose names Nuxt derives routes from.
