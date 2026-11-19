import type { Console, MemoryScope, Project } from "../../protocol";

/** Whose memory a dialog shows: a console's, or the memory of the place a project is at (its host and stored path,
 * shared by every project there). */
export type MemoryTarget = { kind: "console"; console: Console } | { kind: "project"; project: Project };

/** The part of a memory request that names the memory. */
export function targetRequest(target: MemoryTarget): { console: string } | { project: string } {
  return target.kind === "console" ? { console: target.console.id } : { project: target.project.id };
}

/** What identifies the memory across renders, so that a hook starts afresh for another one. */
export function targetKey(target: MemoryTarget): string {
  return target.kind === "console" ? `console:${target.console.id}` : `place:${target.project.host_id}\u0000${target.project.path}`;
}

/** Whether `scope`, one the daemon sent, is the memory `target` shows. A scope of a kind this client does not know is
 * nobody's: it is a tagged union, so the unknown kind is handled here, by hand. */
export function scopeMatches(target: MemoryTarget, scope: MemoryScope): boolean {
  const kind: string = scope.kind;
  if (kind === "console" && "console_id" in scope) return target.kind === "console" && scope.console_id === target.console.id;
  if (kind === "place" && "host_id" in scope) return target.kind === "project" && scope.host_id === target.project.host_id && scope.path === target.project.path;
  return false;
}
