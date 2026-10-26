import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { readLocaleMessages, renderAgentFiles } from "../seo/agent-documents.ts";
import {
  buildJsonLd,
  localeCodeFromPath,
  locales,
  markdownFile,
  ogLocaleAlternates,
  pageKind,
  pagePaths,
  robotsTxt,
} from "../seo/site-seo.ts";

const siteUrl = "https://octoboard.dev/";

test("page paths keep the English home unprefixed and privacy as a sibling file", () => {
  assert.equal(pageKind("/"), "home");
  assert.equal(pageKind("/privacy/"), "privacy");
  assert.equal(pageKind("/zh-Hant/privacy/"), "privacy");
  assert.equal(pageKind("/404/"), "other");
  assert.equal(pageKind("/pr/"), "other");
  assert.equal(pageKind("/id/"), "home");
  assert.equal(localeCodeFromPath("/privacy/"), "en");
  assert.equal(localeCodeFromPath("/id/"), "id");
  assert.equal(markdownFile("/"), "index.md");
  assert.equal(markdownFile("/privacy/"), "privacy.md");
  assert.equal(markdownFile("/zh/"), "zh.md");
  assert.equal(markdownFile("/zh-Hant/privacy/"), "zh-Hant/privacy.md");
});

test("robots allows search, retrieval, and training", () => {
  const robots = robotsTxt(siteUrl);
  assert.match(robots, /^User-agent: \*\nAllow: \/\n/);
  assert.match(robots, /Content-Signal: search=yes, ai-input=yes, ai-train=yes/);
  assert.match(robots, /Content-Usage: train-ai=y/);
  assert.equal(robots.includes("Disallow"), false);
  assert.match(robots, /Sitemap: https:\/\/octoboard\.dev\/sitemap\.xml/);
});

// The sitemap comes from @nuxtjs/sitemap, so it is checked on the generated output of a
// default-environment build (production origin, root base path).
const generatedSitemap = new URL("../.output/public/sitemap.xml", import.meta.url);

test(
  "the generated sitemap names every language once and links its stylesheet",
  {
    skip: !existsSync(generatedSitemap)
      ? "run pnpm --filter @octoboard/web build first"
      : false,
  },
  () => {
    const sitemap = readFileSync(generatedSitemap, "utf8");
    assert.match(sitemap, /<\?xml-stylesheet type="text\/xsl" href="[^"]*\/__sitemap__\/style\.xsl"\?>/);
    assert.equal(sitemap.match(/<url>/g)?.length, locales.length * pagePaths.length);
    assert.match(sitemap, /hreflang="zh-Hans" href="https:\/\/octoboard\.dev\/zh\/"/);
    assert.match(sitemap, /hreflang="x-default" href="https:\/\/octoboard\.dev\/"/);
    assert.equal(sitemap.includes("/zh-Hans/"), false);
    assert.equal(sitemap.includes("/404"), false);
  },
);

test("each locale has a 1200 by 630 share image", () => {
  for (const locale of locales) {
    const bytes = readFileSync(
      new URL(`../public/og-image-${locale.code}.png`, import.meta.url),
    );
    assert.equal(bytes.readUInt32BE(16), 1200, locale.code);
    assert.equal(bytes.readUInt32BE(20), 630, locale.code);
  }
});

test("Open Graph locales use a territory, including Simplified Chinese", () => {
  assert.equal(locales.find((locale) => locale.code === "zh")?.ogLocale, "zh_CN");
  assert.equal(locales.find((locale) => locale.code === "en")?.ogLocale, "en_US");
  assert.equal(ogLocaleAlternates("en").includes("en_US"), false);
  assert.equal(ogLocaleAlternates("en").length, locales.length - 1);
  for (const locale of locales) {
    assert.match(locale.ogLocale, /^[a-z]{2}_[A-Z]{2}$/);
  }
});

test("structured data describes the macOS app without claiming it can be installed", () => {
  const homeGraph = buildJsonLd({
    page: "home",
    canonical: "https://octoboard.dev/ja/",
    websiteUrl: "https://octoboard.dev/ja/",
    language: "ja",
    title: "Octoboard",
    description: "Command center",
    siteName: "Octoboard",
    siteDescription: "Command center",
    origin: "https://octoboard.dev",
    siteUrl,
    operatorName: "BaiJunjie",
    contactEmail: "support@octoboard.dev",
    repositoryUrl: "https://github.com/baijunjie/octoboard",
    logoUrl: `${siteUrl}logo.png`,
    licenseUrl: "https://github.com/baijunjie/octoboard/blob/main/LICENSE",
    languages: ["en", "zh-Hans"],
    featureTitles: ["Coordinate"],
    questions: [{ question: "Where?", answer: "Locally." }],
  });
  const home = JSON.stringify(homeGraph);
  const app = (
    homeGraph["@graph"] as Array<Record<string, unknown>>
  ).find((node) => node["@type"] === "SoftwareApplication");
  assert.equal(app?.url, siteUrl);
  assert.equal(Object.hasOwn(app ?? {}, "featureList"), false);
  assert.equal(Object.hasOwn(app ?? {}, "description"), false);
  assert.match(home, /"@type":"FAQPage"/);
  assert.match(home, /"@type":"SoftwareApplication"/);
  assert.match(home, /"operatingSystem":"macOS"/);
  assert.equal(home.includes("offers"), false);
  assert.equal(home.includes("aggregateRating"), false);
  assert.equal(home.includes("installUrl"), false);

  const privacy = JSON.stringify(
    buildJsonLd({
      page: "privacy",
      canonical: `${siteUrl}privacy/`,
      websiteUrl: siteUrl,
      language: "en",
      title: "Privacy",
      description: "Privacy",
      siteName: "Octoboard",
      siteDescription: "Command center",
      origin: "https://octoboard.dev",
      siteUrl,
      operatorName: "BaiJunjie",
      contactEmail: "support@octoboard.dev",
      repositoryUrl: "https://github.com/baijunjie/octoboard",
      logoUrl: `${siteUrl}logo.png`,
      licenseUrl: "https://github.com/baijunjie/octoboard/blob/main/LICENSE",
      languages: ["en"],
      featureTitles: [],
      questions: [],
    }),
  );
  assert.match(privacy, /"@type":"WebPage"/);
  assert.equal(privacy.includes("FAQPage"), false);
  assert.equal(privacy.includes("SoftwareApplication"), false);
});

test("agent documents follow the pages and do not publish a download that does not exist", async () => {
  const messages = await readLocaleMessages(
    fileURLToPath(new URL("../i18n/locales/", import.meta.url)),
  );
  const files = renderAgentFiles({
    siteUrl,
    appName: "Octoboard",
    operatorName: "BaiJunjie",
    contactEmail: "support@octoboard.dev",
    repositoryUrl: "https://github.com/baijunjie/octoboard",
    messages,
  });
  const home = files["index.md"];
  const llms = files["llms.txt"];
  assert.match(home, /Canonical: https:\/\/octoboard\.dev\//);
  assert.match(home, /macOS on Apple Silicon · Early prototype/);
  assert.match(home, /Octoboard is free and open source under the MIT License\./);
  assert.equal(home.includes("Awaiting instructions"), false);
  assert.match(home, /Switch accounts\. Keep the conversation\./);
  assert.match(home, /Do I need to set up my projects and CLI configuration again\?/);
  assert.equal(home.includes("What is Octoboard useful for?"), false);
  for (const locale of locales) {
    const body = files[markdownFile(locale.code === "en" ? "/" : `/${locale.code}/`)];
    const catalog = messages[locale.code];
    const headings = [
      "continuityTitle",
      "accountFeatureTitle",
      "handoffFeatureTitle",
      "usageFeatureTitle",
      "previewTitle",
      "coordinationTitle",
      "developerTitle",
      "developerShiftTitle",
      "developerContextTitle",
      "developerQualityTitle",
      "localTitle",
      "platformsTitle",
      "faqTitle",
    ].map((key) => catalog[key].replaceAll("\n", " "));
    let previous = -1;
    for (const title of headings) {
      const position = body.indexOf(title);
      assert.ok(position > previous, `${locale.code}: missing or reordered ${title}`);
      previous = position;
    }
    for (const key of [
      "accountFeatureDescription", "handoffFeatureDescription", "usageFeatureDescription",
      "developerIntro", "developerShiftBody", "developerContextBody", "developerQualityBody",
      "developerPurpose", "developerInvitation", "developerByline",
    ]) {
      const text = catalog[key]
        .replaceAll("{appName}", "Octoboard")
        .replaceAll("{operatorName}", "BaiJunjie");
      assert.equal(body.split(text).length - 1, 1, `${locale.code}: ${key} must appear once`);
    }
  }
  assert.equal(home.includes("Replay workflow"), false);
  assert.match(files["privacy.md"], /GitHub privacy statement/);
  assert.match(files["ja.md"], /Canonical: https:\/\/octoboard\.dev\/ja\//);
  assert.match(llms, /Support: support@octoboard\.dev/);
  assert.match(llms, /https:\/\/octoboard\.dev\/index\.md/);
  assert.match(llms, /https:\/\/octoboard\.dev\/ja\.md/);
  assert.match(files["sitemap.md"], /https:\/\/octoboard\.dev\/zh\/privacy\.md/);
  assert.match(
    files["llms-full.txt"],
    /\*\*Source:\*\* https:\/\/octoboard\.dev\/privacy\//,
  );
  for (const [name, body] of Object.entries(files)) {
    assert.equal(body.includes("{"), false, name);
  }
});
