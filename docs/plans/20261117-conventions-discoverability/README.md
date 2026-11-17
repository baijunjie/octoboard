# Make the coding and architecture conventions findable and followed

## Problem

Sessions keep breaking project conventions they were never shown. The case that started this: a feature written by a
subagent branched on which agent it was in generic code (daemon and UI), against the provider-neutral rule. An audit of
`main` found four more such places in production code, two of them added after the rule was written down and passed by
the change check.

Why the rule was missed:

- The instruction file (`CLAUDE.md`) did not mention it; a session reaches it only by reading the structure docs end to
  end. (A short "Provider-neutral core" section has since been added to `CLAUDE.md`.)
- In `docs/structure/daemon.md` and `docs/structure/ui.md` it is one bullet deep inside the roughly 30 bullets of
  "Where new code goes", with no counterexample and no steps for what to do when a new per-agent difference has no
  capability or interface method yet.
- The development memory on building a feature for several agents opens by saying special handling of one agent is
  sometimes right, and gives placement only as a pointer, which reads as permission to special-case in place.
- Comments in generic modules describe one agent's behavior, and existing lists of agent facts sit in generic code, so
  the surrounding code teaches the wrong pattern.
- No check enforces it, and the change and UI checkers' definitions do not name it.
- Briefs to subagents prescribed per-agent behavior without saying where it lives.

The provider rule is the trigger, but the same weaknesses (rules buried in long structure docs, not surfaced at entry,
not checked) apply to the other coding and architecture conventions.

## Plan outline

1. Make the provider-neutral rule prominent and actionable where sessions read it.
2. Review the rest of the coding and architecture conventions the same way, and give each a findable home and a clear
   entry point.

## Key decisions

- This work is carried out in its own session, separate from feature work.
- The instruction file only states the rules every session must apply and points to where the full rule lives; the
  rule itself stays in the structure docs, its single home.

## Milestones

1. [Provider-neutral rule](01-provider-neutral-rule.md)
2. [Other coding and architecture conventions](02-other-conventions.md)

## Open

- Whether to add an automated check (a `scripts/verify/` step or lint rules) that refuses agent ids, agent-specific
  string literals and direct imports of a provider module outside the provider directories, with an explicit allowlist
  for existing exceptions. Not yet decided by the user.
- Whether and when to fix the four production violations the audit found (interrupt title gate, cross-project guard
  payload parsing, Codex-only relocation causes with their UI recovery copy, inherited session marker variables). Not
  part of this plan unless the user decides so.
- Whether the comment conventions should forbid describing one agent's behavior in generic modules.
