# Other coding and architecture conventions

> Goal: every coding and architecture convention a session must apply is findable from the instruction file or the
> documentation index, in one home, stated as a rule a reviewer can check.
> Completion criteria: an inventory of the conventions with each one's home and how it is enforced (check, checker
> definition, or reading only); the high-impact ones that were buried are surfaced as milestone 1 did for the provider
> rule; the user has reviewed the inventory and the changes.

## Implementation plan

- [ ] Inventory the coding and architecture conventions in the structure docs, `docs/architecture.md`,
      `docs/ui-standard.md`, the development memory and the instruction file: for each, where it lives, whether it is
      stated more than once, and what enforces it.
- [ ] Mark the ones whose violation is costly and that are easy to miss (deep in a long list, no check, contradicted
      by nearby code or comments), and propose for each: a more prominent place, a one-line mention in the instruction
      file, a required check in the checker definitions, or an automated check.
- [ ] Present the inventory and proposals to the user, then carry out the ones they choose.

## Notes for the developer

- Development notes: keep each rule in a single home and point to it from elsewhere; the instruction file states only
  what every session must apply. Follow the `agent-docs` skill for who writes which doc.
- Reference docs: `docs/README.md`, `docs/project-map.md`, `docs/structure/`, `docs/architecture.md`,
  `docs/ui-standard.md`.
