# Website search indexing

## Search indexing and agent-readable copies

Search indexing, live retrieval by agents, and use of the pages for model training are allowed.
`robots.txt` at the deployment root allows the whole site and does not disallow the Markdown copies.
It sets `Content-Usage` to `train-ai=y` and `Content-Signal` to `search=yes, ai-input=yes, ai-train=yes`,
and points `Sitemap` at the absolute URL of `sitemap.xml`.

The published host does not send `X-Robots-Tag`, `Link`, or `Vary`, and it does not negotiate
`Accept: text/markdown`. A Markdown copy is its own URL. The response is not marked `noindex`, so the
copy can be indexed beside its HTML page.

`sitemap.xml` lists each homepage and each privacy URL once, for every language, and nothing else. Each entry carries an
alternate for every language and an `x-default` alternate. The language value is the BCP-47 tag identified in
`docs/product/website.md`, "Pages and languages": Simplified Chinese is `zh-Hans` while its route prefix stays `zh`.
`x-default` is the unprefixed English URL of that page. Every listed URL ends with a slash. Entries carry no `lastmod`
and no image entries. The standalone product preview is not listed and requests `noindex`. The error page is not listed;
see `docs/product/website.md`, "Missing pages and errors". `sitemap.xml` is the only sitemap: there is no sitemap index
and no per-language sitemap. It references an XSL stylesheet published at `__sitemap__/style.xsl` under the deployment
root, so a browser opening it shows its entries as a table.

Each HTML homepage and privacy page, in every language, has an absolute canonical URL with a trailing slash, and
hreflang alternates for the HTML versions, also with trailing slashes. `x-default` is again the
unprefixed English URL of that page. Those alternates use the same BCP-47 tags as the sitemap, and the
HTML page adds two tags that repeat an existing URL: `zh` is the Simplified Chinese page, the same URL
as `zh-Hans`, and `pt` is the Brazilian Portuguese page, the same URL as `pt-BR`. The sitemap does not
include `zh` or `pt`.

Open Graph locale is `language_TERRITORY`. The current page uses `og:locale`; every other language uses
`og:locale:alternate`. The values are `ar_AR`, `de_DE`, `en_US`, `es_ES`, `fr_FR`, `hi_IN`, `id_ID`,
`it_IT`, `ja_JP`, `ko_KR`, `pt_BR`, `ru_RU`, `th_TH`, `tr_TR`, `vi_VN`, `zh_CN`, and `zh_TW`. A value
such as `zh_Hans` is not left on the page. `og:type` is `website`, `og:site_name` is the application
name, and `og:url` is the canonical URL. `og:title` and `og:description` are that page's localized SEO
title and description.

Each language has one share image, 1200 by 630, at the deployment root as `og-image-<route code>.png`. The route code is
the URL code in `docs/product/website.md`, "Pages and languages", so Simplified Chinese is `og-image-zh.png`, not
`zh-Hans`. The image shows the mascot published as `logo.png` at the deployment root, the pixel wordmark published as
`wordmark.png` at the deployment root, that language's brand slogan drawn as text under the wordmark, followed by the
localized sentence that states the slogan's two meanings (see `docs/product/website.md`, "Brand slogan"), the three
lines of the homepage title, and the hostname `octoboard.dev`. The meaning sentence wraps into at most three lines. The
wordmark, the slogan, the meaning sentence and the title lines share one start edge. Those lines are the title on the
homepage, not the SEO title sentence. Arabic places the mascot on the right and aligns the text to that inner edge.
Neither the mascot nor the wordmark is mirrored.

`og:image` and `twitter:image` are the absolute URL of that language's image. `og:image:width` is 1200 and
`og:image:height` is 630. `twitter:card` is `summary_large_image`. `og:image:alt` is the application name,
an em dash, that language's brand slogan, an em dash, the sentence that states its two meanings,
an em dash, then the three homepage title lines joined with spaces, with a space on each side of each dash.
The privacy page uses the same image and the same alt text as that language's homepage. The unlocalized 404
page uses the English image and the same alt text as the English homepage. It is not a separate card.
An error within the running site does not set a share image.
Indexing exclusions for the 404 page and that error are in `docs/product/website.md`, "Missing pages and errors".

The homepage and privacy pages also link `rel="alternate"` with type `text/markdown` to that page's Markdown
file, and `rel="describedby"` to the absolute URL of `llms.txt`. Each of them carries one JSON-LD graph.

The graph always includes a person and a website. The person is the origin plus `#identity`, with the operator name and
email in `docs/product/website-privacy.md`, "Privacy disclosures", and its URL is the deployment root. The website is
that language's homepage URL plus `#website`: the homepage URL, the application name, the homepage SEO description, and
that page's language, published by the person.

A homepage also includes an FAQ page and a software application. The FAQ page is that page's canonical
URL plus `#webpage`. It contains the nine questions and, as part of that page, an item list of all 16
localized feature titles, in feature-wall order. The software application is the origin plus
`#app` on every language. Its category is `DeveloperApplication`, its operating system is `macOS`, and its URL is the
deployment root, the English homepage, not the localized homepage. It names the repository and the MIT
license linked from the site. Its image is the mascot file `logo.png` at the deployment root, not the
share image and not the brand artwork. It lists every language the website publishes, using the BCP-47
tags. It has no description, no feature list, no offer, no install URL, and no rating. The benefit titles stay on that
page's FAQ page, not on the shared application entry. The application entry does not say that a package can be
downloaded; availability is described in `docs/product/website.md`, "Platform availability".

A privacy page has a web page entry at that page's canonical URL plus `#webpage`. It does not include
the FAQ page or the software application.

Agent-readable files are published at the deployment root and are crawlable:

- One Markdown file for each homepage and privacy page. The English homepage is `index.md` and the
  English privacy page is `privacy.md`. Another language's homepage is `<route code>.md`, so Simplified
  Chinese is `zh.md`, not `zh-Hans.md`. Another language's privacy page is `<route code>/privacy.md`.
- `llms.txt`, in English. Its notes state that Octoboard is free and open source under the MIT License, and cover
  local data handling, account switching, agent handoff, usage and resets, the nine questions and answers, the
  support email, and the repository.
  It then lists every language, each linking to that language's homepage Markdown and stating that the language has
  two pages. The English
  entry is the one whose pages are listed in the file; each other language is listed as pages to open.
  The page list names only the two English pages, with their SEO titles and descriptions, and the file
  links to `sitemap.md`.
- `llms-full.txt` contains every language and both pages. English comes first, then the other languages in the order in
  `docs/product/website.md`, "Pages and languages", homepage before privacy page. Each part gives the page's SEO title,
  the HTML URL as its source, the SEO description, and the Markdown body.
- `sitemap.md` lists every Markdown URL with that page's localized SEO title. The unprefixed homepage
  stands alone, the unprefixed privacy page stands alone, and each other language's two pages are
  listed together.

Each page file states `Canonical:` and then that page's HTML URL. A link from one of these copies to
another page of the site points at the Markdown file. Links that leave the site — the releases page,
the repository, the license, the GitHub privacy statement, and email — stay on those targets.

The Markdown carries the page's localized wording in the same section order. It includes all three feature groups and
their 16 titles and descriptions, the statement that Octoboard is free and open source under the MIT License, the
preview scene names and sample-data explanation, and every workflow node's task and report. The developer's motivation
follows the workflow in each homepage copy and in `llms-full.txt`, including its title, opening, three subsections,
product purpose, invitation, and byline. It does not include the embedded application's sample screens. Release links
are those in `docs/product/website.md`, "Homepage and navigation" and sit beside the early-prototype status. The
workflow is identified as an illustration. The copies omit the header, the footer, decorative art, the labels and
numbers that mark a section, the scroll cue, the MIT badge, and interactive controls. The brand slogan stays, and the
homepage Markdown also includes the sentence that states the slogan's two meanings. The privacy page's introductory line
and the workflow's participant labels stay.
