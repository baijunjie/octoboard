import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import test from "node:test";
import appConfig from "../../config/app.json" with { type: "json" };
import { loadWithEnv } from "./moduleTestSupport.ts";

const siteConfig = fileURLToPath(new URL("./site.config.ts", import.meta.url));
const load = (env?: Record<string, string | undefined>) =>
  loadWithEnv(siteConfig, ["default", "docsSegment"], env).default as { origin: string; websiteBase: string; base: string; productName: string; repositoryUrl: string };

test("by default the docs are at /docs/ of the website address in the application settings", () => {
  const site = load();
  assert.equal(site.origin, new URL(appConfig.websiteUrl).origin);
  assert.equal(site.websiteBase, "/");
  assert.equal(site.base, "/docs/");
  assert.equal(site.productName, appConfig.name);
  assert.equal(site.repositoryUrl, appConfig.repositoryUrl);
  assert.equal(loadWithEnv(siteConfig, ["docsSegment"]).docsSegment, "docs");
});

test("the docs' base path is the website's base path with docs/ added, however the base path is written", () => {
  for (const [written, websiteBase] of [["/site/", "/site/"], ["site", "/site/"], ["/site", "/site/"], ["site/", "/site/"], ["/a/b/", "/a/b/"], ["/", "/"], ["", "/"]]) {
    const site = load({ NUXT_APP_BASE_URL: written });
    assert.equal(site.websiteBase, websiteBase, JSON.stringify(written));
    assert.equal(site.base, `${websiteBase}docs/`, JSON.stringify(written));
  }
});

test("the origin is the deployment's, reduced to its origin, and the application settings' when none is set", () => {
  assert.equal(load({ NUXT_PUBLIC_SITE_ORIGIN: "https://example.com" }).origin, "https://example.com");
  assert.equal(load({ NUXT_PUBLIC_SITE_ORIGIN: "https://example.com:8443/some/path/?q=1" }).origin, "https://example.com:8443");
  assert.equal(load({ NUXT_PUBLIC_SITE_ORIGIN: "" }).origin, new URL(appConfig.websiteUrl).origin);
  // The origin does not move the base path, nor the base path the origin.
  const both = load({ NUXT_PUBLIC_SITE_ORIGIN: "https://example.com", NUXT_APP_BASE_URL: "/product/" });
  assert.deepEqual([both.origin, both.base], ["https://example.com", "/product/docs/"]);
});
