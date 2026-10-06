# Internationalization Development Plan

## Problem and approach

Octoboard speaks English only: the desktop UI's text, the macOS menu, system notifications and the errors the daemon
reports are all hard-coded English, and the website has no language list yet. This plan makes Octoboard available in
17 languages across all of them: the desktop UI, the native menu and notifications, the daemon's user-facing messages
and the website.

## Key design decisions

- **17 languages**: ar, de, en, es, fr, hi, id, it, ja, ko, pt-BR, ru, th, tr, vi, zh-Hans, zh-Hant. The desktop
  application and the website use this same list.
- **en is the base language and the fallback**: when the system language maps to none of the 17, the UI is English.
  The fallback is named on its own, never taken as the list's first entry.
- **The list is ordered alphabetically by language tag** (not by display name). This is the order of the language
  options in Settings and of the website's language menu, and it decides ties: when several scripts of one language
  match, the earlier one wins, so zh-Hans comes before zh-Hant and a bare `zh` with no script or region maps to
  simplified Chinese.
- **The language follows the system by default, and Settings lets the user pick one by hand**: Settings offers
  "System" plus the 17 languages. The choice is remembered per client in that client's own storage, as the appearance
  choice is; the daemon neither stores nor carries it.
- **"The current language" is always the language the UI actually renders, never the system locale**: when the system
  language is not in the list, the UI falls back to English while the system locale stays as it is; everything
  localized — the native menu, notifications, the daemon's messages as shown — follows the UI, not the system.
- **How a system language maps onto the list** (the website uses the same rules for a browser's languages, without the
  first one):
  - Legacy language codes are normalized first (`in` → `id`, `iw` → `he` and the like).
  - A different language never matches.
  - Chinese with only a region gets the script that region conventionally uses: TW / HK / MO → Hant, CN / SG → Hans;
    other regions get none.
  - A script conflict never matches (zh-Hant never lands on zh-Hans); when either side has no script, the language
    alone decides, so pt-PT and a bare `pt` map to pt-BR.
  - A language that maps to nothing is dropped, and the UI falls back to English.
- **Plurals follow CLDR**: every message with a count gives every plural category each language requires.
- **ar mirrors the whole UI**: directional icons (forward, back, external-link arrows) point the mirrored way; an
  expand / collapse chevron switches between its "forward" and "down" glyphs instead of rotating the forward one (which,
  already mirrored to point left, would point up after a quarter turn). The terminal's content is never mirrored.
- **The daemon reports user-facing errors and notices as stable codes with parameters**, and the UI renders the text
  in the current language. The protocol changes accordingly.
- **Translations are internationalization resources** and stay in their own language in the repository, as the
  project's language rule allows.

## Milestones

1. [Language foundation in the desktop UI](01-language-foundation.md)
2. [Daemon messages as codes](02-daemon-message-codes.md)
3. [Native menu in the current language](03-native-menu.md)
4. [Translations and right-to-left](04-translations-and-rtl.md)
5. [Website languages](05-website-languages.md)

## Open

- Which i18n library `packages/ui` uses for its message catalog (it must handle CLDR plurals and interpolation, and
  feed the locale to HeroUI / react-aria's own built-in strings).
- How the native menu gets its labels: a catalog of its own in the shell, or labels the UI hands it through the
  platform adapter.
- What the UI shows for a daemon code it does not know (for instance a newer daemon than the UI): the daemon's English
  text carried alongside the code, or a generic message.
- Whether a report page in the report panel is mirrored under ar. A page is model-authored HTML with its own direction.
- Whether text aimed at the agents rather than the user (the hub's instructions, tool descriptions, injected prompts)
  is localized. This plan covers only what the user reads in Octoboard's own UI.
- The website's URL scheme per language and its default language, which follow the website's own plan.
