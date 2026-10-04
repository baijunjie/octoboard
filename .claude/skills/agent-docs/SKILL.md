---
name: agent-docs
description: Reading and maintaining project docs (documentation index, project map, product docs) and development memory, plus governing code comment conventions; keep it in mind before starting work to understand the project or a piece of code, when wrapping up a change that touches structure or behavior or produces a lesson worth recording, and whenever writing a code comment. Use when the user says "look at the project docs", "how is this part designed", "what pitfalls does this project have", "what should I know before starting work", "this stage is done", "write down what we learned this time", "have the docs drifted", "write comments", "how should this comment be written", "can a comment reference a doc".
---

# Project docs and development memory

| Category | Answers | Location | Maintained by |
|------|------|------|--------|
| documentation index | which docs live under `docs/` and what each one covers | `docs/README.md` | you (the main agent) |
| project map | where the code is, what this part does | `docs/project-map.md` + a nearby `README.md` for each module in the code tree | `map-writer` |
| product docs | what the system looks like now | `docs/product/` | `product-writer` |
| development memory | how a developer should work in this project | `docs/memory/` | `memory-writer` |

The doc directories all live inside the project they belong to.
The locations in the table are the default; when the project already has its own convention for doc directories, follow the project's.

- Development memory is a loose collection of smallest units and **cannot be referenced by code comments or other docs**; wherever it is needed, write that sentence straight in.
- Other docs under `docs/` are maintained by the agent responsible for the corresponding task, and this skill does not touch them. Long-lived ones among them (such as topic docs — a focused writeup kept as the record of one task, e.g. research conclusions, measured baselines) are registered in the documentation index; temporary docs that are deleted once done (development plans, tickets, etc.) are not registered.
- Content that only holds on this machine (local paths, environment values, etc.) goes into none of these categories: docs are committed with the repo, so it breaks on another machine or with multiple people.
- Report when this skill cannot work it out or you are not sure.

## Before starting work: read

1. Read the `docs/README.md` documentation index first, and read only the product docs and development memory relevant to the current task.
2. Before touching a piece of code, locate which module it belongs to in the project map, read that module's `README.md`, then read the corresponding product doc to confirm current behavior.
3. When development memory conflicts with your default approach, **the memory wins**; an explicit instruction from the user on the spot still takes precedence.

## Wrapping up: dispatch subagents as needed

**The main session does not write the project map, product docs, or development memory itself**; how to write them and the criteria live in each subagent's own definition. Dispatch only the ones this change calls for — dispatch as many rows as apply; most small changes need none:

| This change | Dispatch |
|----------|------|
| Directory structure, module responsibilities, or external interfaces changed | `map-writer` |
| Product behavior, contracts, interactions, or business rules changed | `product-writer` |
| There is a lesson of the kind "you would take the wrong path next time without knowing it before starting work" | `memory-writer` |

- Dispatch **one after another** in the table's order, not concurrently; when dispatching `memory-writer`, hand it the earlier subagents' reports too, so what is already written is not recorded again.
- The subagent cannot see your conversation: tell it what was finished this time and what you want recorded, and give it **source material it can read** (the change scope, task notes, requirement descriptions, etc.); anything that exists only in the conversation has to be written up as key points inside what you tell it first. Whether it is worth writing and which doc it goes into is its call — it may refuse.
- The subagent only reads the source material and does not touch it; what happens to it afterwards is your business.

Handle what a subagent hands back according to where it belongs; for a destination not in this table, you decide:

| Belongs to | Handling |
|------|------|
| Another category (project map, product docs, or development memory) | If that subagent has not been dispatched yet, dispatch it in order carrying these items; if it already has been, dispatch it again carrying these items |
| A code comment somewhere | You write it yourself from the key points per the "Comment conventions" section |
| Only holds on this machine | Record it in your own local-machine memory not committed with the repo (not `docs/memory/`); if there is no such mechanism, state it clearly in your final reply |

When there is no subagent mechanism: read the installed `map-writer` / `product-writer` / `memory-writer` definitions, write it yourself in the same order following the rules inside — **the default is to write nothing**; lay out the file changes in the format of a subagent report, to be used for updating the documentation index.

## Wrapping up: update the documentation index

Subagents do not touch the documentation index; you update `docs/README.md` from the added, deleted, and renamed files they report, creating it if it does not exist:

- List the long-lived docs under `docs/` **flat and recursively** (excluding the documentation index itself), grouped by category or directory, one entry per line, written as a pointer someone can use to judge which doc to open.
- Subdirectories of `docs/` get no `README.md` acting as a directory index; the list lives only in the documentation index, in that one place. Temporary docs that are deleted once done are organized by their own structure and are not bound by this rule. A module's nearby `README.md` is not under `docs/`, is carried by the project map, and does not go into the documentation index.
- Entries are registered by whoever owns the doc: for the three categories above you register them from the subagents' reports; other long-lived docs are registered by the agent that wrote them — when you dispatch an agent to write such a doc, tell it to register its own files. If you find someone else's long-lived doc missing from the index, or an entry that has gone stale, state it clearly in your final reply; do not fix it for them.

## Checking against the current state

When the user asks whether the docs have drifted but there is no new change, dispatch `map-writer` or `product-writer` according to the scope, making clear that this is checking against the current state with no change this time; afterwards update the documentation index and confirm the code is aligned as usual.

## After the docs change: confirm the code is aligned with them

When a subagent has changed a referenced section, it lists in its report, one by one, the code comments and other docs that reference it:

- Look at every code comment: if what it says no longer matches the revised doc, fix the comment; if code behavior does not match the doc, first judge whether the code has a bug or the doc is wrong, then decide which side to change — do not default to changing the code.
- References in other docs go to that doc's owner: for the project map, product docs, or development memory, dispatch the corresponding subagent again carrying these items; for other long-lived docs, fix it yourself if it is this round's responsibility, otherwise state it clearly in your final reply and do not fix it for them.
- Never finish leaving the state "the doc has already changed, the code is still as it was".

## Comment conventions

A comment's reader is a future maintainer who does not know the context here, and the criterion is "cannot be got from reading the code itself, but must be known when maintaining it".

**Should be written:**

- Design intent and the reasoning behind trade-offs, implicit premises and constraints the code does not show (quirky behavior of an external interface, concurrency, units / time zones, etc.), the overall idea behind a complex algorithm.
- `TODO`: temporary code, partial implementations, and anything waiting on an external dependency must be marked, stating what it is waiting for.

**Should not be written:**

- Comments that restate the literal meaning of the code.
- Change history: what was deleted, what it used to be, "added" / "modified" markers.
- Commented-out code; delete it outright.
- Comparisons with other implementations, cross-references pointing at code elsewhere.
- Process information: the issue that prompted this change, bug number, requirement discussion, review feedback, and remarks aimed at this round's reviewer ("switched to a cache here").

### Scope of explanation

A comment explains only the level it sits at: a function comment explains that function, a file-header comment explains that file.

- Cross-file, system-level explanation (how modules cooperate, the call chain, architectural layering, cross-module data flow and state machines) belongs to the docs, not comments.
- When something outside genuinely has to be mentioned, write only the one sentence directly relevant here, and reference a doc for the rest.

### Referencing docs

- **Reference only systematic, formal docs**: docs that are maintained long-term, committed with the codebase, stable in location, plus external standards and third-party official documentation.
- **Do not reference loose entries or temporary docs**: loose records such as lesson entries or caveat lists need their content written straight in when needed; process by-products deleted once done — development plans, design discussions, tasks / tickets / issues, chat logs — are never referenced.
- A reference only points the way, **it does not restate the referenced doc's content**; "why it is written this way here" is still spelled out in the comment, the doc only fills in the cross-file whole picture.
- A doc inside the repo is written as **path + section heading text**, the path relative to the repo root, no line numbers, no anchors, so that the reference can still be found by search after the doc changes, e.g. `// For the full state transition rules see docs/product/order.md's "Order state machine" section`.
- An external doc is written as **name + version / number + section**, URL optional: `RFC 7231 §6.5.1`.
