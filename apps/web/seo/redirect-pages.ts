// The site's redirect pages: static pages at fixed addresses that send the visitor straight on to another address,
// with a plain link behind the redirect. They are not routes, so they stay out of the sitemap and the language
// variants; like the 404 page they are not indexed, and their few sentences are English only.

import { remoteGuidePage } from "./page-outline.ts";
import { docsPath, locales, localizedPath, pagePaths, retiredLocaleCodes } from "./site-seo.ts";

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

// A redirect page to `url`, whose link reads `title` below `notice`. With `carry`, the script sends the visitor's query
// and fragment along, for a page that stands in for another page of the site; the meta refresh and the link stay the
// fixed address.
export function redirectPage(input: {
  url: string;
  title: string;
  notice: string;
  baseURL: string;
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
<link rel="icon" type="image/png" href="${escapeHtml(input.baseURL)}favicon.png">
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

// The former remote connections page of each language, whose content moved to the docs: a redirect to the docs' remote
// connections guide in the same language, keyed by its path in the generated output.
// The page was also published under the retired Chinese codes (`retiredLocaleCodes`), whose addresses go straight to
// the guide in the language the code moved to.
export function renderRemoteRedirectPages(baseURL: string): Record<string, string> {
  const codes: [string, string][] = [
    ...locales.map(({ code }): [string, string] => [code, code]),
    ...Object.entries(retiredLocaleCodes),
  ];
  return Object.fromEntries(
    codes.map(([code, current]) => [
      `${localizedPath(code, "remote/")}index.html`,
      redirectPage({
        url: docsPath(baseURL, current, remoteGuidePage),
        title: "Remote connections guide",
        notice: "The remote connections page has moved to the documentation. If nothing happens, use the link below.",
        baseURL,
      }),
    ]),
  );
}

// The English names the retired-locale pages use for their link, since the page itself is English.
const RETIRED_LANGUAGE_NAMES: Record<string, string> = { "zh-CN": "Simplified Chinese", "zh-TW": "Traditional Chinese" };

// Every page published under a retired Chinese code (`retiredLocaleCodes`): a redirect to the same page under the code
// it moved to, keyed by its path in the generated output. The former remote connections page is not among them, as
// `renderRemoteRedirectPages` sends its addresses to the docs.
export function renderRetiredLocalePages(baseURL: string): Record<string, string> {
  return Object.fromEntries(
    Object.entries(retiredLocaleCodes).flatMap(([retired, current]) =>
      pagePaths.map((path) => [
        `${localizedPath(retired, path)}index.html`,
        redirectPage({
          url: `${baseURL}${localizedPath(current, path)}`,
          title: RETIRED_LANGUAGE_NAMES[current] ?? current,
          notice: "This page has moved. If nothing happens, use the link below.",
          baseURL,
          carry: true,
        }),
      ]),
    ),
  );
}
