# Internationalization in code conventions

How code produces text the user reads: catalogs, keys, lookup, placeholders, plurals and the website's own catalogs. How
copy is worded and translated (register, terminology, capitalization, per-language rules) stays in the
`knowledge-i18n-copy` skill; where the UI standard and the skill disagree the UI standard wins
(`docs/conventions/ui-standard.md`). Provider-specific copy placement is in `docs/conventions/provider-neutral-core.md`;
coded daemon failures and how the UI words them are in `docs/conventions/error-codes.md`. Paths are under
`packages/ui/src/i18n/` unless they say otherwise.

## What is translated, and where it lives

- **Every string the user reads in `packages/ui` is a message in `messages/en.ts` and in every catalog registered in
  `CATALOGS`**: labels, tooltips, accessible names, toasts, notifications and the native menu's labels. A translated
  catalog is typed `Translation<...>` with the plural categories its language needs, so a missing message or category
  fails `pnpm --filter ./packages/ui typecheck`, and `catalog.test.ts` checks placeholders and plural categories.
- **The English catalog is the source.** Message keys and their placeholders are typed off it (`MessageKey`,
  `MessageParams`); a catalog is a TypeScript module `messages/<language>.ts`. A message a language lacks, and a
  language with no catalog, render in English (`FALLBACK_LANGUAGE`).
- **Only what the user reads in Octoboard's own UI is translated.** Text aimed at an agent (a console session's
  instructions, tool descriptions and replies, launch prompts) stays English and is not catalog copy, and a report
  page's content is not localized; see `docs/product/language.md`, "What follows the language".
- **A provider's own copy (`provider.<id>.*`) lives in that provider's presentation module, not in the core catalogs.**
  The copy a provider setting needs (its `heading` at least) is in `docs/conventions/provider-neutral-core.md`.
- **A HeroUI part given no visible text gets a catalog message of your own.** Read its file, following the parts it
  delegates to, for a hardcoded English string (`Tag.RemoveButton` "Remove tag", `CloseButton` "Close"), which both
  ships untranslated and shadows the name react-aria would supply, and pass your message over it. React-aria's built-in
  names have no hi, id, th or vi text and fall back to English, so pass a catalog message through its own prop
  (`incrementAriaLabel`, `aria-label`) wherever react-aria would supply a name.
- **Before shortening or replacing a message, grep the catalog for the fact it carries and read the product doc of the
  surface.** A clause may be the UI's only statement of a fact; keep it, or move it into copy the surface shows anyway.
- **An error alert's opening sentence adds to the title or is absent.** Do not write a `.summary` that repeats the
  title; mark a code that is only raised under such titles `noSummary: true` in `errors/codes.ts`.

## Looking messages up

- **A component looks a message up through `useT()`** (`react.tsx`); the module-level `t()` (`language.ts`) is only for
  code outside React. A component calling `t()` keeps the old language after the user switches, with no error.
- **A helper that returns display text takes the `Translate` function as its first parameter** (`statusLabel(t, status)`
  in `sessions/sessionLabel.ts`); a module-level table (options, menu items, rows) stores a `PlainMessageKey` and looks
  it up at render, never text resolved when the module loads.
- **A sentence with a variable part is one message with a `{placeholder}`, never fragments joined in code.** Word order,
  particles and cases differ by language (ko particles, de cases, tr vowel harmony, ja order). Only independent phrases
  (labels, buttons) get their own keys. Not: `` `${t("x.deleted")} ${name}` ``.
- **Markup inside a sentence goes through `<Message id params>`**, so the translation decides where it goes; markup,
  attribute names and entities inside a string are never translated, added or removed.
- **Anything that depends on a count is a plural message selected by `count`**, never `count === 1 ? ... : ...`.
- **Two whole sentences in one paragraph are joined by `common.sentenceSeparator`**, never a hard-coded space; see
  `docs/conventions/ui-standard.md`, "Two sentences in one paragraph". A state that is said in one breath elsewhere
  is a `.sentence` message of its own, not two messages joined (`docs/conventions/ui-standard.md`, "An empty state is a
  glyph, a heading, and what fills it").
- **An independent phrase with one meaning across the app is a shared key** (the search field's `common.search` is set
  in `docs/conventions/ui-standard.md`, "Input placeholders"); a word read differently per scenario gets a
  key per scenario; an action and a state are different keys.
- **No catalog carries a pending variant of a button's label**: see `docs/conventions/ui-standard.md`, "Buttons".
- **A word the user types to confirm is one string**, used for the prompt and for the comparison
  (`components/dialog/ConfirmDialog.tsx`), so the two cannot drift apart.

## Keys

- **Do not rename a key for tidiness.** Delivered translations are matched by key, so a rename makes every language's
  translation fail to match. Whether a change is polish (keep the key and the old translations) or a change of meaning
  (a new key, the old one left) is judged as the skill's "Changing existing copy" says; when only some uses of a shared
  key change, move those uses to a new key.

## Placeholders, numbers and plurals

- **A translation keeps exactly the English placeholders** (names and types; only the order may change) and never
  translates a placeholder's name. Placeholders are written `{name}`; `catalog.test.ts` checks it.
- **Global placeholders are filled by `format` and `<Message>`, and a caller never passes them**: `{appName}` (from
  `config/app.json`), the product names (`{git}`, `{tauri}` and each provider's `params`), `{osName}` and the platform
  words (`{trash}`, `{systemSettings}`, ...), which come from the host. The product name is never a literal and never
  translated (`catalog.test.ts` refuses the name from `config/app.json` in any catalog's key or message); when copy
  names it at all is in `docs/conventions/ui-standard.md`, "Naming the application in copy".
- **Numbers, dates, times, currencies and measurements are not written into a message.** The platform formatter produces
  them (`Intl`, the helpers in `src/format/`) and they enter through a placeholder; a number placeholder is formatted
  for the language by `format`.
- **A plural message carries every CLDR category its language needs** (ar zero/one/two/few/many/other, ru
  one/few/many/other), never only one/other and never an English plural `s` appended after a placeholder.
- **A value in left-to-right script inside an RTL sentence is isolated**: see `docs/conventions/ui-standard.md`, "Text
  direction" (`isolate()`, `<bdi dir="auto">`).
- **Case-insensitive comparison of text the daemon also folds uses `toLowerCase()`**, never `toLocaleLowerCase()`
  (project and agent tags, account names, anything the daemon deduplicates with Rust's `to_lowercase()`). Under a
  Turkish locale `I` folds to `ı`, and the UI and daemon then disagree on duplicates. Locale-aware forms are for
  comparisons that concern only the user's own language, as `ConfirmDialog`'s typed word does.

## Website copy

- **The website's copy is `apps/web/i18n/locales/<language>.json`**: a flat object of keys to strings, English the
  source, placeholders written `{name}`. It is separate from the UI's catalogs and is not typed;
  `pnpm --filter @octoboard/web check:locales` checks that every supported language has a file with exactly English's
  keys and placeholders.
- **A Vue component looks copy up through `useSiteCopy().t`**, which fills `{appName}` and `{operatorName}` from the
  runtime configuration; `useI18n` is for the current locale only. The product and operator names, and the identity and
  contacts from `config/app.json`, are never literals in a catalog or a component.
