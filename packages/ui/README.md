# Octoboard UI

`@octoboard/ui`: the Octoboard interface, a React 19 + HeroUI 3 + Tailwind 4 application built with Vite. The desktop
application ([`apps/desktop/`](../../apps/desktop/README.md)) bundles its `dist/`, and the same build runs in a plain
browser. It talks to the `octoboardd` daemon ([`apps/daemon/`](../../apps/daemon/README.md)) only over the protocol in
[`apps/daemon/PROTOCOL.md`](../../apps/daemon/PROTOCOL.md). A separate build renders it over sample data as the
website's product preview. HeroUI and React Aria reach the application only through the wrapper layer,
[`src/primitives/`](src/primitives/README.md), which lint enforces.

## Development

This is a package of the repository's pnpm workspace: install from the workspace root with `pnpm install`, then run the
scripts from this directory.

- `pnpm dev` — the Vite dev server on port 5174, which `pnpm tauri dev` in `apps/desktop/` also starts.
- `pnpm gallery` — the same dev server on port 5175, opening the UI state gallery (below).
- `pnpm gallery:sweep` — loads gallery scenarios in a headless browser and fails on any gallery failure (see "Sweeping
  the gallery").
- `pnpm typecheck` — `tsc --noEmit`, including gallery, website-preview and test sources.
- `pnpm typecheck:app` — typechecks the production application through `tsconfig.app.json`.
- `pnpm compiler:bailouts` — lists the components and hooks the React Compiler leaves uncompiled (all of `src/`, or the
  files given), exiting 1 when there are any.
- `pnpm test` — `vitest run`: the unit tests beside the code, plus jsdom tests for stateful classes and components.
- `pnpm test:app` — application tests, excluding the gallery, fixtures, website preview and WKWebView benchmark tests.
- `pnpm build` — generates third-party license data, runs `typecheck:app` and the i18n catalog check (`vitest run
  src/i18n`), then writes `dist/` with relative asset paths.
- `pnpm build:preview` — builds the website demonstration into `dist-preview/` with `vite.preview.config.ts`
  (the site's `build-preview` passes `--outDir` to write into `apps/web/public/preview/`).
- `pnpm bench:wkwebview` — a latency benchmark of the production bundle in the system WKWebView (macOS only); see
  [`scripts/wkbench/README.md`](scripts/wkbench/README.md).

### Pointing it at a daemon

Start `octoboardd` and read its port and key from the first line it prints on stdout, `octoboardd listening on
127.0.0.1:<port> with key <key>`. The daemon makes a new key every time it starts and refuses every connection that does
not carry it, so the UI needs both. Either:

- open `http://localhost:5174/?port=<port>&key=<key>`, or set `VITE_DAEMON_PORT` and `VITE_DAEMON_KEY`; or
- run `OCTOBOARD_DAEMON_PORT=<port> pnpm dev` and open `http://localhost:5174/?key=<key>` (or set `VITE_DAEMON_KEY`),
  which reaches the daemon through a same-origin `/ws` proxy (`vite.config.ts`).

The key is only on that line: the daemon does not write it anywhere (`~/.octoboard/octoboardd.port` holds the port alone), so
a `VITE_DAEMON_KEY` kept in the repo-root `.env.local` (gitignored; template: `.env.local.example`) goes stale at every
daemon restart. The page never sends its address as a referrer (`index.html`), so the key in it stays between the page
and the daemon.

### The UI state gallery

A dev-only page showing the real `App` in each of its states over a fixture daemon, one scenario per state; start it
with `pnpm gallery`. How it works, how to write a scenario and the conventions for writing one are in
[`src/gallery/README.md`](src/gallery/README.md); the shared fixtures runtime it runs on is
[`src/fixtures/README.md`](src/fixtures/README.md).

#### Sweeping the gallery

`pnpm gallery:sweep` loads scenarios in a headless browser and fails on any gallery failure; its flags and the
Playwright setup are in [`src/gallery/README.md`](src/gallery/README.md), section "Sweeping the gallery".

`pnpm gallery:compare` compares screenshots from two served galleries and writes an HTML difference report. It is a
manual command, excluded from automatic checks; see "Comparing gallery screenshots" in
[`src/gallery/README.md`](src/gallery/README.md) for setup and usage.

## Read next

- [The UI's structure: directories, the lint-enforced import layering, where new code goes and the shared entry
  points](../../docs/structure/ui.md)
- The [documentation index](../../docs/README.md), which lists the product docs for every part of the window
