# Website

The official website presents Octoboard, its developer's motivation, and its privacy policy. It is a static public
site, separate from the application: visiting it does not connect to a local daemon, open a real agent session, or
require an account.

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

All 17 languages have complete website translations, including navigation, all 16 feature cards, preview
controls, the illustrative workflow, the developer's motivation, questions and answers, privacy text, client
error messages, and page metadata: Arabic, German, English, Spanish, French, Hindi, Indonesian, Italian,
Japanese, Korean, Brazilian Portuguese, Russian, Thai, Turkish, Vietnamese, Simplified Chinese, and Traditional
Chinese. This coverage is specific to the website; the application's translation coverage is described in
`docs/product/language.md`, "What is translated so far".

English is the default. On a normal site page that opens in English, the site checks the remembered language after
the app mounts. If there is no remembered language, it matches the browser's preferred languages to the supported
languages and opens the matching version of the same page. Unknown language tags and English leave the page on its
default route. A page opened with a language prefix is not redirected. The language selector names each language in
that language and opens the corresponding version of the current page: switching from a privacy page stays on a
privacy page. Arabic uses a right-to-left layout. Page URLs remain usable directly, including after a reload. The
site remembers language changes for one year in a same-site cookie.
The unlocalized static 404 page does not perform language matching.

In the header, the selected language's name fades where its single line is clipped. Hovering
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
a slash. Entries carry no `lastmod` and no image entries. The standalone product preview is not listed and requests
`noindex`. The error page is not listed; see "Missing pages and errors". `sitemap.xml` is the only sitemap: there is
no sitemap index and no per-language sitemap. It references an XSL stylesheet published at `__sitemap__/style.xsl`
under the deployment root, so a browser opening it shows its entries as a table.

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
URL plus `#webpage`. It contains the nine questions and, as part of that page, an item list of all 16
localized feature titles, in feature-wall order. The software application is the origin plus
`#app` on every language. Its category is `DeveloperApplication`, its operating system is `macOS`, and its URL is the
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
- `llms.txt`, in English. Its notes state that Octoboard is free and open source under the MIT License, and cover
  local data handling, account switching, agent handoff, usage and resets, the nine questions and answers, the
  support email, and the repository.
  It then lists every language, each linking to that language's homepage Markdown and stating that the language has
  two pages. The English
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

The Markdown carries the page's localized wording in the same section order. It includes all three feature
groups and their 16 titles and descriptions, the statement that Octoboard is free and open source under the MIT
License, the preview scene names and sample-data explanation, and every workflow node's task and report. The
developer's motivation follows the workflow in each homepage copy and in `llms-full.txt`, including its title,
opening, three subsections, product purpose, invitation, and byline. It does not include the embedded
application's sample screens. Release links are those in "Homepage and navigation" and sit beside the
early-prototype status. The workflow is identified as an illustration. The copies omit the header, the footer,
decorative art, the labels and numbers that mark a section, the scroll cue, the MIT badge, and interactive
controls. The brand slogan stays, and the homepage Markdown also includes the sentence that states the slogan's
two meanings. The privacy page's introductory line and the workflow's participant labels stay.

## Homepage and navigation

The homepage uses Octoboard's original silver mechanical octopus, red eyes, and pixel wordmark on a dark surface.
The first screen introduces one workspace for existing coding agents and their accounts: retaining a conversation
when switching accounts of the same agent, handing work to another agent brand, and choosing an agent to coordinate
tasks across projects. The coordinator is a role taken by the chosen agent. The first screen offers links to view
releases, visit its GitHub repository, and open the preview section.

The page continues through the grouped feature wall, the product preview, one cross-project coordination
example, the developer's motivation, local data and open-source information with platform availability, and questions.
These six sections share a numbered heading, a short label marked by a red square, and a title. Section and
workflow-step numbers are prominent visual markers.

The first numbered section presents 16 feature cards in three groups, in this order:

- Accounts and usage (six cards): managing accounts across agents; retaining the conversation when switching
  accounts of the same agent; optional automatic selection of an available account when starting a session,
  considering account status and usage reset times; usage readings and reset countdowns; optional automatic
  switching after a usage limit; and waiting when no account of the same agent is available, then resuming limited
  work when a fresh reading confirms an account's usage recovery.
- Context and continuity (three cards): monitoring context size and agent-reported window usage to see how close
  the conversation is to the length the agent can process, and moving later work to a fresh session before a long
  conversation risks losing details through summary compression; passing work to a new session while the original
  agent retains its conversation record for reviewing or resuming to ask about earlier decisions and implementation
  details; and another agent brand taking over from a handoff record of the task, progress and remaining work.
- Agents and projects (seven cards): coordinating different agents' strengths; organizing work across projects;
  separate native project contexts; retaining existing CLI instructions, Skills, Hooks and MCP configuration;
  entering a delegated project's session to guide its executing agent directly, without relaying every detail
  through the coordinator; monitoring session statuses and requests for decisions; and reviewing files and Git diffs.

Every card has a title and description. Cards form three columns on wide screens, two at widths of 1050 pixels or
less, and one at 600 pixels or less. Each group has a numbered heading and decorative icon. The feature wall, the
homepage Markdown and the homepage structured-data list present the same features in the same order.

The coordination introduction identifies the restaurant promotion as a cross-project example. The HTML page also
invites visitors to select a project in the illustration to inspect its assigned task and example report; the
Markdown copy keeps the case introduction without instructions for interacting with the diagram.

The workflow illustration places Octoboard's mascot and coordinating agent at the center of eight distinct
project-agent nodes: Promotion console, Store management console, Customer ordering website, Restaurant order API,
Mobile ordering app, Staff ordering tablet, Checkout register and Kitchen display. Every node identifies its agent
brand, concrete example project and directory. Codex handles Promotion console, Restaurant order API and Checkout
register; Claude Code handles Store management console, Mobile ordering app and Kitchen display; Grok Build handles
Customer ordering website and Staff ordering tablet. The nodes surround the coordinator on wide screens and form
two columns below it on small screens. Curved connections carry outgoing task signals and incoming report signals.

Selecting a node by pointer or keyboard highlights it and shows its directory, native project context, instructions,
Skills and hooks, assigned task, and report below the diagram. Promotion console at `apps/admin-web` is selected
initially.
Each project has its own development task and report in the example of adding a meal deal promotion to restaurant
ordering, checkout and operations software. Agents implement promotion controls for prices, dates and eligible
locations in Promotion console; listings, stock management and order redemption in Store management console;
browsing, ordering and order details in Customer ordering website; and promotion validation, pricing, stock
deductions and duplicate-order protection in Restaurant order API. Mobile ordering app gains browsing, ordering and
redemption-code features; Staff ordering tablet gains table orders, meal-deal add-ons and order synchronization;
Checkout register gains discount checks and redemption; and Kitchen display gains preparation tickets and
food-ready status updates. The reports describe the resulting capabilities for administrators, staff and customers.

The three stages below the illustration explain giving a goal to the coordinator, letting project agents work
within their own native project context, and following or intervening in their conversations. Each project agent
uses its native support for instructions, Skills, hooks, tools and permissions. The illustration is fixed sample
content: it does not show live activity, run agents, or submit work.

Section 04 explains why the developer created Octoboard, as a signed personal statement. Its opening expresses the
belief that people no longer need to write code themselves and can focus on the product they want to deliver.
Three subsections develop that position:

- Moving beyond the code editor: manual review and rewriting cannot keep pace with AI's output; the developer sees
  implementation becoming a black box as irreversible, considers AI's logic more complete than most programmers',
  and regards code conventions designed for human readers as less central when AI writes and maintains the code.
  The Simplified Chinese statement describes those conventions as gradually losing their meaning.
- Requirements and context: the developer sees unclear requirements, missing context and unstated assumptions as
  the main source of misunderstanding, making problem definition and shared context central to coordinating agents.
- Quality through testing: the developer regards thorough testing against real needs, including workflows, edge
  cases and failures, as the deciding quality standard, and reading code alone as insufficient.

The statement connects those beliefs to a workspace for coordinating agents, carrying context forward and following
delivery, then invites readers who share the belief that people need not write code themselves. It is signed by
BaiJunjie in every language; the author name is not translated. These are the developer's convictions and motivation,
not verified claims about AI performance or promises of fully autonomous development or guaranteed software quality.

Section 05 combines local data handling, the source repository and MIT License, and platform availability.
Its data-handling summary describes local application storage, no upload of code or conversations to Octoboard's
own servers, and configured agents and tools connecting to their providers. The open-source panel states that
Octoboard is free and open source under the MIT License.

The nine questions in section 06 cover setup, data handling, independent project sessions and subagents, agent brands,
direct participation, requirements and cost, remote hosts, concurrent edits, and returning to the original CLI, in
that order. The section's introductory label is separate from its questions heading. Each question can be expanded
or collapsed independently, with the first expanded initially; several answers can remain open at once. The plus
marker rotates into a cross when an answer opens and rotates back when it closes. Only collapsed question summaries
receive a hover background; an expanded question and its answer share a uniform background.

- Setup: existing project folders can be added in place. The default account uses the existing CLI configuration,
  and the agent continues loading project instructions, Skills, hooks and MCP servers. Additional accounts use
  separate configurations and can optionally share supported parts of the default configuration.
- Data: application data stays local, and code and conversations are not uploaded to Octoboard's own servers.
  Visitors use their own accounts and subscriptions or configured services for Claude Code, Codex and Grok Build.
  Those tools manage credentials and native conversation records and send prompts, code and related context to the
  configured providers under those providers' policies. Collaboration also passes task descriptions, progress and
  relevant context to the participating agents.
- Independent sessions: each project runs a native session in its own directory, loading local configuration through
  its agent's own mechanisms while the coordinator organizes the work. A single conversation can mix project context;
  changing directories or using subagents alone does not guarantee loading the target project's full configuration,
  and loading behavior depends on the tool.
- Agent brands: the coordinator and each project can use Claude Code, Codex or Grok Build independently, retaining
  their own instructions and toolchains. Tasks, relevant context and reports pass between sessions. The answer
  presents different brands' strengths as a reason to coordinate them.
- Participation: visitors can enter a project session to add context, adjust requirements or review code. Sessions
  started by the coordinator continue reporting results, open items and decisions needed back to it.
- Requirements and cost: visitors install and configure the agent CLIs themselves. Octoboard is free and open
  source under the MIT License; agents and model services follow the chosen providers' subscriptions or billing
  terms. Self-hosted model configurations remain usable when the chosen agent supports them.
- Remote hosts: running project agents on another server or computer is planned and unavailable.
- Parallel work: sessions share the project directory by default, so simultaneous edits to the same files can
  conflict. Users can ask agents to work in separate Git worktrees, giving changes separate working directories,
  and review those changes before merging.
- Portability: project files remain in their original folders. Available native conversation records can be resumed
  through the original CLI; an additional account requires using that account's configuration directory.

The primary action in the header, hero, and closing section uses the same localized label for viewing releases
and opens the repository's GitHub Releases page. It does not link directly to an installer file. The hero identifies
macOS Apple Silicon and labels the app as an early prototype. The closing note repeats that status beside the MIT
License. Other repository links open GitHub; the footer links to the developer's motivation at the localized
homepage's `#developer-intent` and to the localized privacy page, explicitly names the
MIT License in its license link, and provides the operator's email address. The hero, open-source section and footer
use icon-only GitHub links with 44-by-44-pixel targets, accessible names and hover titles.

The footer forms a compact centered group: the wordmark and localized tagline, followed by navigation links and
copyright. Its navigation has an accessible name identifying it as footer navigation. On small screens, the wordmark
and tagline stack vertically.

The closing section displays the mascot beside the wordmark, with the localized brand slogan as text under the
wordmark, centered on the wordmark. Under that group, centered, is the sentence that states the slogan's two
meanings (see "Brand slogan"). Decorative graphics do not act as slide or pagination controls.

The floating header stays visible while scrolling. Its inline navigation contains only the feature wall
(`#benefits`) and product preview (`#product-preview`), alongside language selection and the releases action.
The inline links are hidden at widths of 1200 pixels or less. These links also lead from the privacy page to the
localized homepage. The header has a glass surface with edge refraction where supported and a sheen that follows
the pointer. Browsers without refraction support use blurred glass; those without backdrop blur use a more opaque
surface.

On the homepage, a separate page directory links to the overview (`#overview`), feature wall (`#benefits`), product
preview (`#product-preview`), coordination explanation (`#how-it-works`), developer motivation (`#developer-intent`),
local data and open source (`#local-workspace`), platform availability (`#platforms`), questions (`#questions`), and
closing releases section (`#download`), in that order. Platform availability remains within the fifth numbered
section; its separate directory entry does not change the six section numbers. Links leave space for the floating
header above their destination. Scroll position identifies the active destination, and reaching the page bottom
identifies the closing section.

At widths of 1440 pixels or more, the directory is a fixed side rail outside the main content, on the right for
left-to-right languages and the left for Arabic. It remains visible while scrolling and highlights the current
section. The feature entry expands to show the three feature-group links while the feature section is active or
its links have keyboard focus; the active group is highlighted as well.

At narrower widths, a page-directory button in the header opens a nonmodal second-level navigation menu. It shows
all section links and all three feature-group links. The button also identifies the current section, except at
700 pixels or less, where only the page-directory label remains. Long labels wrap. The directory does not block
interaction with the rest of the page or lock its scrolling, and keyboard focus is not trapped.

Opening the directory focuses its first link. Selecting a destination closes the menu and moves focus to that
region; Escape returns focus to the directory button. Clicking or moving focus outside closes it. Shift+Tab from
the first link returns to the button; Tab from the last link closes the menu and continues to the language selector.
If the window becomes narrow while a rail link has focus, focus moves to the directory button. If it becomes wide
while the open menu holds focus, the menu closes and focus moves to the current page section. Both directory
presentations have accessible page-directory names and identify the active links with `aria-current="location"`.

## Product preview

The preview shows the actual application's interface populated with fictional data and identifies itself as a sample.
Three buttons select the project overview, focus mode, or code review; the project overview is the initial scene. All
three scenes show the same ongoing development and integration snapshot for the restaurant meal deal promotion across
the eight independent example projects named in "Homepage and navigation". They show the coordinating and project
sessions, a project in focus mode with its files and shell terminal, and a selected order-validation diff. Choosing a
scene resets the demonstration to that view of the shared snapshot. The coordinator's terminal gives the same progress
summary in every scene.

The focus-mode scene opens Restaurant order API's project focus mode, including its three sessions bound to the
coordinator. Its working Codex session is selected. On desktop, the Files and Terminal panels open together beside
the agent terminal, with Files above Terminal. The Terminal panel contains a sample `zsh` shell whose `pwd` and
`git branch --show-current` output match the project's directory and branch.

Seven sessions, including the coordinator, are working. The sessions for Promotion console and Checkout register await
instructions after completing their current checks, with integration feedback still pending. Restaurant order API's
order-validation review session awaits instructions while a test watcher (`pnpm test --watch`) runs in the background,
so it shows as working in background. Its order-checking session waits for the user to confirm the discount rule. These
statuses retain their application meanings in `docs/product/session-statuses.md`.

Each session's agent terminal imitates its agent's own terminal interface: Claude Code, Codex and Grok Build each
have a distinct header, prompt, tool-call rows, input area and hints. These looks are approximations and name no
model or version. A terminal shows the session's task prompt, then what its agent has done so far (file reads, edits
with their added and removed line counts and, for some, the changed lines, command runs and the agent's remarks),
then a state that follows the session's status:

- A working session shows a status line with a spinner and an elapsed timer, and appends further steps from its own
  short list 4 to 7 seconds apart; once the list is used up, only the spinner and timer keep running. Each session's
  timer starts from a different sample elapsed time. Progress counts from when the preview loaded, so reopening a
  session shows the steps already reached instead of replaying them, and a tab returning from the background catches
  up at once.
- A session awaiting instructions shows its finished summary and an empty input area; the order-validation review
  session's terminal also names its running background task (`pnpm test --watch`).
- The order-checking session waiting for the user shows its agent's question on whether meal deals can be combined
  with member discounts, with numbered choices and keeping them separate selected.

Visitors who prefer reduced motion see static terminals: no spinner motion, no appended steps and a fixed timer. The
file reads, edits, test results and the coordinator's diff statistics shown in the terminals are derived from the
sample repositories, so they agree with the files and diffs the visitor can open. The terminals carry no sample-data
line of their own; the caption or label described below identifies the preview as a sample.

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

Selecting the coordinator and opening its report panel shows a progress report in every scene. The report separates
each project's implemented results from its ongoing integration checks or pending feedback. It presents sample
figures of eight coordinated projects, 48 checks passed so far and one decision
awaiting confirmation: whether the meal deal can be combined with a member discount. The sample keeps the discounts
separate while other implementation and integration tasks continue. The code-review scene shows
`src/meal-deal-orders.ts` on the Restaurant order API project, with changes that calculate prices from an eligible
active offer, reserve available stock and prevent repeated requests from creating duplicate orders, alongside sample
tests and promotion rules.
These files and check counts are demonstration data.

Every agent counts as installed in the preview, and the rail's account quota panel shows sample readings of each agent's
default account: Claude Code on the Max plan and Codex on the Plus plan, each with a five-hour and an account-wide
weekly window, and Grok Build on SuperGrok with a weekly credit pool. Their reset times count from when the preview
loaded, so no visit finds a reset already past. These readings are demonstration data.

The homepage embeds a complete desktop viewport scaled to fit its available width. The preview reserves its final
responsive height while loading, keeping section-link destinations stable as it initializes. The application fills the
embedded viewport, with the sample-data explanation in a caption below it. Once ready, the embedded
application accepts pointer and keyboard interaction, including its report and project panes, and is available to
assistive technology. While loading, it remains hidden and cannot receive interaction. A continuous border with
four rounded corners frames the preview. At rest, its lower edge, including both application content and border,
fades into the page. Hovering over the preview removes the fade and reveals the complete border. After a pointer
click, moving the pointer outside restores the fade even if a control inside still has focus. Keyboard interaction
keeps the preview unfaded while focus remains inside it, including inside the report panel. The fade leaves focus,
text entry, selection and scrolling intact. Devices without hover show no fade.
The standalone link opens the selected scene and language in a new tab at `preview/website-preview.html`, relative
to the site's deployment root. The standalone preview displays its sample-data label above the application.

On devices whose primary pointer is touch, both previews keep text-entry fields read-only so tapping or automatically
focusing the terminal or another text field does not open the software keyboard. Buttons, menus, selection and scrolling
remain interactive. Devices whose primary pointer is a mouse or trackpad retain keyboard text entry, including
touchscreen desktops. This restriction applies only to the website previews, not to the application itself.

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
on narrow screens the sidebar is a drawer, while the report, Files and Terminal panels dock beside the agent terminal,
or, on phones, float over it one at a time and close with a tap beside them. The overview scene shows the agent terminal
with no panel open. On phones, the focus-mode scene initially floats the Terminal panel, and visitors can switch to
Files; switching floating panels leaves the desktop layout with both panels open. The session-list toggle opens the
project's focus cards. The code-review scene opens the Files panel and the sample
change in the diff viewer automatically, including on phones. The preview does not connect to a daemon, run real
agents, read or change real project files, or retain its preferences after a reload.
It does not replace the website's stored language preference. The application controls follow the selected supported
UI language; the fictional tasks, the agents' prose and task prompts in the terminals, and the report use Simplified
Chinese for that language and English for all others. What the agents' tools print in the terminals, such as line
counts, test results and key hints, stays in English in every language.

The embedded view shows a loading message until ready. A reported startup failure, or no readiness after 20 seconds,
shows a localized preview-specific error message that directs the visitor to choose a view above to retry. Selecting
another scene or the current scene again starts a new loading attempt. The standalone preview shows an alert if its
startup fails. JavaScript is required to load and operate either preview.

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

Platform availability appears within the local-data and open-source section, before the questions. Its five entries
have equal visual weight in a desktop row and become a compact list of icons and text on small screens. The macOS
entry uses brighter text and identifies the version as an early prototype. The section introduction describes the
macOS app as an early prototype still in development. The site does not give an installer date.

Status: not implemented — the Linux server and the Windows, iOS and Android apps. Each appears with a platform icon
and no download or availability date.

Linux is a planned server, not another app version beside Windows, iOS, and Android, and not a Linux desktop app. The
platform introduction describes it as a server accessed from another machine's browser. It is not available. The Linux
row has its own server-specific planned status. Windows, iOS, and Android remain planned and not available, and those
rows use the shared planned badge. The homepage Markdown includes the same introduction and the same status for each
row.

## Motion and accessibility

The hero text and mascot enter with a short motion. On both mobile and desktop, the mascot's eight tentacles
swing independently, its body floats up and down and rocks from side to side, and its red eyes brighten and dim.
Red and silver pixel particles move through the surrounding space without covering content or intercepting
interaction. Sections reveal as they enter view, except the feature wall and the developer's motivation, whose
text remains visible without a reveal transition, including at high zoom. Question expansion and control state
changes also have visual transitions.

A control beside the mascot pauses and resumes the homepage's decorative animation, including the mascot, particles and
workflow signals. The mascot and particles pause when their area is offscreen or the page is in the background; workflow
signals do the same for the diagram. They resume when visible again unless the visitor has paused them. The workflow's
signals repeat along the connections rather than implying a real task's progress; selecting a project remains
available while motion is paused.

A reduced-motion preference disables decorative animations and transitions, hides the particles and motion
control, and leaves the content visible. The header keeps a static blurred glass surface without refraction or a
moving sheen. Without JavaScript, the pre-rendered homepage and privacy content remain readable, and questions
can still be expanded and collapsed; section reveals never leave that content hidden. The diagram retains its
initial selection and, on wide screens, static connections. Selecting another node, the product preview and
opening the page-directory menu require JavaScript. The header provides a skip link to the main content, and
interactive controls, including questions and project nodes, show keyboard focus.

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
  uses Google Analytics to understand website visits, with cookies and usage data sent to Google; GitHub Pages also
  logs visitor IP addresses for security, with a link to GitHub's privacy statement;
- information voluntarily supplied through email or GitHub Issues, including the visibility of public issues.

The local-data explanation is not a promise that all workflow data remains on one device. There is no separate
website terms page; the software's license is linked separately from the privacy policy.

## Remote host support

Status: not implemented. The privacy page identifies remote host support as running agents on another server or
computer, says it is planned and unavailable, and excludes it from the policy's scope.

Remote host support runs in one direction only: the daemon the user is talking to reaching a second machine to run
sessions there. The server form runs the other way — one daemon, on a server, with the user's browser on another machine
— and is not what this section or the privacy page's remote-host disclosure describe. The homepage's platform
information describes that planned server (see "Platform availability"). The privacy page does not. The site's
local-data statements describe the machine the daemon runs on, which in the server form is the server rather than the
machine in front of the user, and a privacy page covering that form has to say that the interface then travels over the
network between the server and the user's browser. That addition has not been written.
