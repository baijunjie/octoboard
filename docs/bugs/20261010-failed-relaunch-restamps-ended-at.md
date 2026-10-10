> Severity: Minor

# A reopen whose launch fails re-stamps the session's archived time

## Symptom

Reopening an archived session whose launch cannot start moves that session to the top of its archive and makes it
read as "archived just now", although nothing about it changed: it is still the same archived session, archived when
it was.

## Reproduction steps

1. Archive a session of a project.
2. Note where it sits in that project's archive and what its row says ("archived 20 minutes ago", say).
3. Make its launch fail without touching the session itself — renaming the project's directory away is enough, since
   `term::launch` refuses on an unreachable working directory before spawning anything.
4. Press **Reopen** on its row in the archive view. The reopen is refused with a toast naming the directory, and the
   session correctly stays archived.
5. Look at the archive again: the session now sits first, reading "archived now".

## Expected vs. actual

- Expected: a reopen that failed to launch leaves the session exactly as it was. `docs/product/sessions.md`, "Where
  archived sessions are kept", orders the archive by when each session was archived, and the archive view's rows say
  how long ago that was — both of which this moves.
- Actual: the session's `ended_at` is set to the moment of the failed attempt, so its place in the archive and the
  time its row reports both follow the attempt rather than the archiving.

## Environment

- macOS (Darwin 25.2.0), Apple Silicon; a packaged local build from `pnpm build:app`.
- Seen on branch `feat/lead-sessions-in-sidebar` at `ffe4e45`, in daemon code identical to `main`'s.
- Measured on a lead session archived at 22:26, whose `ended_at` read 22:28:10 after a second failed reopen.

## Scope of impact

- Every failed relaunch of any session, whichever way it was reached (the archive view's Reopen, selecting an
  interrupted session, an owner's `reopen_session`), not only the chain under a lead session.
- Nothing else about the rollback is affected: status, bindings and processes all go back to where they were, and
  nothing below the failure is started.
- Workaround: none; the recorded time is simply wrong from then on.

## Leads

- Verified: the line is in `relaunch_session`'s error branch in `apps/daemon/src/coordinator.rs`, which sets
  `session.ended_at = Some(now_millis())` beside the status rollback the comment there is about. Restoring the status
  is what that branch is for; the timestamp does not need restoring, since the failed launch never changed it.
- Verified: `ended_at` is what the archive is ordered and labelled by — `archivedSessions` in
  `packages/ui/src/sidebar/order.ts` sorts on `ended_at ?? started_at`, and `packages/ui/src/archive/ArchiveView.tsx`
  renders it as the row's "archived …" time.
- Verified: it predates this round — `git log -L` puts the line at `04b8cdc feat(orchestration): hub tools,
  reporting and the raised hand`.

## Acceptance criteria

- [ ] A session archived at a known time, whose reopen then fails, still reports that time and keeps its place in
      the archive.
- [ ] The status, binding and process rollback that branch performs is unchanged.
- [ ] A daemon test covers a failed relaunch leaving `ended_at` as it was.
