# Internationalization in code conventions

How code produces text the user reads, in the UI and on the website. How copy is worded and translated is the
`knowledge-i18n-copy` skill; coded daemon failures are `docs/conventions/error-codes.md`. Paths are under
`packages/ui/src/i18n/` unless they say otherwise.

## General principles

- **Every string the user reads is a catalog message, and English is the source.** Text in code ships untranslated.
- **A sentence is one message.** Word order, particles and cases differ by language, so code never assembles one.
- **Never rename a key for tidiness.** Delivered translations are matched by key, so a rename makes every language's
  translation fail to match.

## What is translated, and where it lives

- **Every string the user reads in `packages/ui` is a message in `messages/en.ts` and in every catalog registered in
  `CATALOGS`**: labels, tooltips, accessible names, toasts, notifications and the native menu's labels. A translated
  catalog is typed `Translation<...>`, so a missing message or plural category fails the package's typecheck, and
  `catalog.test.ts` checks placeholders and plural categories.
- **A language is identified everywhere by its tag in `languages.ts`** (Chinese is `zh-CN` and `zh-TW`); only the macOS
  bundle maps them to Apple's identifiers, in `apps/desktop/scripts/native-localizations.mjs`.
- **Only what the user reads in Octoboard's own UI is translated.** Text aimed at an agent (a console session's
  instructions, tool descriptions and replies, launch prompts) stays English and is not catalog copy, and a report
  page's content is not localized; see `docs/product/language.md`, "What follows the language".
- **A provider's own copy (`provider.<id>.*`) lives in that provider's presentation module, not in the core catalogs**
  (`docs/conventions/provider-neutral-core.md`).
- **A HeroUI part given no visible text gets a catalog message of your own.** Read its file, following the parts it
  delegates to, for a hardcoded English string (`Tag.RemoveButton` "Remove tag", `CloseButton` "Close"), which both
  ships untranslated and shadows the name react-aria would supply, and pass your message over it. React-aria's built-in
  names have no hi, id, th or vi text and fall back to English, so pass a catalog message through its own prop
  (`incrementAriaLabel`, `aria-label`) wherever react-aria would supply a name.

## Looking messages up

- **A component looks a message up through `useT()`** (`react.tsx`); the module-level `t()` (`language.ts`) is only for
  code outside React. A component calling `t()` keeps the old language after the user switches, with no error.
- **A helper that returns display text takes the `Translate` function as its first parameter** (`statusLabel(t, status)`
  in `sessions/sessionLabel.ts`); a module-level table (options, menu items, rows) stores a `PlainMessageKey` and looks
  it up at render, never text resolved when the module loads.
- **A sentence with a variable part is one message with a `{placeholder}`, never fragments joined in code.** Only
  independent phrases (labels, buttons) get their own keys. Not: `` `${t("x.deleted")} ${name}` ``.
- **Markup inside a sentence goes through `<Message id params>`**, so the translation decides where it goes; markup,
  attribute names and entities inside a string are never translated, added or removed.
- **Anything that depends on a count is a plural message selected by `count`**, never `count === 1 ? ... : ...`.
- **Two whole sentences in one paragraph are joined by `common.sentenceSeparator`**, never a hard-coded space: the
  separator is the language's (none after 。), and VoiceOver drops a text node that is only whitespace, so the space
  belongs to the text node of the sentence after it (`<Message>` keeps a separator between two placeholders out of a
  node of its own where it can). A state said in one breath elsewhere is a `.sentence` message of its own, not two
  messages joined.
- **An independent phrase with one meaning across the app is a shared key**; a word read differently per scenario gets
  a key per scenario; an action and a state are different keys.

## Keys

- **A change of meaning updates every catalog in the same change and keeps the key**; only when some uses of a shared
  key change do those uses move to a new key, and a key nothing uses any more is deleted, not left: every translation
  lives in the repository and is type-checked against the English, so no delivered translation can go stale.

## Placeholders, numbers and plurals

- **A translation keeps exactly the English placeholders** (names and types; only the order may change) and never
  translates a placeholder's name. Placeholders are written `{name}`; `catalog.test.ts` checks it.
- **Global placeholders are filled by `format` and `<Message>`, and a caller never passes them**: `{appName}` (from
  `config/app.json`), the product names (`{git}`, `{tauri}` and each provider's `params`), `{osName}` and the platform
  words (`{trash}`, `{systemSettings}`, ...), which come from the host. The product name is never a literal
  (`catalog.test.ts` refuses it in any catalog's key or message).
- **A word a language's vocabulary in `src/i18n/platformTerms.ts` provides is used through its placeholder, never
  written out** (Japanese and Korean `{appWindow}`, with a form such as `{appWindowWith}` where the sentence needs
  one): the vocabulary differs between the daemon host's systems, and a literal word is right on only one of them.
- **Numbers, dates, times, currencies and measurements are not written into a message.** The platform formatter produces
  them (`Intl`, the helpers in `src/format/`) and they enter through a placeholder; a number placeholder is formatted
  for the language by `format`.
- **A plural message carries every CLDR category its language needs** (ar zero/one/two/few/many/other, ru
  one/few/many/other), never only one/other and never an English plural `s` appended after a placeholder.
- **Case-insensitive comparison of text the daemon also folds uses `toLowerCase()`**, never `toLocaleLowerCase()`
  (project and agent tags, account names, anything the daemon deduplicates with Rust's `to_lowercase()`). Under a
  Turkish locale `I` folds to `ı`, and the UI and daemon then disagree on duplicates. Locale-aware forms are for
  comparisons that concern only the user's own language, as `ConfirmDialog`'s typed word does.

## Website copy

- **The website's copy is `apps/web/i18n/locales/<language>.json`, looked up through `useSiteCopy().t`** (which fills
  `{appName}` and `{operatorName}`; `useI18n` is for the current locale only); `check:locales` holds every language to
  English's keys and placeholders and refuses the product name in a message.
- **The product and operator names, and the identity and contacts from `config/app.json`, are never literals in a
  catalog or a component.**
