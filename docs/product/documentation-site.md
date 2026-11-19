# Documentation site

The documentation site holds Octoboard's how-to guides: what Octoboard is, installing it on each system, connecting
to it on another machine, and how each of its features works. It is a static public site, separate from the
application and from the website's own pages, served under `/docs/` of the website's address and published together
with the website (see `docs/product/website.md`, "Publishing"). The website keeps short summaries that link into it (see
`docs/product/website.md` and `docs/product/website-download-page.md`). Visiting it does not connect to a daemon or
require an account.

## Addresses and languages

Paths below are relative to the website's deployment root. Every page address ends with a slash.

- English: `/docs/` and `/docs/<page>/`, without an `en` prefix. `/docs/` is the introduction.
- Other languages: `/docs/<tag>/` and `/docs/<tag>/<page>/`, where the tag is the language's tag from "The offered
  languages" in `docs/product/language.md`, the same 17 languages as the application and the website. Simplified Chinese
  is `/docs/zh-CN/`, Traditional Chinese `/docs/zh-TW/`.

The pages of the Start here, Installation and Remote connections groups exist in all 17 languages. The Features pages
exist in English and Simplified Chinese only. A page a language does not have shows the English page at that language's
address, marked as not translated yet; it is still listed in that language's sidebar and in the sitemap. Arabic pages
are laid out right to left; commands and code read left to right in every language.

A language selector names each language in that language and opens the current page in the chosen one. The docs open
in the language their address names: they do not follow the language the website remembers or detects. The website's
links into the docs open them in the language of the website page they are on.

The two Chinese languages were earlier published under `/docs/zh/` and `/docs/zh-Hant/`. Every page there is a redirect
page to the same page under `/docs/zh-CN/` or `/docs/zh-TW/`, carrying the visitor's query and fragment along. It is in
English, offers a plain link for a browser that does not follow the redirect, requests `noindex, nofollow`, and is not
in the docs' sitemap.

## Pages and sidebar

The sidebar has four groups, in this order. The Features group holds three subgroups:

| Group | Page | Path |
|---|---|---|
| Start here | Introduction | `/docs/` |
| Installation | macOS | `install/macos/` |
| Installation | Windows | `install/windows/` |
| Installation | Linux server | `install/linux/` |
| Remote connections | Choose how to connect | `choose-how-to-connect/` |
| Remote connections | Connect over SSH | `remote-connections/` |
| Remote connections | Browser over Tailscale | `browser-over-tailscale/` |
| Remote connections | Browser over UU Remote | `browser-over-uu-remote/` |
| Features › Accounts and usage | Managed accounts | `features/managed-accounts/` |
| Features › Accounts and usage | Account continuity | `features/account-continuity/` |
| Features › Accounts and usage | Quota scheduling | `features/quota-scheduling/` |
| Features › Accounts and usage | Usage monitor | `features/usage-monitor/` |
| Features › Accounts and usage | Start usage windows automatically | `features/start-usage-windows/` |
| Features › Accounts and usage | Shared configuration | `features/shared-configuration/` |
| Features › Context and continuity | Context usage and fresh sessions | `features/fresh-session/` |
| Features › Context and continuity | Agent handoff | `features/agent-handoff/` |
| Features › Context and continuity | Open in the agent's app | `features/open-in-agent-app/` |
| Features › Agents and projects | Cross-project coordination | `features/cross-project-work/` |
| Features › Agents and projects | Native project sessions | `features/native-project-sessions/` |
| Features › Agents and projects | Session status and intervention | `features/session-intervention/` |
| Features › Agents and projects | Process monitoring | `features/process-monitoring/` |
| Features › Agents and projects | Prevent sleep | `features/keep-awake/` |
| Features › Agents and projects | Reviewing changes | `features/review-changes/` |

Group and subgroup labels are translated; each page's entry is its title in the language being read. The Features group
and its three subgroups carry, in every language, the same words as the website's Features navigation label and the
titles of its feature wall's three groups (see "Homepage and navigation" in `docs/product/website.md`), and its 15
pages follow the feature wall's 15 cards in the same order. The website does not link to the Features pages. The pages
cover:

- **Introduction**: what Octoboard is, that it runs the user's own coding agents rather than including one, and which
  guide to read first for each situation.
- **macOS** and **Windows**: the system requirements, a download button, the installation steps (on Windows also the
  per-user install, the Microsoft Defender SmartScreen warning the unsigned installer may raise, and WebView2), installing
  a coding agent (on Windows with Git for Windows for Claude Code and Codex), and that later versions arrive through the
  app's own updates.
- **Linux server**: signing in to the server with SSH, installing and starting `octoboardd`, then connecting from the
  desktop app or a browser; the PATH caveat; keeping it running as a systemd user service with lingering; the default
  port and `--port`; upgrading; and the archive downloads.
- **Choose how to connect**: a comparison of SSH, Tailscale and UU Remote — where each is used from, which hosts it
  supports, what is turned on or installed on each computer, which service the traffic passes through, whether it works
  across networks, and what each is best for.
- **Connect over SSH**: connecting the desktop app to a Linux server, using another Mac or Windows PC as the host,
  connecting from a browser through an SSH forward, troubleshooting, security and working in a remote window.
- **Browser over Tailscale** and **Browser over UU Remote**: reaching a host's browser address through that relay
  without SSH sign-in on the host, with the commands for each computer, troubleshooting, turning it off and security.
- **Features** pages, one per feature:
  - **Managed accounts**: adding several accounts for each agent, signing them in and keeping them organized.
  - **Account continuity**: moving a session to another account of the same agent while keeping the conversation, by
    hand or automatically at the usage limit.
  - **Quota scheduling**: how each account's remaining usage and reset time decide which account takes new work and
    when a stopped task continues.
  - **Usage monitor**: every account's remaining usage and reset countdowns in one view.
  - **Start usage windows automatically**: starting a usage window's countdown soon after it resets with one small
    request, so the next reset is not pushed later.
  - **Shared configuration**: letting accounts of one agent share skills, plugins and settings, and seeing each
    account's connectors and MCP servers.
  - **Context usage and fresh sessions**: how full an agent's context is, starting the next stage in a fresh session,
    and looking back at the earlier conversation.
  - **Agent handoff**: moving a session's work to a new session of another agent, which takes over its place from a
    handoff record, and the compatibility switches that let one agent load another's setup.
  - **Open in the agent's app**: opening a stopped session's conversation in the Claude or Codex desktop app on the
    computer that runs the daemon.
  - **Cross-project coordination**: giving one request to a console session, which starts a session in each project it
    touches, follows them and brings their reports together.
  - **Native project sessions**: each session being the agent's own command-line program in one project, so the
    project's instructions, skills, hooks and MCP servers load as usual.
  - **Session status and intervention**: telling which sessions are working, waiting or at rest, being called to the
    ones waiting for the user, and typing straight into their terminal.
  - **Process monitoring**: the processes agents start, with their memory, ports and local web links, and stopping
    them when a session ends unless they are kept.
  - **Prevent sleep**: keeping the computer that runs the daemon awake, and where the system allows it its display from
    turning off or locking, so agents keep working while the user is away.
  - **Reviewing changes**: browsing a project's files and the Git changes of any local branch beside the session, read
    only.

### How the guides are written

The guides are written for readers who are not technical. Where a guide needs them, it explains its terms in a
collapsed "New to this?" block, lists what is needed before starting, gives numbered steps, says how the reader knows a
step worked, and ends with what to do when it does not.

Each Features page opens with a paragraph stating what the feature is for, followed by a lead screenshot. Then come a
"Why it works this way" section, whose first point is that intent and whose others state the feature's rules and
limits, a "How to use it" section of numbered steps (Account continuity splits it into switching by hand and switching
automatically) and a "Next" list of related Features pages. The screenshots show the application in English, in its
light theme, with fictional sample data: every language's page shows the same English images, with the images'
descriptions in the page's language. A screenshot is shown at the size the interface really has, never enlarged, and
shrinks only to fit a narrower column. Some show the whole window; others show one element alone, mostly set on a
transparent margin so its rounded corners and shadow show whole.

The commands, addresses and values the guides quote are the ones the website and the application use: the install
command names the deployment's own address (see "The install script" in `docs/product/installing-the-server-form.md`),
the download buttons and links use the website's fixed download addresses and releases page (see "Fixed download
addresses" in `docs/product/website-download-page.md`), and the default port is the daemon's, 3369. Controls the guides
name are named by the application's own labels.

The relay guides are written for any host they support: Tailscale for a Mac, a Windows PC or a Linux server, UU Remote
for a Mac or a Windows PC only, since UU Remote has no port mapping on Linux. Both, and the comparison page, warn:

- that their steps have not been fully tested between two devices;
- to use a relay only on computers nobody else signs in to, since another user account on the host or on the reader's
  own computer could catch the browser's key. They add that Connect over SSH does not have this weakness on a Mac or
  Linux host, nor on the reader's own computer in the desktop app, while a browser on the reader's computer still has it.

## Look and navigation

The site has its own styling, apart from the website's. Its title is the website's mascot logo, then the pixel
wordmark "Octoboard", then the word for "Docs" in the page's language — the same word as the website's Docs navigation
label (Docs, 文档, Dokumentation, الوثائق and so on). Assistive technology reads it as the wordmark followed by that
word, such as "Octoboard 文档". The lockup keeps its left-to-right order in every language, is scaled down to fit a
narrow header so that it always shows in full, and opens the docs' home in the language being read.

Browser tab titles are the page's title followed by the site title, which is worded as each language says it, such as
"Octoboard Docs", "Octoboard 文档", "Documentación de Octoboard" or "Документация Octoboard"; `og:site_name` is the same
site title. The 404 page, built once in English, keeps the English site title. The site's icon is the website's.

The accent colour is the website's red, `#ff4b3e`, in the dark theme, where it also colours links, the title's word and
the current sidebar entry; the light theme uses a darker red, `#d92d20`, where the brighter red would not contrast
enough against white. The header offers full-text search, a link to the repository on GitHub, the theme and the language
selectors.

## Search indexing

Each page names its own canonical address and has language alternates for all 17 languages plus an `x-default`
alternate, the English page; the language values are the language tags. The docs publish their own sitemap index,
`/docs/sitemap-index.xml`, listing every page in every language, including the pages shown in English as not
translated yet (17 × 23 = 391 addresses), and none of the redirect pages; the website's `robots.txt` names it (see
"Search indexing and agent-readable copies" in `docs/product/website-search-indexing.md`). The docs have no Markdown
copies or `llms.txt` of their own; the website's agent-readable files link to their pages.

A missing address under `/docs/` gets the website's 404 page (see "Missing pages and errors" in
`docs/product/website.md`).
