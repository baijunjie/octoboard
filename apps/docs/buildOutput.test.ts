import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import test from "node:test";
import { LANGUAGES, textDirection } from "@octoboard/ui/languages";
import site from "./site.config.ts";
import { docsTitles, docsWords } from "./siteTitle.ts";

// The built site (`pnpm --filter @octoboard/docs build`, which may use any deployment origin and base path; CI runs it
// on the Pages build), where the pieces the configuration wires together show: the locale table (`locales.ts`), the
// page ids that keep a locale directory's case (`src/content.config.ts`), the 404 page's language alternates
// (`src/routeData.ts`), the redirect pages at the retired Chinese addresses (`redirects.ts`) and the localized site
// title (`siteTitle.ts`, `src/components/SiteTitle.astro`). None of these runs
// outside the Astro build, so each test skips itself until a build exists.

const dist = new URL("./dist/", import.meta.url);
const skip = existsSync(new URL("index.html", dist)) ? false : "run pnpm --filter @octoboard/docs build first";
const read = (file: string) => readFileSync(new URL(file, dist), "utf8");
const directories = (path = "") => readdirSync(new URL(path, dist), { withFileTypes: true }).filter((entry) => entry.isDirectory()).map((entry) => entry.name);
const pagesUnder = (code: string): string[] =>
  (readdirSync(new URL(`${code}/`, dist), { recursive: true }) as string[])
    .filter((file) => file.endsWith("index.html"))
    .map((file) => file.slice(0, -"index.html".length));

test("every language has its home page under its tag, with its own lang and reading direction, and English is the root", { skip }, () => {
  for (const language of LANGUAGES) {
    const page = read(language === "en" ? "index.html" : `${language}/index.html`);
    const html = /<html lang="([^"]*)" dir="([^"]*)"/.exec(page);
    assert.deepEqual(html?.slice(1), [language, textDirection(language)], language);
  }
});

test("the Chinese pages sit under zh-CN and zh-TW in those cases, not lower-cased and not taken for English ones", { skip }, () => {
  const names = directories();
  for (const tag of ["zh-CN", "zh-TW"]) {
    assert.ok(names.includes(tag), tag);
    assert.equal(names.includes(tag.toLowerCase()), false, tag.toLowerCase());
  }
  for (const [tag, other] of [["zh-CN", "zh-TW"], ["zh-TW", "zh-CN"]]) {
    for (const page of ["install/linux/", "install/macos/", "install/windows/", "remote-connections/"]) {
      const html = read(`${tag}/${page}index.html`);
      assert.match(html, new RegExp(`<html lang="${tag}"`), `${tag}/${page}`);
      assert.match(html, new RegExp(`<link rel="canonical" href="[^"]*/docs/${tag}/${page}"`), `${tag}/${page}`);
      assert.ok(html.includes(`hreflang="${other}"`), `${tag}/${page}`);
    }
  }
});

test("a page has a language alternate for every language, and the 404 page, built once at the root, has none", { skip }, () => {
  const alternates = (file: string) => [...read(file).matchAll(/<link rel="alternate" hreflang="([^"]*)"/g)].map((match) => match[1]);
  assert.deepEqual(alternates("install/linux/index.html")?.slice().sort(), [...LANGUAGES, "x-default"].sort());
  assert.deepEqual(alternates("404.html"), []);
  assert.match(read("404.html"), /<html lang="en"/);
});

test("every page of the two Chinese languages has a redirect page at its retired address, and only they do", { skip }, () => {
  for (const [retired, tag] of [["zh", "zh-CN"], ["zh-Hant", "zh-TW"]]) {
    const pages = pagesUnder(tag);
    assert.ok(pages.length >= 5, `${tag} has its pages`);
    assert.deepEqual(pagesUnder(retired).sort(), pages.sort(), retired);
    for (const page of pages) {
      const redirect = read(`${retired}/${page}index.html`);
      // The website's icon sits at the website's base, one level above the docs' own.
      const websiteBase = /<link rel="icon" type="image\/png" href="([^"]*)favicon\.png">/.exec(redirect)?.[1];
      assert.ok(websiteBase?.endsWith("/"), `${retired}/${page}`);
      assert.ok(redirect.includes(`<meta http-equiv="refresh" content="0; url=${websiteBase}docs/${tag}/${page}">`), `${retired}/${page}`);
      assert.match(redirect, /<meta name="robots" content="noindex, nofollow">/);
    }
  }
  assert.deepEqual(directories().filter((name) => /^zh/i.test(name)).sort(), ["zh", "zh-CN", "zh-Hant", "zh-TW"]);
  const sitemap = read("sitemap-0.xml");
  assert.equal(/\/docs\/zh\/|\/docs\/zh-Hant\//.test(sitemap), false);
  assert.ok(sitemap.includes("/docs/zh-CN/") && sitemap.includes("/docs/zh-TW/"));
});

test("the site map index the website's robots.txt names is built", { skip }, () => {
  assert.match(read("sitemap-index.xml"), /<loc>[^<]*\/docs\/sitemap-0\.xml<\/loc>/);
});

const unescapeHtml = (text: string) =>
  text.replace(/&(#x[0-9a-f]+|#\d+|amp|lt|gt|quot|apos);/gi, (_, entity: string) => {
    if (entity.startsWith("#x")) return String.fromCodePoint(parseInt(entity.slice(2), 16));
    if (entity.startsWith("#")) return String.fromCodePoint(Number(entity.slice(1)));
    return { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" }[entity.toLowerCase()] ?? entity;
  });
const attribute = (tag: string, name: string) => {
  const match = new RegExp(`\\s${name}(?:="([^"]*)")?(?=[\\s/>])`).exec(tag);
  return match === null ? undefined : unescapeHtml(match[1] ?? "");
};

test("every language's home page is titled and drawn in its own words, with the product name filled in", { skip }, () => {
  for (const language of LANGUAGES) {
    const page = read(language === "en" ? "index.html" : `${language}/index.html`);
    const title = unescapeHtml(/<title>([^<]*)<\/title>/.exec(page)?.[1] ?? "");
    const siteTitle = docsTitles[language].replace("{name}", site.productName);
    assert.ok(title.endsWith(siteTitle), `${language}: <title> "${title}" ends with "${siteTitle}"`);
    const siteName = [...page.matchAll(/<meta\b[^>]*>/g)].map((match) => match[0]).find((tag) => attribute(tag, "property") === "og:site_name");
    assert.equal(siteName === undefined ? undefined : attribute(siteName, "content"), siteTitle, `${language}: og:site_name`);

    const link = [...page.matchAll(/<a\b[^>]*>([\s\S]*?)<\/a>/g)].find((match) => attribute(match[0], "class")?.split(/\s+/).includes("site-title"));
    assert.ok(link, `${language}: the header's title link`);
    assert.equal(attribute(link[0], "aria-label"), undefined, `${language}: the title link has no aria-label`);
    const word = [...link[1].matchAll(/<span\b[^>]*>([\s\S]*?)<\/span>/g)].find((match) => attribute(match[0], "class")?.split(/\s+/).includes("word"));
    assert.ok(word, `${language}: the word element`);
    assert.ok(unescapeHtml(word[1]).includes(docsWords[language]), `${language}: the word "${word[1]}" holds "${docsWords[language]}"`);
    const wordmark = [...link[1].matchAll(/<img\b[^>]*>/g)].find((match) => attribute(match[0], "class")?.split(/\s+/).includes("wordmark"));
    assert.ok(wordmark, `${language}: the wordmark image`);
    assert.equal(attribute(wordmark[0], "translate"), "no", `${language}: the wordmark is not translated`);
    assert.equal(attribute(wordmark[0], "alt"), site.productName, `${language}: the wordmark's alt text`);
  }
});
