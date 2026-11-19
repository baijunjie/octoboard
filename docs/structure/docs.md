# Documentation site structure (`apps/docs/`)

`@octoboard/docs`: the product's how-to documentation (introduction, installation on each system, connecting to a remote
host), built with Starlight (Astro 6) as static files, with its own styling, in the application's 17 languages. It has no
backend and never connects to `octoboardd`. It is served under `/docs/` of the website's address and is deployed in the
same GitHub Pages deployment as the website ([`web.md`](web.md)), which links into it and keeps only summaries of this
content. [`apps/docs/README.md`](../../apps/docs/README.md) holds the commands and the deployment settings a developer
needs to build and preview it; what the site offers a reader (addresses, languages, the fallback to English) is in
`docs/product/documentation-site.md`.

Paths in this doc that do not start with `apps/` or `packages/` are relative to `apps/docs/`.

## Why a separate app at `/docs/`

- **A separate Starlight site rather than more Nuxt pages.** The website is built for marketing pages (no sidebar, search
  or per-topic pages), and the docs do not need to match its look, so a documentation framework replaces building
  navigation, search and a table of contents into it. Starlight was chosen over VitePress, Docus and Docusaurus for its
  built-in internationalization: it lays out right-to-left languages (Arabic) and, for a page a language does not have
  yet, shows the English page marked as untranslated, where VitePress would need a complete page set per language.
- **Served from the website's own Pages site, not a subdomain.** One repository has one Pages site and a subdomain
  would need a second repository. `.github/workflows/website.yml` builds both with the same origin and base path and
  uploads one output, with the docs' build copied into `docs/` of the website's, so the website's domain, HTTPS and base
  path handling apply to the docs unchanged. A change to this app (Markdown pages included) deploys the site, and
  `release.yml` starts the deployment after each publication.
- **One root `404.html`.** Pages serves only the root one, so a missing address under `/docs/` shows the website's 404
  page. The docs still build a 404 page of their own, once and at their root, and `src/routeData.ts` drops the language
  alternates Starlight gives it.
- **The docs' sitemap is not merged into the website's.** The website's `robots.txt` (`robotsTxt` in
  `apps/web/seo/site-seo.ts`) names the docs' own `sitemap-index.xml`.

## Directory map

| Path | Role |
|---|---|
| `astro.config.ts`, `site.config.ts` | The Astro and Starlight configuration (locales, sidebar, custom CSS, the title component, the route middleware, the redirect integration). `site.config.ts` reads the website's deployment variables (`NUXT_PUBLIC_SITE_ORIGIN`, `NUXT_APP_BASE_URL`) at build time with the website's own defaults (the `websiteUrl` of `config/app.json`, and `/`), so one deployment configures both sites, and derives the base path, `<website base>docs/`; `astro.config.ts` takes the site icon by its deployed address under the website's base, since Starlight puts its own base path in front of any address that is not a full URL |
| `locales.ts`, `sidebar.ts` | The Starlight locale table, built from the application's language list (`@octoboard/ui/languages`; English is the unprefixed root locale and the fallback, so a page a language lacks shows the English page at that language's address, marked as untranslated; every other language's path segment is its tag, as on the website, so Simplified Chinese is `zh-CN`), and the sidebar layout: groups name their pages one by one, a group's label is translated there (keyed by language tag), and a page's entry takes the page's own title |
| `redirects.ts` | The Astro integration that writes, after the build, a redirect page at every former address under the retired Chinese path segments (`zh/`, `zh-Hant/`, now `zh-CN/`, `zh-TW/`), so a link or search result that still names a former address arrives at the same page; the page is unlocalized, not indexed, and carries the visitor's query and fragment on. A copy of the website's table (`retiredLocaleCodes`) and page template (`redirectPage`) |
| `src/commands.ts` | Every command, address and value the pages quote (install command, default port, `octoboardd` commands, connection scheme, the Tailscale and bridge commands, the website's download addresses), so a page never writes one out; the port comes from `config/app.json` and the addresses from `site.config.ts` |
| `src/content.config.ts`, `src/routeData.ts` | The content collection (a page's id keeps its locale directory's spelling, so `zh-TW/` is not lower-cased into a locale that does not exist) and the Starlight route middleware |
| `siteTitle.ts` | The localized site title: `docsWords` (the word for "Docs" in each language) and `docsTitles` (the whole title per language, with `{name}` for the product name), from which `astro.config.ts` builds Starlight's per-locale `title` |
| `src/components/SiteTitle.astro`, `src/styles/theme.css` | The header title (the website's logo and pixel wordmark imported by their paths from `apps/web/public/` so the build bundles them, then the language's word for "Docs" from `docsWords`, in a `<site-title-fit>` custom element that scales the lockup down to fit the header's title column) and the theme (the website's accent red, also for the dark theme's accent text; the light theme's is darkened for contrast) |
| `src/content/docs/` | The pages, Markdown or MDX: English at the root, a translation at the same path under its language's directory, e.g. `src/content/docs/ja/index.mdx` (the introduction, `install/` per system, the remote connection guides); the introduction and the guides are MDX and take the product name and every command, value and website address they quote from `src/commands.ts` |
| `src/snippets/<language>/` | MDX copy several guides of one language share, written once per language outside the content collection and imported by each guide of that language (`src/snippets/en/host-browser-address.mdx`); a language that translates a guide adds its own copy of the snippets that guide imports |
| `*.test.ts`, `moduleTestSupport.ts` | The Node tests (`pnpm --filter @octoboard/docs test`): the deployment settings, the locale table, the title table (`siteTitle.test.ts`), the redirect pages, the quoted commands, `webParity.test.ts` (below) and `buildOutput.test.ts` (the built site; skips itself until a build exists) |

## Dependency direction

- The docs depend on `@octoboard/ui` (`workspace:*`) for one thing, the language list (`@octoboard/ui/languages`), and
  on `config/app.json` for the product name, repository address and default server port.
- The docs read two images from the website (`apps/web/public/logo-640.png`, `wordmark.png`) and take its icon
  (`favicon.png`) by its deployed address instead of keeping copies; they import no website code into the Astro build. The few things the website also decides are
  copied here (the deployment settings in `site.config.ts`, the redirect page and the retired path segments in
  `redirects.ts`, the install command and the download addresses in `src/commands.ts`, the word for "Docs" in each language in `siteTitle.ts`), and `webParity.test.ts` imports the website's modules (`apps/web/site.config.ts`,
  `seo/downloads.ts`, `seo/redirect-pages.ts`, `seo/site-seo.ts`) and reads its catalogs (`apps/web/i18n/locales/*.json`,
  for `navDocs`) and fails until both sides agree.
- The website links to docs pages by their path (`docsPages` in `apps/web/seo/site-seo.ts`); a website test reads this
  app's `src/content/docs/` to check that every such page exists in every language. These cross-package reads are
  declared in `scripts/verify/scope.mjs` (`DOCS_IMPORTS_FROM_UI`, `DOCS_READS_WEB`, `WEB_READS_DOCS`), which selects the
  docs checks when one of those files changes.

## Where new code goes

- A page: `src/content/docs/<path>` in English, its translation at the same path under the language's directory; a
  language without it shows the English page. List the page in `sidebar.ts`, whose groups name their pages one by one,
  and, when the website links to it, in `docsPages` in `apps/web/seo/site-seo.ts` (which then needs the page in every
  language).
- A command, address or value a page quotes: `src/commands.ts`, not the page.
- Copy several guides of one language share: `src/snippets/<language>/`, imported by each guide of that language.
- A new language: its row comes from `@octoboard/ui/languages` through `locales.ts`; the sidebar's translated group
  labels in `sidebar.ts` are typed per language.
- Anything the website also decides: the copy goes here and the comparison into `webParity.test.ts`.

## Shared entry points

| Need | Entry point |
|---|---|
| The commands, addresses and values a page quotes | `src/commands.ts` |
| Where the docs are served (origin, base path) | `site.config.ts` |
| Locale table | `locales.ts` |
| Sidebar groups and their translated labels | `sidebar.ts` |

Checks: `pnpm --filter @octoboard/docs typecheck` (`astro check`), `build` and `test`, and the repository's lint over
`apps/docs`; `pnpm verify` runs them in that order when the app is in scope.
