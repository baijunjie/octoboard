> Severity: Minor

## Symptom

The gallery's fixture daemon answers a request for a change's whole bodies with `invalid_change` for every kind
of change its fixture carries no `expand` for, where the real daemon would read those bodies and answer with
them.

## Reproduction steps

1. In `packages/ui`, add a temporary test beside `src/gallery/fixtures/projectGit.test.ts` that builds the
   fixture daemon over `SAMPLE_GIT` (`createFixtureDaemon({ id: "sample-git", group: "Project pane: Git",
   title: "" })`, as the guard test's `sampleDaemon` does).
2. Ask it for the bodies of the deletion of `docs/old-notes.md` in `wt-main`:
   `{ type: "read_project_change_bodies", project: "p-1", worktree: "wt-main", change: { group: "unstaged",
   old: { state: "present", path: "docs/old-notes.md" }, new: { state: "absent" } },
   old: { state: "absent" }, new: { state: "absent" } }`.
3. The request rejects with `invalid_change` ("no patch to expand").
4. The same holds for the binary change of `docs/logo.png` and for the untracked `src/log.ts`, and for all of
   their counterparts in the `main..feature/ranking-experiments` comparison.

## Expected vs. actual

- Expected: the reply carries each present file side's body and its line count, as
  `apps/daemon/PROTOCOL.md`'s "Reading a change's bodies" describes and as `Reader::whole`
  (`apps/daemon/src/browse/changes.rs`) does — it reads every present file side whatever the change's kind, and
  an absent side stays absent. A request for an untracked change, or one with a side outside the project's
  scope, is the one that is refused with `invalid_change`, from the request alone.
- Actual: every change whose fixture carries no `expand` is refused with `invalid_change`, which covers a
  deletion, a binary change, an untracked file, a side outside the project, and a change the daemon refuses to
  read at all.

## Environment

`packages/ui`, the dev-only UI state gallery; no daemon, platform or configuration involved. Present in
`packages/ui/src/gallery/fixtureDaemon.ts` since the Git-mode fixtures were added, and unchanged by the
refusal and the factories added on 2026-10-12.

## Scope of impact

Nobody is affected while using the app: the window asks for a change's bodies only where the viewer offers to
expand a diff's collapsed lines, which it does only for a change with two file sides and one expandable
section — never for a deletion, an untracked file, a side outside the project, or a change drawn as images. The
defect is in the gallery's own data: a reply it sends that the real daemon never sends, which is what the
fixture daemon is meant not to do.

## Leads

- Verified: the fixture daemon's `read_project_change_bodies` arm gates on `!found.expand` and answers
  `invalid_change`, where `expand` says only how a bodies request is answered, not whether the daemon would
  answer one at all.
- Verified: `FixtureChange` carries a side's whole text as `whole`, a string, so a binary change's body (which
  the daemon answers with as well) has no field to come from — the contract would have to carry a `FileContent`
  per side for a bodies reply, not only a text.
- Inferred: what the daemon answers for `data/fixtures.bin`, the change whose patch is past the `patch_bytes`
  limit, is not settled — a bodies request makes no patch, so that limit may not apply to it and a different
  one may.
- Verified: the condition that decides this is `expand`, a field of the fixture, rather than the shape of the
  change, so it cannot follow the daemon's own rule without the contract changing.

## Acceptance criteria

- [ ] A request for the bodies of the sample repository's deletion, binary change, untracked file and
      out-of-scope change is answered the way the real daemon answers it, checked against
      `apps/daemon/src/browse/changes.rs` rather than against the protocol document's prose alone.
- [ ] A request the real daemon refuses from the request alone — an untracked change, or one with a side
      outside the project's scope — is still refused with `invalid_change`.
- [ ] What the daemon answers for a change it refuses to read at all is established and the fixture matches it.
- [ ] Whether a change has bodies to read follows from the kind of change its factory built, not from a field a
      call site may or may not pass.
- [ ] `packages/ui`'s type check, build and tests stay clean, and every "Project pane: Git" scenario shows what
      it showed before.
