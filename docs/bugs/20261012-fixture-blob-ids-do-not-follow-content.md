> Severity: Minor

## Symptom

The sample repository's change fixtures name object ids that contradict Git's own rule that an id names its
content: one id stands for two different contents, and one content is named by several ids.

## Reproduction steps

1. Open `packages/ui/src/gallery/fixtures/projectGit.ts`.
2. `3b18e51` is the old side of the rename `nest → nest/leaf.ts`, whose content is `PIPELINE.old`, and both
   sides of the rename `nest2/leaf.ts → nest2`, whose content is `PIPELINE.new` — one id, two contents.
3. `PIPELINE.old` is also named by `a1b2c3d` (`src/pipeline.ts`), `c3d4e5f` (`src/limits.ts`), `e5f6a7b`
   (`src/moved.ts`) and `a7b8c9d` (`src/flaky.ts`), all of which are the same generated text — one content,
   five ids. The same holds for `PIPELINE.new` across `b2c3d4e`, `d4e5f6a`, `f6a7b8c`, `b8c9d0e`, `9c8d7e6`
   and `0a1b2c3`.

## Expected vs. actual

- Expected: the same id always names the same content, and the same content is always named by the same id.
  `apps/daemon/PROTOCOL.md`, "Reading", states the first half outright; the second is what Git's
  content-addressing means, and the daemon relies on it when it counts a blob's lines by id
  (`Reader::object_lines`).
- Actual: an id in these fixtures is an arbitrary label per side, consistent with nothing.

## Environment

`packages/ui`, the dev-only UI state gallery; no daemon, platform or configuration involved. Present since the
Git-mode fixtures were added.

## Scope of impact

Nobody is affected while using the app: nothing in the window keys off a change's object ids across changes —
a change is found by its path and its group, and a side's id only has to match between a diff reply and the
request for its bodies, which it does. The defect is in the gallery's own data, and it is what an invariant
check over the fixtures would trip on first.

## Leads

- Verified: the duplicate and conflicting ids listed above, read out of `projectGit.ts` directly.
- Verified: nothing outside the fixtures compares two changes' ids; `sameSide` in
  `packages/ui/src/gallery/fixtureDaemon.ts` matches on a side's path and state.
- Inferred: the general fix is the one the gallery-fixture-fidelity work parked as the most expensive of its
  options — one invariant check derived from the reply rules in `apps/daemon/PROTOCOL.md`, run over every
  scenario's replies, so that a new rule is covered by adding one invariant rather than one guard test per
  field. A fixture that names an id per content rather than per side would satisfy it by construction.
- Inferred: the same fidelity question stands for the other fixture kinds the gallery holds — the project
  files and the viewer's own subjects — which carry no such check either.

## Acceptance criteria

- [ ] Every object id in the sample repository's fixtures names exactly one content, and every content that
      appears on more than one side is named by one id.
- [ ] A check holds that rule, rather than it being restored by hand: a fixture added later that breaks it
      fails a test run.
- [ ] `packages/ui`'s type check, build and tests stay clean, and every "Project pane: Git" scenario shows
      what it showed before.
