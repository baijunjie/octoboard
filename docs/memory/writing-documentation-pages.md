# Writing documentation pages

Where pages, snippets and quoted commands live is in `docs/structure/docs.md`. Three rules for writing and translating
the pages (`apps/docs/src/content/docs/`) that no check enforces:

## Quote an app label with a platform word as the app shows it on the system the step is for

A catalog message with a platform placeholder (`{systemSettings}`, `{trash}`, `{menuBarIcon}`, ...) reads differently on
each system. When a docs page or the website quotes such a label, take the page language's catalog message and fill
the placeholder with the word that system's host uses: Windows and Linux take the `settings` term, so "Open
{systemSettings}" reads "Open Settings" there, not "Open Windows Settings"; only macOS reads "Open System Settings".
Which term each system takes is in `packages/ui/src/platform/host.ts`, each language's word for each term in
`packages/ui/src/i18n/platformTerms.ts`. A step that covers several systems quotes each system's label and names the
system.

## In a translated page, an anchor uses the target heading's slug in the same language

Starlight derives a heading's id from its text, so a translated page's headings have translated ids. Every `#fragment`
in a translated page, to the same page or another, is the slug of the target heading as that language's page writes
it; an English fragment copied from the English page is a broken link (the English slug is right only when that
language has no translation of the target page, which then shows the English one). Retitling a heading changes the
anchors that point to it in that language. After translating or retitling, build the site and check that each fragment
matches an `id` in the built target page under `apps/docs/dist/`; `apps/docs/buildOutput.test.ts` does not check fragments.

## Write inline values as `<code dir="ltr">`, not backticks

Every inline value a page quotes (a command, an address such as `127.0.0.1`, a port, a path or file name, a key the
reader types) is `<code dir="ltr">...</code>`, in every language including English, since translations copy the English
page's markup, and inside Asides, lists and tables too. Without it a value inside a right-to-left paragraph of the
Arabic pages loses its left-to-right order.
