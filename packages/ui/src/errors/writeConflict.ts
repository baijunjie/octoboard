// The two halves of a checked write (see "Checked writes" in `apps/daemon/PROTOCOL.md`): the `base_versions` a write
// carries, and what a refused one (`write_conflict`) names. Every write that carries versions goes through here.

import { CodedError } from "./codedError";

/** A record's `versions`: the version of each field a form edits, by its dotted path. */
export type FieldVersions = Record<string, number>;

/** The `base_versions` of a write that changes `paths`: for each, the version `read` holds, which is the one the edit
 * was based on. A path `read` has no version of (a record that predates versions) is left out, so it is not checked;
 * `undefined` when no path is checked, which is how the wire leaves a write unchecked. */
export function baseVersions(read: FieldVersions | undefined, paths: readonly string[]): FieldVersions | undefined {
  const based = Object.fromEntries(paths.flatMap((path) => (read?.[path] === undefined ? [] : [[path, read[path]]])));
  return Object.keys(based).length > 0 ? based : undefined;
}

/** The paths of a per-agent change, `<prefix>.<agent>` for each agent it names (an account reference, whole). */
export function agentPaths(prefix: string, changes: object | undefined): string[] {
  return Object.keys(changes ?? {}).map((agent) => `${prefix}.${agent}`);
}

/** The paths of a per-agent change of fields, `<prefix>.<agent>.<field>` for each field set under each agent (a launch
 * setting's model and effort, or each compatibility switch). */
export function agentFieldPaths(prefix: string, changes: Record<string, object | undefined> | undefined): string[] {
  return Object.entries(changes ?? {}).flatMap(([agent, fields]) => Object.entries(fields ?? {}).flatMap(([field, value]) => field === "compatibility" ? Object.keys(value as object).map((setting) => `${prefix}.${agent}.compatibility.${setting}`) : [`${prefix}.${agent}.${field}`]));
}

/** What a `write_conflict` names: the paths whose versions differed and the versions the record reaches. */
export interface WriteConflict {
  fields: string[];
  /** Of each of `fields`, the version the record has now (or will have once its broadcast arrives). */
  versions: FieldVersions;
}

/** `error` as the `write_conflict` it is, or `undefined` for any other failure. The `versions` the daemon sends as JSON
 * text: one that does not read as an object of whole numbers names no versions, and the paths it fails to name are
 * left out of it. */
export function writeConflict(error: unknown): WriteConflict | undefined {
  if (!(error instanceof CodedError) || error.code !== "write_conflict") return undefined;
  const fields = (error.params.fields ?? "").split(",").filter((path) => path !== "");
  let named: unknown;
  try {
    named = JSON.parse(error.params.versions ?? "{}");
  } catch {
    named = undefined;
  }
  const versions: FieldVersions = {};
  if (typeof named === "object" && named !== null) {
    for (const path of fields) {
      const version = (named as Record<string, unknown>)[path];
      if (typeof version === "number" && Number.isFinite(version)) versions[path] = version;
    }
  }
  return { fields, versions };
}

/** Whether `current`, the versions of the record in the store, has reached every version `conflict` names: from then on
 * the store shows the value the conflict is about. The reply can arrive before the broadcast of that change. */
export function reachedConflict(conflict: WriteConflict, current: FieldVersions | undefined): boolean {
  return conflict.fields.every((path) => conflict.versions[path] === undefined || (current?.[path] ?? -1) >= conflict.versions[path]);
}
