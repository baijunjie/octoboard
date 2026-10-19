> Severity: Moderate

## Symptom

All eight tests in `packages/ui/src/website-preview/repositories.test.ts` fail on `main`: reading a changed file's live
content from a preview repository's worktree is refused as "does not exist any more".

## Reproduction steps

1. Check out `main` at `e96716741` (also reproduced on `0d3ccd08d`), run `pnpm install`.
2. In `packages/ui`, run `pnpm exec vitest run --testTimeout=60000 src/website-preview/repositories.test.ts`.
3. Every case of "the website's eight independent repository fixtures" fails, one per repository: preview-admin,
   -customer, -kitchen, -merchant, -mobile, -pos, -server, -tablet.

## Expected vs. actual

- Expected: all eight cases pass — the test asserts that each preview repository's source, directories, files and Git
  are routed to the same repository, and it passed before the regression.
- Actual: each case throws at the `read_project_file` request (`repositories.test.ts:112`), e.g.
  `Error: ⁨src/domain/promotion.ts⁩ does not exist any more.` for preview-admin, each on its own file.

## Environment

macOS (Darwin 25.2.0), Node 22.22.2, vitest as pinned in `packages/ui`. No local configuration involved; the test
runs on the website preview's fixture daemon only.

## Scope of impact

The packages/ui unit test run is red for everyone, which hides other regressions in that run. Whether the website
preview itself shows the same refusal when a changed file is opened is unknown.

## Leads

- Verified: the stack is `browsed` in `packages/ui/src/gallery/fixtureDaemon.ts:206`, reached from
  `Object.request` at `fixtureDaemon.ts:602`, from `src/website-preview/daemon.ts:66`.
- Verified: the failure is present on `0d3ccd08d`, before the added-agents change was merged, so that change is not the
  cause.
- Inferred, not checked: `36c297a08` ("open file paths in a session's terminal with a click") changed how the fixture
  daemon reads a worktree's files, and the website preview's repositories do not satisfy the new check.

## Acceptance criteria

- [ ] `pnpm exec vitest run src/website-preview/repositories.test.ts` in `packages/ui` passes all eight cases.
- [ ] The full packages/ui vitest run passes.
- [ ] The gallery tests that cover the fixture daemon's file reads still pass.
