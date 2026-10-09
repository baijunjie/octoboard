# Website

The official website presents Octoboard and its privacy policy. It is a static public site, separate from the
application: visiting it does not connect to a local daemon, open a real agent session, or require an account.

## Publishing

Every push to `main` publishes the static site to GitHub Pages. Pushes to other branches do not. The same
publication can be started manually, and a manual run publishes the selected revision to that same site.

Publication uses the origin and base path configured for the repository's GitHub Pages site, passed to generation
as `NUXT_PUBLIC_SITE_ORIGIN` and `NUXT_APP_BASE_URL`. An origin reported as `http` is changed to `https` before
generation, because Pages can report `http` until its certificate exists. An empty base path is `/`. Generation
without these values uses the origin `https://octoboard.dev` and the base path `/`.

The GitHub Pages custom domain is `octoboard.dev`. It is a repository setting, not a CNAME file, and a CNAME file
is ignored when publishing this way. `www.octoboard.dev` is the www variant of that domain. When DNS records for
both names exist, GitHub Pages redirects `www.octoboard.dev` to `octoboard.dev`.

## Pages and languages

The site has a homepage and a privacy page. Paths below are relative to the site's deployment root:

- English: `/` and `/privacy/`, without an `en` prefix.
- Other languages: `/<locale>/` and `/<locale>/privacy/`.
- Simplified Chinese uses the route prefix `zh`, with the language identified as `zh-Hans` in page metadata.
  Traditional Chinese uses `zh-Hant` for both.

All 17 languages have complete website translations, including navigation, the illustrative workflow, questions and
answers, privacy text, client error messages, and page metadata: Arabic, German, English, Spanish, French, Hindi,
Indonesian, Italian, Japanese, Korean, Brazilian Portuguese, Russian, Thai, Turkish, Vietnamese, Simplified Chinese,
and Traditional Chinese. This coverage is specific to the website; the application's translation coverage is described
in `docs/product/language.md`, "What is translated so far".

English is the default. The site does not redirect visitors based on their browser language or store a language
preference in a cookie. The language selector names each language in that language and opens the corresponding
version of the current page: switching from a privacy page stays on a privacy page. Arabic uses a right-to-left
layout. Page URLs remain usable directly, including after a reload.

## Search indexing and agent-readable copies

Search indexing, live retrieval by agents, and use of the pages for model training are allowed.
`robots.txt` at the deployment root allows the whole site and does not disallow the Markdown copies.
It sets `Content-Usage` to `train-ai=y` and `Content-Signal` to `search=yes, ai-input=yes, ai-train=yes`,
and points `Sitemap` at the absolute URL of `sitemap.xml`.

The published host does not send `X-Robots-Tag`, `Link`, or `Vary`, and it does not negotiate
`Accept: text/markdown`. A Markdown copy is its own URL. The response is not marked `noindex`, so the
copy can be indexed beside its HTML page.

`sitemap.xml` lists each homepage and each privacy URL once, for every language, and nothing else.
Each entry carries an alternate for every language and an `x-default` alternate. The language value is
the BCP-47 tag identified in "Pages and languages": Simplified Chinese is `zh-Hans` while its route
prefix stays `zh`. `x-default` is the unprefixed English URL of that page. Every listed URL ends with
a slash. The error page is not listed; see "Missing pages and errors".

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

Each language has one share image, 1200 by 630, at the deployment root as `og-image-<route code>.png`.
The route code is the URL code in "Pages and languages", so Simplified Chinese is `og-image-zh.png`,
not `zh-Hans`. The image shows the mascot published as `logo.png` at the deployment root, the brand
artwork published as `branding/brand-slogan.png` at the deployment root, the three lines of the homepage
title, and the hostname `octoboard.dev`. The brand artwork is the pixel wordmark and the English slogan
"The Programming Terminator"; see "Brand slogan". It is the same on every language and is not translated.
Those lines are the title on the homepage, not the SEO title sentence. Arabic places the mascot on the
right and aligns the text to that inner edge. Neither the mascot nor the brand artwork is mirrored.

`og:image` and `twitter:image` are the absolute URL of that language's image. `og:image:width` is 1200 and
`og:image:height` is 630. `twitter:card` is `summary_large_image`. `og:image:alt` is the application name,
an em dash, "The Programming Terminator", an em dash, then the three homepage title lines joined with
spaces, with a space on each side of each dash.
The privacy page uses the same image and the same alt text as that language's homepage. The unlocalized 404
page uses the English image and the same alt text as the English homepage. It is not a separate card.
An error within the running site does not set a share image.
Indexing exclusions for the 404 page and that error are in "Missing pages and errors".

The homepage and privacy pages also link `rel="alternate"` with type `text/markdown` to that page's Markdown
file, and `rel="describedby"` to the absolute URL of `llms.txt`. Each of them carries one JSON-LD graph.

The graph always includes a person and a website. The person is the origin plus `#identity`, with
the operator name and email in "Privacy disclosures", and its URL is the deployment root. The website
is that language's homepage URL plus `#website`: the homepage URL, the application name, the homepage
SEO description, and that page's language, published by the person.

A homepage also includes an FAQ page and a software application. The FAQ page is that page's canonical
URL plus `#webpage`. It contains the nine questions and, as part of that page, an item list of the four
localized benefit titles. The software application is the origin plus `#app` on every
language. Its category is `DeveloperApplication`, its operating system is `macOS`, and its URL is the
deployment root, the English homepage, not the localized homepage. It names the repository and the MIT
license linked from the site. Its image is the mascot file `logo.png` at the deployment root, not the
share image and not the brand artwork. It lists every language the website publishes, using the BCP-47
tags. It has no description, no feature list, no offer, no install URL, and no rating. The benefit titles stay on that
page's FAQ page, not on the shared application entry. The application entry does not say that a package can be
downloaded; availability is described in "Platform availability".

A privacy page has a web page entry at that page's canonical URL plus `#webpage`. It does not include
the FAQ page or the software application.

Agent-readable files are published at the deployment root and are crawlable:

- One Markdown file for each homepage and privacy page. The English homepage is `index.md` and the
  English privacy page is `privacy.md`. Another language's homepage is `<route code>.md`, so Simplified
  Chinese is `zh.md`, not `zh-Hans.md`. Another language's privacy page is `<route code>/privacy.md`.
- `llms.txt`, in English. Its notes cover local data handling, planned remote host support, the nine
  questions and answers, the support email, and the repository. It then lists every language, each
  linking to that language's homepage Markdown and stating that the language has two pages. The English
  entry is the one whose pages are listed in the file; each other language is listed as pages to open.
  The page list names only the two English pages, with their SEO titles and descriptions, and the file
  links to `sitemap.md`.
- `llms-full.txt` contains every language and both pages. English comes first, then the other languages
  in the order in "Pages and languages", homepage before privacy page. Each part gives the page's SEO
  title, the HTML URL as its source, the SEO description, and the Markdown body.
- `sitemap.md` lists every Markdown URL with that page's localized SEO title. The unprefixed homepage
  stands alone, the unprefixed privacy page stands alone, and each other language's two pages are
  listed together.

Each page file states `Canonical:` and then that page's HTML URL. A link from one of these copies to
another page of the site points at the Markdown file. Links that leave the site — the releases page,
the repository, the license, the GitHub privacy statement, and email — stay on those targets.

The Markdown carries the page's localized wording. It includes the visible text, including the license
caption and the illustrative workflow's Working and Awaiting instructions labels. Download links are
the releases links in "Homepage and navigation" and sit beside the statement that the macOS app is an
early prototype still in development. The workflow is identified as an illustration. The copies omit
the header, the footer, decorative art, the labels and numbers that mark a section, the scroll cue,
the MIT badge, and the replay control. The brand slogan, the privacy page's introductory line, and
the workflow's participant labels stay.

## Homepage and navigation

The homepage uses Octoboard's original silver mechanical octopus, red eyes, and pixel wordmark on a dark surface.
It presents requests that span several projects and four benefits: reducing manual coordination between projects,
keeping independent native project sessions with their own rules, retaining different agent brands and toolchains,
and letting the user work directly with the project agent handling a task. The explanation connects these benefits
to transferring tasks and results between frontend, backend, and documentation projects, preserving local project
configuration, and explaining specific requirements or code details directly to the agent doing the work.
The different-brands benefit also includes assigning a step to another brand. A coordinator running Claude Code can give
image generation to Codex, because Claude Code does not generate images, and can give Codex the steps that drive a local
application or a browser, where Codex is stronger. The coordinator then continues from what Codex reports.

A statement about collaboration across repositories appears beside a static illustration connecting the three agent
brands to Octoboard's mascot. It leads into the three stages of coordination: give the goal to a coordinator, let
project agents work, and follow or intervene in their conversations. The workflow illustration follows one team
invitation request through three independent project sessions: Codex handles the frontend, Claude Code the backend,
and Grok Build the documentation. All three project cards remain visible, each with its directory, project instructions,
Skills and hooks, assigned task, and report. They appear in three columns on wider screens and stack on smaller screens.

The example combines invitation and team-joining screens, invitation endpoints and permission checks, and a matching
user guide. Each project reports to the coordinator; the summary below the projects represents the same coordinator
shown above them, gathering all three results into one feature delivery. These are fixed, localized examples,
explicitly labelled as an illustration. The diagram does not show live application activity, run agents, or submit work.
Its replay control restarts the demonstration.

The homepage then presents the source repository and MIT License, local data handling and planned remote support,
platform availability, and nine questions. Each question can be expanded or collapsed independently, with the first
expanded initially; several answers can remain open at once. The questions cover suitable tasks, configuration
loading across directories or through subagents, different agent brands, reporting and direct participation, product
understanding, platform availability, preparation and costs, data handling, and planned remote hosts. Configuration
loading is described as dependent on the agent and its tools. Product understanding comes from ongoing conversations,
project knowledge, and reports available to the session, not automatic training or permanent memory.

Supported agents are Claude Code, Codex, and Grok Build. The preparation answer states that users install the agent
CLIs they want to use. Octoboard is free and open source under the MIT License. The agents it runs are the user's
own, including one pointed at a model the user hosts.

The primary action in the header, hero, menu, and closing section uses the same localized download label and opens
the repository's GitHub Releases page. It does not link directly to an installer file. The hero identifies macOS
Apple Silicon and describes the app as an early prototype still in development. The closing note repeats that
status beside the MIT License. The download action does not imply that a package has already been published.
Other repository links open GitHub; the footer links to the localized privacy page, the MIT License as the license
terms, and the operator's email address.

The footer forms a compact centered group: the wordmark and localized tagline, followed by navigation links and
copyright. On small screens, the wordmark and tagline stack vertically.

The closing section displays the original combined mascot, wordmark, and slogan: a horizontal arrangement on wider
screens and a vertical arrangement on small screens. Decorative graphics do not act as slide or pagination controls.

The floating header stays visible while scrolling and links to benefits, the coordination explanation, and questions
from either page. Following a section link leaves space for the header above the destination. Its inline links
identify the current homepage section as the visitor moves through the page. The header has a glass surface with edge
refraction where supported and a sheen that follows the pointer. Browsers without refraction support use blurred glass;
those without backdrop blur use a more opaque surface.
A menu button is available at every screen width and opens a full-screen navigation dialog. The button changes into a
close control; the background cannot be interacted with or scrolled while the menu is open. Tab and Shift+Tab stay
within the dialog. Closing it with the button or Escape returns focus to the header's menu button. Choosing an internal
destination or another language closes the dialog as well.

## Brand slogan

The brand slogan is "The Programming Terminator". Its two intended meanings are the ambition to hand programming work
over to agents, and the image of a powerful Terminator in the programming world, reflected in the mechanical mascot.
It is not a pun on "terminal". The slogan expresses a vision and a brand identity; it does not claim that fully
autonomous development is already implemented.

## Platform availability

The platform section appears between the data-handling section and the questions. Its five entries have equal visual
weight in a desktop row and become a compact list of icons and text on small screens. The macOS entry uses brighter
text and identifies the version as an early prototype. The section introduction says the macOS app is still in
development, that only the first prototype features are in place, and that the details are still being refined.
The site does not describe the macOS app as finished and does not give an installer date.

Status: not implemented — versions for Linux, Windows, iOS, and Android. Each appears with a platform icon and a
planned status, without a download or an availability date. The platform answer repeats the same availability status.

## Motion and accessibility

The hero text and mascot enter with a short motion. Red and silver pixel particles move through the surrounding
space without covering the content or intercepting interaction; they pause when the hero is out of view or the page
is in the background. Sections reveal as they enter view. The standalone collaboration statement reveals its words
in reading order as they reach the viewing area, with word boundaries appropriate to the selected language.
Assistive technology receives the complete statement rather than a sequence of separate animated words.

The workflow illustration runs one finite sequence as it enters view or the replay control is activated: the request
reaches the coordinator, tasks travel to all three project sessions, and execution and reporting are highlighted.
Three reports then reach the summary; it and the coordinator above highlight together to acknowledge receipt before
the shared feature delivery is highlighted. Brief signals travel along the connections, while card highlights hold
before fading softly. All tasks and reports remain readable throughout the sequence.

The illustration uses the application's working pulse and awaiting-instructions speech bubble, with visible labels
and accessible names. They retain the meanings in `docs/product/sessions.md`, "Session statuses"; awaiting instructions
is not a task-completion indicator. The example reports and shared delivery describe the outcome separately.
The sequence and working pulses pause offscreen or in the background; returning to view starts a new pass. A layout
change leaves the connections static instead of continuing a highlight along a changed route. The planned remote-host
illustration has a pulsing connection indicator. Question expansion and control state changes also have visual
transitions.

A reduced-motion preference disables these animations and transitions, hides the particles, and leaves the content
visible. The header keeps a static blurred glass surface without refraction or a moving sheen. Without JavaScript,
the pre-rendered homepage and privacy content remain readable, and questions can still be expanded and collapsed;
section reveals never leave that content hidden. Replaying the illustration and opening the menu require JavaScript;
the replay control is absent when motion is reduced or playback is unavailable. The header provides a skip link to the
main content, and interactive controls, including questions, show keyboard focus.

## Missing pages and errors

An unknown address receives a branded 404 page with the original mascot, an explanation, and a link back to the
homepage. The static GitHub Pages fallback is in English, works without JavaScript, and preserves the requested
missing address rather than replacing it with a different URL. Its return-home link opens the English homepage under
the site's deployment root. Its share image is the English one in "Search indexing and agent-readable copies".

Errors encountered within the running site use localized messages, distinguishing missing pages from other loading
errors. These error views also provide a return-home link. Error pages request exclusion from search indexing
and from following their links (`noindex, nofollow`). The static error route is not included in the
sitemap, and error pages do not link a Markdown copy, point at `llms.txt`, or carry structured data.

## Privacy disclosures

The privacy page identifies BaiJunjie as the operator and gives `support@octoboard.dev` for privacy questions.
Its disclosures distinguish:

- application configuration and session data stored locally, with no upload of code, conversations, or usage data
  to developer-operated servers;
- third-party agents and tools, which can send data to their configured providers under those providers' policies;
- repository operations, including remote status checks, which can contact configured Git remotes;
- task information passed between agent sessions, which can consequently reach more than one provider;
- the static website, with no added analytics, advertising trackers, or cookies, and GitHub Pages' security logging
  of visitor IP addresses, with a link to GitHub's privacy statement;
- information voluntarily supplied through email or GitHub Issues, including the visibility of public issues.

The local-data explanation is not a promise that all workflow data remains on one device. There is no separate
website terms page; the software's license is linked separately from the privacy policy.

## Remote host support

Status: not implemented. The homepage and the privacy page say that remote host support is planned and do not describe
a workflow. A decorative illustration sits beside that status.
