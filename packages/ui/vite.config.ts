import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

import appConfig from "../../config/app.json" with { type: "json" };
import { loadEnv } from "../../scripts/env.mjs";

// Fills `%APP_NAME%` in `index.html` (the page title) from `config/app.json`, the source the app's
// name comes from.
const appName: Plugin = {
  name: "octoboard-app-name",
  transformIndexHtml: (html) => html.replaceAll("%APP_NAME%", appConfig.name),
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
  if (command === "serve") loadEnv(".env", ".env.local");
  const daemonPort = process.env.OCTOBOARD_DAEMON_PORT;

  return {
    // Relative asset URLs, so the build loads from any static directory: a shell's bundled assets, or
    // a path the daemon serves it under.
    base: "./",
    plugins: [react(), tailwindcss(), appName],
    define: { __OCTOBOARD_DEV_PROXY__: JSON.stringify(Boolean(daemonPort)) },
    clearScreen: false,
    server: {
      port: 5174,
      strictPort: true,
      proxy: daemonPort ? { "/ws": { target: `ws://127.0.0.1:${daemonPort}`, ws: true } } : undefined,
    },
  };
});
