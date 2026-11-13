import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { LANGUAGES } from "@octoboard/ui/languages";
import { formatPrivacyDate } from "../seo/privacy-date.ts";
import { featureGroups, homeFeatures } from "../seo/page-outline.ts";
import { readLocaleMessages, renderAgentFiles } from "../seo/agent-documents.ts";
import type { Downloads } from "../seo/downloads.ts";
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
const appConfig = JSON.parse(readFileSync(new URL("../../../config/app.json", import.meta.url), "utf8"));
const port = appConfig.defaultServerPort;
const repositoryUrl = "https://github.com/baijunjie/octoboard";
const releaseBase = `${repositoryUrl}/releases/download`;
const downloads: Downloads = {
  macos: `${releaseBase}/desktop-v1.2.3/Octoboard_1.2.3_universal.dmg`,
  windows: `${releaseBase}/desktop-v1.2.3/Octoboard_1.2.3_x64-setup.exe`,
  "linux-x86_64": `${releaseBase}/desktop-v1.2.3/octoboardd_1.2.3_linux_x86_64.tar.gz`,
  "linux-aarch64": `${releaseBase}/desktop-v1.2.3/octoboardd_1.2.3_linux_aarch64.tar.gz`,
};

test("page paths keep the English home unprefixed and the other pages as sibling files", () => {
  assert.equal(pageKind("/"), "home");
  assert.equal(pageKind("/privacy/"), "privacy");
  assert.equal(pageKind("/zh-Hant/privacy/"), "privacy");
  assert.equal(pageKind("/download/"), "download");
  assert.equal(pageKind("/zh/download/"), "download");
  assert.equal(pageKind("/download/macos/"), "other");
  assert.equal(pageKind("/404/"), "other");
  assert.equal(pageKind("/pr/"), "other");
  assert.equal(pageKind("/id/"), "home");
  assert.equal(localeCodeFromPath("/privacy/"), "en");
  assert.equal(localeCodeFromPath("/id/"), "id");
  assert.equal(markdownFile("/"), "index.md");
  assert.equal(markdownFile("/privacy/"), "privacy.md");
  assert.equal(markdownFile("/zh/"), "zh.md");
  assert.equal(markdownFile("/zh-Hant/privacy/"), "zh-Hant/privacy.md");
  assert.equal(markdownFile("/download/"), "download.md");
  assert.equal(markdownFile("/zh/download/"), "zh/download.md");
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
    assert.match(sitemap, /<loc>https:\/\/octoboard\.dev\/download\/<\/loc>/);
    assert.match(sitemap, /<loc>https:\/\/octoboard\.dev\/zh\/download\/<\/loc>/);
    for (const target of ["macos", "windows", "linux-x86_64", "linux-aarch64"]) {
      assert.equal(sitemap.includes(`/download/${target}/`), false);
    }
    assert.equal(sitemap.includes("install.sh"), false);
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

test("the site offers the application's languages, in its order, with Simplified Chinese at /zh/", () => {
  assert.deepEqual(locales.map((locale) => locale.language), [...LANGUAGES]);
  assert.deepEqual(locales.filter((locale) => locale.dir).map((locale) => locale.code), ["ar"]);
  const chinese = locales.find((locale) => locale.language === "zh-Hans");
  assert.equal(chinese?.code, "zh");
  assert.equal(chinese?.ogLocale, "zh_CN");
  assert.equal(chinese?.file, "zh-Hans.json");
  assert.equal(chinese?.name, "简体中文");
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

test("structured data points at the fixed download addresses without claiming an offer", () => {
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
    operatorName: appConfig.author.name,
    contactEmail: "support@octoboard.dev",
    repositoryUrl: "https://github.com/baijunjie/octoboard",
    logoUrl: `${siteUrl}logo.png`,
    licenseUrl: "https://github.com/baijunjie/octoboard/blob/main/LICENSE",
    downloadUrls: [`${siteUrl}download/macos/`, `${siteUrl}download/linux-x86_64/`],
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
  assert.match(home, /"operatingSystem":"macOS, Windows, Linux"/);
  assert.deepEqual(app?.downloadUrl, [`${siteUrl}download/macos/`, `${siteUrl}download/linux-x86_64/`]);
  assert.equal(home.includes("offers"), false);
  assert.equal(home.includes("aggregateRating"), false);
  assert.equal(home.includes("installUrl"), false);

  const downloadPage = JSON.stringify(
    buildJsonLd({
      page: "download",
      canonical: `${siteUrl}download/`,
      websiteUrl: siteUrl,
      language: "en",
      title: "Download",
      description: "Download",
      siteName: "Octoboard",
      siteDescription: "Command center",
      origin: "https://octoboard.dev",
      siteUrl,
      operatorName: appConfig.author.name,
      contactEmail: "support@octoboard.dev",
      repositoryUrl: "https://github.com/baijunjie/octoboard",
      logoUrl: `${siteUrl}logo.png`,
      licenseUrl: "https://github.com/baijunjie/octoboard/blob/main/LICENSE",
      downloadUrls: [`${siteUrl}download/windows/`],
      languages: ["en"],
      featureTitles: [],
      questions: [],
    }),
  );
  assert.match(downloadPage, /"@type":"WebPage"/);
  assert.match(downloadPage, /"@type":"SoftwareApplication"/);
  assert.match(downloadPage, /"downloadUrl":\["https:\/\/octoboard\.dev\/download\/windows\/"\]/);
  assert.equal(downloadPage.includes("FAQPage"), false);

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
      operatorName: appConfig.author.name,
      contactEmail: "support@octoboard.dev",
      repositoryUrl: "https://github.com/baijunjie/octoboard",
      logoUrl: `${siteUrl}logo.png`,
      licenseUrl: "https://github.com/baijunjie/octoboard/blob/main/LICENSE",
      downloadUrls: [],
      languages: ["en"],
      featureTitles: [],
      questions: [],
    }),
  );
  assert.match(privacy, /"@type":"WebPage"/);
  assert.equal(privacy.includes("FAQPage"), false);
  assert.equal(privacy.includes("SoftwareApplication"), false);
});

test("agent documents follow the pages and carry the download and install information", async () => {
  const messages = await readLocaleMessages(
    fileURLToPath(new URL("../i18n/locales/", import.meta.url)),
  );
  const files = renderAgentFiles({
    siteUrl,
    appName: "Octoboard",
    operatorName: appConfig.author.name,
    contactEmail: "support@octoboard.dev",
    repositoryUrl,
    downloads,
    messages,
  });
  const home = files["index.md"];
  const llms = files["llms.txt"];
  assert.match(home, /Canonical: https:\/\/octoboard\.dev\//);
  assert.match(home, /macOS on Apple silicon and Intel · Windows on x64 · Linux server on x86_64 and aarch64 · Beta/);
  assert.match(home, /- macOS: Beta/);
  assert.match(home, /- Windows: Beta/);
  assert.match(home, /- Linux: Server · Beta/);
  assert.equal(home.includes("Windows: Planned"), false);
  assert.ok(home.includes("[Download](https://octoboard.dev/download.md)"));
  const page = files["download.md"];
  assert.match(page, /Canonical: https:\/\/octoboard\.dev\/download\//);
  assert.ok(page.includes(`[Download for macOS](${downloads.macos})`));
  assert.ok(page.includes(`[Download for Windows](${downloads.windows})`));
  assert.ok(page.includes("curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh"));
  assert.ok(page.includes("`sh -s -- --service`"));
  assert.ok(page.includes("`octoboardd start`"));
  assert.ok(
    page.includes(`\`ssh -o ExitOnForwardFailure=yes -L 127.0.0.1:${port}:/home/<you>/.octoboard/browser.sock <you>@<server>\``),
  );
  assert.ok(page.includes('`loginctl enable-linger "$USER"`'));
  // The script turns lingering on itself; the command is for when it says it could not.
  assert.match(page, /The script then turns on lingering[^\n]*if it says it could not, run `loginctl enable-linger "\$USER"` once/);
  // octoboardd may not be on the PATH right after the install.
  assert.ok(page.includes("If the install says `~/.local/bin` is not on your PATH, run `octoboardd start` by the full path it prints"));
  assert.ok(page.includes("`octoboardd restart`"));
  assert.equal(page.includes("your-server"), false);
  assert.ok(page.includes(`[x86_64 archive](${downloads["linux-x86_64"]})`));
  assert.ok(page.includes(`[aarch64 archive](${downloads["linux-aarch64"]})`));
  assert.ok(page.includes("[All releases](https://github.com/baijunjie/octoboard/releases)"));
  assert.match(page, /macOS 11 or later/);
  assert.match(page, /Select “More info”, then “Run anyway”/);
  assert.ok(page.includes("loginctl enable-linger"));
  assert.match(home, /Requirements and installation steps for each system are on the download page\./);
  assert.equal(home.includes("x64-setup.exe"), false);
  assert.match(page, /installs Octoboard for your user account only/);
  assert.match(page, /Claude Code and Codex also need Git for Windows/);
  assert.match(page, /Install the coding-agent CLIs you want to use yourself/);
  assert.match(home, /The app is free and open source under the MIT License\./);
  assert.equal(home.includes("Awaiting instructions"), false);
  assert.match(home, /Switch accounts\. Continue the task\./);
  assert.match(home, /Do I need to set up my projects and CLI configuration again\?/);
  assert.equal(home.includes("What is Octoboard useful for?"), false);
  for (const locale of locales) {
    const body = files[markdownFile(locale.code === "en" ? "/" : `/${locale.code}/`)];
    const catalog = messages[locale.code];
    for (const key of ["faqProjectIsolationAnswer", "faqAgentChoiceAnswer"]) {
      const answer = catalog[key].replaceAll("{appName}", "Octoboard");
      assert.ok(body.includes(answer), `${locale.code}: current FAQ answer`);
    }
    const privacy = files[markdownFile(locale.code === "en" ? "/privacy/" : `/${locale.code}/privacy/`)];
    assert.ok(privacy.includes(formatPrivacyDate(locale.language)));
    assert.ok(privacy.includes(catalog.privacyWebsiteConsentBody));
    const headings = [
      "featureWallTitle",
      ...featureGroups.flatMap((group) => [group.key, ...group.features.map((key) => `${key}FeatureTitle`)]),
      "previewTitle",
      "coordinationGoalTitle",
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
      "featureWallDescription", ...homeFeatures.map(({ key }) => `${key}FeatureDescription`),
      "developerIntro", "developerShiftBody", "developerContextBody", "developerQualityBody",
      "developerPurpose", "developerInvitation", "developerByline",
    ]) {
      const text = catalog[key]
        .replaceAll("{appName}", "Octoboard")
        .replaceAll("{operatorName}", appConfig.author.name);
      assert.equal(body.split(text).length - 1, 1, `${locale.code}: ${key} must appear once`);
    }
  }
  assert.equal(home.includes("Replay workflow"), false);
  assert.match(files["privacy.md"], /GitHub privacy statement/);
  assert.match(files["ja.md"], /Canonical: https:\/\/octoboard\.dev\/ja\//);
  assert.match(llms, /Support: support@octoboard\.dev/);
  assert.match(llms, /https:\/\/octoboard\.dev\/index\.md/);
  assert.match(llms, /https:\/\/octoboard\.dev\/ja\.md/);
  assert.match(llms, /macOS on Apple silicon and Intel · Windows on x64 · Linux server on x86_64 and aarch64 · Beta/);
  assert.match(llms, /macOS: Beta/);
  assert.match(llms, /Windows: Beta/);
  assert.match(llms, /Requirements and installation steps for each system are on the download page\./);
  assert.match(llms, /Linux: Server · Beta/);
  assert.ok(llms.includes("curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh"));
  assert.ok(llms.includes(downloads["linux-aarch64"]));
  assert.equal(llms.includes("Windows: Planned"), false);
  assert.match(llms, /iOS: Planned/);
  assert.match(llms, /Android: Planned/);
  assert.match(llms, /\[All releases\]\(https:\/\/github\.com\/baijunjie\/octoboard\/releases\)/);
  assert.match(llms, /\[all languages\]\(https:\/\/octoboard\.dev\/llms-full\.txt\)/);
  assert.match(files["sitemap.md"], /https:\/\/octoboard\.dev\/zh\/privacy\.md/);
  const sections = files["sitemap.md"].match(/^## .+$/gm) ?? [];
  assert.deepEqual(sections, ["## Root", ...locales.filter((locale) => locale.code !== "en").map((locale) => `## ${locale.code}`).sort()]);
  assert.match(files["sitemap.md"], /## Root\n- \[[^\n]+\]\(https:\/\/octoboard\.dev\/index\.md\)\n- \[[^\n]+\]\(https:\/\/octoboard\.dev\/download\.md\)\n- \[[^\n]+\]\(https:\/\/octoboard\.dev\/privacy\.md\)/);
  assert.match(
    files["llms-full.txt"],
    /\*\*Source:\*\* https:\/\/octoboard\.dev\/privacy\//,
  );
  for (const [name, body] of Object.entries(files)) {
    assert.equal(body.includes("{"), false, name);
  }
});

test("agent entry links resolve and language bundles contain only their own pages", async () => {
  const messages = await readLocaleMessages(
    fileURLToPath(new URL("../i18n/locales/", import.meta.url)),
  );
  for (const siteUrl of ["https://octoboard.dev/", "https://example.com/product/"]) {
    const files = renderAgentFiles({
      siteUrl,
      appName: "Octoboard",
      operatorName: appConfig.author.name,
      contactEmail: "support@octoboard.dev",
      repositoryUrl,
      downloads,
      messages,
    });
    assert.equal((files["llms-full.txt"].match(/\*\*Source:\*\*/g) ?? []).length, locales.length * 3);
    for (const locale of locales) {
      const file = `llms-full-${locale.code}.txt`;
      assert.ok(files["llms.txt"].includes(`${siteUrl}${file}`), file);
      const bundle = files[file];
      assert.equal((bundle.match(/\*\*Source:\*\*/g) ?? []).length, 3, file);
      const prefix = locale.code === "en" ? "" : `${locale.code}/`;
      for (const path of ["", "privacy/", "download/"]) {
        assert.ok(bundle.includes(`**Source:** ${siteUrl}${prefix}${path}\n`), file);
        assert.ok(bundle.includes(files[markdownFile(`/${prefix}${path}`)].trim()), file);
      }
      const catalog = messages[locale.code];
      assert.ok(bundle.includes(catalog.workspaceSeoSummary), file);
      for (const name of ["Claude Code", "Codex"]) {
        assert.ok(catalog.codingAgentsSeoTitle.includes(name), `${locale.code}: ${name}`);
      }
      for (const name of ["Claude Code", "Codex", "Grok Build", "macOS", "Windows"]) {
        assert.ok(catalog.workspaceSeoSummary.includes(name), `${locale.code}: ${name}`);
      }
    }
    for (const [file, body] of Object.entries(files)) {
      for (const match of body.matchAll(/\]\((https:\/\/[^)]+\.(?:md|txt))\)/g)) {
        const href = match[1];
        if (href.startsWith(siteUrl)) {
          assert.ok(Object.hasOwn(files, href.slice(siteUrl.length)), `${file}: broken link ${href}`);
        }
      }
    }
  }
});

test("the policy revision date stays on November 13 across time zones", () => {
  const previous = process.env.TZ;
  try {
    for (const zone of ["Pacific/Honolulu", "Asia/Tokyo", "UTC"]) {
      process.env.TZ = zone;
      assert.equal(formatPrivacyDate("en"), "November 13, 2026");
      assert.equal(formatPrivacyDate("zh-Hans"), "2026年11月13日");
    }
  } finally {
    if (previous === undefined) delete process.env.TZ;
    else process.env.TZ = previous;
  }
});
