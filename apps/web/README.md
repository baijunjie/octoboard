# Octoboard website

The public product website at [octoboard.dev](https://octoboard.dev), built with Nuxt 4 and Nuxt i18n. It generates
localized home and privacy pages as static files for GitHub Pages and embeds a product preview built from the shared
React UI over sample data; it has no backend and does not connect to `octoboardd`.

This is the `@octoboard/web` package in the repository's pnpm workspace. Dependencies and their lockfile belong to the
workspace root.

## Commands

Run these commands from the repository root:

- `pnpm install` — installs workspace dependencies and prepares Nuxt's generated types.
- `pnpm --filter @octoboard/web dev` — builds the React product preview, then starts the website development server.
- `pnpm --filter @octoboard/web check:locales` — checks language coverage, message keys and interpolation
  placeholders.
- `pnpm --filter @octoboard/web typecheck` — prepares Nuxt and checks the website's TypeScript and Vue code.
- `pnpm --filter @octoboard/web test` — runs the website's Node test; the sitemap check reads the generated output, so
  it is skipped until `build` has run.
- `pnpm --filter @octoboard/web build` — builds the React product preview and generates the static website in
  `apps/web/.output/public`; the deployment runs the same command.
- `pnpm --filter @octoboard/web generate:og-images` — runs `python3 scripts/generate-og-images.py` to draw one share
  image per language.
- `pnpm --filter @octoboard/web preview` — serves the generated output locally at the configured base path.

## Configuration

`NUXT_PUBLIC_SITE_ORIGIN` sets the public origin and `NUXT_APP_BASE_URL` the deployment path, at build time; they
default to the `websiteUrl` of `config/app.json` and `/` (`site.config.ts`). Use the same values for `build` and
`preview`. Pushes to `main` publish the site through [`.github/workflows/website.yml`](../../.github/workflows/website.yml).

## Read next

- [What the website publishes: pages, languages, search indexing, preview and privacy](../../docs/product/website.md)
- [The website's structure: directories, its imports from the UI package, where new code
  goes](../../docs/structure/web.md)
- The [documentation index](../../docs/README.md)
