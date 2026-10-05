import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// A daemon started by hand binds an OS-assigned port, so `OCTOBOARD_DAEMON_PORT` names it for the
// dev server's proxy: the page then reaches `/ws/...` on its own origin, which is what the UI does
// when the daemon itself serves it (src/daemon.ts). Without the variable there is no proxy, and the
// UI is pointed at the daemon with `?port=` or `VITE_DAEMON_PORT` instead.
const daemonPort = process.env.OCTOBOARD_DAEMON_PORT;

export default defineConfig({
  // Relative asset URLs, so the build loads from any static directory: a shell's bundled assets, or
  // a path the daemon serves it under.
  base: "./",
  plugins: [react(), tailwindcss()],
  define: { __OCTOBOARD_DEV_PROXY__: JSON.stringify(Boolean(daemonPort)) },
  clearScreen: false,
  server: {
    port: 5174,
    strictPort: true,
    proxy: daemonPort ? { "/ws": { target: `ws://127.0.0.1:${daemonPort}`, ws: true } } : undefined,
  },
});
