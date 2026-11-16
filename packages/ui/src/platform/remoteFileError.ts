import { shellFailure } from "./shellFailure";

/** Why the shell could not store or open the copy of a remote file, as it says: `launchable` what this computer would
 * run, so nothing was stored; `storage` the copy could not be written; `incomplete` the parts that arrived are not the
 * file's size; `open` the copy is whole but the system could not mark it or open it. `detail` is the system's text,
 * which cannot be localized. */
export class RemoteFileError extends Error {
  readonly kind: "launchable" | "storage" | "incomplete" | "open";
  constructor(kind: RemoteFileError["kind"], detail = "") {
    super(detail);
    this.kind = kind;
  }
}

const KINDS: readonly RemoteFileError["kind"][] = ["launchable", "storage", "incomplete", "open"];

/** What a rejection of one of the shell's file commands is: a `RemoteFileError` where it says one of its kinds, else
 * `undefined`. */
export function remoteFileError(failure: unknown): RemoteFileError | undefined {
  const { kind, detail } = shellFailure(failure);
  const known = KINDS.find((candidate) => candidate === kind);
  return known ? new RemoteFileError(known, detail) : undefined;
}
