import type { Account, Agent } from "../protocol";

/** Whether `path` is one the daemon accepts as a config directory: absolute, or starting with
 * `~/` (or just `~`), which it expands. Only the shape is judged — whether the directory exists
 * is not asked, since an agent may be pointed at one that is yet to be created. */
export function isAbsoluteConfigDir(path: string): boolean {
  const trimmed = path.trim();
  return trimmed.startsWith("/") || trimmed === "~" || trimmed.startsWith("~/");
}

const DEFAULT_ACCOUNT_KEY = "Default";

/** What a typed account name collides with: another account of the same agent (by its stored
 * name) or the agent's default account. */
export type NameCollision = { kind: "account"; name: string } | { kind: "default"; name: string };

/** `toLowerCase()` and not a locale-aware variant, deliberately: it has to match the daemon's
 * locale-independent `to_lowercase()`, and a locale-aware one would diverge under tr-TR. */
function nameKey(name: string): string {
  return name.trim().toLowerCase();
}

/** The account name `typed` would collide with within `agent`, compared trimmed and
 * case-insensitively as the daemon does. The default account takes part under `defaultName`, its
 * name in the current language: the daemon only knows it by a fixed English key, so a name that
 * collides only in this language has to be caught here. `editing` is the account being edited,
 * which does not collide with itself. */
export function nameCollision(
  typed: string,
  agent: Agent,
  accounts: Account[],
  defaultName: string,
  editing?: string,
): NameCollision | undefined {
  const key = nameKey(typed);
  if (key === nameKey(defaultName)) return { kind: "default", name: defaultName };
  // The daemon reserves this English word as its language-independent comparison key for the
  // default account, not as display text; checking it here reports that collision in place too.
  if (key === nameKey(DEFAULT_ACCOUNT_KEY)) return { kind: "default", name: defaultName };
  const taken = accounts.find((a) => a.agent === agent && a.id !== editing && nameKey(a.name) === key);
  return taken && { kind: "account", name: taken.name };
}
