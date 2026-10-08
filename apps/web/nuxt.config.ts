import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import appConfig from "../../config/app.json";
import site from "./site.config";

const locales = [
  { code: "ar", language: "ar", file: "ar.json", name: "العربية", dir: "rtl" },
  { code: "de", language: "de", file: "de.json", name: "Deutsch" },
  { code: "en", language: "en", file: "en.json", name: "English" },
  { code: "es", language: "es", file: "es.json", name: "Español" },
  { code: "fr", language: "fr", file: "fr.json", name: "Français" },
  { code: "hi", language: "hi", file: "hi.json", name: "हिन्दी" },
  { code: "id", language: "id", file: "id.json", name: "Bahasa Indonesia" },
  { code: "it", language: "it", file: "it.json", name: "Italiano" },
  { code: "ja", language: "ja", file: "ja.json", name: "日本語" },
  { code: "ko", language: "ko", file: "ko.json", name: "한국어" },
  {
    code: "pt-BR",
    language: "pt-BR",
    file: "pt-BR.json",
    name: "Português (Brasil)",
  },
  { code: "ru", language: "ru", file: "ru.json", name: "Русский" },
  { code: "th", language: "th", file: "th.json", name: "ไทย" },
  { code: "tr", language: "tr", file: "tr.json", name: "Türkçe" },
  { code: "vi", language: "vi", file: "vi.json", name: "Tiếng Việt" },
  { code: "zh", language: "zh-Hans", file: "zh-Hans.json", name: "简体中文" },
  {
    code: "zh-Hant",
    language: "zh-Hant",
    file: "zh-Hant.json",
    name: "繁體中文",
  },
] as const;

const pagePaths = ["", "privacy/"];
const localizedPath = (code: string, path: string) =>
  `${code === "en" ? "" : `${code}/`}${path}`;
const prerenderRoutes = locales.flatMap(({ code }) =>
  pagePaths.map((path) => `/${localizedPath(code, path)}`),
);
const xmlEscape = (text: string) =>
  text
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;");

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
    locales: [...locales],
    baseUrl: site.origin,
    trailingSlash: true,
    detectBrowserLanguage: false,
  },
  hooks: {
    "nitro:init"(nitro) {
      nitro.hooks.hook("prerender:done", async () => {
        const entries = pagePaths.flatMap((path) => {
          const alternate =
            locales
              .map(
                ({ code, language }) =>
                  `<xhtml:link rel="alternate" hreflang="${language}" href="${xmlEscape(site.url + localizedPath(code, path))}"/>`,
              )
              .join("") +
            `<xhtml:link rel="alternate" hreflang="x-default" href="${xmlEscape(site.url + path)}"/>`;
          return locales.map(
            ({ code }) =>
              `<url><loc>${xmlEscape(site.url + localizedPath(code, path))}</loc>${alternate}</url>`,
          );
        });
        const sitemap =
          '<?xml version="1.0" encoding="UTF-8"?>\n' +
          '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" ' +
          'xmlns:xhtml="http://www.w3.org/1999/xhtml">\n' +
          entries.join("\n") +
          "\n</urlset>\n";
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
            sitemap,
          ),
          writeFile(
            resolve(nitro.options.output.publicDir, "robots.txt"),
            `User-agent: *\nAllow: /\n\nSitemap: ${site.url}sitemap.xml\n`,
          ),
        ]);
      });
    },
  },
});
