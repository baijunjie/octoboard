import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import babel from "@rolldown/plugin-babel";
import tailwindcss from "@tailwindcss/vite";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import { Features } from "lightningcss";
import type { Plugin } from "vite";
import { configDefaults, defineConfig } from "vitest/config";

import appConfig from "../../config/app.json" with { type: "json" };
import { loadEnv } from "../../scripts/env.mjs";

import { REPORT_ENV } from "./src/test/failedFilesReporter.ts";

// Fills `%APP_NAME%` in `index.html` (the page title) from `config/app.json`, the source the app's
// name comes from.
const appName: Plugin = {
  name: "octoboard-app-name",
  transformIndexHtml: (html) => html.replaceAll("%APP_NAME%", appConfig.name),
};

// Gives the dev-only gallery's scenario page (`gallery-frame.html`, see `src/gallery/`) the Content
// Security Policy of `index.html`, read from it so the two cannot drift. Neither gallery page is part
// of a build, which bundles `index.html` alone.
const galleryFrameCsp: Plugin = {
  name: "octoboard-gallery-frame-csp",
  transformIndexHtml(html, { filename }) {
    if (!filename.endsWith("gallery-frame.html")) return html;
    const index = readFileSync(new URL("./index.html", import.meta.url), "utf8");
    const policy = /http-equiv="Content-Security-Policy"\s+content="([^"]*)"/.exec(index)?.[1];
    if (!policy) throw new Error("index.html has no Content-Security-Policy meta tag to copy");
    return html.replace("%APP_CSP%", policy);
  },
};

// Keeps a build to the two highlighting themes the file viewer uses (see
// `src/viewer/themeShim.ts`) by giving it the shim for the two modules that list them all: the
// renderer library's `@pierre/theming/themes`, and Shiki's own `themes.mjs`, which Shiki's entry
// points import by that relative name. A dependency update that renames either would leave it
// unredirected and quietly bring every theme back, so a build that still contains either real
// module fails instead. The dev server's pre-bundled dependencies and the tests keep every theme.
const themeShim = fileURLToPath(new URL("./src/viewer/themeShim.ts", import.meta.url));
const THEME_LISTS = [/[\\/]@pierre[\\/]theming[\\/]dist[\\/]themes\.js$/, /[\\/]shiki[\\/]dist[\\/]themes\.mjs$/];
const viewerThemes: Plugin = {
  name: "octoboard-viewer-themes",
  enforce: "pre",
  resolveId(source, importer) {
    if (source === "@pierre/theming/themes") return themeShim;
    if (source === "./themes.mjs" && importer && /[\\/]shiki[\\/]dist[\\/][^\\/]+\.mjs$/.test(importer)) return themeShim;
    return null;
  },
  buildEnd() {
    for (const id of this.getModuleIds()) {
      if (THEME_LISTS.some((list) => list.test(id))) {
        this.error(`${id} is in the build: the file viewer's theme shim no longer replaces it (see src/viewer/themeShim.ts)`);
      }
    }
  },
};

export default defineConfig(({ command }) => {
  // A daemon started by hand binds an OS-assigned port, so `OCTOBOARD_DAEMON_PORT` names it for the
  // dev server's proxy: the page then reaches `/ws/...` on its own origin, which is what the UI does
  // when the daemon itself serves it (src/daemon.ts). Without the variable there is no proxy, and the
  // UI is pointed at the daemon with `?port=` or `VITE_DAEMON_PORT` instead.
  //
  // Both come from the root `.env` and `.env.local`, read here because vite never writes a non-`VITE_`
  // value into `process.env` and loads this config before any `.env` of its own; `VITE_DAEMON_PORT`
  // reaches `import.meta.env` through `process.env`. A `vite build` never loads them, so a build cannot
  // bake a dev port into the bundle — but `vitest` resolves this config with `command: "serve"` too, so
  // a test run does load them, and anything a test derives from the port varies with the developer's
  // own `.env` (a separate process from the `vite build` the same script runs afterwards).
  //
  // The variables stay out of `packages/ui/.env`: that directory is vite's own `envDir`, which a
  // `vite build` reads too, so a `VITE_DAEMON_PORT` there would go into a release bundle. Vite also
  // expands `$VAR` in the files it loads itself, where the root files are read literally here.
  if (command === "serve") loadEnv(".env", ".env.local");
  const daemonPort = process.env.OCTOBOARD_DAEMON_PORT;

  return {
    // Relative asset URLs, so the build loads from any static directory: a shell's bundled assets, or
    // a path the daemon serves it under.
    base: "./",
    // React Compiler memoizes components and hooks, so a component below `App` whose inputs did not
    // change is skipped when a store update re-renders its parent. Without it every daemon event
    // that replaces a map re-renders the whole tree: a working session's spinner title alone sends
    // one such event per frame. `reactCompilerPreset` applies to Vite's client environment only, so
    // the build, the dev server, the gallery and jsdom tests run the compiled code, while tests in
    // the node environment run it uncompiled.
    plugins: [react(), babel({ presets: [reactCompilerPreset()] }), tailwindcss(), appName, galleryFrameCsp, viewerThemes],
    define: { __OCTOBOARD_DEV_PROXY__: JSON.stringify(Boolean(daemonPort)) },
    // A build's CSS minifier lowers `:dir()` to `:lang()` lists for its default targets, so every
    // `:dir()` rule would follow the page's language instead of the element's own reading direction:
    // the task-list rules in `src/viewer/markdown.css` then put a right-to-left item's box opposite
    // its list's markers. Keeping `:dir()` throughout (Tailwind's `rtl:`/`ltr:` variants and HeroUI's
    // rules too) makes a build match the dev server, and leaves browsers without `:dir()` to drop
    // the rules that rely on it alone, where the lowered form gets them wrong.
    css: { lightningcss: { exclude: Features.DirSelector } },
    clearScreen: false,
    test: {
      setupFiles: ["./src/test/setup.ts"],
      // Under the load of several parallel runs, a jsdom test that needs about a second idle can take
      // more than the default five. The worker count stays at vitest's default and `fsModuleCache` stays
      // off: both were measured under that load, and the persistent cache used more CPU and wall time,
      // while a cap of four workers used the same CPU and took about 1.5 times as long. Isolation stays
      // at the default too, as the tests rely on module state living for one file only (lazily imported
      // surfaces, HeroUI's toast queue).
      testTimeout: 30_000,
      hookTimeout: 30_000,
      // `pnpm verify` asks which files failed, to re-run only those once they are fixed.
      reporters: process.env[REPORT_ENV] ? [...configDefaults.reporters, "./src/test/failedFilesReporter.ts"] : configDefaults.reporters,
    },
    server: {
      port: 5174,
      strictPort: true,
      proxy: daemonPort ? { "/ws": { target: `ws://127.0.0.1:${daemonPort}`, ws: true } } : undefined,
    },
  };
});
