import { fileURLToPath } from "node:url";
import { defineConfig, mergeConfig } from "vite";
import applicationConfig from "./vite.config.ts";

// A separate build keeps the public, entirely in-memory demonstration out of the desktop bundle.
export default defineConfig(async (environment) => {
  const shared =
    typeof applicationConfig === "function"
      ? await applicationConfig(environment)
      : await applicationConfig;
  return mergeConfig(shared, {
    base: "./",
    build: {
      outDir: fileURLToPath(
        new URL("../../apps/web/public/preview", import.meta.url),
      ),
      emptyOutDir: true,
      rollupOptions: {
        input: {
          website: fileURLToPath(new URL("./website-preview.html", import.meta.url)),
          feature: fileURLToPath(new URL("./feature-preview.html", import.meta.url)),
        },
      },
    },
  });
});
