# Octoboard docs

The documentation site, built with [Starlight](https://starlight.astro.build/) (Astro) as static files. It is served
under `/docs/` of the website's address, in the same GitHub Pages deployment as the website, with its own styling and
the application's 17 languages.

This is the `@octoboard/docs` package in the repository's pnpm workspace. Dependencies and their lockfile belong to the
workspace root.

## Commands

Run these commands from the repository root:

- `pnpm --filter @octoboard/docs dev` — starts the development server.
- `pnpm --filter @octoboard/docs build` — builds the static site into `apps/docs/dist`; the deployment runs the same
  command.
- `pnpm --filter @octoboard/docs preview` — serves the built site locally at its base path.
- `pnpm --filter @octoboard/docs typecheck` — checks the configuration, content and components (`astro check`).
- `pnpm --filter @octoboard/docs test` — runs the package's tests; the checks that read the built site are skipped
  until `build` has run.

## Configuration

The site is built with the website's deployment settings: `NUXT_PUBLIC_SITE_ORIGIN` sets the public origin and
`NUXT_APP_BASE_URL` the website's base path, under which the docs add `docs/`. They default to the `websiteUrl` of
`config/app.json` and `/`. Pushes to `main` that change the docs publish them with the website through
[`.github/workflows/website.yml`](../../.github/workflows/website.yml).

To see the docs together with the website, build both, then run the website's preview
(`pnpm --filter @octoboard/web preview`), which serves the docs at their path.

## Read next

- [What the documentation site publishes: addresses, languages and pages](../../docs/product/documentation-site.md)
- [How the docs are laid out, what they share with the website, and where new pages go](../../docs/structure/docs.md)
