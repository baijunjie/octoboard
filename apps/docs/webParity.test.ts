import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";
import appConfig from "../../config/app.json" with { type: "json" };
import { downloadPagePath, downloadTargets, installCommand, releasesUrl } from "../web/seo/downloads.ts";
import { redirectPage as webRedirectPage } from "../web/seo/redirect-pages.ts";
import { docsPath, retiredLocaleCodes } from "../web/seo/site-seo.ts";
import { docsWords } from "./siteTitle.ts";
import { loadWithEnv } from "./moduleTestSupport.ts";
import { RETIRED_SEGMENTS, redirectPage } from "./redirects.ts";

// The docs keep their own copies of a few things the website decides (`site.config.ts`, `redirects.ts`, the install
// command and the download addresses in `src/commands.ts`, the word for "Docs" in `siteTitle.ts`) rather than import or
// read the website's modules and catalogs in the Astro build. These tests import or read them and compare, so a change
// on either side fails here until the other follows.

const docsSiteConfig = fileURLToPath(new URL("./site.config.ts", import.meta.url));
const webSiteConfig = fileURLToPath(new URL("../web/site.config.ts", import.meta.url));
const docsCommands = fileURLToPath(new URL("./src/commands.ts", import.meta.url));

const environments: Record<string, string | undefined>[] = [
  {},
  { NUXT_PUBLIC_SITE_ORIGIN: "https://example.com:8443/some/path/?q=1" },
  { NUXT_APP_BASE_URL: "/site/" },
  { NUXT_APP_BASE_URL: "site" },
  { NUXT_APP_BASE_URL: "/a/b" },
  { NUXT_APP_BASE_URL: "" },
  { NUXT_PUBLIC_SITE_ORIGIN: "https://example.com", NUXT_APP_BASE_URL: "product" },
];

test("the docs read the website's origin and base path from the deployment settings as the website does, and serve under its docs path", () => {
  for (const env of environments) {
    const web = loadWithEnv(webSiteConfig, ["default"], env).default as { origin: string; baseURL: string };
    const docs = loadWithEnv(docsSiteConfig, ["default"], env).default as { origin: string; websiteBase: string; base: string };
    assert.deepEqual({ origin: docs.origin, websiteBase: docs.websiteBase }, { origin: web.origin, websiteBase: web.baseURL }, JSON.stringify(env));
    assert.equal(docs.base, docsPath(web.baseURL, "en"), JSON.stringify(env));
  }
});

test("the former Chinese path segments are the website's retired locale codes", () => {
  assert.deepEqual(RETIRED_SEGMENTS, retiredLocaleCodes);
});

test("the docs' redirect page is the website's", () => {
  const input = { url: "https://example.com/docs/zh-CN/a/?x=<1>&y=\"2\"", title: "Doc <title> & more", notice: "Moved \"here\".", baseURL: "/site/" };
  assert.equal(
    redirectPage({ url: input.url, title: input.title, notice: input.notice, favicon: `${input.baseURL}favicon.png` }),
    webRedirectPage(input),
  );
  assert.equal(
    redirectPage({ url: input.url, title: input.title, notice: input.notice, favicon: `${input.baseURL}favicon.png`, carry: true }),
    webRedirectPage({ ...input, carry: true }),
  );
});

test("the install command and the download addresses are the ones the website publishes", () => {
  for (const env of environments) {
    const { commands, downloads } = loadWithEnv(docsCommands, ["commands", "downloads"], env) as unknown as {
      commands: Record<string, string>;
      downloads: Record<string, string>;
    };
    const web = loadWithEnv(webSiteConfig, ["default"], env).default as { url: string };
    assert.equal(commands.install, installCommand(web.url), JSON.stringify(env));
    assert.deepEqual(
      downloads,
      {
        ...Object.fromEntries(downloadTargets.map(({ id }) => [id, `${web.url}${downloadPagePath(id)}`])),
        releases: releasesUrl(appConfig.repositoryUrl),
      },
      JSON.stringify(env),
    );
  }
});

test("the word for \"Docs\" in each language is the website's navigation label", () => {
  for (const [language, word] of Object.entries(docsWords)) {
    const web = JSON.parse(readFileSync(new URL(`../web/i18n/locales/${language}.json`, import.meta.url), "utf8")) as { navDocs: string };
    assert.equal(word, web.navDocs, language);
  }
});
