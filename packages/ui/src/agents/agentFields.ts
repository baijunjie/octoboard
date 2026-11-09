// The per-agent settings of a console or a project as its dialog holds them: one `PerAgent` value
// per kind of setting, each `null` while unset. The rules for putting an edit on the wire are the
// same for both and live here. A project's accounts are the exception, three-state rather than a
// string, and are held and diffed apart.

import type { Agent, ClientRequestBody, Launch, LaunchChange, PerAgent, ProjectAccount } from "../protocol";

/** One agent's model and reasoning effort in a new record: a field left out, or blank, is the agent's own default. */
type LaunchSetting = NonNullable<NonNullable<Extract<ClientRequestBody, { type: "create_console" }>["launch"]>[Agent]>;

/** `value` for each of the `served` agents. */
function everyAgent<T>(served: readonly Agent[], value: (agent: Agent) => T): PerAgent<T> {
  return Object.fromEntries(served.map((agent) => [agent, value(agent)]));
}

/** What a dialog holds for each agent's account, taken from the stored record's `source`: `null`
 * where it holds none. */
export function readAccounts(source: PerAgent<string | null> | undefined, served: readonly Agent[]): PerAgent<string | null> {
  return everyAgent(served, (agent) => source?.[agent] ?? null);
}

/** What a dialog holds for each agent's model and effort, taken from the stored record's `source`:
 * `null` where it holds none. */
export function readLaunch(source: PerAgent<Launch> | undefined, served: readonly Agent[]): PerAgent<Launch> {
  return everyAgent(served, (agent) => ({ model: source?.[agent]?.model ?? null, effort: source?.[agent]?.effort ?? null }));
}

/** `values` with `agent`'s entry set to `value`: how a dialog puts one agent's account back to what the record holds. */
export function withAgent<T>(values: PerAgent<T>, agent: Agent, value: T): PerAgent<T> {
  return { ...values, [agent]: value };
}

/** `values` with `agent`'s launch `key` set to `value`, the agent's other field as it was: how a dialog puts one launch
 * setting back to what the record holds. */
export function withLaunchField(values: PerAgent<Launch>, agent: Agent, key: keyof Launch, value: string | null): PerAgent<Launch> {
  return withAgent(values, agent, { ...values[agent]!, [key]: value });
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
  for (const agent of Object.keys(current)) {
    const changed = change(current[agent]!, initial[agent]!);
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
): Partial<PerAgent<LaunchSetting>> | undefined;
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
export function readProjectAccounts(source: PerAgent<ProjectAccount> | undefined, served: readonly Agent[]): PerAgent<ProjectAccount> {
  return everyAgent(served, (agent): ProjectAccount => source?.[agent] ?? { kind: "inherit" });
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
