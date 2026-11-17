# Coding conventions: one home, a mandatory entry point, and code brought in line

## Problem

The project's coding conventions are scattered across the structure docs (`docs/structure/`), `docs/architecture.md`,
`docs/ui-standard.md`, the development memory and the instruction file, often as one bullet deep in a long list. The
instruction file did not say that development must follow them, so sessions and subagents miss them.

The case that started this: a feature written by a subagent branched on which agent it was in generic code, against
the provider-neutral rule (agent-specific facts and behavior live in the agent's provider; generic code branches on
declared capabilities). An audit of `main` found four more such places in production code, two added after the rule
was written down and passed by the change check. Contributing causes: the rule was not mentioned in `CLAUDE.md`, it sat
deep in "Where new code goes", it had no counterexample or steps for adding a missing capability, the development
memory on features for several agents read as permission to special-case in place, generic modules' comments and
lists set the wrong example, nothing checked it, and briefs to subagents did not state it.

## Plan outline

1. Gather every coding-related convention into dedicated convention docs: framework conventions (Rust and the daemon,
   React and the UI, Tauri and the desktop shell, the website), component development and use, the provider-neutral
   core, error codes, protocol and contracts, internationalization in code, tests, comments, and the rest found in the
   inventory.
2. Make the instruction file the mandatory entry point: development must follow the convention docs, and it says where
   they are.
3. Bring all code that does not follow the conventions in line.

## Key decisions

- Each convention has a single home in the convention docs; the structure docs keep their directory maps and point to
  the convention docs instead of restating rules.
- The instruction file states that the conventions are binding and where they live; it does not restate them.
- The work is carried out in a separate session from feature work.

## Milestones

1. [Convention docs](01-convention-docs.md)
2. [Mandatory entry point and review against the conventions](02-entry-point.md)
3. [Bring the code in line](03-bring-code-in-line.md)

## Open

- Whether to add automated checks (a `scripts/verify/` step or lint rules) for conventions that can be checked
  mechanically, starting with agent ids and agent-specific literals in generic code.
- Whether `docs/ui-standard.md` (the visual and interaction standard) moves into the convention docs or stays separate
  with the convention docs covering only how components are written and used.
