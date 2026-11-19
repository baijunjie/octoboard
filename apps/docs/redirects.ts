// The docs' pages under the paths the two Chinese languages had before their path segments became their tags: a static
// redirect page at each one's former address, written once the site is built, so a link or a search result that
// still names a former address arrives at the same page. The website keeps the same table, `retiredLocaleCodes` in
// `apps/web/seo/site-seo.ts`, and writes the same page for its own former addresses (`apps/web/seo/redirect-pages.ts`);
// `webParity.test.ts` fails when the copies drift.
import { mkdir, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import type { AstroIntegration } from "astro";

// Each former path segment, and the language tag it moved to.
export const RETIRED_SEGMENTS: Readonly<Record<string, string>> = { zh: "zh-CN", "zh-Hant": "zh-TW" };

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

// The website's redirect page (`redirectPage` in `apps/web/seo/redirect-pages.ts`): unlocalized, not indexed, an
// immediate redirect with a plain link behind it. With `carry`, the script sends the visitor's query and fragment
// along; the meta refresh and the link stay the fixed address.
export function redirectPage(input: {
  url: string;
  title: string;
  notice: string;
  favicon: string;
  carry?: boolean;
}): string {
  const url = escapeHtml(input.url);
  const target = JSON.stringify(input.url).replaceAll("<", "\\u003c");
  const script = input.carry ? `${target} + location.search + location.hash` : target;
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(input.title)}</title>
<meta name="robots" content="noindex, nofollow">
<meta http-equiv="refresh" content="0; url=${url}">
<link rel="icon" type="image/png" href="${escapeHtml(input.favicon)}">
<style>
body { margin: 0; min-height: 100vh; display: grid; place-content: center; gap: 16px; padding: 24px; background: #000; color: #f5f5f5; font: 16px/1.6 system-ui, sans-serif; text-align: center; }
a { color: #f5f5f5; }
</style>
<script>location.replace(${script});</script>
</head>
<body>
<p>${escapeHtml(input.notice)}</p>
<p><a href="${url}">${escapeHtml(input.title)}</a></p>
</body>
</html>
`;
}

// The redirect pages for the built pages `pathnames` (relative to the docs' base, `base`, which ends in a slash), keyed
// by their path in the output: one at the former address of each page under a language that moved, which carries the
// visitor's query and fragment on to the page. `favicon` is the address of the site icon.
export function retiredSegmentPages(pathnames: readonly string[], base: string, favicon: string): Record<string, string> {
  const pages: Record<string, string> = {};
  for (const pathname of pathnames) {
    const [segment, ...rest] = pathname.replace(/^\/+/, "").split("/");
    const retired = Object.keys(RETIRED_SEGMENTS).find((key) => RETIRED_SEGMENTS[key] === segment);
    if (!retired) continue;
    const path = rest.join("/");
    pages[`${retired}/${path}${path === "" || path.endsWith("/") ? "" : "/"}index.html`] = redirectPage({
      url: `${base}${pathname.replace(/^\/+/, "")}`,
      title: "Documentation",
      notice: "This page has moved. If nothing happens, use the link below.",
      favicon,
      carry: true,
    });
  }
  return pages;
}

export function retiredSegmentRedirects(base: string, favicon: string): AstroIntegration {
  return {
    name: "octoboard-retired-segment-redirects",
    hooks: {
      "astro:build:done": async ({ dir, pages }) => {
        const written = retiredSegmentPages(pages.map(({ pathname }) => pathname), base, favicon);
        await Promise.all(
          Object.entries(written).map(async ([path, html]) => {
            const file = fileURLToPath(new URL(path, dir));
            await mkdir(dirname(file), { recursive: true });
            await writeFile(file, html);
          }),
        );
      },
    },
  };
}
