# Documentation site

The documentation site holds Octoboard's how-to guides: what Octoboard is, installing it on each system, and connecting
to it on another machine. It is a static public site, separate from the application and from the website's own pages,
served under `/docs/` of the website's address and published together with the website (see
`docs/product/website.md`, "Publishing"). The website keeps short summaries that link into it (see
`docs/product/website.md` and `docs/product/website-download-page.md`). Visiting it does not connect to a daemon or
require an account.

## Addresses and languages

Paths below are relative to the website's deployment root. Every page address ends with a slash.

- English: `/docs/` and `/docs/<page>/`, without an `en` prefix. `/docs/` is the introduction.
- Other languages: `/docs/<tag>/` and `/docs/<tag>/<page>/`, where the tag is the language's tag from "The offered
  languages" in `docs/product/language.md`, the same 17 languages as the application and the website. Simplified Chinese
  is `/docs/zh-CN/`, Traditional Chinese `/docs/zh-TW/`.

The first release covers all 17 languages: every page exists in every language. A page a language does not have shows
the English page at that language's address, marked as not translated yet, so a page added later may start in English
alone. Arabic pages are laid out right to left; commands and code read left to right in every language.

A language selector names each language in that language and opens the current page in the chosen one. The docs open
in the language their address names: they do not follow the language the website remembers or detects. The website's
links into the docs open them in the language of the website page they are on.

The two Chinese languages were earlier published under `/docs/zh/` and `/docs/zh-Hant/`. Every page there is a redirect
page to the same page under `/docs/zh-CN/` or `/docs/zh-TW/`, carrying the visitor's query and fragment along. It is in
English, offers a plain link for a browser that does not follow the redirect, requests `noindex, nofollow`, and is not
in the docs' sitemap.

## Pages and sidebar

The sidebar has three groups, in this order:

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

Group labels are translated; each page's entry is its title in the language being read. The pages cover:

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

### How the guides are written

The guides are written for readers who are not technical. Where a guide needs them, it explains its terms in a
collapsed "New to this?" block, lists what is needed before starting, gives numbered steps, says how the reader knows a
step worked, and ends with what to do when it does not.

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
`/docs/sitemap-index.xml`, listing every page in every language (17 × 8 = 136 addresses) and none of the redirect
pages; the website's `robots.txt` names it (see "Search indexing and agent-readable copies" in
`docs/product/website-search-indexing.md`). The docs have no Markdown copies or `llms.txt` of their own; the website's
agent-readable files link to their pages.

A missing address under `/docs/` gets the website's 404 page (see "Missing pages and errors" in
`docs/product/website.md`).
