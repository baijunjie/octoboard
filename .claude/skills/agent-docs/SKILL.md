---
name: agent-docs
description: Entry point for reading and maintaining project docs and development memory; also governs code comment conventions. Before starting work, read the `docs/README.md` documentation index, then read the project map and the product docs and development memory relevant to the task; when wrapping up, dispatch subagents as needed to capture what was learned, then update the documentation index; when writing comments, use the comment conventions to judge what belongs there and which docs may be referenced. Use when the user says "look at the project docs", "how is this part designed", "what pitfalls does this project have", "what should I know before starting work", "this stage is done", "write down what we learned this time", "have the docs drifted", "write comments", "how should this comment be written", "can a comment reference a doc".
---

# Project docs and development memory

| Category | Answers | Location | Maintained by |
|------|------|------|--------|
| documentation index | which docs live under `docs/` and what each one covers | `docs/README.md` | you (the main agent) |
| project map | where the code is, what this part does | `docs/project-map.md` + a nearby `README.md` for each module in the code tree | `map-writer` |
| product docs | what the system looks like now | `docs/product/` | `product-writer` |
| development memory | how a developer should work in this project | `docs/memory/` | `memory-writer` |

The doc directories all live inside the project they belong to. The locations in the table are defaults; when the
project already has its own convention for doc directories, follow the project.

Development memory is a loose collection of smallest units and **cannot be referenced by code comments or other
docs**; wherever it is needed, write that sentence straight in.

Other docs under `docs/` are maintained by the agent responsible for the corresponding task and this skill does not
touch them: long-lived docs (such as topic docs — longer write-ups kept as the record of one task, e.g. research
conclusions, measured baselines) are registered in the documentation index; temporary docs that are deleted once done
are not registered.

Content that only holds on this machine (local paths, environment values, etc.) goes into none of these categories:
docs are committed with the repo, so it breaks on another machine or with multiple people.

## Before starting work: read

1. Read the `docs/README.md` documentation index first, pick out the product docs and development memory relevant to
   the current task, and read only those. If the documentation index does not exist yet, treat it as having no
   relevant entries.
2. Before touching a piece of code, locate which module it belongs to in the project map, read that module's
   `README.md`, then read the corresponding product doc to confirm current behavior. If there is no project map yet,
   go straight to the code tree.
3. Development memory is the lessons of past work; **when it conflicts with your default approach, the memory wins**.
   An explicit instruction from the user on the spot still takes precedence.

## Wrapping up: dispatch subagents as needed

**The main session does not write the project map, product docs, or development memory itself**; how to write them and
the criteria live in each subagent's own definition. Look at this change first and dispatch only the ones that apply;
most small changes need none. One change may hit several rows at once — dispatch every row it hits:

| This change | Dispatch |
|----------|------|
| Directory structure, module responsibilities, or external interfaces changed | `map-writer` |
| Product behavior, contracts, interactions, or business rules changed | `product-writer` |
| There is a lesson of the kind "you would take the wrong path next time without knowing it before starting work" | `memory-writer` |

- Dispatch **one after another** in the table's order, not concurrently; when dispatching `memory-writer`, hand it the
  earlier subagents' reports too, so what is already written into the docs is not recorded again.
- You only tell it "what was finished this time, where the source material is, what you want recorded". The subagent
  cannot see your conversation, so **the source material must be files it can read**: the change scope (commit range
  or changed files), task notes, requirement descriptions, and so on. Information that exists only in the conversation
  has to be written up as key points inside what you tell it. Whether it is worth writing and which doc it goes into
  is its call — **it may refuse**.
- The subagent only reads the source material and does not touch it; what happens to it afterwards (e.g. whether task
  notes are deleted once used) is your business.

Handle what a subagent hands back according to where it belongs:

- Belongs to another category (project map, product docs, or development memory): if that subagent has not been
  dispatched yet, dispatch it in order carrying these items; if it already has been, dispatch it again carrying these
  items.
- Belongs in a code comment somewhere: you write it from the key points per the "Comment conventions" section; when
  unsure, leave it to the user to decide.
- Only holds on this machine: record it in your own (the current agent's) local-machine memory, which is not committed
  with the repo (not `docs/memory/`); if there is no such mechanism, state it clearly in your final reply.
- Everything else (e.g. material belonging to a topic doc): you decide where it goes; when unsure, leave it to the
  user to decide.

Tools without a subagent mechanism: read the installed `map-writer` / `product-writer` / `memory-writer` definitions,
write it yourself in the same order following the rules inside, **the default is to write nothing**, and lay out the
file changes in the format of a subagent report, to be used for updating the documentation index.

## Wrapping up: update the documentation index

Subagents do not touch the documentation index; you update `docs/README.md` from the added, deleted, and renamed files
they report, creating it if it does not exist:

- List the long-lived docs under `docs/` **flat and recursively** (excluding the documentation index itself), grouped
  by category or directory, one entry per line, written as a pointer someone can use to judge which doc to open, not a
  long description. Temporary docs that are deleted once done (development plan docs, tickets, etc.) do not go into
  the documentation index, and do not count as missing registrations.
- Subdirectories of `docs/` get no `README.md` acting as a directory index; the list lives only in the documentation
  index, in that one place. A module's nearby `README.md` is not under `docs/`, is carried by the project map, and
  does not go into the documentation index. Temporary docs that are deleted once done are organized by their own
  structure and are not bound by this rule.
- Entries are registered by whoever owns the doc: for the three categories — project map, product docs, development
  memory — you register them from the subagents' reports; other long-lived docs are registered by the agent that wrote
  them — when you dispatch an agent to write such a doc, tell it to register its own files in the documentation index
  when done. If you find someone else's long-lived doc missing from the index, or an entry that has gone stale (the
  file is gone, the description no longer matches), state it clearly in your final reply; do not fix it for them.

## Checking against the current state

When the user asks whether the docs have drifted but there is no new change, dispatch `map-writer` or `product-writer`
according to the scope to be checked, making clear that this is "check whether the existing docs match the code" with
no change this time; afterwards update the documentation index and confirm the code is aligned as usual.

## After the docs change: confirm the code is aligned with them

A code comment may reference a section of a doc per the "Comment conventions" section; when a subagent has changed a
referenced section, it lists each one in its report:

- Look at every one: if what the comment says no longer matches the revised doc, fix the comment; if code behavior
  does not match the doc, first judge whether the code has a bug or the doc is wrong, then decide whether to change
  the code or go back and change the doc — do not default to changing the code.
- References in other docs pointing at the changed section go to that doc's owner: for development memory, dispatch
  `memory-writer` again carrying these items; other docs you handle yourself, and when unsure leave it to the user to
  decide.
- Never finish leaving the state "the doc has already changed, the code is still as it was".
- The same applies when there is no subagent and you have to write the doc yourself: before you write it, search the
  code once for this doc's path.

## Comment conventions

A comment's only reader is a future maintainer who does not know the context here, and the criterion is "cannot be got
from reading the code itself, but must be known when maintaining it".

**Should be written:**

- Design intent and the reasoning behind trade-offs: why it is implemented this way, why a more obvious form was not
  used.
- Implicit premises and constraints the code does not show: quirky behavior of an external interface, concurrency and
  performance considerations, boundary conditions, error-prone conventions such as units / time zones.
- The overall idea behind a complex algorithm, regex, or bit manipulation, so nobody has to reverse-engineer it line
  by line.
- `TODO`: temporary code, partial implementations, and anything waiting on an external dependency must be marked.
- Update a comment along with the code it describes — a stale comment is more harmful than no comment.

**Should not be written:**

- Redundant comments restating the literal meaning of the code (obvious assignments, getters/setters, etc.).
- Change history: what was deleted, what it used to be, "added" / "modified" markers — leave those to git.
- Commented-out code; delete it outright.
- Comparisons with other implementations, cross-references pointing at code elsewhere — they drift the moment the
  referenced side changes.
- Process information: the issue that prompted this change / bug number / requirement discussion / review feedback.
- Remarks aimed at the reviewer of this change ("switched to a cache here") — a comment belongs to the code, not to
  this diff.

### Scope of explanation

A comment explains only the level it sits at: a function comment explains that function, a file-header comment
explains that file.

- Cross-file, system-level explanation does not go into comments: how modules cooperate, the overall call chain,
  architectural layering, cross-module data flow and state machines. That is the docs' job; put it in comments and it
  scatters into several copies that each drift on their own.
- When this function / this file genuinely has to mention something outside it, write only the one sentence directly
  relevant to it, reference a doc for the rest, and do not expand on it in the comment.
- Do not write explanations on behalf of other files: how a caller should use it, how downstream handles it, belongs
  on their side.

### Referencing docs

- **Reference only systematic, formal docs**: docs that are maintained long-term, committed together with the
  codebase, stable in location, and systematic, plus external standards and third-party official documentation.
- **Do not reference loose entries**: unsystematic, one-thing-per-entry records (lesson entries, caveat lists, etc.)
  are already the smallest unit and cannot be referenced; wherever one is needed, write that content straight in.
- **Do not reference temporary docs**: development plan docs, design discussion records, tasks / tickets / issues,
  chat logs, and any by-product deleted once done — the moment they disappear, the comment is a dead reference.
- A reference only points the way and **does not restate the referenced doc's content**, otherwise the two sides drift
  apart.
- A doc inside the repo must be given as full **path + section**: the path relative to the repo root, the section as
  the heading's exact text, e.g.
  `// For the full state transition rules see the "Order state machine" section of docs/product/order.md`.
- No line numbers, no anchors: both of these ways of pointing break silently when the doc changes. Write path + exact
  heading text, so that the reference can be found by search when the doc side changes.
- For an external doc write **name + version / number + section**, URL optional: `RFC 7231 §6.5.1`.
- A reference does not replace the comment itself: "why it is written this way here" still has to be spelled out in
  the comment; the doc only fills in the cross-file whole picture.
