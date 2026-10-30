# Website

The official website presents Octoboard, its developer's motivation, and its privacy policy. It is a static public
site, separate from the application: visiting it does not connect to a local daemon, open a real agent session, or
require an account.

Its search indexing and agent-readable copies are described in `docs/product/website-search-indexing.md`, its
product preview in `docs/product/website-product-preview.md`, and its privacy disclosures and analytics consent in
`docs/product/website-privacy.md`.

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
The application fragments in the feature wall use the application's translations: their controls and labels are
currently available in English and Simplified Chinese, with English fallback in the other website languages.

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

Every card has a title, description and a focused visual, identified as illustrative sample data.
Five visuals show application interface fragments: accounts, usage readings, session statuses, existing workflow
files, and a Git diff. The status example puts a session waiting for the user first, a session awaiting instructions
second, and a working session last, following the application's ordering. The other eleven visuals illustrate the
capability with account choices, conversation and handoff records, context usage, task continuity or coordination
relationships. The quota-recovery example shows waiting, confirmation that quota is available, and continuing work;
it does not treat a reset time alone as proof of recovery or imply that the task is finished.

These visuals are static presentations: they accept no pointer or keyboard interaction and are hidden from
assistive technology, which reads each card's title and description instead. Their accounts, tasks, files and
readings are fictional; they do not connect to a daemon, run commands or agents, read real files, or change stored
preferences. The application fragments load when approaching the viewport, with a loading placeholder until ready
and a localized error if startup fails or takes more than 20 seconds. They require JavaScript; the card titles,
descriptions and other illustrations remain readable without it. The separately embedded product preview retains
the interactions described in `docs/product/website-product-preview.md`, "Product preview".

Cards form three columns on wide screens, two at widths of 1050 pixels or less, and one at 600 pixels or less. The
last card, reviewing files and Git diffs, spans the full row with its visual beside its text, stacking them on
phones. Each group has a numbered heading and decorative icon. The feature wall, the homepage Markdown and the
homepage structured-data list present the same features in the same order.

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
MIT License in its license link, provides the operator's email address, and ends with the analytics settings button
described in `docs/product/website-privacy.md`, "Analytics consent". The hero, open-source section and footer use
icon-only GitHub links with 44-by-44-pixel targets, accessible names and hover titles.

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
homepage. The static GitHub Pages fallback is in English, works without JavaScript, and preserves the requested missing
address rather than replacing it with a different URL. Its return-home link opens the English homepage under the site's
deployment root. Its share image is the English one in `docs/product/website-search-indexing.md`, "Search indexing and
agent-readable copies".

Errors encountered within the running site use localized messages, distinguishing missing pages from other loading
errors. These error views also provide a return-home link. Error pages request exclusion from search indexing
and from following their links (`noindex, nofollow`). The static error route is not included in the
sitemap, and error pages do not link a Markdown copy, point at `llms.txt`, or carry structured data.
