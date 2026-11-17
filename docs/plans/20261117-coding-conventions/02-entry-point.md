# Mandatory entry point and review against the conventions

> Goal: every session knows before writing code that the convention docs are binding and where they are, and every
> change is reviewed against them.
> Completion criteria: `CLAUDE.md` states that development must follow the convention docs and links them; briefs to
> subagents and sessions are required to name the conventions that apply; `change-checker` and `ui-checker` read the
> convention docs and check every change against them, and a convention violation is a finding that blocks the review
> from passing.

## Implementation plan

- [ ] Add a section to `CLAUDE.md` saying development must follow the convention docs, read before designing a change,
      and where they are; fold the existing "Provider-neutral core" section into it as a pointer.
- [ ] Require briefs to subagents and sessions to state the conventions that apply to the work and where each piece of
      code goes.
- [ ] Make `change-checker` check the changed code against the convention docs: it reads the docs that apply to the
      changed files and reports each violation as a finding naming the rule broken, the rules most often broken (the
      provider-neutral core first) checked explicitly, including a search of the diff for agent ids and
      agent-specific literals in generic code.
- [ ] Make `ui-checker` do the same for the UI conventions (component development and use, the UI parts of the
      provider-neutral core, internationalization in code), alongside the UI standard and copy conventions it already
      checks.
- [ ] A convention violation does not pass the review unless the user accepts it as an exception; state this in both
      checker definitions and in the change check's handling of findings.

## Notes for the developer

- Development notes: the instruction file's workflow section is edited through the `agent-workflow-edit` skill; the
  checker definitions are installed by the setup installers, so put project-specific additions where reinstalling
  keeps them.
- Reference docs: `CLAUDE.md`, `.claude/agents/change-checker.md`, `.claude/agents/ui-checker.md`.
