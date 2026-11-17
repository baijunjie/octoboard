# Convention docs

> Goal: every coding-related convention of the project is stated once, as a checkable rule, in dedicated convention
> docs.
> Completion criteria: the convention docs exist and are registered in the documentation index; the inventory shows
> every convention found and its new home; the old places point to the convention docs instead of restating rules; the
> user has reviewed the docs.

## Implementation plan

- [ ] Inventory the coding conventions in the structure docs, `docs/architecture.md`, `docs/ui-standard.md`, the
      development memory, the skills and agent definitions, and the instruction file: each rule, where it is stated
      (and whether more than once), and what enforces it.
- [ ] Group them into convention docs: at least framework conventions per end, component development and use, the
      provider-neutral core, error codes, protocol and contracts, internationalization in code, tests and comments.
- [ ] Write each rule with a short counterexample where the mistake is easy to make. For the provider-neutral core,
      include the steps for a per-agent difference that has no capability or interface method yet: declare a
      capability on the descriptor or add a method to the provider's interface and implement it in each adapter; in
      the UI, read the served capability and keep provider-named copy and codes in the provider's presentation module.
- [ ] Replace the rules in their old places with pointers, and fix the development memory on building a feature for
      several agents so that it first says agent-specific handling lives in the provider.
- [ ] Present the docs to the user for review.

## Notes for the developer

- Development notes: who writes which doc is set by the `agent-docs` skill (structure docs by `map-writer`, development
  memory by `memory-writer`); new long-lived docs are registered in `docs/README.md`. Skills and agent definitions
  installed by the setup installers may be overwritten on reinstall; put project-specific rules where they survive.
- Reference docs: `docs/README.md`, `docs/project-map.md`, `docs/structure/`, `docs/architecture.md`,
  `docs/ui-standard.md`, `docs/memory/`.
