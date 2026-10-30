# Octoboard website

The public product website, built with Nuxt 4 and Nuxt i18n. It generates localized home and privacy pages as static
files and embeds a separate fixture-backed build of the shared React UI; it has no backend and does not connect to
`octoboardd`.

This is the `@octoboard/web` package in the repository's pnpm workspace. Dependencies and their lockfile belong to
the workspace root.

## Commands

Run these commands from the repository root:

- `pnpm install` — installs workspace dependencies and prepares Nuxt's generated types.
- `pnpm --filter @octoboard/web dev` — builds the React product preview, then starts the website development server.
- `pnpm --filter @octoboard/web check:locales` — checks language coverage, message keys and interpolation placeholders.
- `pnpm --filter @octoboard/web typecheck` — prepares Nuxt and checks the website's TypeScript and Vue code.
- `pnpm --filter @octoboard/web test` — runs the website's Node test; the sitemap check reads the generated output, so
  it is skipped until `build` has run.
- `pnpm --filter @octoboard/web build` — builds the React product preview and generates the static website in
  `apps/web/.output/public`.
- `pnpm --filter @octoboard/web generate` — exposes the same static generation command for deployment.
- `pnpm --filter @octoboard/web generate:og-images` — runs `python3 scripts/generate-og-images.py` to draw one
  share image per language.
- `pnpm --filter @octoboard/web preview` — serves the generated output locally at the configured base path.

## Code boundaries

- [`apps/web/nuxt.config.ts`](nuxt.config.ts) owns public runtime configuration, local font loading, and the
  prerender hook. It passes the locale table from [`apps/web/seo/site-seo.ts`](seo/site-seo.ts) to
  `@nuxtjs/i18n`, prerenders each localized home and privacy route from that table, configures `@nuxtjs/sitemap`
  to emit the single `sitemap.xml` (with hreflang alternates built from the same table, and its XSL stylesheet
  under `__sitemap__/`), and on `prerender:done` writes `robots.txt`, `404.html`, and the agent files into the
  static output.
- [`apps/web/seo/`](seo/) is ordinary project code imported by both the Vue app and `apps/web/nuxt.config.ts`.
  It is not Nuxt's `shared/` directory. A file placed there is externalized, so the import stays a `.ts`
  specifier that does not resolve and `nuxt generate` fails as the prerenderer starts.
- [`apps/web/seo/site-seo.ts`](seo/site-seo.ts) owns locale registration, `robots.txt`, Open
  Graph locale values, canonical and Markdown path helpers, and JSON-LD construction.
- [`apps/web/seo/page-outline.ts`](seo/page-outline.ts) owns the homepage's `featureGroups`, the flattened
  `homeFeatures` shared by Markdown and JSON-LD, and `homeSections` shared by the page directory and scroll
  tracking. It also lists the steps, questions, platforms, hero agents (a name plus an icon file in
  [`apps/web/public/agents/`](public/agents/)), workflow project roles and eight `workflowNodes`, and privacy
  sections. The Vue app and the Markdown renderer both walk it.
- [`apps/web/seo/agent-documents.ts`](seo/agent-documents.ts) renders, from the locale catalogs and that
  outline, one Markdown file per homepage and privacy page, plus `llms.txt`, `llms-full.txt`, and
  `sitemap.md`. It reads catalogs with `node:fs` and is imported from `apps/web/nuxt.config.ts` and the Node
  test, not from Vue.
- [`apps/web/site.config.ts`](site.config.ts) owns only the deployment origin, base path and resulting site URL.
  Product identity and contact details come from [`config/app.json`](../../config/app.json), mapped into public
  runtime configuration by `apps/web/nuxt.config.ts`.
- [`apps/web/app/app.vue`](app/app.vue) assembles the shared shell and localized document head from Nuxt
  i18n, omitting the Open Graph locale tags that module emits, and sets `og:image` and `twitter:image`
  to `${siteUrl}og-image-${locale}.png`;
  [`apps/web/app/pages/`](app/pages/) owns the homepage and privacy page.
- [`apps/web/app/composables/useSiteSeo.ts`](app/composables/useSiteSeo.ts) sets Open Graph locales from
  the site-seo table, the Markdown alternate link, `rel="describedby"` to `llms.txt`, and the JSON-LD
  script on the home and privacy pages.
- [`apps/web/app/components/`](app/components/) owns navigation, language selection, the footer and the illustrative
  homepage features and workflow; `LanguageSelect.vue` is shared by the header's desktop and mobile controls.
- [`apps/web/app/components/SectionHeading.vue`](app/components/SectionHeading.vue) supplies the homepage's shared
  numbered section heading.
- [`apps/web/app/components/FeatureWall.vue`](app/components/FeatureWall.vue) renders the grouped feature cards
  from `featureGroups` in [`apps/web/seo/page-outline.ts`](seo/page-outline.ts), with localized copy.
- [`apps/web/app/components/SectionNavigation.vue`](app/components/SectionNavigation.vue) owns the homepage's
  desktop directory rail and narrow-screen directory menu; [`apps/web/app/components/SectionLinks.vue`](app/components/SectionLinks.vue)
  supplies their shared section and feature-group links. [`apps/web/app/composables/useHomeSection.ts`](app/composables/useHomeSection.ts)
  supplies the active section and feature group from scroll position.
- [`apps/web/app/components/HeroMascot.vue`](app/components/HeroMascot.vue) owns the layered SVG animation of the
  existing mascot artwork; [`apps/web/app/composables/useVisibleMotion.ts`](app/composables/useVisibleMotion.ts)
  supplies element visibility, document visibility and reduced-motion gating for it and the workflow.
- [`apps/web/app/components/ProductPreview.vue`](app/components/ProductPreview.vue) hosts the separately built
  React preview in an iframe, with scene selection, loading/error handling, responsive presentation, and a link to
  the standalone preview. It receives scroll handoffs and keyboard/pointer interaction messages from that iframe.
  The React entry, fixtures and sending bridges belong to
  [`packages/ui/`](../../packages/ui/README.md).
- [`apps/web/app/components/WorkflowDemo.vue`](app/components/WorkflowDemo.vue) renders the illustrative
  eight-node constellation from [`apps/web/seo/page-outline.ts`](seo/page-outline.ts), with the coordinator at its
  center, project selection, task/report details, and measured animated connectors. It also supplies the initial
  server-rendered connector geometry.
- [`apps/web/app/components/SiteIcon.vue`](app/components/SiteIcon.vue) supplies shared decorative icons from the local
  SVG assets in [`apps/web/app/assets/icons/`](app/assets/icons/): platform and GitHub brand icons come from Font Awesome Free
  6.7.2, and other UI icons come from Phosphor. The brand SVGs retain their embedded attribution; both libraries'
  license and attribution files are
  [`apps/web/public/licenses/font-awesome-free.txt`](public/licenses/font-awesome-free.txt) and
  [`apps/web/public/licenses/phosphor-icons.txt`](public/licenses/phosphor-icons.txt).
- [`apps/web/app/composables/useSectionReveal.ts`](app/composables/useSectionReveal.ts) owns the homepage's section
  reveal effects; [`apps/web/app/components/AmbientParticles.vue`](app/components/AmbientParticles.vue) owns the
  hero's decorative canvas and its animation lifecycle. The homepage supplies the shared motion-pause state.
- [`apps/web/app/assets/css/main.css`](app/assets/css/main.css) owns shared visual styles and the site's general
  reduced motion rules; component-specific styles also live in their Vue files.
- [`apps/web/app/composables/useLiquidGlass.ts`](app/composables/useLiquidGlass.ts) owns the header's backdrop filter
  lifecycle, using the displacement field from [`apps/web/app/utils/liquidGlassMap.ts`](app/utils/liquidGlassMap.ts).
  [`apps/web/app/composables/useLiquidGlassPointer.ts`](app/composables/useLiquidGlassPointer.ts) supplies pointer
  state; [`apps/web/app/assets/css/liquid-glass.css`](app/assets/css/liquid-glass.css) owns glass tokens, highlights
  and fallback styles. [`apps/web/app/components/SiteHeader.vue`](app/components/SiteHeader.vue) attaches both hooks.
- [`apps/web/app/components/ErrorContent.vue`](app/components/ErrorContent.vue) supplies the error presentation for
  [`apps/web/app/error.vue`](app/error.vue) and the separate, unlocalized prerender route
  [`apps/web/app/pages/404.vue`](app/pages/404.vue).
- [`apps/web/i18n/locales/`](i18n/locales/) holds all website copy, including privacy text and page metadata;
  [`apps/web/app/composables/useSiteCopy.ts`](app/composables/useSiteCopy.ts) supplies product and operator name
  interpolation from public runtime configuration.
- [`apps/web/scripts/build-preview.mjs`](scripts/build-preview.mjs) invokes the UI package's `build:preview` before
  website development and generation, placing the generated assets in the ignored `apps/web/public/preview/`
  directory for Nuxt to serve and copy into the static output.
- [`apps/web/scripts/check-locales.mjs`](scripts/check-locales.mjs) validates each catalog against the English source
  and the supported language list.
- [`apps/web/scripts/generate-og-images.py`](scripts/generate-og-images.py) writes one committed 1200×630 PNG per
  route code in [`apps/web/seo/site-seo.ts`](seo/site-seo.ts) (`zh`, not `zh-Hans`) to
  `apps/web/public/og-image-<code>.png`. It draws
  [`apps/web/public/wordmark.png`](public/wordmark.png) and each catalog's `brandSlogan` and
  title lines. `nuxt generate` does not run this script.
  [`apps/web/nuxt.config.ts`](nuxt.config.ts) throws when the config loads if any of those files is missing,
  and the error tells you to run `python3 apps/web/scripts/generate-og-images.py`.
- [`apps/web/scripts/preview.mjs`](scripts/preview.mjs) serves the static output on loopback at the configured base
  path, including the generated error document for missing files, and serves `.md` as `text/markdown`.
- [`apps/web/test/site-seo.test.ts`](test/site-seo.test.ts) is the website's Node test for the SEO modules.
- [`apps/web/public/`](public/) supplies the local branding, icons and GitHub Pages marker copied into the output.

## Configuration and publishing

`NUXT_PUBLIC_SITE_ORIGIN` selects the public origin at build time, and `NUXT_APP_BASE_URL` selects its deployment
path. Their defaults in `apps/web/site.config.ts` are the origin `https://octoboard.dev` and the base path `/`.
The same configuration supplies page links, assets, canonical URLs and sitemap entries. When overriding these
values, use the same values for generation and preview.

The locale table in [`apps/web/seo/site-seo.ts`](seo/site-seo.ts) maps the 17 catalog files to their route
prefixes, and [`apps/web/nuxt.config.ts`](nuxt.config.ts) passes that table to `@nuxtjs/i18n`. English has
no language prefix; Simplified Chinese uses `zh` in routes and `zh-Hans` for its catalog and language metadata.
The English catalog is the message source, and the locale check validates every other catalog against it.
[`apps/web/app/plugins/detect-language.client.ts`](app/plugins/detect-language.client.ts) handles the website's
client-side locale choice and persistence, reusing the shared language matcher in
[`packages/ui/src/i18n/matchLanguage.ts`](../../packages/ui/src/i18n/matchLanguage.ts).

Static generation includes every localized homepage and privacy page, their loading assets, the separately built
React preview at `preview/website-preview.html`, `sitemap.xml`,
`robots.txt`, and `404.html`, plus one Markdown file for each of those pages, `llms.txt`, `llms-full.txt`, and
`sitemap.md`. English home Markdown is `index.md`; every other home is `{route code}.md` (Simplified Chinese
is `zh.md`). Privacy Markdown is `privacy.md` or `{route code}/privacy.md`. Routes use directory indexes so
GitHub Pages can serve them without a runtime application server. The Nuxt configuration produces the
standalone error document from the rendered error route, removing scripts and preload links. Both sitemaps
cover only the home and privacy pages. `sitemap.xml` comes from `@nuxtjs/sitemap` and carries an
XSL stylesheet so browsers render it as a table; `sitemap.md` is written by the agent files step.

[`.github/workflows/website.yml`](../../.github/workflows/website.yml) runs on pushes to `main` and on manual
dispatch. It reads the GitHub Pages origin and base path with `actions/configure-pages`. An `http` origin is
rewritten to `https` before generation, and an empty Pages base path becomes `/`. The workflow then runs the locale
and type checks, generates the website, and deploys `apps/web/.output/public` to GitHub Pages. The custom domain
`octoboard.dev` is the repository's GitHub Pages setting; publishing does not use a `CNAME` file, and Actions
publishing ignores one.
