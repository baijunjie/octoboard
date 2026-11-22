# Website download page

The website's download page, where every download of Octoboard is offered, the addresses that always lead to the newest
download, and where the site's download links point. The rest of the site is in `docs/product/website.md`; what a
release holds is in `docs/product/installing-the-desktop-app.md` and `docs/product/installing-the-server-form.md`.

## Download page

The download page is `/download/` in English and `/<locale>/download/` in every other language, with the route codes in
`docs/product/website.md`, "Pages and languages"; it is translated into all 17 languages. It shares the site's header,
footer and analytics consent prompt (see `docs/product/website-privacy.md`, "Analytics consent"), and has no page
directory. Its heading names the download of Octoboard, and its introduction says that the macOS and Windows apps and
the Linux server are in beta.

**Tabs.** The page has three panels, macOS, Windows and Linux (the last titled as the Linux server), each with its
platform icon. Without JavaScript all three are shown one after another, each under its own heading, so every panel is
readable. With JavaScript they become tabs, with only the selected panel shown. The tab selected first is the one the
address's fragment names — `#macos`, `#windows` or `#linux` — and otherwise the one for the visitor's system, judged
only once the page has loaded: Windows for Windows, Linux for Linux, macOS for macOS and for anything else, phones and
tablets included. Selecting a tab puts its fragment in the address without adding a history entry, and changing the
fragment selects that tab. The tabs follow the usual keyboard pattern: only the selected tab is in the tab order, the
arrow keys move between tabs (mirrored in Arabic's right-to-left layout) and wrap around, and Home and End go to the
first and last tab; moving selects the tab.

Each panel keeps to a summary and links to that system's installation guide on the documentation site, which holds
the installation steps, options and troubleshooting (see "Pages and sidebar" in `docs/product/documentation-site.md`).
Every link into the documentation site opens it in the page's language.

**macOS panel**: a download button for macOS, then that it is for Apple silicon and Intel Macs on macOS 11 or later,
then a "Read the installation guide" link to the documentation site's macOS guide.

**Windows panel**: a download button for Windows, then that it is for x64 on Windows 10 version 21H2 or later, and that
the installer downloads Microsoft Edge WebView2 when it is missing, which needs an internet connection. Once a release
carries the arm64 installer (the Windows on Arm download below resolves to a file rather than the releases page), the
panel says that Windows 11 on Arm takes the arm64 installer below and shows a "Download for Windows on Arm" link to it;
before that, it says instead that on Windows 11 on Arm the x64 app runs under emulation, and shows no such link. Last
comes a "Read the installation guide" link to the documentation site's Windows guide.

**Linux panel**: an introduction says that it installs `octoboardd`, the Octoboard daemon, on a Linux server without
root, reachable from the macOS or Windows app or from a browser over SSH, and that it supports x86_64 and aarch64 on any
distribution. Under an "Install on the server" heading, the install command,
`curl --proto '=https' --tlsv1.2 -fsSL <site origin>/install.sh | sh` (see "The install script" in
`docs/product/installing-the-server-form.md`), is in a horizontally scrollable, focusable block with a Copy button. A
successful copy turns the button's label to Copied for 2 seconds and announces it to screen readers; a copy the browser
refuses changes nothing, the command staying selectable. Links to each architecture's archive, x86_64 and aarch64,
follow, in the website's download-button appearance, wrapping onto new lines where needed. Then come a "Read the
installation guide" link to the documentation site's Linux server guide, a "Read the SSH connection guide" link to its
Connect over SSH guide, and the terminal demonstration (below).

The page's agent-readable Markdown is described in "Search indexing and agent-readable copies" in
`docs/product/website-search-indexing.md`.

Commands and code read left to right in every language, Arabic included. Under the panels, a link opens the
repository's releases page, listing every release.

### The terminal demonstration

An animation of installing and opening the Linux server: a terminal on the server runs the install command, prints the download and
installation, starts the daemon with `octoboardd start` and prints its address with `octoboardd address`, followed by
the line saying to forward it with the SSH command it names; then a terminal on the visitor's own machine appears and
runs that forward of the daemon's socket (without `-N`, so that the SSH session opens a shell on the server); then a
browser window appears, open at that address and showing a picture of the Octoboard interface. Each terminal carries a
translated caption of its step; the commands and output are in English. The output lines are ones the real tools print,
with an example version, key and process id; the install script's are kept the same as the script's own messages for a
first install, leaving out the warning it prints on the error output when it cannot check the signature. Commands are
typed at an uneven, human pace.

It plays once, from the start, when the page loads, and runs only while it is on screen and the page is visible,
resuming where it was. A replay button on the server terminal plays it again from the start. With a reduced-motion
preference it does not play and has no replay button, showing its final state, as it does without JavaScript. The
animation is hidden from assistive technology, which reads a translated text alternative describing the same steps and
saying that the installation guide explains each one.

## Where download links point

Every download link of the site — the macOS and Windows buttons, the Linux archive links, and the links in the
agent-readable copies (see `docs/product/website-search-indexing.md`, "Search indexing and agent-readable copies") —
points at the newest version's file on its versioned release, never at a fixed version. Which file is settled when the
site is generated, from the Stable manifests (see "Where updates come from" in `docs/product/in-app-updates.md` and
"What a release holds" in `docs/product/installing-the-server-form.md`):

- **macOS**: the disk image `Octoboard_<version>_universal.dmg` of the version `stable.json` announces;
- **Windows**: the installer `stable-windows.json` names for `windows-x86_64`, and for Windows on Arm the one it names
  for `windows-aarch64` (a manifest without that entry, published before the arm64 build, leaves that one link on the
  releases page);
- **Linux x86_64 and aarch64**: the archives `stable-linux.json` names for `linux-x86_64` and `linux-aarch64`.

A manifest that does not exist yet (answered `404`) leaves its platform's links pointing at the repository's releases
page. Any other failure — no answer within 15 seconds, another error status, a manifest without a valid version or
entry, or a file address that is not an `https` download of the repository's own releases — fails the generation when
it runs in CI (the `CI` environment variable set and not `false` or `0`), so a publication never goes out with stale
or fallback links unnoticed; generated elsewhere, the links fall back to the releases page with a warning.

Since the links are settled at generation, the site is published again after every release: once a release has been
published, the release workflow starts the website's publication from `main` (see `docs/product/website.md`,
"Publishing"). If that start fails, the release stays published and the website's publication is run by hand.

## Fixed download addresses

Five addresses always lead to the newest download:

| Address | Leads to |
|---|---|
| `/download/macos/` | the macOS disk image |
| `/download/windows/` | the Windows x64 installer |
| `/download/windows-arm64/` | the Windows arm64 installer, or the releases page while no release carries one |
| `/download/linux-x86_64/` | the Linux x86_64 archive |
| `/download/linux-aarch64/` | the Linux aarch64 archive |

Each is a page that redirects at once to the file, as settled above, with a plain link to it behind the redirect for a
browser that does not follow it. They are not localized (their few words are in English), request `noindex, nofollow`,
are not in the sitemap and have no language variants, Markdown copy or structured data of their own. The software
application's structured data lists them as its download addresses (see `docs/product/website-search-indexing.md`,
"Search indexing and agent-readable copies").
