# Language

## The offered languages

Octoboard offers 17 languages, identified by these language tags:

`ar`, `de`, `en`, `es`, `fr`, `hi`, `id`, `it`, `ja`, `ko`, `pt`, `ru`, `th`, `tr`, `vi`, `zh-Hans`, `zh-Hant`

The list is ordered **alphabetically by tag**, not by each language's name. That order is the order of the options in
Settings, and it breaks ties when a system language matches more than one entry (see "How the system's languages map
onto the list" below).

**English (`en`) is the fallback language**: what the UI renders when none of the system's languages maps onto the
list, and what any message missing from a language's translation is shown in. The fallback is a choice of its own; it
is not taken from the list's order.

## Choosing the language

The language is chosen in the Language row of Settings' General section (see `docs/product/settings.md`). The options
are the 17 languages in list order; there is no option that follows the system. Each language is named in that
language itself (日本語, Deutsch, العربية and so on), so a user can find theirs whatever the UI currently shows. A
choice takes effect at once, in the open window.

**The default is picked from the system on the first launch.** When the client has no language stored, the UI maps
the system's preferred languages onto the list (see below) and stores the result as if it had been chosen. From then on
only a choice in Settings changes the language: a later change of the system's languages changes nothing, in the macOS
application and in a browser alike.

The first pick in the macOS application uses the system's full list of preferred languages. The window's web view on
its own would only know the first preference, which would put a user whose first language is not offered on English
even when a later preference is offered; the application therefore hands the web view the system's full list at
launch.

In the macOS application, the words a screen reader speaks for a control's role ("button", "tab") follow the system's
language, not the choice made in Settings: they are in the first of the system's preferred languages that is one of the
17 offered, and in English when none is. Only the UI's own text follows the choice.

## How the system's languages map onto the list

The system's preferred languages are taken most preferred first, and **the first one that maps onto the list decides**.
A preference that maps to nothing is skipped; when none maps, the UI is English.

One preferred language maps onto the list like this:

- Tags are compared case-insensitively, and `_` is accepted in place of `-` (`zh_HK` is `zh-HK`). A tag that is not a
  well-formed language tag maps to nothing.
- Retired language codes are normalized first: `in` → `id`, `iw` → `he`, `ji` → `yi`, `jw` → `jv`, `mo` → `ro`. So `in`
  maps to Indonesian.
- A different language never matches.
- Chinese with a region but no script takes the script that region conventionally uses: `TW`, `HK` and `MO` mean
  Traditional (`Hant`), `CN` and `SG` mean Simplified (`Hans`). Any other region implies no script. A script written in
  the tag itself wins over the region (`zh-Hant-CN` is Traditional).
- A script conflict never matches: a Traditional Chinese preference never lands on `zh-Hans`, and `zh-Latn` maps to
  nothing. When either side has no script, the language alone decides.
- When several entries match, the earlier one in list order wins.

Some consequences:

| System language | UI language |
|---|---|
| `en-GB` | `en` |
| `fr-CA` | `fr` |
| `pt-PT`, `pt` | `pt` |
| `zh-TW`, `zh-HK`, `zh-MO`, `zh-Hant` | `zh-Hant` |
| `zh-CN`, `zh-SG`, `zh`, `zh-Hans` | `zh-Hans` |
| `nl` only | `en` (nothing maps) |
| `nl`, then `fr-CA` | `fr` (the first that maps) |

## What follows the language

**"The current language" is always the language the UI actually renders, never the system's own locale.** When the
system's language is not offered, the UI is English while the system stays as it is, and everything localized follows
the UI rather than the system.

What follows the current language:

- every piece of text the UI itself shows: labels, tooltips, accessible names, dialogs, toasts, status labels and
  Settings;
- the default name when the UI creates a session (see "The session dialog" in `docs/product/opening-sessions.md`);
- the text of the system notifications the UI composes, such as the one for a session waiting for the user (see "The
  raised hand" in `docs/product/session-statuses.md`);
- the errors and notices the daemon reports. The daemon reports each as a stable code with its details, and the UI words
  it in the current language; a console, project, session or shell terminal it mentions is shown by that record's
  current name, a shell terminal's being its title (by its id when the UI does not know the record). A report the UI has
  no wording for — one from a daemon newer than the UI — is shown in the daemon's own English;
- numbers and dates the UI formats, such as a report page's creation time in the report panel;
- the UI components' own built-in text and formatting;
- in the macOS application, the application menu: every item Octoboard defines in it — the Octoboard menu's About,
  Settings…, Services, Hide, Hide Others, Show All and Quit, and the Edit, View and Window menus with their items.
  A change of language relabels the menu at once, on every screen the window shows. The product name in the labels
  is never translated. At launch the menu is English until the window's UI has loaded. The items macOS adds to
  these menus by itself — AutoFill, Start Dictation…, Emoji & Symbols, Close All — are not Octoboard's and do not
  follow the language; they are currently shown in English;
- in the desktop application, the menu bar icon's menu (the notification-area icon's on Windows): its group headings,
  its session lines' wording, its quota submenus' titles and lines, the update entry, Open and Quit (see "The icon's
  menu" in `docs/product/menu-bar-icon.md`). Like the application menu, it is relabelled at once on a change of
  language and is English until the window's UI has loaded. The icon's accessible name — on Windows its tooltip — follows the language too (see "The icon's look" and
  "The notification-area icon on Windows" in `docs/product/menu-bar-icon.md`); without a quota it is the product name
  alone. The product name and the
  agents' names are never translated;
- the document's language and writing direction. Arabic (`ar`) is the one right-to-left language, and choosing it
  mirrors the window (see `docs/product/right-to-left-layout.md`).

What does not follow it:

- **The system's permission prompts.** The explanations for access requested by programs running in the
  application follow macOS's native language choice independently of the window, with the coverage described in
  "Other consents programs ask for" in `docs/product/macos-privacy-permissions.md`.

- **Existing session names.** A created name keeps its saved wording when the UI's language changes, including one that
  used the localized default when it was created (see "The session dialog" in `docs/product/opening-sessions.md`).
- **A report page.** It is content the console session's model wrote, with its own language and direction, and it is
  never mirrored under a right-to-left language; only the panel around it follows the UI (see
  `docs/product/report-panel.md`).
- **The terminal.** What the agent draws is laid out left to right whatever the language, and is never mirrored.
- **Text aimed at the agents** — the console session's instructions, the tool descriptions and the tools' replies, the
  prompts injected into a launch — stays English whatever the language. Only what the user reads in Octoboard's own UI
  is localized. What a session is told of the user's language names it, in English, without translating anything else
  (see "The guidance on the user's language" in `docs/product/launching-agents.md`).
- **The prefilled Issue report.** The title and body **Report issue** fills in are English whatever the language,
  being written for the maintainers (see `docs/product/issue-reports.md`); the guidance around it in the alert follows
  the language.
- **What a daemon error quotes from elsewhere** is shown as it came: a path or an agent's name inside the localized
  wording, and the operating system's, `git`'s or an agent's own message never inside it but apart, as raw text (see
  "Names and diagnostic details" in `docs/product/error-alerts.md`).

### The system's own words

Where the UI names something of the operating system's — the place a removal moves things to, the settings app, the
icon Octoboard puts in the system's bar, the system itself — it uses that system's own word for it, in the current
language: the Trash and the Recycle Bin, System Settings and Settings, the menu bar icon and the notification area
icon, macOS, Windows and Linux. The product docs call these the Trash, System Settings and the menu bar icon whatever
the system. The file manager is called the file manager on every system.

Which system's words are used depends on where the thing named is. What happens on the machine the daemon runs on — a
folder moved to the Trash, a setting to change there, the system named in a
daemon's error — is worded in that machine's terms, which the daemon announces with its host facts, since a browser
may be on another machine; until a daemon has announced them, and for a daemon too old to announce one, the window's
own are used. What concerns the window's own machine — its notifications, its permissions, its menu bar icon — is
worded in the window's own terms.

## Where the choice is stored

The choice is remembered **per client**, in that client's own browser storage, as the appearance choice is (see "Where
the choice is stored" in `docs/product/appearance.md`). A desktop window and a browser tab, or two machines reaching the
same daemon, each keep their own, and each renders its own whatever the daemon holds (see "The language the daemon
holds" below). A client whose storage has been cleared picks the language from the system again on its next load.
A configuration export that includes the application settings carries the language the exporting client shows, and an
import asked to apply them switches the client that runs the import to it, when that client offers it (see "The
settings a client holds" in `docs/product/settings-in-a-configuration.md`).
Without that storage nothing is kept: every load picks the language from the system, and a choice in Settings lasts
until the page is reloaded.

## The language the daemon holds

The daemon also holds one language, taken as **the user's language**: the language sessions are told to talk to the user
in (see "The guidance on the user's language" in `docs/product/launching-agents.md`). It is not a client's: there is one
value for every client, the one reported most recently, and none until a client reports one. It is stored as
`interface_language`, a tag from the 17 offered (see "The offered languages" above); the daemon refuses any other tag.

A client reports its language:

- **when the user picks one in Settings**. The window changes language whatever becomes of the report; a report the
  daemon does not receive or refuses is shown as an error and is not sent again later;
- **on connecting, only while the daemon holds none yet**, so the daemon starts from the language the client shows.
  A client never reports on connecting otherwise, so clients set to different languages do not overwrite each other at
  every start: the daemon keeps the most recent report.

The first launch's pick from the system is not reported by itself; it reaches the daemon only through the report on
connecting. A configuration export that includes the application settings carries the language the exporting client
shows in its place, and an import asked to apply them sets the held language to it when it is one of the offered
languages, leaving it as it is otherwise (see "Application settings are left alone unless they are asked for" in
`docs/product/settings-in-a-configuration.md`). A console session reads it and changes it through
the settings tools, under the same rule about offered tags (see "Application settings: `get_settings` and
`update_settings`" in `docs/product/agent-settings-tools.md`); such a change, like a client's report, changes no
window's own language.

## What is translated so far

**All 17 offered languages are complete**: under each, all the text listed in "What follows the language" above —
the UI's own text, the notifications it composes, the daemon's errors and notices, the macOS application menu and the
menu bar icon's menu — is in that language. Arabic combines translated text with the mirrored window.

English remains the fallback for a missing translation; a daemon report the UI has no wording for is shown in the
daemon's own English, as described above.
