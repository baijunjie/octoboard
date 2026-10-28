---
name: knowledge-i18n-copy
description: UI copy conventions for a multilingual app. Use when writing, translating or reviewing UI copy and localized strings (Localizable.xcstrings, strings.xml, web i18n resources), and for "change this copy", "fill in the missing translations", "make the wording consistent", "how should this confirmation dialog read", "how should this error message read", "Title Case or sentence case".
---

# UI copy conventions

In every language, write so it reads smoothly, is understood in one pass, and is not wordy: cut what can be cut, but do
not give up the language's register or a sentence's completeness to save words. Descriptions, body text and hints finish
their sentences, with no slang and no unfinished fragments; title slots, buttons, labels, status values and input
placeholders can be fragments by nature, and follow their own subsections; buttons use short words. How long a language
runs is judged by its own row in "Register per language".

Match the source language in the facts stated: the same amount of information, with the length following the language's
register, and the sentence structure following the target language's own UI idioms. **Do not carry the source language's
sentence frame across word by word** (the 「确认」 in the Chinese 「确认删除该文件？」 is a Chinese construction; de simply
writes Diese Datei löschen?).
When reviewing, read every string through as a whole sentence and judge it against that language's row; do not just scan
for keywords.

Where the project's own copy conventions, glossary or design system conflict with this document, the project's win. Values
marked "default" here suit a typical consumer app; a project with a different positioning (B2B, finance and so on) may
change them.

When unsure, report back: stop, spell out what needs deciding (with the options) or what information is missing, together with what you have already found out, and let the recipient of the report (the user, or the dispatcher passing it on to the user) decide before going on; do not decide for yourself.

## Register per language

Simplified Chinese's short written-style sentences cure the convoluted, long-winded phrasing of spoken Chinese; that is
not the register for every other language. Write each language only at the level the table below gives it, as long as it
needs to be: don't squeeze it down to Simplified's character count, and don't push it a notch toward officialese or
honorifics for politeness. When a project has a different positioning, change the level for the whole language: changing
the level means using that language's polite or formal address throughout, and the items in the "Avoid" column that
belong to the level (honorifics, administrative tone, officialese, stacked honorifics) are lifted with it; the bans on
mixing, slang and nominalized openings do not move with positioning. Within one language, never mix.
When a control squeezes the length: first switch to a shorter idiomatic phrasing in that language, then fall back to an
abbreviation or a different word, and only last reduce how much information is given; never truncate, and never damage
the source language for this.

| Language | Default register | Avoid |
|---|---|---|
| zh-Hans | Short written-style sentences | Convoluted spoken phrasing, Japanese word order, the Japanese middle dot 「・」 |
| zh-Hant | Short sentences, Taiwan usage by default, 「你」 for the user; the same concepts as Simplified, worded with Traditional UI vocabulary | Simplified officialese, Simplified vocabulary carried over as is |
| en | Complete sentences with common contractions | — |
| ja | Sentences in です/ます | Stacked honorifics (させていただきます), plain form (〜かも) |
| ko | Statements in -(스)ㅂ니다 (없습니다), requests in 해 주세요 | Mixing -세요 / -(으)십시오 in one product, 반말 |
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
| vi | Pronoun-less imperatives; keep bạn có thể where it means "you can" | bạn có thể or quý khách added only for politeness |

Buttons use the verb form the language's UIs conventionally use, not an honorific command to the user:

| Language | Button form | Examples |
|---|---|---|
| de, fr, es, pt-BR, ru | Infinitive | Löschen, Supprimer, Eliminar, Apagar, Удалить |
| it | Second-person singular imperative | Elimina, Annulla |
| ja | Noun or short name | 削除, キャンセル; not 削除してください |
| ko | Noun | 삭제, 취소 |

When the user's gender is unknown, do not default adjectives and participles to the masculine (connecté, connesso,
conectado); use a gender-free construction instead (a noun, an infinitive, a different subject). Words that agree with a
noun agree with it in gender and number (invariati / invariate).

### Simplified Chinese

| Don't | Do |
|---|---|
| 这个文件删不掉 | 无法删除该文件 |
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
| Emphatic warnings, meaning only the sentence in a destructive confirmation that states the consequence | Don't contract: cannot, do not, it is; failure copy contracts as the "Errors" section says |
| you have | Contract to you've only where the result is still English; You've more doesn't hold |
| Noun + verb | Don't contract onto the noun: the computer's not → the computer isn't; the possessive 's is unaffected |
| Object it + is | Don't contract to it's: The best way to find it is to ask them. |

A contraction replaces only the one word it stands for.

### Pitfalls in other languages

| Language | Rule |
|---|---|
| de | Lowercase sie means "she" or "they"; leave it |
| it | With tu, lowercase lei means "she"; leave it |
| it | Write a condition as a full conditional clause (Se inserisci di nuovo lo stesso nome, viene ricreato), not a gerund clause |
| th | For a short "can't", use verb + ไม่ได้ (ลบไม่ได้), not ไม่สามารถ…ได้; it can also close a causal sentence. ไม่ได้ before the verb means "didn't" (ไม่ได้ลบ) — don't put it on the wrong side |
| th | Where it would produce a doubled negation (…ที่ไม่ได้ใช้ไม่ได้), keep ไม่สามารถ…ได้; keep the positive สามารถ |
| ko | A particle after a placeholder is written with both forms: 을(를), 은(는), 이(가), 과(와), (으)로 |
| ru | For something impossible by ability or rule, use нельзя + infinitive; for an attempt that failed this time, Не удалось… (Не удалось удалить) |

## Text not held to the UI register

| Text | How to treat it |
|---|---|
| Voice assistant trigger phrases (Siri / Shortcuts / Google Assistant) | The user says them out loud and they are deliberately colloquial; exempt entirely |
| Legal text | No contractions, no rewriting into short UI sentences |
| Marketing and website promotion | A register of its own; passages shared with the UI still follow the UI register |

When the website or legal pages mention a name from the app's UI, copy the app's actual translation language by
language; when the app renames something, go back and sync it.

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
- A search field does not correspond to a field to fill in, so it does not follow the example pattern; write it as a
  prompting question (找什么？ / What are you looking for?).

### Reference and scope

- Don't repeat an object the page has already scoped: on a page inside a team space, "all members" is enough; don't
  copy "all members of the team space" from the product docs. The title of a destructive confirmation is the exception;
  write it per "Destructive actions".
- When one sentence covers several cases, use that one sentence ("this project" rather than separate "your project /
  the team's project"); split it only when "this affects other people" and nothing else on the screen says so.
- When a UI name or permission name needs quoting, use the language's quotation marks; when one sentence lists several
  items, either quote all of them or none, and a single one mentioned on its own gets no quotes:

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

Clear, delete, remove and discard each mean something different: clear = the contents are emptied, the container and its
settings stay; delete = the object is gone; remove = taken out of here, still there at its origin; discard = giving up
changes not yet saved. An action that is in fact irreversible does not use a word that sounds reversible (remove, clear).

- A description of an action states only the scope and consequence directly tied to this action: a button whose name says
  it all gets no description, and a description of an object's nature that is unrelated to this action and does not
  change with state ("visible to all members" and the like) is not hung on either; don't drag in other mechanisms.
- Choose the sentence pattern by what is to be stated. A consequence does not repeat the action itself (「清除后，将删除…」
  is ill-formed). The patterns in the table are for Chinese; for other languages the same content takes that language's
  UI idiom.

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

A confirmation dialog's title and its confirm button both use the action word on the button that triggered it, not a
synonym:

| Part | How to write it |
|---|---|
| Title | In Chinese 「确认<动作><对象>？」; in other languages, that language's action question; include the count or the name when there is one |
| Body, when the triggering button already has a description | Omit it; don't restate that description |
| Body, when the trigger has no description (swipe, toggle, menu item) | State the scope or consequence; omit it when there is nothing to state |
| Confirm button | When triggered by a toggle, name the knock-on consequence: 「关闭并清除」. Never "Yes / No" |

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

## Ellipsis

The ellipsis is always the single character `…` (U+2026), never three dots; Chinese also uses `…`, not 「……」.

The table below governs command copy only, meaning buttons and menu items that run something when tapped, the two
treated alike: judge by "after the tap, does the user still have to give another input", not by whether a window or
panel opens. iOS follows the table; on Android and the web, command copy takes no ellipsis by default. In-progress
states and status banners are not governed by this rule; every platform writes them per "In-progress states".

| After the tap | Ellipsis | Examples |
|---|---|---|
| The user must give another input or make a choice to decide what runs (asks where, what name, which one); a pure confirmation doesn't count | Add | 浏览…, 导出…, 重命名… |
| Takes effect immediately, or just jumps to another page, view or panel | Don't add | 保存, 删除, 查看详情, 设置 |

- The ellipsis belongs to the command itself and does not vary by language: if one language adds it, the same copy in
  every language adds it.

## In-progress states

This governs "the action the user just triggered is still running".

- Chinese writes 「…中」 or 「正在…」; other languages use their own progressive form or a status noun, not a literal
  rendering of 「中」. Every language ends with an ellipsis (see "Ellipsis" for how to write it; exceptions below):

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
- A button in progress keeps its visible text.

## Errors

- Always show a localized sentence the user can understand. The exception's own text (`message`,
  `localizedDescription` and the like) may never stand in for it.
- Each failure whose cause can be recognized gets its own copy; unrecognized ones get a generic sentence. The generic
  sentence says "please try again", so a full-screen failure with no retry on screen gets a separate sentence that
  does not suggest retrying.
- A dialog title says clearly "what did not get done", one sentence per action, never a vague "Operation failed".
- en failure titles: Couldn't for an attempt that did not succeed; Can't for something the rules do not allow.
  Capitalization follows the title slot under "Capitalization".

- The body states the specific cause. Choose buttons by case:

  | Case | Buttons |
  |---|---|
  | Only a cause | One acknowledge button, named as the system dialog names it (for zh-Hans see the "Simplified Chinese" table) |
  | A cause with a way out the user can take in one tap | "Way out + Cancel" |

- A load failure may never be expressed through an empty state ("No records yet" or a reading of 0): write "Couldn't
  load" + the cause + "Try Again". One that truly cannot be retried (a one-time token, an expired link) gets a separate
  sentence that does not suggest retrying, as in the item above, with the title unchanged.

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
- Korean particles attach directly to the parenthesis: AI 스위치 (%1$s)을(를); “%1$s”을(를).
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

Sentence case throughout, whether an element name or a complete sentence: Android follows Material, and the web does so
by default.

### iOS

Follows the title-style capitalization of the Apple Style Guide, with word choice taken from Apple's own iOS
localization for that language.

| Role | Capitalization |
|---|---|
| UI element names: navigation titles, buttons, menu items, segmented controls, tabs, list rows and toggle labels (when phrases), section headers, App Intent titles | Title Case |
| Fragments or short sentences with no closing punctuation in a title slot (alert, banner, empty state, sheet, upsell) | Title Case (iCloud Storage Is Full) |
| Fragment questions | Title Case (Delete Note?) |
| Complete-sentence titles with closing punctuation (including several sentences), and complete questions (Are you sure you want to…?) | Sentence case; prefer a fragment question for a confirmation title |
| Button in-progress states, badges, tags, status capsules, readout values and status values | Title Case (Loading Plans…, Not Connected) |
| Complete sentences: descriptions, body text, footers, screen-reader labels, and list rows and toggle labels that are themselves complete sentences | Sentence case |
| Input placeholders | The prefix and the whole string in sentence case; the example word keeps the way that value is normally written in that language's UI, and does not change case for having moved into a placeholder (en `Ex. Outside`, es `p. ej. exterior`) |
| Plural and substitution branches within one key | Follow the sentence they are in |

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
- A contraction takes the case of the word it replaces; don't lowercase the whole string (write Couldn't, Don't in a
  Title Case element).
- tr case conversion uses the tr-TR mapping: i ↔ İ, ı ↔ I, not the default Unicode conversion.
- A word the user must type to confirm: all caps in languages with case; languages without case are unaffected. A
  type-to-confirm has two strings, the word the prompt asks for and the word actually compared; change both together,
  because changing only one side means users of that language can never get through.
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

## Changing existing copy

Judge by the old translation: if it still holds as is under the new source text, it is polish; if the facts it states
are now wrong or it leaves out one thing, the meaning has changed.

| Old translation under the new source text | What to do |
|---|---|
| Still holds (polish) | Keep the original key and every language's old translation, so the UI keeps showing the old translation; mark the key for re-review; prefer changing only the language that has the problem |
| Facts wrong or one thing missing (the meaning changed) | Use a new key and don't overwrite the old one; one key carries one meaning and is not reused across pages; every language is retranslated, and the old sentence does not stay in the UI |

- Translations already delivered are matched by key; don't rename keys for tidiness, since changing one makes every
  language's translation fail to match.

## Across platforms

- One feature uses the same wording and the same level of detail on every platform (iOS, Android, web), with section
  headers and descriptions present or absent alike; only a feature some platform lacks gets wording of its own.
  Capitalization, ellipsis, system dialog names and platform-specific names (the system's "Settings" and the like)
  follow each platform's rules; word-for-word identity is not required.
- When syncing from one platform to another, first check that the facts in the sentence hold on the target platform
  (permission items, whether the feature exists), and fix grammar errors in the source on the source first.
- A sentence that points at a control on this platform uses that control's existing translated name on this platform.
- In an RTL language (ar and so on), when a placeholder carries LTR content (a file name, URL, path, version number, a
  number with a Latin unit), wrap it in FSI / PDI (U+2068 / U+2069) or the platform's bidi isolation API: the bidi
  algorithm alone will misplace the adjacent spaces, parentheses and digits.
- Placeholder syntax follows each platform. A string with two or more placeholders must number them (iOS `%1$@`,
  Android `%1$s`; the web uses ICU named arguments) so translations can reorder them.
- The count, names and types of placeholders (`%@`, `%d`, `%s`, ICU named-argument names) are identical in every
  language and the source language; only the order may change, and named arguments' names are not translated: getting
  this wrong crashes or shows a wrong value in that one language only.
- A sentence is one key with placeholders; don't concatenate sentence fragments in code: a concatenated sentence breaks
  on ko particles, de cases, tr vowel harmony and ja word order. Only independent phrases (labels, buttons) get their
  own keys.
- Give plurals all their CLDR categories (ar has zero, one, two, few, many, other; ru has one, few, many, other), not
  just one / other; never hand-append an English plural s after a placeholder.
- In Android strings, write an apostrophe as `\'` and a double quote as `\"` (aapt resource escaping); don't add another
  layer of backslashes, or they show up in the UI.
- An Android string that must keep leading or trailing spaces is wrapped whole in double quotes, or the spaces are
  written as `\u0020`: a bare space is eaten by aapt.
- In a formatted string, a `%` to be displayed is written `%%`; in ICU copy, a `{` or `}` to be displayed is wrapped in
  single quotes (`'{'`).
- Markup inside a string (HTML, ICU, `<xliff:g>`, Markdown), attribute names and character entities are not translated,
  added or removed, and move only as a whole with the word order; the visible text inside the markup is localized as
  usual.
- Numbers, dates, times, currencies and measurements are not written into literals; the platform's formatter produces
  them and they are substituted through placeholders. Only typography rules such as the space between a number and its
  unit or percent sign are written per language in the copy conventions.

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
  a count-dependent message is a plural message selected by `count`. Two whole sentences shown in one paragraph are
  joined by `common.sentenceSeparator` (a space in en, nothing in zh-Hans), never a hard-coded space, or by one catalog
  message holding both as placeholders, whose text between them is the language's to say. Either way the separator
  must not end up as a text node of its own that is only whitespace, which VoiceOver drops, running the two sentences
  together: put it in the same text node as the sentence after it. `<Message>` does this itself when the parameter
  after the separator is a string or another `<Message>` whose own text starts with text; one that is any other
  element leaves the separator a node of its own. (`packages/ui/src/i18n/react.tsx`;
  `packages/ui/src/i18n/catalog.ts`; `packages/ui/src/i18n/messages/en.ts`; `docs/voiceover-cases.md`)
- **Glossary**: zh-Hans follows Apple's zh-Hans macOS terminology with the fixed terms 控制台 console, 会话 session,
  控制台会话 console session, 项目会话 project session, 智能体 agent, one half-width space between Chinese and Latin text,
  numbers or placeholders, and full-width punctuation in Chinese sentences, parentheses included (`（{detail}）`).
  (`packages/ui/src/i18n/messages/zh-Hans.ts`, the catalog's doc comment)
- **App name and other app-level facts**: the single source is `config/app.json`. Copy writes the product name as the
  global placeholder `{appName}`, which `format` and `<Message>` fill from that file without the caller passing it;
  never write the name as a literal, and never translate it. (`config/app.json`; `packages/ui/src/i18n/catalog.ts`;
  `docs/product/language.md`, "What follows the language")
- **Exempt text**: text aimed at the agents — the console session's instructions, tool descriptions and replies,
  prompts injected into a launch — stays English and is not UI copy; a report page is content the console session's
  model wrote and is not localized. (`docs/product/language.md`, "What follows the language")
- **Capitalization**: the UI is a web UI and uses sentence case (Close sessions, Try again); the macOS application
  menu's items follow Apple's Title Case (Hide Others, Select All). (`packages/ui/src/i18n/messages/en.ts`)
- **Ellipsis**: this overrides the web default under "Ellipsis" above. A command in the window — a button or menu item —
  takes a trailing ellipsis when the action it names is not done when the press lands: it leads into a flow of two or
  more dialogs or screens (Export…: a scope, then a save location, as the desktop app asks it; Import…: a file, then its
  review), or lands somewhere the user goes on to do it (Manage accounts… in Settings, Grant… finished in System
  Settings). It takes none when it leads to a single dialog or chooser where the user enters or picks one thing and
  confirms (Rename, Choose file), to a pure confirmation, or to a submenu, nor when its name is the navigation itself
  and the landing completes it (Settings, View all, Browse files, Open terminal, Open System Settings). A dialog or
  screen counts as one step, however many keystrokes it takes, and a label read only as an accessible name or tooltip
  takes none. The macOS application menu keeps Apple's (Settings…), and in-progress text and the menu bar's "N more…"
  line keep their own. (the user's decision; `packages/ui/src/i18n/messages/en.ts`)
- **en contractions**: the en catalog uses no contractions — could not, is not, does not — overriding the default
  register above; keep new strings consistent with it. (`packages/ui/src/i18n/messages/en.ts`)
- **A name is mentioned once per screen**: within one dialog, panel or other single view, the name of a user's own
  thing — a session's title, a project, an account, a branch, a path — appears once, normally as the chip in the
  sentence that introduces it (a dialog's subject sentence; its title names nothing, per "A dialog's title" in
  `docs/product/dialogs.md`). Everything after refers to it by pronoun or by role: it, the session, the owner (它、该会话、
  所有者). A session title can run to a paragraph, so each repeat costs the reader a line. Repeat the name only when a
  pronoun or role would be ambiguous — two things of the same kind on the screen that the sentence must tell apart — and
  never just to restate what an earlier sentence already named. This narrows "Reference and scope" above for this
  project. (the user's decision)
- **Empty-state copy**: an empty state's title carries no punctuation at all, a comma inside it included, which is
  stricter than the "Empty-state titles" rule above; what the title cannot hold goes in the line under it, and a
  state said in one breath somewhere else — a notice over content shown anyway, a live region speaking for a panel
  hidden from screen readers — is a `.sentence` message of its own rather than the two joined in code. (the "An empty
  state is a glyph, a heading, and what fills it" section of `docs/ui-accessibility.md`)
- **Per-language rules beyond the generic tables**: these extend "Register per language" (its "Button form" column),
  "Simplified Chinese" and "Pitfalls in other languages" above.
  - ko: requests use 해 주세요 only in explanatory sentences, never on a button (buttons are nouns, per the "Button form"
    column of the "Register per language" table). Statements already in -습니다 are not converted wholesale to 해요체.
    Keep the spaces Korean spacing rules require (할 수 없습니다, not 할수없습니다).
  - de: when switching to du, change by case: nominative Sie → du, accusative Sie → dich, dative Ihnen → dir,
    possessive Ihr- → dein- (with endings: Ihre → deine, Ihren → deinen). This adds to the de row of "Pitfalls in other
    languages" (lowercase sie means "she" or "they"; leave it).
  - it: with Lei, the formal pronoun is capitalized. This adds to the it row of "Pitfalls in other languages" (with tu,
    lowercase lei means "she"; leave it).
  - id: the prepositions di and ke are written apart from the noun (di rumah); as prefixes they are joined (dihapus).
  - zh-Hans: add 要删除吗？ → 确认删除该文件？ to the "Simplified Chinese" table's Don't / Do pairs.

  (the user's decision)
- **Grammatically required contractions**: the contraction table under "English" is en-only, but these preposition +
  article contractions are grammatically required in fr, pt-BR, it, de and es; they are not abbreviations to expand, so
  keep them in the language's copy even where the en rules forbid contracting.

  | Language | Keep |
  |---|---|
  | fr | du, des, au, aux |
  | pt-BR | do, da, dos, das, no, na, nos, nas, ao, à, pelo, pela |
  | it | The full set of preposition + article: del, al, dal, nel, sul, della, nella and so on |
  | de | am, im, beim, vom, zum, zur, ans, ins |
  | es | del, al; the pronoun él does not contract (de él, a él) |

  (the user's decision)
- **Confirmation, error and search copy**: these add to "Destructive actions", "In-progress states", "Errors" and
  "Input placeholders" above.
  - A confirmation body that has to say what is not affected first says what is removed; it does not open with
    "<X> is not affected". The "Consequence" row of the pattern table under "Destructive actions" is scoped to Chinese;
    this extends it to every language.
  - An error alert's opening sentence, the cause "Errors" asks of the body, is optional: it is there only for what the
    title leaves unexplained and never says the title again in other words; when the title says it all, the body
    starts with the remedy (see "The error alert" in `docs/product/error-alerts.md`).
  - An error with a way out plus Cancel makes the way out the default action.
  - A load failure also carries a warning icon; its wording is the one "Errors" above already gives.
  - Only an icon-only button swaps its icon for a spinner while in progress.
  - A search field's prompting question addresses the user in that language's register (de: Was suchst du?).

  (the user's decision)
- **Destructive confirmations**: the destructive Confirm is the form's submit button, so Enter triggers it — no design
  choice permanently removes Return, and copy must not claim otherwise. Only the heaviest loss adds a type-to-confirm
  word, which disables Confirm, and Enter with it, until the word matches; this project prompts and compares the same
  single string, so the "two strings to change together" warning under "Capitalization" has nothing to match here. The
  optional control under the message settles what Confirm does rather than gating it, and Enter submits even from that
  control, so the Confirm label and the control's own label must each read correctly with it left untouched.
  (`packages/ui/src/dialogs/ConfirmDialog.tsx`; `docs/ui-accessibility.md`, "Keyboard")
