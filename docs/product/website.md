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

All 17 languages have complete website translations, including navigation, the two homepage goals, preview controls,
the illustrative workflow, questions and answers, privacy text, client error messages, and page metadata: Arabic, German, English, Spanish, French, Hindi,
Indonesian, Italian, Japanese, Korean, Brazilian Portuguese, Russian, Thai, Turkish, Vietnamese, Simplified Chinese,
and Traditional Chinese. This coverage is specific to the website; the application's translation coverage is described
in `docs/product/language.md`, "What is translated so far".

English is the default. On a normal site page that opens in English, the site checks the remembered language after
the app mounts. If there is no remembered language, it matches the browser's preferred languages to the supported
languages and opens the matching version of the same page. Unknown language tags and English leave the page on its
default route. A page opened with a language prefix is not redirected. The language selector names each language in
that language and opens the corresponding version of the current page: switching from a privacy page stays on a
privacy page. Arabic uses a right-to-left layout. Page URLs remain usable directly, including after a reload. The
site remembers language changes for one year in a same-site cookie.
The unlocalized static 404 page does not perform language matching.

In the header and navigation dialog, the selected language's name fades where its single line is clipped. Hovering
anywhere on the language selector runs that name as a marquee with the timing described in
`docs/product/labels-and-tooltips.md`, "Names too long for their space". It immediately resets to its beginning
when the pointer leaves. The fade follows the name's own reading direction. Reduced motion leaves the name still.

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
a slash. The standalone product preview is not listed and requests `noindex`. The error page is not listed;
see "Missing pages and errors".

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
not `zh-Hans`. The image shows the mascot published as `logo.png` at the deployment root, the pixel
wordmark published as `wordmark.png` at the deployment root, that language's brand slogan drawn as text
under the wordmark, followed by the localized sentence that states the slogan's two meanings
(see "Brand slogan"), the three lines of the homepage title, and the hostname `octoboard.dev`.
The meaning sentence wraps into at most three lines. The wordmark, the slogan, the meaning sentence
and the title lines share one start edge. Those lines are the title on the homepage,
not the SEO title sentence. Arabic places the mascot on the right and aligns the text to that inner edge. Neither
the mascot nor the wordmark is mirrored.

`og:image` and `twitter:image` are the absolute URL of that language's image. `og:image:width` is 1200 and
`og:image:height` is 630. `twitter:card` is `summary_large_image`. `og:image:alt` is the application name,
an em dash, that language's brand slogan, an em dash, the sentence that states its two meanings,
an em dash, then the three homepage title lines joined with spaces, with a space on each side of each dash.
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

The Markdown carries the page's localized wording. It includes the visible text, including the license caption, the
preview scene names and sample-data explanation, and every workflow node's task, report, and Working, Awaiting
instructions and Running in background labels. It does not include the embedded application's sample screens. Download
links are the releases links in "Homepage and navigation" and sit beside the statement that the macOS app is an early
prototype still in development. The workflow is identified as an illustration. The copies omit the header, the footer,
decorative art, the labels and numbers that mark a section, the scroll cue, the MIT badge, and interactive controls. The
brand slogan stays, and the homepage Markdown also includes the sentence that states the slogan's two meanings. The
privacy page's introductory line and the workflow's participant labels stay.

## Homepage and navigation

The homepage uses Octoboard's original silver mechanical octopus, red eyes, and pixel wordmark on a dark surface.
The first screen introduces coordination across projects and offers links to download the app, visit its GitHub
repository, and open the preview section. The second screen gives equal weight to two goals: delegating agent
management across projects and keeping development quality in each project. The quality explanation states that
even with coordinated management, each project still runs its own independent native session, which keeps that
project's instructions, Skills, hooks and tools, and the user can open a conversation to review the work and
steer it.

The page continues through the product preview, the coordination illustration, four supporting principles, local data
and open-source information, platform availability, and questions. These seven sections share a numbered heading,
a short label marked by a red square, and a title. Section and workflow-step numbers are prominent visual markers.
The four principles cover reducing manual coordination, keeping independent native project sessions with their own
rules, coordinating agents from different brands, and letting the user work directly with the project agent
handling a task. The four principles are peers. Each is a title and one short paragraph, and the different-brands
principle does not contain a list. Each project keeps the agent it already uses, and an individual step can be
given to an agent from another brand when needed. Under the four principles, a shared heading explains applying
different agents' strengths within the same project. Three equal columns each show a brand icon and name,
its strengths, and concrete uses, in order: Claude for programming and system design, including feature
implementation, refactoring and architecture; GPT for image generation, device control and proposal planning,
including image assets, validating flows on devices and developing plans; and Grok for copy and language,
including interface copy, product introductions and multilingual expression. The names are Claude, GPT, and Grok,
not the agent products Claude Code, Codex, and Grok Build. On a small screen the three columns stack into one.
The homepage Markdown includes the shared heading and lists the same strengths and uses in the same order
after the four principles. The different-brand FAQ also describes these strengths and the coordinator assigning
work accordingly and collecting results.

The homepage describes projects in five categories: server-side services, administration tools, merchant operations
tools, staff operations tools and customer-facing apps. Staff operations include staff ordering, checkout and
kitchen work; customer-facing apps include web and mobile ordering.

The workflow illustration places Octoboard's mascot and coordinating agent at the center of eight distinct
project-agent nodes: Promotion console, Store management console, Customer ordering website, Restaurant order API,
Mobile ordering app, Staff ordering tablet, Checkout register and Kitchen display. Every node identifies its agent
brand, concrete example project and directory. Codex handles Promotion console, Restaurant order API and Checkout
register; Claude Code handles Store management console, Mobile ordering app and Kitchen display; Grok Build handles
Customer ordering website and Staff ordering tablet. The nodes surround the coordinator on wide screens and form
two columns below it on small screens. Curved connections carry outgoing task signals and incoming report signals,
with a labelled direction legend.

Selecting a node by pointer or keyboard highlights it and shows its directory, native project context, instructions,
Skills and hooks, assigned task, and report below the diagram. Promotion console at `apps/admin-web` is selected
initially.
Each project has its own task and report in the example of launching a meal deal promotion across restaurant
locations. Promotion console sets prices, promotion dates and eligible locations; Store management console handles
listings, stock and order redemption; Customer ordering website supports browsing, ordering and order details; and
Restaurant order API validates promotion dates and locations, calculates prices, deducts stock and prevents
duplicate orders. Mobile ordering app offers customer browsing, ordering and redemption codes. Staff ordering
tablet supports ordering by table, adding meal deals and synchronizing orders. Checkout register handles checkout,
discount validation and redemption; Kitchen display handles preparation tickets and synchronizes food-ready status.

The summary represents the central coordinator collecting the participating projects' results into one feature
delivery. The three stages below it explain giving a goal to the coordinator, letting project agents work, and following or
intervening in their conversations. The illustration is fixed sample content: it does not show live activity, run
agents, or submit work.

The homepage then presents the source repository and MIT License, local data handling and planned remote support,
platform availability, and nine questions. Each question can be expanded or collapsed independently, with the first
expanded initially; several answers can remain open at once. The plus marker rotates into a cross when an answer
opens and rotates back when it closes. The questions cover suitable tasks, loading several
projects into one session, configuration loading across directories or through subagents, different agent brands,
reporting and direct participation, product understanding, platform availability, preparation and costs, data handling,
and planned remote hosts. Configuration loading is described as dependent on the agent and its tools. Product
understanding comes from ongoing conversations, project knowledge, and reports available to the session, not automatic
training or permanent memory.

Supported agents are Claude Code, Codex, and Grok Build. The preparation answer states that users install the agent
CLIs they want to use. Octoboard is free and open source under the MIT License. The agents it runs are the user's
own, including one pointed at a model the user hosts.

The primary action in the header, hero, menu, and closing section uses the same localized download label and opens
the repository's GitHub Releases page. It does not link directly to an installer file. The hero identifies macOS
Apple Silicon and describes the app as an early prototype still in development. The closing note repeats that
status beside the MIT License. The download action does not imply that a package has already been published.
Other repository links open GitHub; the footer links to the localized privacy page, the MIT License as the license
terms, and the operator's email address. The hero, open-source section and footer use icon-only GitHub links with
44-by-44-pixel targets, accessible names and hover titles. The navigation menu retains its text repository link.

The footer forms a compact centered group: the wordmark and localized tagline, followed by navigation links and
copyright. On small screens, the wordmark and tagline stack vertically.

The closing section displays the mascot beside the wordmark, with the localized brand slogan as text under the
wordmark, centered on the wordmark. Under that group, centered, is the sentence that states the slogan's two
meanings (see "Brand slogan"). Decorative graphics do not act as slide or pagination controls.

The floating header stays visible while scrolling and links to the two goals, product preview, coordination
explanation, and questions from either page. Following a section link leaves space for the header above the destination. Its inline links
identify the current homepage section as the visitor moves through the page. The header has a glass surface with edge
refraction where supported and a sheen that follows the pointer. Browsers without refraction support use blurred glass;
those without backdrop blur use a more opaque surface.
A menu button is available at every screen width and opens a full-screen navigation dialog. The button changes into a
close control; the background cannot be interacted with or scrolled while the menu is open. Tab and Shift+Tab stay
within the dialog. Closing it with the button or Escape returns focus to the header's menu button. Choosing an internal
destination or another language closes the dialog as well.

## Product preview

The preview shows the actual application's interface populated with fictional data and identifies itself as a
sample. Three buttons select project management, team reports, or code review; project management is the initial
scene. All three scenes follow the restaurant meal deal promotion across the eight independent example projects
named in "Homepage and navigation", using the same concrete project names. They show the coordinating and project
sessions, a delivery report, and a selected order-validation diff. In every scene, Restaurant order API's
order-validation review session awaits instructions while a test watcher (`pnpm test --watch`) runs in the
background, so it shows as running in background. Choosing a scene resets the demonstration to that
scene's prepared state.

Each of the eight projects has its own sample repository, with a distinct root, feature branch, business files and
Git changes. Selecting another project shows that project's source and diffs. The examples cover promotion
configuration in Promotion console, store availability and redemption in Store management console, browsing and
checkout in Customer ordering website, and order validation, stock reservation and duplicate-request handling in
Restaurant order API. Mobile ordering app has browsing, order retry and paid-order collection passes; Staff ordering
tablet has table orders, meal-deal extras and revision conflicts; Checkout register has server quotes, payment and
redemption confirmation; Kitchen display has station tickets, printer-job status and food-ready updates. Each
repository includes tests for its own role; frontend repositories also include components, API clients, domain rules
and web configuration, while Restaurant order API includes HTTP validation, database queries and migrations.

Files can be opened in the real viewer, and the source shown there agrees with the corresponding diff. All sample
repositories exist only in memory; their displayed source is not executed. The examples do not provide working
restaurant services, payment devices or printers.

The delivery report summarizes each project's result and presents sample figures of eight coordinated projects,
48 passed checks and one decision awaiting confirmation: whether the meal deal can be combined with a member
discount. The sample keeps the discounts separate. The code-review scene shows `src/meal-deal-orders.ts` on the
Restaurant order API project, with changes that calculate prices from an eligible active offer, reserve available
stock and prevent repeated requests from creating duplicate orders, alongside sample tests and promotion rules.
These files and check counts are demonstration data.

The homepage embeds a complete desktop viewport scaled to fit its available width. The application fills the
embedded viewport, with the sample-data explanation in a caption below it. Once ready, the embedded
application accepts pointer and keyboard interaction, including its report and project panes, and is available to
assistive technology. While loading, it remains hidden and cannot receive interaction. A continuous border with
four rounded corners frames the preview. At rest, its lower edge, including both application content and border,
fades into the page. Hovering over the preview or focusing within it removes the fade and reveals the complete
border. Devices without hover show no fade.
The standalone link opens the selected scene and language in a new tab at `preview/website-preview.html`, relative
to the site's deployment root. The standalone preview displays its sample-data label above the application.

On devices whose primary pointer is touch, both previews keep text-entry fields read-only so tapping or automatically focusing
the terminal or another text field does not open the software keyboard. Buttons, menus, selection and scrolling
remain interactive. Devices whose primary pointer is a mouse or trackpad retain keyboard text entry, including
touchscreen desktops. This restriction applies only to
the website previews, not to the application itself.

Vertical mouse-wheel and trackpad gestures scroll the embedded list, terminal, code or report under the pointer
while it has room to scroll in that direction. At its top or bottom boundary, continuing in the same direction
scrolls the surrounding homepage. Horizontal gestures, gestures modified by Shift, Ctrl or Meta, and zoom retain
their normal behavior. The standalone preview keeps native scrolling within the application.

On touch screens the same handoff applies to a one-finger vertical drag, decided once per gesture from its first
movement: if the embedded list, terminal, code or report can scroll in that direction, the drag scrolls it and never
the homepage; otherwise the whole drag scrolls the homepage. Mostly horizontal drags, pinches and drags that start on
a control that handles touches itself, such as a resize handle, are left alone. When a drag that scrolled the
homepage ends while the finger is still moving, the homepage keeps gliding and slows to a stop, as native scrolling
does; touching the screen again, using the wheel, reaching the top or bottom of the page or leaving the page ends the
glide. Visitors who prefer reduced motion get no glide.

When the embedded preview's container is 800 pixels wide or narrower, the application uses that width in a
640-pixel-high viewport. Both the embedded and standalone previews support the application's responsive layout;
on narrow screens the sidebar is a drawer, while the report and Files panels dock beside the terminal. The overview
scene shows the terminal with no panel open, the report scene opens its report panel, and the
code-review scene opens the sample change in the diff viewer automatically, including on phones. The preview does
not connect to a daemon, run real agents, read or change real project files, or retain its preferences after a reload.
It does not replace the website's stored language preference. The application controls follow the selected supported
UI language; the fictional tasks, terminal output and report use Simplified Chinese for that language and English
for all others.

The embedded view shows a loading message until ready. A reported startup failure, or no readiness after 20 seconds,
shows a localized error message; selecting the current scene again retries it. The standalone preview shows an alert
if its startup fails. JavaScript is required to load and operate either preview.

## Brand slogan

The brand slogan is "The Programming Terminator" in English. The sentence that states the slogan's two meanings
carries both readings of the pun: bringing programming to an end, and having capability that rivals the
Terminator. It states these two ideas without repeating the slogan as a prefix. It is not a pun on "terminal".
The slogan expresses a vision and a brand identity; it does not claim that fully autonomous development is
already implemented.

Every language has its own slogan, which keeps those meanings and uses the film's title in that language where one
is established (Brazilian Portuguese "O Exterminador", Thai "คนเหล็ก", Vietnamese "Kẻ hủy diệt"). Translations use
sentence case. Simplified Chinese is 编程终结者, not 编程界的终结者. Each language also has a sentence that states
the two meanings.

The slogan appears as the eyebrow above the homepage title, as text under the wordmark in the closing section, under
the wordmark on the share image, and in that language's homepage Markdown. The meaning sentence is shown under the
wordmark in the closing section, follows the slogan on the share image, and is included in that homepage
Markdown. The image's alt text includes both the slogan and the meaning sentence. On the website the slogan
remains localized text rather than part of the wordmark artwork.

## Platform availability

The platform section appears between the data-handling section and the questions. Its five entries have equal visual
weight in a desktop row and become a compact list of icons and text on small screens. The macOS entry uses brighter
text and identifies the version as an early prototype. The section introduction says the macOS app is still in
development, that only the first prototype features are in place, and that the details are still being refined.
The site does not describe the macOS app as finished and does not give an installer date.

Status: not implemented — the Linux server and the Windows, iOS and Android apps. Each appears with a platform icon
and no download or availability date.

Linux is a planned server, not another app version beside Windows, iOS, and Android, and not a Linux desktop app.
The section introduction and the platform question describe it as the background service on a machine without a
display, opened in a browser on another machine (see "Two deployment forms" in `docs/architecture.md`). It is not
available. The Linux row's status is its own label, "Server, planned" in English. Windows, iOS, and Android remain
planned and not available, and those rows use the shared planned badge. The homepage Markdown includes the same
introduction and the same status for each row.

## Motion and accessibility

The hero text and mascot enter with a short motion. On both mobile and desktop, the mascot's eight tentacles
swing independently, its body floats up and down and rocks from side to side, and its red eyes brighten and dim.
Red and silver pixel particles move through the surrounding
space without covering content or intercepting interaction. Sections reveal as they enter view.

A control beside the mascot pauses and resumes the homepage's decorative animation, including the mascot, particles and
workflow signals. The mascot and particles pause when their area is offscreen or the page is in the background; workflow
signals and the working and running-in-background pulses do the same for the diagram. They resume when visible again
unless the visitor has paused them. The workflow's signals repeat along the connections rather than implying a real
task's progress; selecting a project remains available while motion is paused.

The illustration's legend has three entries: Working with the application's dark-theme blue pulse, the
awaiting-instructions speech bubble, and Running in background, a hollow ring in the awaiting-instructions green that
pulses like Working. The Running in background pulse pauses with the other motion and stands still under a
reduced-motion preference. All three have visible labels and accessible names. They retain the meanings of Working,
Awaiting instructions and Running in background in `docs/product/sessions.md`, "Session statuses"; awaiting instructions
is not a task-completion indicator. The sample reports and shared delivery describe the outcome separately. Question
expansion and control state changes also have visual transitions.

A reduced-motion preference disables decorative animations and transitions, hides the particles and motion control,
and leaves the content visible. The header keeps a static blurred glass surface without refraction or a moving sheen.
Without JavaScript, the pre-rendered homepage and privacy content remain readable, and questions can still be expanded
and collapsed; section reveals never leave that content hidden. The diagram retains its initial selection and,
on wide screens, static connections. Selecting another node, the product preview and opening the menu require
JavaScript. The header provides a skip link to the main content, and interactive controls, including questions and
project nodes, show keyboard focus.

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
  to servers operated by that operator;
- third-party agents and tools, which can send data to their configured providers under those providers' policies;
- repository operations, including remote status checks, which can contact configured Git remotes;
- task information passed between agent sessions, which can consequently reach more than one provider;
- the static website, which uses a same-site cookie to remember the selected or detected language for one year and
  adds no analytics or advertising trackers; GitHub Pages also logs visitor IP addresses for security, with a link
  to GitHub's privacy statement;
- information voluntarily supplied through email or GitHub Issues, including the visibility of public issues.

The local-data explanation is not a promise that all workflow data remains on one device. There is no separate
website terms page; the software's license is linked separately from the privacy policy.

## Remote host support

Status: not implemented. The homepage and the privacy page say that remote host support is planned and do not describe
a workflow. The homepage presents that status in a card beside the open-source information.

Remote host support runs in one direction only: the daemon the user is talking to reaching a second machine to run
sessions there. The server form runs the other way — one daemon, on a server, with the user's browser on another
machine (see "Two deployment forms" in `docs/architecture.md`) — and is not what this section, the planned-remote
question or the privacy page's remote-host disclosure describe. The homepage's platform section and platform
question describe that planned server (see "Platform availability"). The privacy page does not. The site's local-data
statements describe the machine the daemon runs on, which
in the server form is the server rather than the machine in front of the user, and a privacy page covering that form
has to say that the interface then travels over the network between the server and the user's browser. That addition
has not been written.
