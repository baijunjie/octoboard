# Octoboard website

The public product website, built with Nuxt 4 and Nuxt i18n. It generates localized home and privacy pages as static
files; it has no backend and does not connect to `octoboardd` or reuse the desktop UI.

This is the `@octoboard/web` package in the repository's pnpm workspace. Dependencies and their lockfile belong to
the workspace root.

## Commands

Run these commands from the repository root:

- `pnpm install` — installs workspace dependencies and prepares Nuxt's generated types.
- `pnpm --filter @octoboard/web dev` — starts the website development server.
- `pnpm --filter @octoboard/web check:locales` — checks language coverage, message keys and interpolation placeholders.
- `pnpm --filter @octoboard/web typecheck` — prepares Nuxt and checks the website's TypeScript and Vue code.
- `pnpm --filter @octoboard/web build` — generates the static website in `apps/web/.output/public`.
- `pnpm --filter @octoboard/web generate` — exposes the same static generation command for deployment.
- `pnpm --filter @octoboard/web preview` — serves the generated output locally at the configured base path.

## Code boundaries

- [`apps/web/nuxt.config.ts`](nuxt.config.ts) owns locale registration, public runtime configuration, static routes,
  local font loading and generation of the sitemap, robots file and standalone GitHub Pages error document.
- [`apps/web/site.config.ts`](site.config.ts) owns only the deployment origin, base path and resulting site URL.
  Product identity and contact details come from [`config/app.json`](../../config/app.json), mapped into public
  runtime configuration by `apps/web/nuxt.config.ts`.
- [`apps/web/app/app.vue`](app/app.vue) assembles the shared shell and localized document metadata;
  [`apps/web/app/pages/`](app/pages/) owns the homepage and privacy page.
- [`apps/web/app/components/`](app/components/) owns navigation, language selection, the footer and the illustrative
  workflow; `LanguageSelect.vue` is shared by the header's desktop and mobile controls.
- [`apps/web/app/components/WorkflowDemo.vue`](app/components/WorkflowDemo.vue) owns the illustrative workflow's
  three visible project workspaces, task dispatch, three reports to the same coordinator, shared delivery and replay,
  including measured connectors and scoped layout and animation styles.
- [`apps/web/app/components/WorkflowStatus.vue`](app/components/WorkflowStatus.vue) supplies localized working and
  idle indicators matching the application's status glyphs: a working ping and Lucide's `MessageCircleMore` for idle.
  The original SVG paths come from `lucide-react` 1.52.0, with attribution in the component and license text in
  [`apps/web/public/licenses/lucide-icons.txt`](public/licenses/lucide-icons.txt).
- [`apps/web/app/components/SiteIcon.vue`](app/components/SiteIcon.vue) supplies shared decorative icons from the local
  SVG assets in [`apps/web/app/assets/icons/`](app/assets/icons/): platform brand icons come from Font Awesome Free
  6.7.2, and other UI icons come from Phosphor. The brand SVGs retain their embedded attribution; both libraries'
  license and attribution files are
  [`apps/web/public/licenses/font-awesome-free.txt`](public/licenses/font-awesome-free.txt) and
  [`apps/web/public/licenses/phosphor-icons.txt`](public/licenses/phosphor-icons.txt).
- [`apps/web/app/components/WordReveal.vue`](app/components/WordReveal.vue) and
  [`apps/web/app/composables/useSectionReveal.ts`](app/composables/useSectionReveal.ts) own the homepage's word and
  section reveal effects; [`apps/web/app/components/AmbientParticles.vue`](app/components/AmbientParticles.vue) owns
  the hero's decorative canvas and its animation lifecycle.
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
- [`apps/web/scripts/check-locales.mjs`](scripts/check-locales.mjs) validates each catalog against the English source
  and the supported language list.
- [`apps/web/scripts/preview.mjs`](scripts/preview.mjs) serves the static output on loopback at the configured base
  path, including the generated error document for missing files.
- [`apps/web/public/`](public/) supplies the local branding, icons and GitHub Pages marker copied into the output.

## Configuration and publishing

`NUXT_PUBLIC_SITE_ORIGIN` selects the public origin at build time, and `NUXT_APP_BASE_URL` selects its deployment
path. Their defaults in `apps/web/site.config.ts` are the origin `https://octoboard.dev` and the base path `/`.
The same configuration supplies page links, assets, canonical URLs and sitemap entries. When overriding these
values, use the same values for generation and preview.

The locale registration in `apps/web/nuxt.config.ts` maps the 17 catalog files to their route prefixes. English has
no language prefix; Simplified Chinese uses `zh` in routes and `zh-Hans` for its catalog and language metadata.
The English catalog is the message source, and the locale check validates every other catalog against it.

Static generation includes every localized homepage and privacy page, their loading assets, `sitemap.xml`,
`robots.txt` and `404.html`. Routes use directory indexes so GitHub Pages can serve them without a runtime
application server. The Nuxt configuration produces the standalone error document from the rendered error route,
removing scripts and preload links; the sitemap covers only the home and privacy pages.

[`.github/workflows/website.yml`](../../.github/workflows/website.yml) runs on pushes to `main` and on manual
dispatch. It reads the GitHub Pages origin and base path with `actions/configure-pages`. An `http` origin is
rewritten to `https` before generation, and an empty Pages base path becomes `/`. The workflow then runs the locale
and type checks, generates the website, and deploys `apps/web/.output/public` to GitHub Pages. The custom domain
`octoboard.dev` is the repository's GitHub Pages setting; publishing does not use a `CNAME` file, and Actions
publishing ignores one.
