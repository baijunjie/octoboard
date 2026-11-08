import type { AgentCliStatus } from "../../protocol";

const VERSION = /(\d+)\.(\d+)\.(\d+)/;

/** The `major.minor.patch` numbers of a version as a CLI or its package source names it (`2.1.319`, `v0.5.1`,
 * `rust-v0.5.1`); anything after the third number is ignored. Undefined where there are none to read. */
function parts(version: string): number[] | undefined {
  const found = VERSION.exec(version);
  return found ? [Number(found[1]), Number(found[2]), Number(found[3])] : undefined;
}

/** Whether `latest` is a newer version than `installed`, number by number. Undefined where either has no
 * `major.minor.patch` to compare, so the caller says nothing about it rather than guessing. */
export function isNewerVersion(latest: string, installed: string): boolean | undefined {
  const newer = parts(latest);
  const held = parts(installed);
  if (!newer || !held) return undefined;
  for (let at = 0; at < 3; at++) {
    if (newer[at] !== held[at]) return newer[at] > held[at];
  }
  return false;
}

/** Whether an agent's CLI has an update to install: a newer version than the installed one is known, and the install
 * method names a command that updates it. What the agent's actions button marks with a badge, its menu's Version entry
 * with a dot, and what makes the Version dialog's Update button the primary one. */
export function hasCliUpdate(status: AgentCliStatus | undefined): boolean {
  const { installed_version: installed, latest_version: latest } = status ?? {};
  return !!installed && !!latest && status?.install_method !== "unknown" && isNewerVersion(latest, installed) === true;
}
