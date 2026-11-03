// The sessions the archive view's tests and the gallery's "Archive view" scenarios share: one console with a
// project, a live console session with some of its own bound sessions since archived, and sessions that
// are not bound to it.
import { consoleOf, contextOf, minutesAgo, projectOf, sessionOf } from "./builders";

export const console_ = consoleOf("c-1", "Main");
export const archiveProject = projectOf("p-1", console_.id, "Search API");

// A live console session with some of its own bound sessions since archived, among other sessions
// not bound to it. The gallery keeps them out of its project archive's base state, so that mixing them
// in does not perturb the counts its scenarios assert, and `ArchiveView.test.tsx` renders the view on
// these directly.
export const boundConsoleSession = sessionOf("s-bound-console", console_.id, undefined, "Hub", "idle", { colour: "teal" });
export const boundArchived = Array.from({ length: 3 }, (_, i) =>
  sessionOf(`s-bound${i}`, console_.id, archiveProject.id, `Bound archived session ${i}`, "archived", {
    bound_to: boundConsoleSession.id,
    ended_at: minutesAgo(5 + i * 5),
  }),
);
// A second owner, an archived session bound to it instead of `boundConsoleSession`, and an
// archived session with no owner at all — so `ArchiveView.test.tsx` can tell a real filter from
// one that happens to pass everything through.
export const otherConsoleSession = sessionOf("s-other-console", console_.id, undefined, "Hub 2", "idle", { colour: "lime" });
export const otherOwnerArchived = sessionOf("s-other-bound", console_.id, archiveProject.id, "Archived session bound elsewhere", "archived", {
  bound_to: otherConsoleSession.id,
  context: { tokens: 846_209 },
  ended_at: minutesAgo(5),
});
export const unboundArchived = sessionOf("s-unbound-archived", console_.id, archiveProject.id, "Unbound archived session", "archived", {
  context: { tokens: 0 },
  ended_at: minutesAgo(5),
});

// An archived bound coordinator bound to `boundConsoleSession` with two archived sessions of its own:
// a console session's archive reaches them, and lists them under their bound coordinator.
export const archivedCoordinator = sessionOf("s-coordinator-archived", console_.id, archiveProject.id, "Release bound coordinator", "archived", {
  bound_to: boundConsoleSession.id,
  context: contextOf(65),
  ended_at: minutesAgo(8),
});
export const underArchivedCoordinator = Array.from({ length: 2 }, (_, i) =>
  sessionOf(`s-under-coordinator${i}`, console_.id, archiveProject.id, `Session ${i} of the archived bound coordinator`, "archived", {
    bound_to: archivedCoordinator.id,
    ended_at: minutesAgo(2 + i * 3),
  }),
);
