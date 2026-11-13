# Website structure (`apps/web/`)

`@octoboard/web`: the public product website, built with Nuxt 4 (Vue 3, `@nuxtjs/i18n`, `@nuxtjs/sitemap`, `nuxt-gtag`)
and generated as static files for GitHub Pages. It has no backend and never connects to `octoboardd`. It serves
localized home, download and privacy pages (17 language catalogs), one Markdown rendering of each page plus `llms.txt`,
all-language and per-language `llms-full*.txt` bundles and `sitemap.md` for agents, and embeds a separately built,
fixture-backed React preview from `packages/ui/` (built into the ignored `apps/web/public/preview/` by
`apps/web/scripts/build-preview.mjs` before every development run and generation). It also serves the Linux install
script at `/install.sh` and links each platform's newest download, resolved from the release manifests at generation.
The deployment is `.github/workflows/website.yml`, which `release.yml` starts after every publication.

Paths in this doc that do not start with `apps/` or `packages/` are relative to `apps/web/`.

## Directory map

| Path | Role |
|---|---|
| `nuxt.config.ts`, `site.config.ts` | Nuxt configuration: runtime config from `config/app.json`, fonts, the locale table passed to `@nuxtjs/i18n`, prerendered routes, the sitemap, an inline module that resolves the download links into runtime config (skipped on `nuxt prepare`) and, on `prerender:done`, `robots.txt`, `404.html`, the agent files and the download redirect pages; it throws at load when a committed share image is missing. `site.config.ts` holds only the deployment origin (defaulting to the website address in `config/app.json`), base path and site URL |
| `seo/` | Ordinary project code (not Nuxt's `shared/`) imported by both the Vue app and `nuxt.config.ts`: `site-seo.ts` (locale table, `robots.txt`, canonical and Markdown paths, JSON-LD), `downloads.ts` (the download model: the targets (desktop installers and the Linux archives per architecture), the page's tabs, the command and panel keys the download copy quotes, the resolution of each target's newest download from the updater release's stable manifests at generation with its fallback rules, and the fixed redirect pages `/download/<target>/`; pure apart from an injected `fetch`), `page-outline.ts` (the homepage's feature groups, sections, steps, workflow nodes, privacy sections, FAQ answer-key selection and the platform lists — `availablePlatforms`, the platforms on offer (macOS and Windows desktop applications, both marked Beta, and the Linux server), and `plannedPlatforms` — shared by the Vue app, Markdown renderer and JSON-LD), `agent-documents.ts` (renders the Markdown files and `llms*.txt` from the catalogs; Node only), `privacy-date.ts` (the policy revision date and its shared UTC formatter) |
| `app/` | The Vue app: `app.vue` (shell and head), `error.vue`, `pages/` (home, `download.vue` with a tab per system, privacy, the unlocalized `404.vue`), `components/` (header, footer, navigation, feature wall and illustrations, workflow demo, the terminal demo of the download page, the install command and its command messages, product preview host, analytics consent), `composables/` (site SEO, home section tracking, consent, glass, reveal, visible motion, copy interpolation, `usePreviewFrame.ts` the lifecycle of an embedded preview frame), `plugins/` (client-side language detection and analytics consent), `assets/` (CSS, local icons), `utils/` |
| `i18n/locales/` | All website copy, one JSON catalog per language; English is the source |
| `scripts/` | `build-preview.mjs` (runs the UI's `build:preview`), `check-locales.mjs` (catalog coverage, keys, placeholders), `generate-og-images.py` (draws one committed 1200×630 share image per route code, `zh` not `zh-Hans`, from `public/wordmark.png` and each catalog's slogan and title lines; not run by `nuxt generate`, and `nuxt.config.ts` throws when an image is missing; its requirements and fonts are in the script's docstring), `preview.mjs` (serves `.output/public` on loopback at the configured base path, on `PORT` or 4318, with the generated error document for missing files and `.md` as `text/markdown`) |
| `test/` | The Node tests: `site-seo.test.ts` (the SEO modules), `downloads.test.ts` (download resolution and redirect pages) and `install-script.test.ts` (runs `public/install.sh` against a fake release; also holds its port and update key to `config/app.json` and the updater's `tauri.conf.json`, and the example systemd unit's port to the configured one) |
| `public/` | Local branding, licenses, share images, the demo image of the download page, `install.sh` (the POSIX installer of the Linux daemon, served at `/install.sh`) and the GitHub Pages marker copied into the output |

## Dependency direction

- The website depends on `@octoboard/ui` (`workspace:*`) and imports from it, by package name, only what its `exports`
  map names: the language list (`@octoboard/ui/languages`), the language matcher (`@octoboard/ui/match-language`), the
  preview's `postMessage` contract (`@octoboard/ui/preview-messages`) and the agent marks (`@octoboard/ui/agent-marks`);
  it hosts the React preview in iframes and never imports its components.
- Product identity and contacts come from `config/app.json`, mapped into runtime configuration by `nuxt.config.ts`; the
  default server port (`defaultServerPort`) is read by `seo/downloads.ts` and pinned for `public/install.sh` by
  `install-script.test.ts`.
- The download links depend on the release workflow's output: `seo/downloads.ts` reads the stable manifests of the
  `updater` release that `apps/desktop/scripts/publish-release.mjs` writes (names from
  `apps/desktop/scripts/release-manifest.mjs`), so a change to those manifests' names or the macOS disk image's name
  changes this module.
- `seo/agent-documents.ts` reads the catalogs with `node:fs` and is imported by `nuxt.config.ts` and the Node test,
  never by Vue code.

## Where new code goes

- Website copy: the catalogs in `i18n/locales/` (English first); `check:locales` validates every other catalog against
  it.
- A new language: its row comes from `@octoboard/ui/languages` through `seo/site-seo.ts`, plus its catalog and
  share image.
- A download target, tab or the commands the Linux copy quotes: `seo/downloads.ts`, which the Vue app and the Markdown
  renderer share; the install script itself is `public/install.sh`.
- A homepage section, feature group or workflow item: `seo/page-outline.ts`, so the Vue app and the Markdown renderer
  stay in step; its component under `app/components/`.
- Icons are local SVGs in `app/assets/icons/`, drawn by `app/components/SiteIcon.vue`. Platform and GitHub brand icons
  come from Font Awesome Free 6.7.2 and keep their embedded attribution; other UI icons come from Phosphor. The diagram
  glyphs of `app/components/FeatureGlyph.vue` are Lucide icons from `@lucide/vue`. A new local icon goes there from Font
  Awesome Free or Phosphor; the licence and attribution files of all three are in `public/licenses/`
  (`font-awesome-free.txt`, `phosphor-icons.txt`, `lucide-icons.txt`); the agent marks of `AgentMark.vue` are LobeHub's
  (MIT), with their licence in `lobehub-icons.txt` beside them.
- Anything both `nuxt.config.ts` and the Vue app need goes in `seo/` as a plain `.ts` module; a file in Nuxt's `shared/`
  is externalized and breaks `nuxt generate`.

## Shared entry points

| Need | Entry point |
|---|---|
| Locale table, canonical and Markdown paths, JSON-LD | `seo/site-seo.ts` |
| FAQ answer keys across HTML, Markdown and JSON-LD | `seo/page-outline.ts` (`homeQuestionAnswerKey`) |
| Download targets, resolved download links, redirect pages, install command | `seo/downloads.ts` |
| Privacy revision date across HTML and Markdown | `seo/privacy-date.ts` (`privacyUpdatedDate`, `formatPrivacyDate`) |
| Product and operator names in copy | `app/composables/useSiteCopy.ts` |
| SEO head tags on a page | `app/composables/useSiteSeo.ts` |
| Visibility and reduced-motion gating of animation | `app/composables/useVisibleMotion.ts` |
| Hosting a React preview or feature view | `app/components/ProductPreview.vue`, `FeatureAppView.vue`, over `app/composables/usePreviewFrame.ts` |
| An agent's mark | `app/components/AgentMark.vue` |

Checks: `pnpm --filter @octoboard/web check:locales`, `typecheck` and `test` (the sitemap test is skipped until `build`
has run); the website checks are not part of the desktop release gate.
