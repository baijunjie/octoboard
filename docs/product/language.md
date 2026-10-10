# Language

## The offered languages

Octoboard offers 17 languages, identified by these language tags:

`ar`, `de`, `en`, `es`, `fr`, `hi`, `id`, `it`, `ja`, `ko`, `pt-BR`, `ru`, `th`, `tr`, `vi`, `zh-Hans`, `zh-Hant`

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
| `pt-PT`, `pt` | `pt-BR` |
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
- the text of the system notifications the UI composes, such as the one for a session waiting for the user (see "The
  raised hand" in `docs/product/sessions.md`);
- the errors and notices the daemon reports. The daemon reports each as a stable code with its details, and the UI
  words it in the current language; a console, project or session it mentions is shown by that record's current name
  (by its id when the UI does not know the record). A report the UI has no wording for — one from a daemon newer than
  the UI — is shown in the daemon's own English;
- numbers and dates the UI formats, such as a report page's creation time in the report panel's bar;
- the UI components' own built-in text and formatting;
- in the macOS application, the application menu: every item Octoboard defines in it — the Octoboard menu's About,
  Settings…, Services, Hide, Hide Others, Show All and Quit, and the Edit, View and Window menus with their items.
  A change of language relabels the menu at once, on every screen the window shows. The product name in the labels
  is never translated. At launch the menu is English until the window's UI has loaded. The items macOS adds to
  these menus by itself — AutoFill, Start Dictation…, Emoji & Symbols, Close All — are not Octoboard's and do not
  follow the language; they are currently shown in English;
- in the macOS application, the menu bar icon's menu: its group headings, its session lines' wording, Open and Quit
  (see "The icon's menu" in `docs/product/menu-bar-icon.md`). Like the application menu, it is relabelled at once on a
  change of language and is English until the window's UI has loaded; the product name, and the icon's tooltip, which
  is the product name alone, are never translated;
- the document's language and writing direction. Arabic (`ar`) is the one right-to-left language, and choosing it
  mirrors the window (see "Right-to-left layout" in `docs/product/window-layout.md`).

What does not follow it:

- **A report page.** It is content the console session's model wrote, with its own language and direction, and it is never
  mirrored under a right-to-left language; only the panel around it follows the UI (see `docs/product/report-panel.md`).
- **The terminal.** What the agent draws is laid out left to right whatever the language, and is never mirrored.
- **Text aimed at the agents** — the console session's instructions, the tool descriptions and the tools' replies, the prompts
  injected into a launch — stays English whatever the language. Only what the user reads in Octoboard's own UI is
  localized.
- **What a daemon error quotes from elsewhere** — a path, an agent's name, the operating system's or `git`'s own
  message — is shown as it came, inside the localized wording.

## Where the choice is stored

The choice is remembered **per client**, in that client's own browser storage, as the appearance choice is (see "Where
the choice is stored" in `docs/product/appearance.md`). The daemon neither stores it nor carries it. A desktop window
and a browser tab, or two machines reaching the same daemon, each keep their own. A client whose storage has been
cleared picks the language from the system again on its next load. Without that storage nothing is kept: every load
picks the language from the system, and a choice in Settings lasts until the page is reloaded.

## What is translated so far

**English and Simplified Chinese (`zh-Hans`) are complete**: under either, all the text listed in "What follows the
language" above — the UI's own text, the notifications it composes, the daemon's errors and notices, the macOS
application menu and the menu bar icon's menu — is in that language.

**The other 15 languages have no translations yet.** Choosing one, or the first launch picking one, shows that text in
English, while the numbers and dates the UI formats, the document's language and the writing direction still follow the
choice. Under Arabic, therefore, the window is fully mirrored but its text is English.

Status: not implemented — translations into `ar`, `de`, `es`, `fr`, `hi`, `id`, `it`, `ja`, `ko`, `pt-BR`, `ru`,
`th`, `tr`, `vi` and `zh-Hant`.
