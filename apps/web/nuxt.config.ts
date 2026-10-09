import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import appConfig from "../../config/app.json";
import { readLocaleMessages, renderAgentFiles } from "./seo/agent-documents";
import {
  locales,
  localizedPath,
  pagePaths,
  robotsTxt,
  sitemapXml,
} from "./seo/site-seo";
import site from "./site.config";

const prerenderRoutes = locales.flatMap(({ code }) =>
  pagePaths.map((path) => `/${localizedPath(code, path)}`),
);

// The cards are committed. This build does not draw them, so a missing file
// would publish a page whose share image 404s.
for (const { code } of locales) {
  const image = fileURLToPath(
    new URL(`./public/og-image-${code}.png`, import.meta.url),
  );
  if (!existsSync(image)) {
    throw new Error(
      `Missing share image public/og-image-${code}.png. Run python3 apps/web/scripts/generate-og-images.py.`,
    );
  }
}

export default defineNuxtConfig({
  compatibilityDate: "2026-10-08",
  modules: ["@nuxtjs/i18n"],
  devtools: { enabled: false },
  telemetry: false,
  ssr: true,
  css: [
    "@fontsource-variable/geist",
    "~/assets/css/main.css",
    "~/assets/css/liquid-glass.css",
  ],
  app: {
    baseURL: site.baseURL,
    head: {
      viewport: "width=device-width, initial-scale=1",
      link: [
        { rel: "icon", type: "image/png", href: `${site.baseURL}favicon.png` },
        {
          rel: "apple-touch-icon",
          href: `${site.baseURL}apple-touch-icon.png`,
        },
      ],
    },
  },
  runtimeConfig: {
    public: {
      appName: appConfig.name,
      repositoryUrl: appConfig.repositoryUrl,
      siteUrl: site.url,
      siteOrigin: site.origin,
      contactEmail: appConfig.support.email,
      operatorName: appConfig.author.name,
    },
  },
  nitro: {
    preset: "static",
    prerender: {
      autoSubfolderIndex: true,
      failOnError: true,
      routes: [...prerenderRoutes, "/404/"],
    },
  },
  i18n: {
    strategy: "prefix_except_default",
    defaultLocale: "en",
    langDir: "locales",
    locales: locales.map(({ code, language, file, name, dir }) => ({
      code,
      language,
      file,
      name,
      ...(dir ? { dir } : {}),
    })),
    baseUrl: site.origin,
    trailingSlash: true,
    // The client plugin uses the shared language matcher and remembers manual choices.
    detectBrowserLanguage: false,
  },
  hooks: {
    "nitro:init"(nitro) {
      nitro.hooks.hook("prerender:done", async () => {
        const messages = await readLocaleMessages(
          fileURLToPath(new URL("./i18n/locales/", import.meta.url)),
        );
        const agentFiles = renderAgentFiles({
          siteUrl: site.url,
          appName: appConfig.name,
          operatorName: appConfig.author.name,
          contactEmail: appConfig.support.email,
          repositoryUrl: appConfig.repositoryUrl,
          messages,
        });
        // GitHub Pages serves this document at arbitrary missing URLs. Hydrating
        // its prerendered route would replace that URL with /404/.
        const fallback = (
          await readFile(
            resolve(nitro.options.output.publicDir, "404/index.html"),
            "utf8",
          )
        )
          .replace(/<script\b[^>]*>[\s\S]*?<\/script\s*>/gi, "")
          .replace(/<link\b[^>]*\brel="(?:modulepreload|preload)"[^>]*>/gi, "");
        await Promise.all([
          writeFile(
            resolve(nitro.options.output.publicDir, "404.html"),
            fallback,
          ),
          writeFile(
            resolve(nitro.options.output.publicDir, "sitemap.xml"),
            sitemapXml(site.url),
          ),
          writeFile(
            resolve(nitro.options.output.publicDir, "robots.txt"),
            robotsTxt(site.url),
          ),
          ...Object.entries(agentFiles).map(([name, body]) =>
            writeFile(resolve(nitro.options.output.publicDir, name), body),
          ),
        ]);
      });
    },
  },
});
