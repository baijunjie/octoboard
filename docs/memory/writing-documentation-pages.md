# Writing documentation pages

How the pages (`apps/docs/src/content/docs/`) are written is in `docs/conventions/docs-site.md`; where pages and
snippets live is in `docs/structure/docs.md`. What follows is what no check enforces.

## Check anchors against the built pages after translating or retitling a heading

`apps/docs/buildOutput.test.ts` does not check `#fragment` links, and no other check does, so a broken anchor ships
silently. After translating a page or retitling a heading, build the site and check that each fragment pointing to the
changed headings, in that language, matches an `id` in the built target page under `apps/docs/dist/`.
