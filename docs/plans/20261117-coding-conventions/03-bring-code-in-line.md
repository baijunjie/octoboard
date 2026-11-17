# Bring the code in line

> Goal: the code follows the convention docs.
> Completion criteria: an audit against every convention doc lists each violation; every violation is fixed or recorded
> by the user as an accepted exception with its reason; the project's checks pass.

## Implementation plan

- [ ] Audit the code against each convention doc and list the violations by convention and severity.
- [ ] Fix the provider-neutral violations already found on `main`:
  - [ ] The interrupt title gate that branches on Claude Code: declare the behavior on the provider's screen reader.
  - [ ] The cross-project guard's parsing of each agent's tool-call payload: move it behind the provider's hook
        interpreter.
  - [ ] The Codex-only relocation failure causes, and the UI recovery copy that names Codex: make them Codex's own
        provider codes, with their copy in Codex's presentation module.
  - [ ] The inherited session marker variables listed in generic code: declare them on each provider's descriptor.
- [ ] Fix the remaining violations in batches by area, each batch independently mergeable.

## Notes for the developer

- Development notes: each batch goes through the normal workflow in `CLAUDE.md` (tests, change check, docs, checks).
  A fix that changes a wire contract regenerates the protocol artifacts.
- Reference docs: the convention docs from milestone 1; the audit of provider-neutral violations is summarized above.
