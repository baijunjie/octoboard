---
name: knowledge-i18n-copy
description: UI copy conventions for a multilingual app. Use when writing, translating or reviewing UI copy and localized strings (Localizable.xcstrings, strings.xml, web i18n resources), and for "change this copy", "fill in the missing translations", "make the wording consistent", "how should this confirmation dialog read", "how should this error message read", "Title Case or sentence case".
---

# UI copy conventions

In every language, write short, write smoothly, and make it readable in one pass. Sentences are finished; buttons use
short words. No slang, no unfinished fragments.

Match the source language in the facts stated and in length, but use the target language's own UI idioms for the
sentence structure. **Do not carry the source language's sentence frame across word by word** (the 「确认」 in the
Chinese 「确认删除该文件？」 is a Chinese construction; de simply writes Diese Datei löschen?).
When reviewing, read every string through as a whole sentence and judge it against that language's register; do not
just scan for keywords.

Where the project's own copy conventions or glossary conflict with this document, the project's win. Values marked
"default" here suit a typical consumer app; a project with a different positioning (B2B, finance and so on) may
change them. Report anything that neither this document nor the project's conventions settle, or that you are unsure
of.

## Register per language

Simplified Chinese uses short written-style sentences to cure the convoluted, long-winded phrasing of spoken Chinese;
that is not the register for every language. Pushing another language one notch toward officialese or honorifics
usually makes it longer, the opposite of "write short".
Once a register is chosen for a language, use it throughout, without mixing.

| Language | Default register | Avoid |
|---|---|---|
| zh-Hans | Short written-style sentences | Convoluted spoken phrasing, Japanese word order, the Japanese middle dot 「・」 |
| zh-Hant | Short sentences, Taiwan usage by default, 「你」 for the user; the same concepts as Simplified, worded with Traditional UI vocabulary | Simplified officialese, Simplified vocabulary carried over as is |
| en | Complete sentences with common contractions | Slang, fragments |
| ja | です/ます sentences, short button names (キャンセル, 後で) | Stacked honorifics (させていただきます), plain form (〜かも) |
| ko | Statements in -(스)ㅂ니다 (없습니다), requests in 해 주세요, buttons as nouns (삭제, 취소) | Mixing -세요 / -(으)십시오 in one product, 반말 |
| fr | vous, keep the ne of negation | Administrative tone (il convient de) |
| ru | вы (lowercase mid-sentence), simple verbs | Officialese (при осуществлении) |
| ar | Short Modern Standard Arabic sentences | Dialect, long classical sentences |
| id | Anda, short imperatives; keep Anda dapat | Formal words (mohon kesediaan) |
| hi | Everyday आप; keep the loanwords UIs commonly use (ऐप, कैमरा) | Purely literary Sanskritized Hindi |
| tr | Plain polite form (Silinemez) | Officialese suffixes (gerçekleştirilememektedir) |
| de | du, verbal sentences | Nominalized openings (Bei Aktivierung → Wenn du das einschaltest) |
| es | tú | Mixing tú / usted in one product |
| it | tu | Mixing tu / Lei in one product |
| pt-BR | você | Over-formal address such as o senhor |
| th | Omit the pronoun | ท่าน, ครับ / ค่ะ on every sentence |
| vi | Pronoun-less imperatives | bạn có thể or quý khách added only for politeness |

Buttons use the verb form the language's UIs conventionally use, not an honorific command to the user:

| Language | Button form | Examples |
|---|---|---|
| de, fr, es, pt-BR, ru | Infinitive | Löschen, Supprimer, Eliminar, Apagar, Удалить |
| it | Second-person singular imperative | Elimina, Annulla |
| ja | Noun or short name | 削除, キャンセル; not 削除してください |
| ko | Noun | 삭제, 취소; 해 주세요 only in explanatory sentences |

When the user's gender is unknown, do not default adjectives and participles to the masculine (connecté, connesso,
conectado); use a gender-free construction instead (a noun, an infinitive, a different subject). Words that agree with a
noun agree with it in gender and number (invariati / invariate).

### Simplified Chinese

| Don't | Do |
|---|---|
| 这个文件删不掉 | 无法删除该文件 |
| 要删除吗？ | 确认删除该文件？ |
| 好的 | iOS 「好」, Android and web 「确定」 (each following the system dialog) |
| 用过 N 次, 使用 N 次 | 已使用 N 次; as a label, 「使用次数」 + the value |
| 第 1/共 2 件 | 第 1 件（共 2 件） |
| 先去…再删 | 请先… |
| 匹配 100% | 匹配度 100% |

「即可」 is short written style; keep it.

### Traditional Chinese

Do not carry over Simplified officialese:

| Don't | Do |
|---|---|
| 「該」 as a demonstrative (該檔案) | 此; leave the 「該」 in words such as 「應該」 alone |
| 屆時 | 到時 |
| 所限 | Spell the limit out |

Do not convert Simplified vocabulary straight into Traditional characters:

| Simplified | Traditional (Taiwan) |
|---|---|
| 文件 (a computer file) | 檔案 |
| 视频 | 影片 |
| 软件 | 軟體 |
| 默认 | 預設 |
| 网络 | 網路 |
| 信息 (information) | 資訊 |
| 信息, 消息 (a message) | 訊息 |
| 质量 | 品質 |

### English

| Case | How to write it |
|---|---|
| Common contractions | Use them, e.g. can't, don't, isn't, aren't, doesn't, didn't, it's, that's, there's, you're, won't, you'll, couldn't |
| Uncommon contractions | Don't use them: could've, it'll |
| Emphatic warnings | Don't contract: cannot, do not, it is |
| you have | Contract to you've only where have is an auxiliary (You've changed…); not where it means possess (You've more space ✗) |
| Noun + verb | Don't contract onto the noun: the computer's not → the computer isn't; the possessive 's is unaffected |
| Object it + is | Don't contract to it's: The best way to find it is to ask them. |

A contraction replaces only the one word it stands for, and takes that word's case: in a Title Case element write
Couldn't, Don't, without lowercasing the whole string.

### Pitfalls in other languages

| Language | Rule |
|---|---|
| de | When switching to du, change by case: nominative Sie → du, accusative Sie → dich, dative Ihnen → dir, possessive Ihr- → dein- (with endings: Ihre → deine, Ihren → deinen); lowercase sie means "she" or "they" — leave it |
| it | With tu, lowercase lei means "she" — leave it; with Lei, the formal pronoun is capitalized |
| it | Write a condition as a full conditional clause (Se inserisci di nuovo lo stesso nome, viene ricreato), not a gerund clause |
| th | For a short "can't", use verb + ไม่ได้ (ลบไม่ได้), not ไม่สามารถ…ได้; it can also close a causal sentence. ไม่ได้ before the verb means "didn't" (ไม่ได้ลบ) — don't put it on the wrong side |
| th | Where it would produce a doubled negation (…ที่ไม่ได้ใช้ไม่ได้), keep ไม่สามารถ…ได้; keep the positive สามารถ |
| vi | bạn có thể meaning "you can" is the original meaning; keep it |
| ko | Statements already in -습니다 are not to be converted wholesale to 해요체 |
| ko | Keep the spaces Korean spacing rules require (할 수 없습니다, not 할수없습니다) |
| ko | A particle after a placeholder is written with both forms: 을(를), 은(는), 이(가), 과(와), (으)로 |
| ru | For something impossible by ability or rule, use нельзя + infinitive; for an attempt that failed this time, Не удалось… (Не удалось удалить) |
| tr | Convert case by tr-TR: i ↔ İ, ı ↔ I; never the default Unicode case mapping, including the all-caps word of a type-to-confirm |
| id | The prepositions di and ke are written apart from the noun (di rumah); as prefixes they are joined (dihapus) |

### Grammatically required contractions

These are not abbreviations to expand:

| Language | Keep |
|---|---|
| fr | du, des, au, aux |
| pt-BR | do, da, dos, das, no, na, nos, nas, ao, à, pelo, pela |
| it | The full set of preposition + article: del, al, dal, nel, sul, della, nella and so on |
| de | am, im, beim, vom, zum, zur, ans, ins |
| es | del, al; the pronoun él does not contract (de él, a él) |

## Text not held to the UI register

| Text | How to treat it |
|---|---|
| Voice assistant trigger phrases (Siri / Shortcuts / Google Assistant) | The user says them out loud and they are deliberately colloquial; exempt entirely |
| Legal text | No contractions, no rewriting into short UI sentences |
| Marketing and website promotion | A register of its own; passages shared with the UI still follow the UI register |

## Writing sentences

### Input placeholders

- Write only an example. Don't repeat the field name, and don't write "optional" — the label carries the field name.
  Choose the prefix by the kind of example:

  | Kind of example | How to write it |
  |---|---|
  | A format example: email, URL, domain | In every language, the example alone, no prefix: name@example.com; name and example may be replaced by local words |
  | A content example: a name, a place, a value and so on | Add the language's prefix, below |

  | Language | Prefix | Example |
  |---|---|---|
  | zh-Hans, zh-Hant | 例如： | 例如：室外 |
  | ja | 例: (half-width colon plus a space) | 例: 屋外 |
  | en | Ex. | Ex. Outside |
  | ko | 예: | 예: 실외 |
  | fr | Ex. : (a narrow no-break space U+202F before the colon) | Ex. : Extérieur |
  | de | z. B. (a no-break space in the middle) | z. B. Außen |
  | es | p. ej. | p. ej. exterior |
  | it | Es.: | Es.: Esterno |
  | pt-BR | Ex.: | Ex.: Área externa |
  | ru | Например: | Например: снаружи |
  | ar | مثال: | مثال: بالخارج |
  | id | Misalnya: | Misalnya: Di luar |
  | hi | उदाहरण: | उदाहरण: बाहर |
  | tr | Örneğin: | Örneğin: Dışarıda |
  | th | เช่น | เช่น ข้างนอก |
  | vi | Ví dụ: | Ví dụ: Bên ngoài |

- Make examples concrete enough to be really usable, and keep examples across fields consistent in meaning (if the
  name is a charger, the keyword is a power adapter, not something else).
- A search field does not correspond to a field to fill in, so it does not follow the example pattern; when written as
  a prompting question (找什么？ / What are you looking for?), address the user in that language's register (de Was
  suchst du?).

### Reference and scope

- Don't repeat an object the page has already scoped: on a page inside a team space, "all members" is enough; don't
  copy "all members of the team space" from the product docs.
- When one sentence covers several cases, use that one sentence ("this project" rather than separate "your project /
  the team's project"); split it only when "this affects other people" and nothing else on the screen says so.
- Don't hang a description of an object's nature ("visible to all members" and the like) on the description of an
  action such as a toggle or a clear.
- When one sentence lists several system permission names, quote either all of them or none; a single one mentioned on
  its own gets no quotes.
- When a UI name or permission name needs quoting, use the language's quotation marks:

  | Language | Quotes |
  |---|---|
  | zh-Hans | “” |
  | zh-Hant, ja | 「」 |
  | en | “” |
  | fr | « », with narrow no-break spaces inside |
  | de | „“ |
  | ru | «» |
  | es, it, pt-BR | As the project decides; “” by default |

### Destructive actions

Clear, delete, remove and discard each mean something different; one action uses one word on every platform and in
every place.

- A description under a button states only the scope or consequence the button's name cannot convey; a button whose
  name says it all gets no description. State only the consequences directly tied to this action; don't drag in other
  mechanisms.
- Choose the sentence pattern by what is to be stated. A consequence does not repeat the action itself (「清除后，将删除…」
  is ill-formed).

  | Stating | Pattern |
  |---|---|
  | Consequence | After <action>, <consequence>. <Which things> are not affected. |
  | Scope | Only <action>… |

- "Not affected" lists the specific things one by one, not a vague word that can be misread; other languages use a
  phrasing of "these things remain":

  | Language | Write | Not |
  |---|---|---|
  | en | stay the same, are kept | unaffected |
  | de | bleiben unverändert | sind nicht betroffen |
  | it | restano invariati | non sono interessati |

- Don't write "cannot be undone" unless the app really has a recoverable deletion it needs to be distinguished from.

Confirmation dialogs:

| Part | How to write it |
|---|---|
| Title | In Chinese 「确认<动作><对象>？」; in other languages, that language's action question. The action is the word on the button that triggered it, not a synonym; include the count or the name when there is one |
| Body, when the triggering button already has a description | Omit it; don't restate that description |
| Body, when the trigger has no description (swipe, toggle, menu item) | State the scope or consequence; omit it when there is nothing to state |
| Body that has to say what is not affected | First say what is removed; don't open with "<X> is not affected" |
| Confirm button | Named like the triggering button; when triggered by a toggle, name the knock-on consequence: "Turn Off and Clear". Never "Yes / No" |
| Default button | A destructive button is never the default and cannot be triggered by pressing Return |

### Descriptions under toggles

The description changes with the toggle's state; one sentence does not try to explain both on and off:

| Toggle state | What the description says |
|---|---|
| Off | What happens once it is turned on |
| On | What is happening right now |

The destructive consequences of turning it off do not go into the description; the turn-off confirmation states them.

### Empty-state titles

A phrase-style empty-state title has no closing punctuation, in any language: No Activity Yet; Chinese and Japanese
likewise take no 「。」.

## In-progress states

This governs "the action the user just triggered is still running".

- Chinese writes 「…中」 or 「正在…」; other languages use their own progressive form or a status noun, not a literal
  rendering of 「中」. Every language ends with an ellipsis (exceptions below), the single character `…` (U+2026), not
  three dots:

  | Language | Form | Example |
  |---|---|---|
  | zh | verb + 中, or 正在 + verb | 同步中…, 正在新建项目… |
  | ja | verb + 中 | 同期中… |
  | en | Present participle | Syncing… |
  | de | Passive progressive | Wird synchronisiert… |

- Write what is happening, not the button's own name:

  | Button | In progress |
  |---|---|
  | 新建项目 | 正在新建项目… |
  | 转为共享 | 转换中… |

- Each button in a group gets its own; they don't share a vague "Processing…".
- A status banner title takes an ellipsis; a title paired with a determinate progress count says "what this is" and
  takes none.
- A button in progress must still show visible text; it is never replaced wholesale by a bare spinner. Only an
  icon-only button swaps its icon for a spinner.

## Errors

- Always show a localized sentence the user can understand. The exception's own text (`message`,
  `localizedDescription` and the like) may never stand in for it.
- Each failure whose cause can be recognized gets its own copy; unrecognized ones get a generic sentence. The generic
  sentence says "please try again", so a full-screen failure with no retry on screen gets a separate sentence that
  does not suggest retrying.
- A dialog title says clearly "what did not get done", one sentence per action, never a vague "Operation failed".
- en failure titles: Couldn't for an attempt that did not succeed; Can't for something the rules do not allow.
  Capitalization follows the platform:

  | Platform | Example |
  |---|---|
  | iOS | Couldn't Clear History |
  | Android, web | Couldn't clear history |

- The body states the specific cause. Choose buttons by case:

  | Case | Buttons |
  |---|---|
  | Only a cause | One acknowledge button, named as the system dialog names it (for zh-Hans see the "Simplified Chinese" table) |
  | A cause with a way out the user can take in one tap | "Way out + Cancel", the way out being the default action; when the way out is destructive it is not the default |

- A load failure may never be expressed through an empty state ("No records yet" or a reading of 0). Use the error
  form instead: a warning icon + "Couldn't load" + the cause + "Try Again".

## Parentheses and spaces

- Chinese and Japanese use full-width parentheses （）, with no space outside: 第 3 件（共 5 件）, 按年（省 20%）.
  A project may switch Chinese to half-width; see below.
- A project that switches to half-width parentheses uses half-width throughout, never mixed with full-width. Spacing
  outside them depends on the language:

  | Language | Outside, next to Han characters | Outside, next to Latin text, digits or a placeholder |
  |---|---|---|
  | zh-Hans | No space: 联系人(%ld) | One half-width space: Microsoft Excel (XLSX) |
  | zh-Hant | One half-width space: 聯絡人 (%ld) | One half-width space |
  | ja | No half-width rule; use full-width | — |

- Other languages use half-width `()`, with one half-width space between them and the adjacent text outside, none
  inside.
- Add no space where the outside is at the very start or end of the string, next to the language's own sentence
  punctuation (。，、 and , . ; : ? !), or where a space is already there.
- Korean particles attach directly to the parenthesis: AI 스위치 (%1$s)를; “%1$s”을(를).
- The same applies to half-width parentheses concatenated in code: the space goes into the literal (`" ("`).
- Language-specific punctuation:

  | Language | Rule |
  |---|---|
  | fr | A narrow no-break space (U+202F) before : ; ! ? and %; capital letters keep their accents (Échec) |
  | es | Questions and exclamations take the opening ¿ ¡ |
  | ar | Question mark ؟, comma ،; write parentheses as plain () and leave mirroring to the bidi algorithm; never hand-write reversed parentheses |

- In Japanese, by default no space goes between an ASCII word and kana or kanji: Apple IDを作成, アカウントID. A
  project that adopts the half-width-space style (ID を表示, テナント ID) uses it throughout, never mixed.

## Capitalization

In languages with case, capitalization is decided by the text's **role** in the UI, not by the translation's grammar.

### Android and web

Sentence case throughout, whether an element name or a complete sentence: Android follows Material; the web does so by
default, and follows the project's design system where it says otherwise.

### iOS

Follows the title-style capitalization of the Apple Style Guide, with word choice taken from Apple's own iOS
localization for that language.

| Role | Capitalization |
|---|---|
| UI element names: navigation titles, buttons, menu items, segmented controls, tabs, list rows and toggle labels (when phrases), section headers, App Intent titles | Title Case |
| Fragments or short sentences with no closing punctuation in a title slot (alert, banner, empty state, sheet, upsell) | Title Case (iCloud Storage Is Full) |
| Fragment questions | Title Case (Delete Note?) |
| Complete-sentence titles with closing punctuation (including several sentences), and complete questions (Are you sure you want to…?) | Sentence case; prefer a fragment question for a confirmation title |
| Button in-progress states, badges, tags, readout values and status values | Title Case (Loading Plans…, Not Connected) |
| Complete sentences: descriptions, body text, footers, placeholders, screen-reader labels, and list rows and toggle labels that are themselves complete sentences | Sentence case |
| Fragments, plurals and substitution branches spliced into a sentence | Follow that sentence |

Title Case words per language:

| Language | Lowercase | Easily misjudged, actually capitalized | First and last word |
|---|---|---|---|
| en | Articles, coordinating conjunctions (and, but, or, nor), prepositions of four letters or fewer (to, at, by, for, in, of, off, on, out, up, via, from, into, onto, over, with, per) and as | Prepositions of five letters or more (About, Between, Through), subordinating conjunctions (If, When, Because), particles of phrasal verbs (Turn Off, Sign In) | Capitalized |
| id | dan, atau, di, ke, dari, untuk, yang, dengan, dalam, pada, secara, hingga, sampai, tentang, per, oleh | tanpa, antara | Capitalized |
| pt-BR | a, o, os, as, de, da, do, das, dos, e, em, no, na, nos, nas, ou, para, com, ao, à, um, uma, por, pelo, pela, pelos, pelas, até, sem, sobre, que, and contractions like desta / nesta / deste / neste | entre, contra, após | Capitalized |
| tr | Only ve, ile, veya, yahut, and the separately written clitics and question particles de, da, ki, ya (including ya da), mı, mi, mu, mü; an attached -de, -da, -ki is part of the word and is not split off | Other postpositions: için, kadar, gibi | Separately written clitics and question particles stay lowercase even first or last |

- de, fr, es, it, vi, ru and the other languages with case: use sentence case as Apple does, not Title Case.
- Words not in the table are always capitalized. Exception: in id and pt-BR, a preposition not in the table with no
  sample in Apple's localization is lowercased, but the words in the "easily misjudged, actually capitalized" column
  are always capitalized; tr has no such exception and recognizes only the list above (TDK Yazım Kılavuzu, "Büyük
  harflerin kullanıldığı yerler").
- en hyphenated compounds capitalize the second part (Add-On, Follow-Up, %lld-Day), except Built-in and Plug-in; in id
  and pt-BR each part is judged on its own.

### General

- Text inside parentheses is judged by its own role in the UI; being in parentheses does not change it.
- A word the user must type to confirm: all caps in languages with case; languages without case are unaffected.
- These do not change with the rules:

  | Kind | Examples |
  |---|---|
  | Brands and abbreviations | iCloud, AI, API, URL |
  | Plan names | — |
  | Official names of system permissions and system screens, exactly as the system writes them | Motion & Fitness, Settings |
  | Default data names, legal document names | — |
  | Capitalization required by grammar | German nouns, Indonesian Anda |

## Terminology

- One thing has one word across the whole app — the same in buttons, titles, errors and history. When renaming, find
  every key by "same page + same concept"; Chinese often drops the noun after a measure word (「N 条」), so search again
  in en and the other languages.
- When a page shows several things that one short name could refer to, use the full name; only a page about a single
  kind of object uses the short name.
- Two easily confused concepts (such as a storage limit and a monthly quota) get two distinct words in every language;
  neither side borrows the other's word.
- The UI never shows implementation terms (token, database names and the like); use units and names the user
  understands.
- The app's own name comes from a placeholder fed by a single source of truth, never a literal in the copy.

## Across platforms

- One feature uses the same wording and the same level of detail on every platform (iOS, Android, web), with section
  headers and descriptions present or absent alike; only a feature some platform lacks gets wording of its own.
  Capitalization, system dialog names and platform-specific names (the system's "Settings" and the like) follow each
  platform's rules; word-for-word identity is not required.
- When syncing from one platform to another, first check that the facts in the sentence hold on the target platform
  (permission items, whether the feature exists), and fix grammar errors in the source on the source first.
- A sentence that points at a control on this platform uses that control's existing translated name on this platform.
- Placeholder syntax follows each platform. A string with two or more placeholders must number them (iOS `%1$@`,
  Android `%1$s`; the web uses ICU named arguments) so translations can reorder them.
- Give plurals all their CLDR categories (ar has zero, one, two, few, many, other; ru has one, few, many, other), not
  just one / other; never hand-append an English plural s after a placeholder.
- In Android strings, write an apostrophe as `\'` and a double quote as `\"` (aapt resource escaping); don't add another
  layer of backslashes, or they show up in the UI.
- When the website or legal pages mention a name from the app's UI, copy the app's actual translation language by
  language; when the app renames something, go back and sync it.

## This project

- **Languages**: 17 languages — `ar`, `de`, `en`, `es`, `fr`, `hi`, `id`, `it`, `ja`, `ko`, `pt-BR`, `ru`, `th`, `tr`,
  `vi`, `zh-Hans`, `zh-Hant`; only `en` and `zh-Hans` have catalogs so far, the other 15 render English.
  (`packages/ui/src/i18n/languages.ts`; `docs/product/language.md`, "What is translated so far")
- **Source language**: the English catalog is the source every other language is translated from, and the type of
  every message key and its placeholders is read off it. (`packages/ui/src/i18n/messages/en.ts`;
  `packages/ui/src/i18n/catalog.ts`)
- **Missing-translation fallback**: `en` — a language without a catalog, or a message a catalog lacks, renders in
  English. (`packages/ui/src/i18n/languages.ts`, `FALLBACK_LANGUAGE`; `packages/ui/src/i18n/catalog.ts`, `CATALOGS`)
- **Resources and commands**: catalogs are TypeScript modules in `packages/ui/src/i18n/messages/<language>.ts`,
  registered in `CATALOGS`; placeholders are named `{name}`, and a count-dependent message is a plural object by CLDR
  category. `pnpm --filter ./packages/ui typecheck` fails on a translated catalog missing a message or plural category;
  `pnpm --filter ./packages/ui exec vitest run src/i18n` (`catalog.test.ts`, also run by the package's `build`) checks
  plural categories and that each translation keeps the English placeholders. (`packages/ui/src/i18n/catalog.ts`; `packages/ui/src/i18n/catalog.test.ts`; `packages/ui/package.json`)
- **How UI code uses the catalog**: a component looks messages up through `useT()`; the module-level `t()` is only for
  code outside React. A helper that returns display text takes the `Translate` function as its first parameter, and a
  module-level table stores a `PlainMessageKey`, looked up at render. A sentence with a variable part is one message
  with a `{placeholder}`, never fragments joined in code; markup inside a sentence goes through `<Message id params>`;
  a count-dependent message is a plural message selected by `count`. (`packages/ui/src/i18n/react.tsx`;
  `packages/ui/src/i18n/catalog.ts`)
- **Glossary**: zh-Hans follows Apple's zh-Hans macOS terminology with the fixed terms 控制台 console, 会话 session, 枢纽
  hub, 智能体 agent, one half-width space between Chinese and Latin text, numbers or placeholders, and full-width
  punctuation in Chinese sentences, parentheses included (`（{detail}）`).
  (`packages/ui/src/i18n/messages/zh-Hans.ts`, the catalog's doc comment)
- **App name and other app-level facts**: the single source is `config/app.json`. Copy writes the product name as the
  global placeholder `{appName}`, which `format` and `<Message>` fill from that file without the caller passing it;
  never write the name as a literal, and never translate it. (`config/app.json`; `packages/ui/src/i18n/catalog.ts`;
  `docs/product/language.md`, "What follows the language")
- **Exempt text**: text aimed at the agents — the hub's instructions, tool descriptions and replies, prompts injected
  into a launch — stays English and is not UI copy; a report page is content the hub's model wrote and is not
  localized. (`docs/product/language.md`, "What follows the language")
- **Capitalization**: the UI is a web UI and uses sentence case (Close sessions, Try again); the macOS application
  menu's items follow Apple's Title Case (Hide Others, Select All). (`packages/ui/src/i18n/messages/en.ts`)
- **en contractions**: the en catalog uses no contractions — could not, is not, does not — overriding the default
  register above; keep new strings consistent with it. (`packages/ui/src/i18n/messages/en.ts`)
