# Website structure (`apps/web/`)

`@octoboard/web`: the public product website, built with Nuxt 4 (Vue 3, `@nuxtjs/i18n`, `@nuxtjs/sitemap`, `nuxt-gtag`)
and generated as static files for GitHub Pages. It has no backend and never connects to `octoboardd`. It serves
localized home, download and privacy pages (17 language catalogs), one Markdown rendering of each page plus `llms.txt`,
all-language and per-language `llms-full*.txt` bundles and `sitemap.md` for agents, and embeds a separately built,
fixture-backed React preview from `packages/ui/` (built into the ignored `apps/web/public/preview/` by
`apps/web/scripts/build-preview.mjs` before every development run and generation). It also serves the Linux install
script at `/install.sh` and links each platform's newest download, resolved from the release manifests at generation.
The how-to content (installation steps, remote connections) is not here but in the documentation site
([`docs.md`](docs.md)), which the header, footer and download page link to and which is deployed under `/docs/` of this
site. The deployment is `.github/workflows/website.yml`, which builds both and `release.yml` starts after every
publication.

Paths in this doc that do not start with `apps/` or `packages/` are relative to `apps/web/`.

## Directory map

| Path | Role |
|---|---|
| `nuxt.config.ts`, `site.config.ts` | Nuxt configuration: runtime config from `config/app.json`, fonts, the locale table passed to `@nuxtjs/i18n`, prerendered routes, the sitemap, an inline module that resolves the download links into runtime config (skipped on `nuxt prepare`) and, on `prerender:done`, `robots.txt`, `404.html`, the agent files and the redirect pages (the download addresses, the former remote connections page, the retired Chinese locale codes); it throws at load when a committed share image is missing. The sitemap leaves out the pages that ask not to be indexed, and the prerender skips the docs' address, which the deployment adds. `site.config.ts` holds only the deployment origin (defaulting to the website address in `config/app.json`), base path and site URL |
| `seo/` | Ordinary project code (not Nuxt's `shared/`) imported by both the Vue app and `nuxt.config.ts`: `site-seo.ts` (locale table and the retired Chinese locale codes, `robots.txt` (which also names the docs' sitemap), canonical and Markdown paths, JSON-LD, the set of pages that ask not to be indexed (`isIndexedPage`, `unindexedSitemapExcludes`), and the docs pages the website links to (`docsPages`, `docsPath`)), `downloads.ts` (the download model: the targets (desktop installers and the Linux archives per architecture), the page's tabs with the docs page of each, the command and panel keys the download copy quotes, the resolution of each target's newest download from the updater release's stable manifests at generation with its fallback rules, and the redirect pages `/download/<target>/` built from them (`renderDownloadPages`); pure apart from an injected `fetch`), `redirect-pages.ts` (the redirect page template `redirectPage` and the static pages for the former remote connections addresses and the retired Chinese locale codes), `page-outline.ts` (the homepage's feature groups, sections, workflow steps and nodes, privacy sections, FAQ answer-key selection and the platform lists — `availablePlatforms`, the platforms on offer (macOS and Windows desktop applications, both marked Beta, and the Linux server), and `plannedPlatforms` — shared by the Vue app, Markdown renderer and JSON-LD, plus `remoteGuidePage`, the docs page the remote connections links open), `agent-documents.ts` (renders the Markdown files and `llms*.txt` from the catalogs; Node only), `privacy-date.ts` (the policy revision date and its shared UTC formatter) |
| `app/` | The Vue app: `app.vue` (shell and head), `error.vue`, `pages/` (home, `download.vue` with a tab per system, privacy, the unlocalized `404.vue`), `components/` (header and footer, which link to the docs, navigation, feature wall and illustrations, the home page's remote connections overview with its SSH diagram `RemoteRoute.vue`, workflow demo, the terminal demo of the download page, the install command, product preview host, analytics consent), `composables/` (site SEO, home section tracking, consent, glass, reveal, visible motion, copy interpolation, `usePreviewFrame.ts` the lifecycle of an embedded preview frame), `plugins/` (client-side language detection and analytics consent), `assets/` (CSS, local icons), `utils/` |
| `i18n/locales/` | All website copy, one JSON catalog per language; English is the source |
| `scripts/` | `build-preview.mjs` (runs the UI's `build:preview`), `check-locales.mjs` (catalog coverage, keys, placeholders, and that no message contains the product name; `productNameLiterals` is exported for its test), `generate-og-images.py` (draws one committed 1200×630 share image per route code (the language tag), from `public/wordmark.png` and each catalog's slogan and title lines; not run by `nuxt generate`, and `nuxt.config.ts` throws when an image is missing; its requirements and fonts are in the script's docstring), `preview.mjs` (serves `.output/public` on loopback at the configured base path, on `PORT` or 4318, with the generated error document for missing files, `.md` as `text/markdown` and `.wasm` as `application/wasm`, and the docs from `apps/docs/dist` under `docs/`, which stands in for the deployment's copy so either build can be redone alone) |
| `test/` | The Node tests: `check-locales.test.ts` (the product-name rule of `scripts/check-locales.mjs`), `site-seo.test.ts` (the SEO modules, the redirect pages, and that every docs page the website links to exists in every language, read from `apps/docs/src/content/docs/`), `downloads.test.ts` (download resolution and the download redirect pages), `preview.test.ts` (runs a copy of `scripts/preview.mjs` in a throwaway tree) and `install-script.test.ts` (runs `public/install.sh` against a fake release; also holds its port and update key to `config/app.json` and the updater's `tauri.conf.json`, and the example systemd unit's port to the configured one; the script's `OCTOBOARD_UPDATE_PUBKEY` environment variable replaces the embedded update public key for minisign's signature check (and, set, keeps the installed `octoboardd` from being asked to check it) and is for tests only, which pass a throwaway key, while `OCTOBOARD_MANIFEST_BASE` is the user-facing override) |
| `public/` | Local branding, licenses, share images, the demo image of the download page, `install.sh` (the POSIX installer of the Linux daemon, served at `/install.sh`) and the GitHub Pages marker copied into the output |

## Dependency direction

The rules behind these edges are in `docs/conventions/website.md`, "General principles" and "Imports and
dependencies".

- The website depends on `@octoboard/ui` (`workspace:*`) and imports from it, by package name, only what its `exports`
  map names: the language list (`@octoboard/ui/languages`), the language matcher (`@octoboard/ui/match-language`), the
  preview's `postMessage` contract (`@octoboard/ui/preview-messages`) and the agent marks (`@octoboard/ui/agent-marks`);
  it hosts the React preview in iframes and never imports its components.
- Product identity and contacts come from `config/app.json`, mapped into runtime configuration by `nuxt.config.ts`; the
  default browser port (`defaultBrowserPort`) is read by `seo/downloads.ts`; `install-script.test.ts` holds
  `public/install.sh` to naming no port of its own.
- The download links depend on the release workflow's output: `seo/downloads.ts` reads the stable manifests of the
  `updater` release that `apps/desktop/scripts/publish-release.mjs` writes (names from
  `apps/desktop/scripts/release-manifest.mjs`), so a change to those manifests' names or the macOS disk image's name
  changes this module.
- The website and the documentation site (`apps/docs/`, [`docs.md`](docs.md)) import no code from each other at build
  time. The website knows the docs' paths (`docsPages` in `seo/site-seo.ts`) and a test reads the docs' content directory
  to check them; the docs read the website's logo and wordmark and keep copies of its deployment settings, redirect page
  and download addresses, which `apps/docs/webParity.test.ts` compares with this site's modules.
- `seo/agent-documents.ts` reads the catalogs with `node:fs` and is imported by `nuxt.config.ts` and the Node test,
  never by Vue code.

## Where new code goes

- Website copy: the catalogs in `i18n/locales/` (English first); `check:locales` validates every other catalog against
  it.
- A new language: its row comes from `@octoboard/ui/languages` through `seo/site-seo.ts`, plus its catalog and
  share image.
- A download target, tab or the commands the Linux copy quotes: `seo/downloads.ts`, which the Vue app and the Markdown
  renderer share; the install script itself is `public/install.sh`.
- A link into the docs: a page of `docsPages` in `seo/site-seo.ts`, built with `docsPath` from the site's base path,
  never hard-coded; the page has to exist in every language of the docs (`docs.md`'s "Where new code goes").
- A page that should not be indexed: the page kinds in `unindexedPageKinds` (`seo/site-seo.ts`); the sitemap excludes
  follow from it.
- A homepage section, feature group or workflow item: `seo/page-outline.ts`, so the Vue app and the Markdown renderer
  stay in step; its component under `app/components/`.
- Icons are local SVGs in `app/assets/icons/`, drawn by `app/components/SiteIcon.vue`; platform and GitHub brand icons
  come from Font Awesome Free 6.7.2 and keep their embedded attribution, other UI icons from Phosphor, the diagram
  glyphs of `app/components/FeatureGlyph.vue` from Lucide (`@lucide/vue`) and the agent marks of `AgentMark.vue` from
  LobeHub; the licence files of the sources are in `public/licenses/`, and adding an icon is in
  `docs/conventions/website.md`, "Icons".
- Anything both `nuxt.config.ts` and the Vue app need goes in `seo/` (the reason is in `docs/conventions/website.md`,
  "Imports and dependencies").

## Shared entry points

The rows are the helper list; the rule to use them in place of the lower level is in `docs/conventions/website.md`,
"General principles".

| Need | Entry point |
|---|---|
| Locale table, canonical and Markdown paths, JSON-LD | `seo/site-seo.ts` |
| FAQ answer keys across HTML, Markdown and JSON-LD | `seo/page-outline.ts` (`homeQuestionAnswerKey`) |
| Download targets, resolved download links, install command | `seo/downloads.ts` |
| Redirect pages (the former remote connections page, the retired Chinese locale codes `zh` and `zh-Hant`; the download addresses use the same template) | `seo/redirect-pages.ts`; the retired codes themselves in `seo/site-seo.ts` (`retiredLocaleCodes`) |
| Links into the docs | `seo/site-seo.ts` (`docsPages`, `docsPath`); the remote guide page `remoteGuidePage` in `seo/page-outline.ts` |
| Which pages are indexed, and the sitemap excludes | `seo/site-seo.ts` (`isIndexedPage`, `unindexedSitemapExcludes`) |
| Privacy revision date across HTML and Markdown | `seo/privacy-date.ts` (`privacyUpdatedDate`, `formatPrivacyDate`) |
| Product and operator names in copy | `app/composables/useSiteCopy.ts` |
| SEO head tags on a page | `app/composables/useSiteSeo.ts` |
| Visibility and reduced-motion gating of animation | `app/composables/useVisibleMotion.ts` |
| Hosting a React preview or feature view | `app/components/ProductPreview.vue`, `FeatureAppView.vue`, over `app/composables/usePreviewFrame.ts` |
| An agent's mark | `app/components/AgentMark.vue` |

Checks: `pnpm --filter @octoboard/web check:locales`, `typecheck` and `test` (the sitemap test is skipped until `build`
has run); the website checks are not part of the desktop release gate.
