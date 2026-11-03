import type { ProcessCount, ProcessTree } from "../protocol";
import type { Daemon } from "../store";

type Request = Daemon["request"];

/** What a process list is asked for: one session's trees, or a project's. */
export type ProcessTarget = { session_id: string } | { project_id: string };

/** The trees of a session's current launch, or of a project's running sessions followed by its
 * project processes. Rejects like any request: a session with no process is refused. */
export async function listProcesses(request: Request, target: ProcessTarget): Promise<ProcessTree[]> {
  return (await request({ type: "list_processes", ...target })).trees;
}

/** Stops the trees `ids` name; resolves with the ids of those it stopped. */
export async function stopProcesses(request: Request, ids: string[]): Promise<string[]> {
  return (await request({ type: "stop_processes", ids })).ids;
}

/** Makes the trees `ids` the user's; resolves with the ids of those it kept. */
export async function keepProcesses(request: Request, ids: string[]): Promise<string[]> {
  return (await request({ type: "keep_processes", ids })).ids;
}

/** How many trees each project and each console session has: an entry per project with any, then one per console
 * session with any. */
export async function listAllProcesses(request: Request): Promise<ProcessCount[]> {
  return (await request({ type: "list_all_processes" })).counts;
}
