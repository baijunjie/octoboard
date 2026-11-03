// The causes a coded failure carries in `params.cause`, which the UI words itself: each is read with the client's
// own fallback, as a daemon newer than this client may send one it does not know.

const SOURCE_UNAVAILABLE_CAUSES = ["missing", "link", "not_a_directory", "other"] as const;

/** The `cause` of a `source_unavailable`: why a folder a request is scoped to cannot be read. */
export type SourceUnavailableCause = (typeof SOURCE_UNAVAILABLE_CAUSES)[number];

/** A worktree's `scope_cause`: a `source_unavailable` cause, or a read the system denied. */
export type ScopeCause = SourceUnavailableCause | "permission_denied";

/** `raw` as a `source_unavailable` cause: none, from a daemon that does not send one, is `missing`, and one this
 * client does not know is `other`. */
export function sourceUnavailableCause(raw: string | undefined): SourceUnavailableCause {
  if (raw === undefined) return "missing";
  return SOURCE_UNAVAILABLE_CAUSES.find((cause) => cause === raw) ?? "other";
}

/** `raw` as a worktree's `scope_cause`, by the same rules as `sourceUnavailableCause`. */
export function scopeCause(raw: string | undefined): ScopeCause {
  return raw === "permission_denied" ? raw : sourceUnavailableCause(raw);
}

const STORE_WRITE_CAUSES = ["access", "disk_full", "other"] as const;

/** The `cause` of a `store_write_failed`: why the application could not read or write its own database. */
export type StoreWriteCause = (typeof STORE_WRITE_CAUSES)[number];

/** `raw` as a `store_write_failed` cause: one this client does not know, and none, are `other`, whose remedy covers
 * a data folder that is writable on a disk that is available. */
export function storeWriteCause(raw: string | undefined): StoreWriteCause {
  return STORE_WRITE_CAUSES.find((cause) => cause === raw) ?? "other";
}

const PATH_LINK_REFUSED_CAUSES = ["ambiguous", "launchable", "not_private", "not_a_file_or_folder", "unreadable", "other"] as const;

/** The `cause` of a `path_link_refused`: why a path inside a root is not opened from a link. */
export type PathLinkRefusedCause = (typeof PATH_LINK_REFUSED_CAUSES)[number];

/** `raw` as a `path_link_refused` cause: one this client does not know, or none, is `other` (a path that cannot be
 * opened from a link, for no reason it can say). */
export function pathLinkRefusedCause(raw: string | undefined): PathLinkRefusedCause {
  return PATH_LINK_REFUSED_CAUSES.find((cause) => cause === raw) ?? "other";
}

const RELOCATION_CAUSES = ["permission_denied", "busy", "no_space", "config_unreadable", "sqlite_home", "symlink", "not_restored", "other"] as const;

/** The `cause` of a `relocation_failed`: why a conversation move did not complete. */
export type RelocationCause = (typeof RELOCATION_CAUSES)[number];

/** `raw` as a `relocation_failed` cause: one this client does not know, or none, is `other`. */
export function relocationCause(raw: string | undefined): RelocationCause {
  return RELOCATION_CAUSES.find((cause) => cause === raw) ?? "other";
}
