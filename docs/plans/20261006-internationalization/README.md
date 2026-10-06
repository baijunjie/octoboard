# Internationalization Development Plan

## Problem and approach

Octoboard speaks English only: the desktop UI's text, the macOS menu, system notifications and the errors the daemon
reports are all hard-coded English, and the website has no language list yet. This plan makes Octoboard available in
17 languages across all of them: the desktop UI, the native menu and notifications and the daemon's user-facing
messages. The website takes the same list in its own development plan.

The mechanism is built first on macOS with English and simplified Chinese translated; the other 15 languages are
translated before the application ships.

## Key design decisions

- **17 languages**: ar, de, en, es, fr, hi, id, it, ja, ko, pt-BR, ru, th, tr, vi, zh-Hans, zh-Hant. The desktop
  application and the website use this same list.
- **en is the base language and the fallback**: when the system language maps to none of the 17, the UI is English.
  The fallback is named on its own, never taken as the list's first entry.
- **The list is ordered alphabetically by language tag** (not by display name). This is the order of the language
  options in Settings and of the website's language menu, and it decides ties: when several scripts of one language
  match, the earlier one wins, so zh-Hans comes before zh-Hant and a bare `zh` with no script or region maps to
  simplified Chinese.
- **The first launch picks the language from the system, and from then on only the user's choice counts**: with no
  language stored yet, the system's preferred languages are mapped onto the list and the result is stored; Settings
  offers the 17 languages, and a later change of the system's languages changes nothing. The choice is remembered per
  client in that client's own storage, as the appearance choice is; the daemon neither stores nor carries it.
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
- **The message catalog is the UI's own typed catalog, no i18n library**: each language is a TypeScript module shaped
  like the English one, plural messages give the CLDR categories their language needs and are selected with
  `Intl.PluralRules`, so the type checker is what catches a missing message or plural category; the current language
  is handed to HeroUI / react-aria through its `I18nProvider`.
- **The native menu takes its labels from the UI** through the platform adapter, so there is one catalog.
- **A daemon code carries its English text alongside**: the UI shows the English text for a code it does not know (a
  newer daemon than the UI, say).
- **A report page is not mirrored under ar**: it is model-authored content with its own direction, as hosts of
  third-party content generally leave it; only the panel around it follows the UI.
- **Text aimed at the agents stays English** (the hub's instructions, tool descriptions, injected prompts), as agent
  tools generally keep their system prompts; this plan covers only what the user reads in Octoboard's own UI.
- **Translations are internationalization resources** and stay in their own language in the repository, as the
  project's language rule allows.

## Milestones

1. Language foundation in the desktop UI (closed)
2. Daemon messages as codes (closed)
3. Native menu in the current language (closed)
4. Translations and right-to-left (closed)
6. [Remaining translations](06-remaining-translations.md)
