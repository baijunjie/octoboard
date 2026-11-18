import { CodedError } from "../errors/codedError";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import type { ProcessCount, ProcessTree, UnkeepableReason, UnstoppedProcess } from "../protocol";
import type { Daemon } from "../store";
import { knownValues } from "../values/knownValue";

type Request = Daemon["request"];

/** What a process list is asked for: one session's trees, or a project's. */
export type ProcessTarget = { session_id: string } | { project_id: string };

/** The trees of a session's current launch, or of a project's running sessions followed by its
 * project processes. Rejects like any request: a session with no process is refused. */
export async function listProcesses(request: Request, target: ProcessTarget): Promise<ProcessTree[]> {
  return (await request({ type: "list_processes", ...target })).trees;
}

/** Stops the trees `ids` name; resolves with the ids of those it stopped. Rejects with `notStoppedRefusal` once the stop
 * is through when it left a process running, which a daemon older than the field never says. */
export async function stopProcesses(request: Request, ids: string[], t: Translate): Promise<string[]> {
  const reply = await request({ type: "stop_processes", ids });
  const left = reply.not_stopped?.[0];
  if (left) throw notStoppedRefusal(left, t);
  return reply.ids;
}

/** The failure of a stop that left `process` running, worded by why: the system refused it for lack of permission, or
 * anything else, which a reason this window does not know is taken for. */
export function notStoppedRefusal(process: UnstoppedProcess, t: Translate): CodedError {
  const denied = process.reason === "access_denied";
  return new CodedError(t(denied ? "daemon.process_not_stopped.accessDenied" : "daemon.process_not_stopped"), "process_not_stopped", { reason: process.reason });
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

/** Why a tree cannot be kept, as the sentence of the error the daemon refuses a keep with, so the reason shown ahead of
 * the choice and the refusal read alike. */
const UNKEEPABLE_MESSAGES: Record<UnkeepableReason, PlainMessageKey> = {
  agent_console: "daemon.process_shares_agent_console",
  agent_command_group: "daemon.process_in_agent_command_group",
};

const isUnkeepableReason = knownValues<UnkeepableReason>({ agent_console: true, agent_command_group: true });

/** A tree the daemon says cannot be kept. */
export type UnkeepableTree = ProcessTree & { keepable: false };

/** Whether `tree` cannot be kept: only a `keepable` the daemon sends as `false` says so, so a tree from a daemon older
 * than the field can be kept, as it always could. */
export function isUnkeepable(tree: ProcessTree): tree is UnkeepableTree {
  return tree.keepable === false;
}

/** The sentence saying why `tree` cannot be kept. A reason this window does not know is said plainly, as is a tree
 * with none. */
export function unkeepableMessage(tree: UnkeepableTree): PlainMessageKey {
  const reason = tree.unkeepable_reason;
  return isUnkeepableReason(reason) ? UNKEEPABLE_MESSAGES[reason] : "processes.unkeepable";
}
