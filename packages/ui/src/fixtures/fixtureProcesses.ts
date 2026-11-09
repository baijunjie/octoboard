// What the fixture daemon answers for the processes the Processes panel lists and the confirmations stop or keep.
import { type ProcessCount, type ProcessTree, type UnstoppedProcess, type UnstoppedReason } from "../protocol";
import { isLive } from "../sessions/sessionStatus";
import { neverReplied, type FixtureContext, type Handlers } from "./fixtureContext";

/** How long after its first `list_processes` (or, for the rail's list, its first `list_all_processes`) the fixture
 * daemon starts answering with a scenario's `listLater`: past the moments a dialog asks twice while it mounts
 * (StrictMode), before its next poll. */
export const LIST_LATER_AFTER_MS = 1000;

/** The process ledger of a scenario (`processes`), which the replies about processes read and the archive of a session
 * and the removal of a project change. */
export function createProcessLedger({ scenario, store, fail }: FixtureContext) {
  // The process ledger. A tree of a launch being followed is listed while its session runs; stopping one takes it out
  // of what the daemon follows, as the real ledger does, and so does the end of its session. Keeping one moves it to
  // its project's processes, marked kept, which no session stops and which stay listed.
  let followed = [...(scenario.processes?.trees ?? [])];
  const projectProcesses = new Map(Object.entries(scenario.processes?.projectProcesses ?? {}).map(([project, trees]) => [project, [...trees]]));
  const hasProcess = (session: string) => {
    const record = store.getState().sessions.get(session);
    return record !== undefined && isLive(record.status);
  };
  const oldestFirst = <T extends ProcessTree>(trees: T[]): T[] => [...trees].sort((a, b) => a.started_at - b.started_at);
  const treesOfSession = (session: string): ProcessTree[] => (hasProcess(session) ? oldestFirst(followed.filter((tree) => tree.session_id === session)) : []);
  // Stopping a tree names it by its id and takes it from the ledger when it still qualifies: a followed launch's of a
  // session that runs, or a project process.
  const stopTrees = (ids: string[]): string[] => {
    const taken = [
      ...followed.filter((tree) => ids.includes(tree.id) && hasProcess(tree.session_id)).map((tree) => tree.id),
      ...[...projectProcesses.values()].flat().filter((tree) => ids.includes(tree.id)).map((tree) => tree.id),
    ];
    followed = followed.filter((tree) => !taken.includes(tree.id));
    for (const [project, trees] of projectProcesses) projectProcesses.set(project, trees.filter((tree) => !taken.includes(tree.id)));
    return taken;
  };
  // A stop that leaves trees running reports each tree that qualifies for stopping as not stopped, naming its first
  // process as its command gives it (its id begins with the process id), and takes none from the ledger.
  const leftRunning = (ids: string[], reason: UnstoppedReason): UnstoppedProcess[] =>
    [...followed.filter((tree) => hasProcess(tree.session_id)), ...[...projectProcesses.values()].flat()]
      .filter((tree) => ids.includes(tree.id))
      .map((tree) => ({ tree_id: tree.id, pid: Number(tree.id.split(":")[0]), name: tree.command.split(" ")[0].split("/").pop() ?? "", reason }));
  // Keeping applies to a followed launch's tree of a session that runs in a project: a console session keeps nothing,
  // and a project process is already beyond every session.
  const keepTrees = (ids: string[]): string[] => {
    const kept = followed.flatMap((tree) => {
      const project = store.getState().sessions.get(tree.session_id)?.project_id;
      return ids.includes(tree.id) && hasProcess(tree.session_id) && typeof project === "string" ? [{ project, tree }] : [];
    });
    followed = followed.filter((tree) => !kept.some((entry) => entry.tree === tree));
    for (const { project, tree: { session_id: _session, ...rest } } of kept) projectProcesses.set(project, [...(projectProcesses.get(project) ?? []), { ...rest, kept: true }]);
    return kept.map(({ tree }) => tree.id);
  };
  // A keep, or an archive that keeps, naming a tree the daemon lists as unkeepable is refused whole, before any tree is
  // kept: that tree would end with its session whatever kept it. The process named is the tree's first, as its command
  // gives it, and its id begins with the process id.
  const refuseUnkeepable = (ids: string[]): void => {
    const tree = followed.find((candidate) => ids.includes(candidate.id) && !candidate.keepable);
    if (tree === undefined) return;
    const name = tree.command.split(" ")[0].split("/").pop() ?? "";
    fail({ code: "process_shares_agent_console", params: { pid: tree.id.split(":")[0], name }, message: `process ${tree.id.split(":")[0]} shares its agent's console` });
  };
  // The count of trees as the daemon's `list_all_processes` answers it: one entry for each project the store has that
  // has trees, in the store's order (its running sessions' trees and the ones no session owns), then one for each
  // console session with trees. What is kept under a project the store no longer has is not reported.
  const allProcessCounts = (): ProcessCount[] => {
    const { projects, sessions } = store.getState();
    const owned = (project: string) => [...sessions.values()].filter((session) => session.project_id === project).flatMap((session) => treesOfSession(session.id));
    const counts: ProcessCount[] = [...projects.keys()].map((project_id) => ({ project_id, trees: owned(project_id).length + (projectProcesses.get(project_id)?.length ?? 0) }));
    for (const session of sessions.values()) if (session.project_id === null) counts.push({ session_id: session.id, trees: treesOfSession(session.id).length });
    return counts.filter((count) => count.trees > 0);
  };
  /** The trees of a project that no session owns (kept ones, and ones left behind in a session's ending), oldest first. */
  const projectTrees = (project: string) => oldestFirst(projectProcesses.get(project) ?? []);
  /** The end of sessions takes the trees they were following out of the ledger. */
  const endSessions = (ended: ReadonlySet<string>) => {
    followed = followed.filter((tree) => !ended.has(tree.session_id));
  };
  return { hasProcess, oldestFirst, treesOfSession, stopTrees, leftRunning, keepTrees, refuseUnkeepable, projectTrees, endSessions, allProcessCounts };
}

/** The process ledger of `createProcessLedger`. */
export type ProcessLedger = ReturnType<typeof createProcessLedger>;

/** The replies about processes. */
export function createProcessReplies({ scenario, store, fail, refuseUnknownProject }: FixtureContext, ledger: ProcessLedger): Handlers {
  const { hasProcess, treesOfSession, stopTrees, leftRunning, keepTrees, refuseUnkeepable, projectTrees, allProcessCounts } = ledger;
  let firstListedAt: number | undefined;
  let firstAllListedAt: number | undefined;

  return {
    list_processes: (body) => {
      if (scenario.processes?.list === "pending") return neverReplied();
      if (scenario.processes?.list) return fail(scenario.processes.list);
      firstListedAt ??= Date.now();
      if (scenario.processes?.listLater && Date.now() - firstListedAt >= LIST_LATER_AFTER_MS) return fail(scenario.processes.listLater);
      if ("session_id" in body) {
        if (!store.getState().sessions.has(body.session_id)) return fail({ code: "unknown_session", params: { session: body.session_id }, message: `unknown session ${body.session_id}` });
        if (!hasProcess(body.session_id)) return fail({ code: "session_not_running", params: { session: body.session_id }, message: `session ${body.session_id} has no running process` });
        return { type: "processes_listed", id: null, trees: treesOfSession(body.session_id) };
      }
      refuseUnknownProject(body.project_id);
      const refused = scenario.processes?.projectList?.[body.project_id];
      if (refused) return fail(refused);
      const running = [...store.getState().sessions.values()].filter((session) => session.project_id === body.project_id && hasProcess(session.id));
      return { type: "processes_listed", id: null, trees: [...running.flatMap((session) => treesOfSession(session.id)), ...projectTrees(body.project_id)] };
    },
    list_all_processes: () => {
      if (scenario.processes?.list === "pending") return neverReplied();
      if (scenario.processes?.list) return fail(scenario.processes.list);
      firstAllListedAt ??= Date.now();
      if (scenario.processes?.listLater && Date.now() - firstAllListedAt >= LIST_LATER_AFTER_MS) return fail(scenario.processes.listLater);
      return { type: "all_processes_listed", id: null, counts: allProcessCounts() };
    },
    stop_processes: (body) => {
      if (scenario.processes?.stop === "pending") return neverReplied();
      const stop = scenario.processes?.stop;
      if (stop && "leavesRunning" in stop) return { type: "processes_stopped", id: null, ids: [], not_stopped: leftRunning(body.ids, stop.leavesRunning) };
      if (stop) return fail(stop);
      return { type: "processes_stopped", id: null, ids: stopTrees(body.ids) };
    },
    keep_processes: (body) => {
      if (scenario.processes?.keep === "pending") return neverReplied();
      if (scenario.processes?.keep) return fail(scenario.processes.keep);
      refuseUnkeepable(body.ids);
      return { type: "processes_kept", id: null, ids: keepTrees(body.ids) };
    },
  };
}
