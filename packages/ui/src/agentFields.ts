// The per-agent fields of a console or a project as its dialog holds them: one flat record keyed
// by the wire field names, each `null` while unset. The dialog supplies the field table of its own
// owner; the rules for putting an edit on the wire are the same for both and live here. A project's
// accounts are the exception, three-state rather than a string, and are held and diffed apart.

import { AGENT_ACCOUNT_FIELD, AGENT_OPTIONS } from "./agents";
import type { AccountField, ProjectAccount } from "./protocol";

/** What a dialog holds for the fields in `keys`, taken from the stored record `source`: `null`
 * where it holds none. */
export function readAgentFields<K extends string>(
  keys: readonly K[],
  source: Partial<Record<K, string | null>>,
): Record<K, string | null> {
  const fields = {} as Record<K, string | null>;
  for (const key of keys) fields[key] = source[key] ?? null;
  return fields;
}

/** The fields of `current` to send, diffed against `initial`, what the dialog opened with. Every
 * agent's fields are compared, so an edit to an agent no longer in view is kept, and a field is
 * sent only when it was touched, which leaves a stored account reference that was since removed
 * (shown as the default) alone until another account is chosen.
 *
 * On `"update"` a change back to unset is an explicit `null` (clear), never an omitted field:
 * omitting it would leave the old value in place while the picker says otherwise. On `"create"`
 * nothing is stored yet, so what is sent is whatever was set, and unset is simply absent. */
export function changedAgentFields<K extends string>(
  mode: "create",
  current: Record<K, string | null>,
  initial: Record<K, string | null>,
): Partial<Record<K, string>>;
export function changedAgentFields<K extends string>(
  mode: "update",
  current: Record<K, string | null>,
  initial: Record<K, string | null>,
): Partial<Record<K, string | null>>;
export function changedAgentFields<K extends string>(
  mode: "create" | "update",
  current: Record<K, string | null>,
  initial: Record<K, string | null>,
): Partial<Record<K, string | null>> {
  const changes: Partial<Record<K, string | null>> = {};
  for (const key of Object.keys(current) as K[]) {
    if (current[key] === initial[key]) continue;
    if (mode === "update" || current[key]) changes[key] = current[key];
  }
  return changes;
}

/** What a project dialog holds for each agent's account, taken from the stored record `source`:
 * inheriting where it holds none, as a project about to be created does. */
export function readProjectAccounts(
  source: Partial<Record<AccountField, ProjectAccount>>,
): Record<AccountField, ProjectAccount> {
  const inherit: ProjectAccount = { kind: "inherit" };
  return Object.fromEntries(
    AGENT_OPTIONS.map(({ value }) => [AGENT_ACCOUNT_FIELD[value], source[AGENT_ACCOUNT_FIELD[value]] ?? inherit]),
  ) as Record<AccountField, ProjectAccount>;
}

const sameAccount = (a: ProjectAccount, b: ProjectAccount): boolean =>
  a.kind === "account" ? b.kind === "account" && a.id === b.id : a.kind === b.kind;

/** The project accounts of `current` to send, diffed against `initial` on the rules of
 * `changedAgentFields`: only what was touched, every agent's, with a change back to inheriting an
 * explicit `null` on `"update"` and simply absent on `"create"`. */
export function changedProjectAccounts(
  mode: "create" | "update",
  current: Record<AccountField, ProjectAccount>,
  initial: Record<AccountField, ProjectAccount>,
): Partial<Record<AccountField, ProjectAccount | null>> {
  const changes: Partial<Record<AccountField, ProjectAccount | null>> = {};
  for (const key of Object.keys(current) as AccountField[]) {
    if (sameAccount(current[key], initial[key])) continue;
    if (current[key].kind !== "inherit") changes[key] = current[key];
    else if (mode === "update") changes[key] = null;
  }
  return changes;
}
