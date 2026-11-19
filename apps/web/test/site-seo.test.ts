import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { LANGUAGES } from "@octoboard/ui/languages";
import { formatPrivacyDate } from "../seo/privacy-date.ts";
import { featureGroups, homeFeatures, remoteGuidePage } from "../seo/page-outline.ts";
import { renderRemoteRedirectPages, renderRetiredLocalePages } from "../seo/redirect-pages.ts";
import { readLocaleMessages, renderAgentFiles } from "../seo/agent-documents.ts";
import { downloadTabs, fallbackDownloads, type Downloads } from "../seo/downloads.ts";
import {
  buildJsonLd,
  isIndexedPage,
  localeCodeFromPath,
  locales,
  currentLocaleCode,
  retiredLocaleCodes,
  localizedPath,
  markdownFile,
  ogLocaleAlternates,
  pageKind,
  pagePaths,
  docsPages,
  docsPath,
  robotsTxt,
} from "../seo/site-seo.ts";

const siteUrl = "https://octoboard.dev/";
const appConfig = JSON.parse(readFileSync(new URL("../../../config/app.json", import.meta.url), "utf8"));
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
  assert.equal(pageKind("/zh-TW/privacy/"), "privacy");
  assert.equal(pageKind("/download/"), "download");
  assert.equal(pageKind("/zh-CN/download/"), "download");
  assert.equal(pageKind("/zh/download/"), "other");
  assert.equal(pageKind("/remote/"), "other");
  assert.equal(pageKind("/download/macos/"), "other");
  assert.equal(pageKind("/404/"), "other");
  assert.equal(pageKind("/pr/"), "other");
  assert.equal(pageKind("/id/"), "home");
  assert.equal(localeCodeFromPath("/privacy/"), "en");
  assert.equal(localeCodeFromPath("/id/"), "id");
  assert.equal(markdownFile("/"), "index.md");
  assert.equal(markdownFile("/privacy/"), "privacy.md");
  assert.equal(markdownFile("/zh-CN/"), "zh-CN.md");
  assert.equal(markdownFile("/zh-TW/privacy/"), "zh-TW/privacy.md");
  assert.equal(markdownFile("/download/"), "download.md");
  assert.equal(markdownFile("/zh-CN/download/"), "zh-CN/download.md");
  assert.throws(() => markdownFile("/remote/"));
});

test("the feature wall groups keep distinct capabilities in reading order", () => {
  assert.deepEqual(featureGroups.map(({ features }) => features), [
    ["managedAccounts", "accountContinuity", "quotaScheduling", "usageMonitor", "startUsageWindows", "sharedConfiguration"],
    ["freshSession", "agentHandoff", "openAgentApp"],
    ["crossProjectWork", "nativeProjectSessions", "sessionIntervention", "processMonitoring", "keepAwake", "reviewChanges"],
  ]);
});

test("the docs link keeps the language's code, with English unprefixed", () => {
  assert.equal(docsPath("/", "en"), "/docs/");
  assert.equal(docsPath("/", "zh-CN"), "/docs/zh-CN/");
  assert.equal(docsPath("/octoboard/", "zh-TW"), "/octoboard/docs/zh-TW/");
  assert.equal(docsPath("/", "en", docsPages.linux), "/docs/install/linux/");
  assert.equal(docsPath("/octoboard/", "zh-CN", docsPages.remoteConnections), "/octoboard/docs/zh-CN/remote-connections/");
  assert.equal(docsPath(siteUrl, "ar", docsPages.macos), "https://octoboard.dev/docs/ar/install/macos/");
  assert.equal(docsPath("/", "ja", docsPages.chooseHowToConnect), "/docs/ja/choose-how-to-connect/");
});

test("each download tab and the remote guide link to a page the docs have in every language", () => {
  assert.deepEqual(downloadTabs.map((tab) => [tab.id, tab.docsPage]), [
    ["macos", "install/macos/"],
    ["windows", "install/windows/"],
    ["linux", "install/linux/"],
  ]);
  assert.equal(remoteGuidePage, "remote-connections/");
  const content = new URL("../../docs/src/content/docs/", import.meta.url);
  for (const locale of locales) {
    for (const page of Object.values(docsPages)) {
      const stem = `${locale.code === "en" ? "" : `${locale.code}/`}${page.replace(/\/$/, "")}`;
      const found = [".md", ".mdx"].some((extension) => existsSync(new URL(`${stem}${extension}`, content)));
      assert.ok(found, `${locale.code}: the docs have no ${page}`);
    }
  }
});

test("robots allows search, retrieval, and training", () => {
  const robots = robotsTxt(siteUrl);
  assert.match(robots, /^User-agent: \*\nAllow: \/\n/);
  assert.match(robots, /Content-Signal: search=yes, ai-input=yes, ai-train=yes/);
  assert.match(robots, /Content-Usage: train-ai=y/);
  assert.equal(robots.includes("Disallow"), false);
  assert.match(robots, /Sitemap: https:\/\/octoboard\.dev\/sitemap\.xml/);
  assert.match(robots, /Sitemap: https:\/\/octoboard\.dev\/docs\/sitemap-index\.xml/);
});

test("every page published under a retired Chinese code redirects to the same page under the language's tag", () => {
  const pages = renderRetiredLocalePages("/site/");
  const expected: Record<string, string> = {
    "zh/index.html": "/site/zh-CN/",
    "zh/privacy/index.html": "/site/zh-CN/privacy/",
    "zh/download/index.html": "/site/zh-CN/download/",
    "zh-Hant/index.html": "/site/zh-TW/",
    "zh-Hant/privacy/index.html": "/site/zh-TW/privacy/",
    "zh-Hant/download/index.html": "/site/zh-TW/download/",
  };
  assert.deepEqual(Object.keys(pages).sort(), Object.keys(expected).sort());
  for (const [path, target] of Object.entries(expected)) {
    assert.ok(pages[path].includes(`<meta http-equiv="refresh" content="0; url=${target}">`), path);
    assert.match(pages[path], /<meta name="robots" content="noindex, nofollow">/);
    // The script carries the query and fragment on to the new address, and the page names the language in English.
    assert.ok(pages[path].includes(`location.replace(${JSON.stringify(target)} + location.search + location.hash)`), path);
    assert.ok(pages[path].includes(`<a href="${target}">`), path);
    assert.equal(/\P{ASCII}/u.test(pages[path]), false, path);
  }
  assert.ok(pages["zh/index.html"].includes(">Simplified Chinese</a>"));
  assert.ok(pages["zh-Hant/index.html"].includes(">Traditional Chinese</a>"));
  assert.equal(currentLocaleCode("zh"), "zh-CN");
  assert.equal(currentLocaleCode("zh-Hant"), "zh-TW");
  assert.equal(currentLocaleCode("ja"), "ja");
});

test("a retired Chinese code maps to a current language's code and is never one itself", () => {
  assert.deepEqual({ ...retiredLocaleCodes }, { zh: "zh-CN", "zh-Hant": "zh-TW" });
  for (const [retired, current] of Object.entries(retiredLocaleCodes)) {
    assert.ok(locales.some((locale) => locale.code === current), current);
    assert.equal(locales.some((locale) => locale.code === retired), false, retired);
    assert.equal(currentLocaleCode(retired), current);
    assert.equal(currentLocaleCode(current), current);
    // A retired address is not a language's page: a path under it is English, and no page kind.
    assert.equal(localeCodeFromPath(`/${retired}/privacy/`), "en");
    assert.equal(pageKind(`/${retired}/privacy/`), "other");
  }
  // A code that is not retired, whether current, unknown or an object's own property name, comes back as it is.
  for (const code of ["en", "zh-CN", "zh-TW", "ja", "zh-Hans", "ZH", "", "constructor", "toString", "__proto__"]) {
    assert.equal(currentLocaleCode(code), code, JSON.stringify(code));
  }
});

test("each language's former remote connections address redirects to the docs guide in that language", () => {
  const pages = renderRemoteRedirectPages("/site/");
  assert.deepEqual(
    Object.keys(pages).sort(),
    [...locales.map(({ code }) => code), "zh", "zh-Hant"].map((code) => localizedPath(code, "remote/index.html")).sort(),
  );
  assert.ok(pages["zh/remote/index.html"].includes('url=/site/docs/zh-CN/remote-connections/"'));
  assert.ok(pages["zh-Hant/remote/index.html"].includes('url=/site/docs/zh-TW/remote-connections/"'));
  for (const { code } of locales) {
    const page = pages[localizedPath(code, "remote/index.html")];
    const target = docsPath("/site/", code, remoteGuidePage);
    assert.ok(page.includes(`<meta http-equiv="refresh" content="0; url=${target}">`), code);
    // The remote guide's fragments are dropped: the sections of the old page no longer exist.
    assert.ok(page.includes(`location.replace(${JSON.stringify(target)})`), code);
    assert.ok(page.includes(`<a href="${target}">`), code);
    assert.match(page, /<meta name="robots" content="noindex, nofollow">/);
    assert.equal(page.includes('rel="canonical"'), false);
  }
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
    // The pages that ask not to be indexed are the ones the sitemap leaves out.
    const indexedPaths = pagePaths.filter((path) => isIndexedPage(pageKind(`/${path}`)));
    assert.equal(sitemap.match(/<url>/g)?.length, locales.length * indexedPaths.length);
    assert.equal(sitemap.includes("/privacy/"), false);
    assert.match(sitemap, /hreflang="zh-CN" href="https:\/\/octoboard\.dev\/zh-CN\/"/);
    assert.match(sitemap, /hreflang="zh-TW" href="https:\/\/octoboard\.dev\/zh-TW\/"/);
    assert.match(sitemap, /hreflang="x-default" href="https:\/\/octoboard\.dev\/"/);
    assert.equal(sitemap.includes("/zh/"), false);
    assert.equal(sitemap.includes("/zh-Hant/"), false);
    assert.equal(sitemap.includes("/404"), false);
    assert.match(sitemap, /<loc>https:\/\/octoboard\.dev\/download\/<\/loc>/);
    assert.match(sitemap, /<loc>https:\/\/octoboard\.dev\/zh-CN\/download\/<\/loc>/);
    assert.equal(sitemap.includes("/remote/"), false);
    for (const target of ["macos", "windows", "linux-x86_64", "linux-aarch64"]) {
      assert.equal(sitemap.includes(`/download/${target}/`), false);
    }
    assert.equal(sitemap.includes("install.sh"), false);
    for (const { code } of locales) {
      const path = localizedPath(code, "remote/index.html");
      const page = readFileSync(new URL(path, generatedSitemap), "utf8");
      assert.ok(page.includes(`url=${docsPath("/", code, remoteGuidePage)}"`), path);
    }
  },
);

test(
  "a page that asks not to be indexed says so in every language, and the other pages keep their canonical and alternates",
  {
    skip: !existsSync(generatedSitemap)
      ? "run pnpm --filter @octoboard/web build first"
      : false,
  },
  () => {
    const read = (path: string) => readFileSync(new URL(path, generatedSitemap), "utf8");
    for (const { code } of locales) {
      for (const path of pagePaths) {
        const kind = pageKind(`/${path}`);
        const file = localizedPath(code, `${path}index.html`);
        const page = read(file);
        if (isIndexedPage(kind)) {
          assert.equal(page.includes("noindex"), false, file);
          assert.match(page, /rel="canonical"/, file);
          assert.match(page, /hreflang="x-default"/, file);
          assert.match(page, /rel="alternate" type="text\/markdown"/, file);
          for (const link of page.match(/<link[^>]*hreflang="[^"]*"[^>]*>/g) ?? []) {
            assert.equal(link.includes("/privacy/"), false, `${file}: ${link}`);
          }
        } else {
          assert.match(page, /<meta name="robots" content="noindex, follow">/, file);
          assert.equal(page.includes('rel="canonical"'), false, file);
          assert.equal(page.includes("hreflang="), false, file);
          assert.equal(page.includes('type="text/markdown"'), false, file);
        }
      }
    }
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

test("the site offers the application's languages, in its order, with each language at its tag", () => {
  assert.deepEqual(locales.map((locale) => locale.language), [...LANGUAGES]);
  assert.deepEqual(locales.filter((locale) => locale.dir).map((locale) => locale.code), ["ar"]);
  const chinese = locales.find((locale) => locale.language === "zh-CN");
  assert.equal(chinese?.code, "zh-CN");
  assert.equal(chinese?.ogLocale, "zh_CN");
  assert.equal(chinese?.file, "zh-CN.json");
  assert.equal(chinese?.name, "简体中文");
  const traditional = locales.find((locale) => locale.language === "zh-TW");
  assert.equal(traditional?.code, "zh-TW");
  assert.equal(traditional?.ogLocale, "zh_TW");
  assert.equal(traditional?.file, "zh-TW.json");
});

test("Open Graph locales use a territory, including Simplified Chinese", () => {
  assert.equal(locales.find((locale) => locale.code === "zh-CN")?.ogLocale, "zh_CN");
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
    languages: ["en", "zh-CN"],
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
  assert.ok(page.includes(`[x86_64 archive](${downloads["linux-x86_64"]})`));
  assert.ok(page.includes(`[aarch64 archive](${downloads["linux-aarch64"]})`));
  assert.ok(page.includes("[All releases](https://github.com/baijunjie/octoboard/releases)"));
  assert.match(page, /macOS 11 or later/);
  // The steps and troubleshooting are in the docs; the download page links to them.
  for (const moved of ["octoboardd start", "enable-linger", "--service", "Run anyway", "for your user account only", "Git for Windows"]) {
    assert.equal(page.includes(moved), false, `the download page no longer carries ${moved}`);
  }
  for (const system of ["macos", "windows", "linux"]) {
    assert.ok(page.includes(`[Read the installation guide](https://octoboard.dev/docs/install/${system}/)`), system);
  }
  assert.equal(home.includes("x64-setup.exe"), false);
  assert.match(home, /The app is free and open source under the MIT License\./);
  assert.equal(home.includes("Awaiting instructions"), false);
  assert.match(home, /Do I need to set up my projects and CLI configuration again\?/);
  assert.equal(home.includes("What is Octoboard useful for?"), false);
  for (const locale of locales) {
    const body = files[markdownFile(locale.code === "en" ? "/" : `/${locale.code}/`)];
    const catalog = messages[locale.code];
    const download = files[markdownFile(locale.code === "en" ? "/download/" : `/${locale.code}/download/`)];
    for (const key of ["downloadMacosRequirements", "downloadWindowsRequirements"]) {
      assert.ok(download.includes(catalog[key]), `${locale.code}: missing requirements: ${key}`);
    }
    const linux = [
      `## ${catalog.downloadLinuxTitle}`,
      `### ${catalog.downloadInstallStep}`,
      "```sh\ncurl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh\n```",
      catalog.downloadArchivesLabel,
      `[${catalog.downloadGuideLink}](${docsPath(siteUrl, locale.code, docsPages.linux)})`,
    ];
    let previousPart = -1;
    for (const part of linux) {
      const position = download.indexOf(part);
      assert.ok(position > previousPart, `${locale.code}: missing or reordered Linux part: ${part}`);
      previousPart = position;
    }
    for (const tab of downloadTabs) {
      const guide = `[${catalog.downloadGuideLink}](${docsPath(siteUrl, locale.code, tab.docsPage)})`;
      assert.ok(download.includes(guide), `${locale.code}: ${tab.id} installation guide`);
    }
    for (const key of ["faqProjectIsolationAnswer", "faqAgentChoiceAnswer"]) {
      const answer = catalog[key].replaceAll("{appName}", "Octoboard");
      assert.ok(body.includes(answer), `${locale.code}: current FAQ answer`);
    }
    const privacy = files[markdownFile(locale.code === "en" ? "/privacy/" : `/${locale.code}/privacy/`)];
    assert.ok(privacy.includes(formatPrivacyDate(locale.language)));
    assert.ok(privacy.includes(catalog.privacyWebsiteConsentBody));
    assert.ok(privacy.includes(catalog.privacyRemoteHostsBody));
    const guide = docsPath(siteUrl, locale.code, remoteGuidePage);
    assert.ok(body.includes(`[${catalog.remoteGuideLink}](${guide})`), locale.code);
    assert.ok(download.includes(`[${catalog.downloadSshGuideLink}](${guide})`), locale.code);
    assert.equal(download.includes(`[${catalog.remoteGuideLink}](`), false, locale.code);
    const headings = [
      "featureWallTitle",
      ...featureGroups.flatMap((group) => [group.key, ...group.features.map((key) => `${key}FeatureTitle`)]),
      "previewTitle",
      "coordinationGoalTitle",
      "remoteTitle",
      "developerTitle",
      "developerShiftTitle",
      "developerContextTitle",
      "developerQualityTitle",
      "localTitle",
      "platformsTitle",
      "faqTitle",
    ].map((key) => catalog[key].replaceAll("\n", " ").replace("{appName}", "Octoboard"));
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
  for (const { key } of homeFeatures) {
    const feature = messages.en[`${key}FeatureDescription`].replaceAll("{appName}", "Octoboard");
    assert.equal(llms.split(feature).length - 1, 1, `${key} appears once in llms.txt`);
  }
  assert.equal(home.includes("Replay workflow"), false);
  assert.match(files["privacy.md"], /GitHub privacy statement/);
  assert.match(files["privacy.md"], /“Analytics settings” in the site footer/);
  assert.equal(Object.keys(files).some((name) => /(^|\/)remote\.md$/.test(name)), false);
  assert.match(files["ja.md"], /Canonical: https:\/\/octoboard\.dev\/ja\//);
  assert.match(llms, /Support: support@octoboard\.dev/);
  assert.match(llms, /https:\/\/octoboard\.dev\/index\.md/);
  assert.match(llms, /https:\/\/octoboard\.dev\/ja\.md/);
  assert.match(llms, /macOS on Apple silicon and Intel · Windows on x64 · Linux server on x86_64 and aarch64 · Beta/);
  assert.match(llms, /macOS: Beta/);
  assert.match(llms, /Windows: Beta/);
  assert.match(llms, /Requirements for each system, and a link to its installation guide, are on the download page\./);
  assert.equal(llms.includes("installation steps for each system are on the download page"), false);
  assert.match(llms, /Linux: Server · Beta/);
  assert.ok(llms.includes("curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh"));
  assert.ok(llms.includes(downloads["linux-aarch64"]));
  assert.ok(llms.includes("[Read the installation guide](https://octoboard.dev/docs/install/linux/)"));
  assert.ok(llms.includes("[Read the SSH connection guide](https://octoboard.dev/docs/remote-connections/)"));
  assert.ok(llms.includes("[Read the installation guide](https://octoboard.dev/docs/install/macos/)"));
  assert.ok(llms.includes("[Read the installation guide](https://octoboard.dev/docs/install/windows/)"));
  // Each app's guide follows its own download link, ahead of the next app's.
  assert.ok(
    llms.includes(
      `(${downloads.macos})\n- [Read the installation guide](https://octoboard.dev/docs/install/macos/)\n- [Download for Windows]`,
    ),
  );
  assert.ok(llms.includes("[Docs](https://octoboard.dev/docs/)"));
  assert.equal(llms.includes("Windows: Planned"), false);
  assert.match(llms, /iOS: Planned/);
  assert.match(llms, /Android: Planned/);
  assert.match(llms, /\[All releases\]\(https:\/\/github\.com\/baijunjie\/octoboard\/releases\)/);
  assert.match(llms, /\[all languages\]\(https:\/\/octoboard\.dev\/llms-full\.txt\)/);
  assert.match(files["sitemap.md"], /https:\/\/octoboard\.dev\/zh-CN\/privacy\.md/);
  assert.equal(files["sitemap.md"].includes("remote.md"), false);
  assert.equal(llms.includes("remote.md"), false);
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
      downloads: siteUrl === "https://octoboard.dev/" ? downloads : fallbackDownloads(repositoryUrl),
      messages,
    });
    const linkTargets = Array.from(files["llms.txt"].matchAll(/\]\((https?:\/\/[^)\s]+)\)/g), ([, href]) => href);
    assert.equal(new Set(linkTargets).size, linkTargets.length, `${siteUrl}: unique llms.txt links, including release fallbacks`);
    assert.equal((files["llms-full.txt"].match(/\*\*Source:\*\*/g) ?? []).length, locales.length * pagePaths.length);
    for (const locale of locales) {
      const file = `llms-full-${locale.code}.txt`;
      assert.ok(files["llms.txt"].includes(`${siteUrl}${file}`), file);
      const bundle = files[file];
      assert.equal((bundle.match(/\*\*Source:\*\*/g) ?? []).length, pagePaths.length, file);
      const prefix = locale.code === "en" ? "" : `${locale.code}/`;
      for (const path of pagePaths) {
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

test("the policy revision date stays on November 18 across time zones", () => {
  const previous = process.env.TZ;
  try {
    for (const zone of ["Pacific/Honolulu", "Asia/Tokyo", "UTC"]) {
      process.env.TZ = zone;
      assert.equal(formatPrivacyDate("en"), "November 18, 2026");
      assert.equal(formatPrivacyDate("zh-CN"), "2026年11月18日");
    }
  } finally {
    if (previous === undefined) delete process.env.TZ;
    else process.env.TZ = previous;
  }
});
