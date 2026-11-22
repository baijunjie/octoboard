// The causes a coded failure carries in `params.cause`, which the UI words itself: each is read with the client's
// own fallback, as a daemon newer than this client may send one it does not know.

import type { SharingCause } from "../protocol";

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

const PATH_LINK_REFUSED_CAUSES = ["ambiguous", "launchable", "not_private", "not_a_file_or_folder", "unreadable", "unanswered", "other"] as const;

/** The `cause` of a `path_link_refused`: why a path inside a root is not opened from a link. */
export type PathLinkRefusedCause = (typeof PATH_LINK_REFUSED_CAUSES)[number];

/** `raw` as a `path_link_refused` cause: one this client does not know, or none, is `other` (a path that cannot be
 * opened from a link, for no reason it can say). */
export function pathLinkRefusedCause(raw: string | undefined): PathLinkRefusedCause {
  return PATH_LINK_REFUSED_CAUSES.find((cause) => cause === raw) ?? "other";
}

const RELOCATION_CAUSES = ["permission_denied", "busy", "no_space", "symlink", "not_restored", "other"] as const;

/** The `cause` of a `relocation_failed`: why a conversation move did not complete. */
export type RelocationCause = (typeof RELOCATION_CAUSES)[number];

/** `raw` as a `relocation_failed` cause: one this client does not know, or none, is `other`. */
export function relocationCause(raw: string | undefined): RelocationCause {
  return RELOCATION_CAUSES.find((cause) => cause === raw) ?? "other";
}

const ACCOUNT_TRASH_FAILED_CAUSES = ["unanswered", "unreadable", "other_unlooked", "signing_in"] as const;

/** The `cause` of an `account_directory_trash_failed`: why an account's directory was not moved to the Trash. */
export type AccountTrashFailedCause = (typeof ACCOUNT_TRASH_FAILED_CAUSES)[number];

/** `raw` as an `account_directory_trash_failed` cause: none, and one this client does not know, are `undefined`, a
 * move that failed for no reason the client can name, worded by the code's own key. */
export function accountTrashFailedCause(raw: string | undefined): AccountTrashFailedCause | undefined {
  return ACCOUNT_TRASH_FAILED_CAUSES.find((cause) => cause === raw);
}

const SESSION_DID_NOT_STOP_CAUSES = ["open_in_app"] as const;

/** The `cause` of a `session_did_not_stop`: the operation that waited for the session's process to end, where the daemon
 * names one other than the account switch the code was first raised for. */
export type SessionDidNotStopCause = (typeof SESSION_DID_NOT_STOP_CAUSES)[number];

/** `raw` as a `session_did_not_stop` cause: none, and one this client does not know, are `undefined`, worded by the
 * code's own key (the account switch's). */
export function sessionDidNotStopCause(raw: string | undefined): SessionDidNotStopCause | undefined {
  return SESSION_DID_NOT_STOP_CAUSES.find((cause) => cause === raw);
}

const PROJECT_TRASH_FAILED_CAUSES = ["unanswered", "unreadable"] as const;

/** The `cause` of a `project_directory_trash_failed`: why a project's directory was not moved to the Trash. */
export type ProjectTrashFailedCause = (typeof PROJECT_TRASH_FAILED_CAUSES)[number];

/** `raw` as a `project_directory_trash_failed` cause: none, and one this client does not know, are `undefined`, worded
 * by the code's own key. */
export function projectTrashFailedCause(raw: string | undefined): ProjectTrashFailedCause | undefined {
  return PROJECT_TRASH_FAILED_CAUSES.find((cause) => cause === raw);
}

const SHARING_CAUSES = Object.keys({ permission_denied: 0, busy: 0, not_found: 0, no_space: 0, conflict: 0, default_moved: 0, other: 0 } satisfies Record<SharingCause, 0>) as SharingCause[];

/** `raw` as the `cause` of a sharing failure (`account_sharing_failed`, `account_sharing_repair_failed`) or of one of
 * its entries: one this client does not know, and none, are `other`, which names no reason a reader could act on. */
export function sharingCause(raw: string | undefined): SharingCause {
  return SHARING_CAUSES.find((cause) => cause === raw) ?? "other";
}

const INVALID_SEARCH_PATTERN_CAUSES = ["unclosed_group", "unopened_group", "unclosed_class", "invalid_repetition", "invalid_escape", "unsupported", "line_break", "other"] as const;

/** The `cause` of an `invalid_search_pattern`: why a content search's query, read as a regular expression, does not
 * compile. */
export type InvalidSearchPatternCause = (typeof INVALID_SEARCH_PATTERN_CAUSES)[number];

/** `raw` as an `invalid_search_pattern` cause: one this client does not know, or none, is `other`. */
export function invalidSearchPatternCause(raw: string | undefined): InvalidSearchPatternCause {
  return INVALID_SEARCH_PATTERN_CAUSES.find((cause) => cause === raw) ?? "other";
}

const INVALID_FILE_MASK_CAUSES = ["unclosed_class", "unclosed_alternation", "unopened_alternation", "invalid_range", "other"] as const;

/** The `cause` of an `invalid_file_mask`: why a glob of a content search's file mask does not compile. */
export type InvalidFileMaskCause = (typeof INVALID_FILE_MASK_CAUSES)[number];

/** `raw` as an `invalid_file_mask` cause: one this client does not know, or none, is `other`. */
export function invalidFileMaskCause(raw: string | undefined): InvalidFileMaskCause {
  return INVALID_FILE_MASK_CAUSES.find((cause) => cause === raw) ?? "other";
}
