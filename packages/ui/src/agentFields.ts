// The per-agent settings of a console or a project as its dialog holds them: one `PerAgent` value
// per kind of setting, each `null` while unset. The rules for putting an edit on the wire are the
// same for both and live here. A project's accounts are the exception, three-state rather than a
// string, and are held and diffed apart.

import { AGENT_OPTIONS } from "./agents";
import type { Agent, Launch, LaunchChange, PerAgent, ProjectAccount } from "./protocol";

/** `value` for every agent. */
function everyAgent<T>(value: (agent: Agent) => T): PerAgent<T> {
  return Object.fromEntries(AGENT_OPTIONS.map(({ value: agent }) => [agent, value(agent)])) as PerAgent<T>;
}

/** What a dialog holds for each agent's account, taken from the stored record's `source`: `null`
 * where it holds none. */
export function readAccounts(source: PerAgent<string | null> | undefined): PerAgent<string | null> {
  return everyAgent((agent) => source?.[agent] ?? null);
}

/** What a dialog holds for each agent's model and effort, taken from the stored record's `source`:
 * `null` where it holds none. */
export function readLaunch(source: PerAgent<Launch> | undefined): PerAgent<Launch> {
  return everyAgent((agent) => ({ model: source?.[agent].model ?? null, effort: source?.[agent].effort ?? null }));
}

/** The agents' entries of `current` that differ from `initial`, what the dialog opened with, as
 * `change` makes them; `undefined` when none does. Every agent is compared, so an edit to an agent
 * no longer in view is kept, and an entry is sent only when it was touched, which leaves a stored
 * account reference that was since removed (shown as the default) alone until another account is
 * chosen. */
function changedPerAgent<T, C>(
  current: PerAgent<T>,
  initial: PerAgent<T>,
  change: (current: T, initial: T) => C | undefined,
): Partial<PerAgent<C>> | undefined {
  const changes: Partial<PerAgent<C>> = {};
  for (const { value: agent } of AGENT_OPTIONS) {
    const changed = change(current[agent], initial[agent]);
    if (changed !== undefined) changes[agent] = changed;
  }
  return Object.keys(changes).length > 0 ? changes : undefined;
}

/** The accounts of `current` to send, diffed against `initial`.
 *
 * On `"update"` a change back to unset is an explicit `null` (clear), never an omitted agent:
 * omitting it would leave the old value in place while the picker says otherwise. On `"create"`
 * nothing is stored yet, so what is sent is whatever was set, and unset is simply absent. */
export function changedAccounts(
  mode: "create",
  current: PerAgent<string | null>,
  initial: PerAgent<string | null>,
): Partial<PerAgent<string>> | undefined;
export function changedAccounts(
  mode: "update",
  current: PerAgent<string | null>,
  initial: PerAgent<string | null>,
): Partial<PerAgent<string | null>> | undefined;
export function changedAccounts(
  mode: "create" | "update",
  current: PerAgent<string | null>,
  initial: PerAgent<string | null>,
): Partial<PerAgent<string | null>> | undefined {
  return changedPerAgent(current, initial, (now, before) => {
    if (now === before) return undefined;
    return mode === "update" || now ? now : undefined;
  });
}

/** The launch settings of `current` to send, diffed against `initial` on the rules of
 * `changedAccounts`, field by field: only what was touched, with a change back to unset an
 * explicit `null` on `"update"` and simply absent on `"create"`. */
export function changedLaunch(
  mode: "create",
  current: PerAgent<Launch>,
  initial: PerAgent<Launch>,
): Partial<PerAgent<{ model?: string; effort?: string }>> | undefined;
export function changedLaunch(
  mode: "update",
  current: PerAgent<Launch>,
  initial: PerAgent<Launch>,
): Partial<PerAgent<LaunchChange>> | undefined;
export function changedLaunch(
  mode: "create" | "update",
  current: PerAgent<Launch>,
  initial: PerAgent<Launch>,
): Partial<PerAgent<LaunchChange>> | undefined {
  return changedPerAgent(current, initial, (now, before) => {
    const change: LaunchChange = {};
    for (const key of ["model", "effort"] as const) {
      if (now[key] === before[key]) continue;
      if (mode === "update" || now[key]) change[key] = now[key];
    }
    return Object.keys(change).length > 0 ? change : undefined;
  });
}

/** What a project dialog holds for each agent's account, taken from the stored record's `source`:
 * inheriting where it holds none, as a project about to be created does. */
export function readProjectAccounts(source: PerAgent<ProjectAccount> | undefined): PerAgent<ProjectAccount> {
  return everyAgent((agent): ProjectAccount => source?.[agent] ?? { kind: "inherit" });
}

const sameAccount = (a: ProjectAccount, b: ProjectAccount): boolean =>
  a.kind === "account" ? b.kind === "account" && a.id === b.id : a.kind === b.kind;

/** The project accounts of `current` to send, diffed against `initial` on the rules of
 * `changedAccounts`: only what was touched, every agent's, with a change back to inheriting an
 * explicit `null` on `"update"` and simply absent on `"create"`. */
export function changedProjectAccounts(
  mode: "create",
  current: PerAgent<ProjectAccount>,
  initial: PerAgent<ProjectAccount>,
): Partial<PerAgent<ProjectAccount>> | undefined;
export function changedProjectAccounts(
  mode: "update",
  current: PerAgent<ProjectAccount>,
  initial: PerAgent<ProjectAccount>,
): Partial<PerAgent<ProjectAccount | null>> | undefined;
export function changedProjectAccounts(
  mode: "create" | "update",
  current: PerAgent<ProjectAccount>,
  initial: PerAgent<ProjectAccount>,
): Partial<PerAgent<ProjectAccount | null>> | undefined {
  return changedPerAgent<ProjectAccount, ProjectAccount | null>(current, initial, (now, before) => {
    if (sameAccount(now, before)) return undefined;
    if (now.kind !== "inherit") return now;
    return mode === "update" ? null : undefined;
  });
}
