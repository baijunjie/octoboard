import { format } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Language } from "../i18n/languages";
import type { AccountSharing, MessageParams, SharingEntryCause, SharingLeft } from "../protocol";
import { CODE, codeRecord } from "./codes";

/** What a sharing change can leave an entry as, every value of `SharingLeft`. */
const SHARING_LEFT = Object.keys({ unchanged: 0, backed_up: 0, partly_moved: 0, linked: 0, absent: 0 } satisfies Record<SharingLeft, 0>) as SharingLeft[];

/** One entry `account_sharing_failed` or `account_sharing_repair_failed` could not bring in line, an entry of the
 * `account_sharing` event's `failure` with its cause read as an entry cause and its sessions always listed. `sessions`
 * is set for a conversation turning sharing off could not take back, and names the Octoboard sessions that record it.
 * The list travels as one JSON param. */
export type SharingFailureItem = Omit<NonNullable<AccountSharing["failure"]>["items"][number], "cause" | "sessions"> & {
  sessions: string[];
  cause: SharingEntryCause;
};

/** The causes in the order a summary lists them; `other` also stands for a cause this client does not know. */
const SHARING_CAUSES: readonly SharingEntryCause[] = ["permission_denied", "busy", "not_found", "no_space", "conflict", "other"];

/** The entries a sharing failure names, or nothing when the param is absent, empty or not the
 * shape above. An operation that could not start at all sends an empty list: `account_sharing_failed`
 * says why in `detail`, and `account_sharing_repair_failed` says why in the notice's own message. */
export function sharingFailureItems(code: string, params: MessageParams): SharingFailureItem[] | undefined {
  if (!codeRecord(code)?.sharingItems || !params.items) return undefined;
  try {
    return parseSharingItems(JSON.parse(params.items));
  } catch {
    return undefined;
  }
}

/** The entries of a sharing failure from their wire form (the `items` param once parsed, or `failure.items` of the
 * `account_sharing` event), or nothing when the list is empty or any entry is not the shape above. */
export function parseSharingItems(items: unknown): SharingFailureItem[] | undefined {
  if (!Array.isArray(items) || items.length === 0) return undefined;
  const parsed: SharingFailureItem[] = [];
  for (const item of items) {
    if (!item || typeof item !== "object") return undefined;
    const entry = item as Partial<SharingFailureItem>;
    const sessions = entry.sessions ?? [];
    if (typeof entry.item !== "string" || entry.item.length === 0) return undefined;
    if (typeof entry.left !== "string" || !(SHARING_LEFT as readonly string[]).includes(entry.left)) return undefined;
    if (typeof entry.detail !== "string") return undefined;
    if (!Array.isArray(sessions) || sessions.some((id) => typeof id !== "string" || id.length === 0)) return undefined;
    const cause = SHARING_CAUSES.includes(entry.cause as SharingEntryCause) ? entry.cause as SharingEntryCause : "other";
    parsed.push({ item: entry.item, left: entry.left, detail: entry.detail, sessions, cause });
  }
  return parsed;
}

/** The entries grouped by cause, in the order the causes first appear. */
export function sharingCauseGroups(items: SharingFailureItem[]): { cause: SharingEntryCause; items: SharingFailureItem[] }[] {
  const groups: { cause: SharingEntryCause; items: SharingFailureItem[] }[] = [];
  for (const item of items) {
    const group = groups.find((candidate) => candidate.cause === item.cause);
    if (group) group.items.push(item);
    else groups.push({ cause: item.cause, items: [item] });
  }
  return groups;
}

/** The catalog key of the line that says why a change could not start or failed as a whole, when no entry is named;
 * nothing for `other` or an unknown cause, which says nothing a reader could act on. */
export function sharingReasonKey(cause: string | undefined) {
  switch (cause) {
    case "permission_denied": return "daemon.sharing.reason.permission_denied";
    case "busy": return "daemon.sharing.reason.busy";
    case "not_found": return "daemon.sharing.reason.not_found";
    case "no_space": return "daemon.sharing.reason.no_space";
    case "conflict": return "daemon.sharing.reason.conflict";
    case "default_moved": return "daemon.sharing.reason.default_moved";
    default: return undefined;
  }
}

/** For a sharing failure (`account_sharing_failed`, `account_sharing_repair_failed`), the raw text that says why the
 * change failed as a whole, which a failure with entries may leave empty: the daemon's `detail`, or for a repair the
 * notice's own message. Nothing at all for any other failure. */
export function sharingRawReason(error: { code: string; params: MessageParams; message: string }): { raw: string | undefined } | undefined {
  if (!codeRecord(error.code)?.sharingItems) return undefined;
  return { raw: error.code === CODE.account_sharing_failed ? error.params.detail : error.message };
}

/** Whether a change that failed as a whole has a reason to put in front of the user: the cause's own line, or the raw
 * reason behind Details that the guidance points at. */
export function hasSharingReason(cause: string | undefined, detail: string | undefined): boolean {
  return sharingReasonKey(cause) !== undefined || !!detail;
}

/** The catalog key of the line that says what to do about a cause before turning the switch again, or nothing when
 * turning it again is all there is to do (`not_found`) or the warning says what to do itself (`default_moved`).
 * `rawBelow` is for a place that shows the raw reason right after the line rather than behind Details. */
export function sharingFixKey(cause: string | undefined, rawBelow = false) {
  switch (cause) {
    case "permission_denied": return "daemon.sharing.fix.permission_denied";
    case "busy": return "daemon.sharing.fix.busy";
    case "no_space": return "daemon.sharing.fix.no_space";
    case "conflict": return "daemon.sharing.fix.conflict";
    case "other": return rawBelow ? "daemon.sharing.fix.otherBelow" : "daemon.sharing.fix.other";
    default: return undefined;
  }
}

/** The catalog key for what an entry was left as. A conversation uses its own wording for the two states whose
 * meaning changes once the entry is a conversation rather than a directory entry. */
export function sharingStateKey(item: SharingFailureItem):
  | "daemon.sharing.state.unchanged" | "daemon.sharing.state.unchangedConversation"
  | "daemon.sharing.state.backedUp" | "daemon.sharing.state.partlyMoved"
  | "daemon.sharing.state.partlyMovedConversation" | "daemon.sharing.state.linked"
  | "daemon.sharing.state.absent" {
  if (item.sessions.length > 0 && item.left === "unchanged") return "daemon.sharing.state.unchangedConversation";
  if (item.sessions.length > 0 && item.left === "partly_moved") return "daemon.sharing.state.partlyMovedConversation";
  switch (item.left) {
    case "unchanged": return "daemon.sharing.state.unchanged";
    case "backed_up": return "daemon.sharing.state.backedUp";
    case "partly_moved": return "daemon.sharing.state.partlyMoved";
    case "linked": return "daemon.sharing.state.linked";
    case "absent": return "daemon.sharing.state.absent";
  }
}

/** The catalog key of the one line that says why a group of entries was left. */
export function sharingCauseKey(cause: SharingEntryCause) {
  return `daemon.sharing.cause.${cause}` as const;
}

/** An entry as the summary names it: a conversation by the name of the first session that records it (`title` finds
 * a session's name), anything else by its own name. */
export function sharingItemLabel(item: SharingFailureItem, title: (session: string) => string | undefined): { text: string; session: boolean } {
  const named = item.sessions.map(title).find((name) => name !== undefined);
  return named !== undefined ? { text: named, session: true } : { text: item.item, session: false };
}

/** Why the entries were left, one short line per cause ("You do not have permission to change skills/ and 1 more."),
 * then what each was left as. `title` finds a session's name for an entry's label; `sessions` names the sessions that
 * record a conversation, as plain text. */
export function sharingItemsText(
  language: Language,
  items: SharingFailureItem[],
  title: (session: string) => string | undefined,
  sessions: (ids: string[]) => string,
): string {
  const causes = sharingCauseGroups(items).map((group) => {
    const names = group.items.map((item) => isolate(sharingItemLabel(item, title).text));
    const phrase = names.length === 1 ? names[0] : format(language, "daemon.sharing.items", { first: names[0], count: names.length - 1 });
    return format(language, sharingCauseKey(group.cause), { items: phrase, count: names.length } as never);
  });
  const states = items.map((item) => {
    const state = format(language, sharingStateKey(item), {});
    if (item.sessions.length === 0) return format(language, "daemon.sharing.item.summary", { item: isolate(item.item), state });
    return format(language, "daemon.sharing.conversation.summary", { item: isolate(item.item), sessions: sessions(item.sessions), state });
  });
  return [...causes, ...states].join("\n");
}
