import { AGENT_OPTIONS } from "../agents/agents";
import { format, isMessageKey, messageText, globalParam, type MessageKey } from "../i18n/catalog";
import { joinPhrases } from "../i18n/joinPhrases";
import { isolate } from "../i18n/isolate";
import { abbreviateHome } from "../format/pathDisplay";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { countdown, quotaWindowPhrase, type QuotaNaming } from "../accounts/quotaFormat";
import type { Language } from "../i18n/languages";
import { codeRecord, messageFacets } from "./codes";
import { DEFAULT_ACCOUNT_NAME, isKeptForItsFolder, isProjectFolderUnknown, isProtectedFolder, pathLinkRefusedCause, type Console, type MessageParams, type Project, type QuotaWindow, type Session, type SessionBranchCause, type SessionBranchState, type Settings, type SharingEntryCause, type ShellTerminal } from "../protocol";

/** The params that are record ids, and what each is shown as: the record's current name. */
export const RECORD_NAMES: Record<string, (state: RecordNames, id: string) => string | undefined> = {
  console: (state, id) => state.consoles.get(id)?.name,
  project: (state, id) => state.projects.get(id)?.name,
  session: (state, id) => state.sessions.get(id)?.title,
  receiving_session: (state, id) => state.sessions.get(id)?.title,
  shell_terminal: (state, id) => state.shellTerminals?.get(id)?.title,
  account: (state, id) => (state.accounts ?? state.settings?.accounts)?.find((account) => account.id === id)?.name,
};

/** The records an error text names, as the store holds them: `State` satisfies it. */
export interface RecordNames {
  consoles: Map<string, Console>;
  projects: Map<string, Project>;
  sessions: Map<string, Session>;
  homeDir?: string | null;
  shellTerminals?: Map<string, ShellTerminal>;
  accounts?: Settings["accounts"];
  settings?: Settings;
}

/** A structured list never infers boundaries from user-provided names or punctuation. */
export function runningSessionIds(params: MessageParams): string[] | undefined {
  if (!params.session_ids) return undefined;
  try {
    const ids: unknown = JSON.parse(params.session_ids);
    return Array.isArray(ids) && ids.length > 0 && ids.every((id) => typeof id === "string" && id.length > 0) ? ids : undefined;
  } catch {
    return undefined;
  }
}

/** One bound branch or worktree a `session_branches_not_removed` alert names: `name` is its path or
 * branch name as text, `state` why it is still there, `uncommitted` whether a worktree has
 * uncommitted changes, `detail` the reason given for a `failed` one, and `cause` why a `failed` one is left where the
 * UI words that itself (`detail` is then the daemon's English, not shown). */
export type LeftBranchItem = { kind: "worktree" | "branch"; name: string; branch: string | null; state: SessionBranchState; uncommitted?: boolean; detail: string | null; cause?: SessionBranchCause | null };

/** The groups a `session_branches_not_removed` alert sorts its items into, in the order shown, each
 * with the lead line that says once why its items are still there, and for some a note after the list on what to do
 * with them; what could not be removed at all comes first. A worktree kept because it is the home folder, the root of a
 * disk or a folder holding Octoboard's data, or because whether it is or holds another project's folder could not be
 * told, is not among what could not be removed, but in a group of its own beside the project folders. */
type LeftBranchLead =
  | "branchCleanup.group.notRemoved" | "branchCleanup.group.uncommitted" | "branchCleanup.group.submodules"
  | "branchCleanup.group.locked" | "branchCleanup.group.unmerged" | "branchCleanup.group.checkedOut"
  | "branchCleanup.group.projectFolder" | "branchCleanup.group.protectedFolder" | "branchCleanup.group.projectFolderUnknown";
type LeftBranchNote = "branchCleanup.note.projectFolderUnknown";
const LEFT_BRANCH_GROUPS: Array<{ key: string; lead: LeftBranchLead; note?: LeftBranchNote; states: SessionBranchState[]; holds: (item: LeftBranchItem) => boolean }> = [
  { key: "notRemoved", lead: "branchCleanup.group.notRemoved", states: ["failed", "removable"], holds: (item) => !isKeptForItsFolder(item) },
  { key: "uncommitted", lead: "branchCleanup.group.uncommitted", states: ["uncommitted_changes"], holds: () => true },
  { key: "submodules", lead: "branchCleanup.group.submodules", states: ["submodules"], holds: () => true },
  { key: "locked", lead: "branchCleanup.group.locked", states: ["locked"], holds: () => true },
  { key: "unmerged", lead: "branchCleanup.group.unmerged", states: ["unmerged"], holds: () => true },
  { key: "checkedOut", lead: "branchCleanup.group.checkedOut", states: ["checked_out"], holds: () => true },
  { key: "projectFolder", lead: "branchCleanup.group.projectFolder", states: ["project_folder"], holds: () => true },
  { key: "protectedFolder", lead: "branchCleanup.group.protectedFolder", states: ["failed"], holds: isProtectedFolder },
  {
    key: "projectFolderUnknown", lead: "branchCleanup.group.projectFolderUnknown", note: "branchCleanup.note.projectFolderUnknown",
    states: ["failed"], holds: isProjectFolderUnknown,
  },
];
const LEFT_BRANCH_STATES = new Set(LEFT_BRANCH_GROUPS.flatMap((group) => group.states));

/** `items` sorted into their groups, leaving out the empty ones. */
export function leftBranchGroups(items: LeftBranchItem[]): Array<{ key: string; lead: LeftBranchLead; note?: LeftBranchNote; items: LeftBranchItem[] }> {
  return LEFT_BRANCH_GROUPS
    .map(({ key, lead, note, states, holds }) => ({ key, lead, note, items: items.filter((item) => states.includes(item.state) && holds(item)) }))
    .filter((group) => group.items.length > 0);
}

/** The reason given for a `failed` item of a `session_branches_not_removed` alert, to show under its name: none for
 * one whose group says why in the UI's own words. */
export function leftBranchDetail(item: LeftBranchItem): string | null {
  return isKeptForItsFolder(item) ? null : item.detail;
}

/** How an item of a `session_branches_not_removed` alert is named to a person: a worktree's path
 * with the daemon host's home written as `~`, a branch's name as it is. */
export function leftBranchName(item: LeftBranchItem, homeDir: string | null | undefined): string {
  return item.kind === "worktree" ? abbreviateHome(item.name, homeDir) : item.name;
}

/** An item of a `session_branches_not_removed` alert in plain text: its name, the branch a worktree
 * has checked out, and a locked worktree's uncommitted changes or the reason a removal failed. */
function leftBranchText(language: Language, item: LeftBranchItem, homeDir: string | null | undefined): string {
  const path = isolate(leftBranchName(item, homeDir));
  let name = item.kind === "branch"
    ? path
    : item.branch === null
      ? format(language, "branchCleanup.worktreeDetached", { path })
      : format(language, "branchCleanup.worktree", { path, branch: isolate(item.branch) });
  if (item.state === "locked" && item.uncommitted) name = format(language, "branchCleanup.withTag", { name, tag: format(language, "branchCleanup.uncommittedTag", {}) });
  const detail = leftBranchDetail(item);
  return detail ? format(language, "branchCleanup.failed", { name, detail: isolate(detail) }) : name;
}

/** What a sharing change left one entry as. The wire values are the daemon's. */
const SHARING_LEFT = ["unchanged", "backed_up", "partly_moved", "linked", "absent"] as const;
type SharingLeft = (typeof SHARING_LEFT)[number];

/** One entry `account_sharing_failed` or `account_sharing_repair_failed` could not bring in line.
 * `sessions` is set for a conversation turning sharing off could not take back, and names the
 * Octoboard sessions that record it. The list travels as one JSON param. */
export type SharingFailureItem = { item: string; left: SharingLeft; detail: string; sessions: string[]; cause: SharingEntryCause };

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
    parsed.push({ item: entry.item, left: entry.left as SharingLeft, detail: entry.detail, sessions, cause });
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
  return { raw: error.code === "account_sharing_failed" ? error.params.detail : error.message };
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

/** The items of a `session_branches_not_removed` alert, which travel as one JSON param so a name
 * holding a comma, or any other character, is never split. */
export function leftBranchItems(code: string, params: MessageParams): LeftBranchItem[] | undefined {
  if (code !== "session_branches_not_removed" || !params.items) return undefined;
  try {
    const items: unknown = JSON.parse(params.items);
    if (!Array.isArray(items) || items.length === 0) return undefined;
    const valid = items.every((item: Partial<LeftBranchItem> | null) =>
      !!item && typeof item.name === "string" && typeof item.state === "string" && LEFT_BRANCH_STATES.has(item.state));
    return valid ? items as LeftBranchItem[] : undefined;
  } catch {
    return undefined;
  }
}

/** These trust reasons carry an OS write/read diagnostic, not another daemon error code. */
export function hasTrustDiagnostic(code: string, params: MessageParams): boolean {
  return (code === "trust_answer_failed" || code === "trust_not_carried_over") && ["terminal_write_failed", "store_io_failed"].includes(params.reason_code) && !!params.detail;
}

/** The catalog key of a failure's further explanation in the application's own words, which leads its Details: the
 * `.details` of the message it is worded under, else of its code, so a variant can replace its code's explanation. */
export function furtherKey(code: string, params: MessageParams): MessageKey | undefined {
  return [`daemon.${messageCode(code, params)}.details`, `daemon.${code}.details`].find(isMessageKey);
}

/** The rich sentence omits a raw diagnostic that is rendered separately. Plain messages keep the
 * complete catalog sentence for native notifications and other text-only consumers. */
export function richMessageCode(code: string, params: MessageParams): string {
  const record = codeRecord(code);
  return record?.recoveryLine || (record?.detailBlock && record.summaryForDetail !== false && params.detail) ? `${messageCode(code, params)}.summary` : messageCode(code, params);
}

/** Whether the failure is worded with no opening sentence of its own (`noSummary` in `codes.ts`), whatever its
 * `detail`: with none the plain sentence would end on a dangling colon and still say the title again. */
export function lacksSummary(code: string): boolean {
  return codeRecord(code)?.noSummary === true;
}

/** Whether the catalog words the failure's rich sentence: its `.summary`, or the plain message where the code has no
 * diagnostic to leave out; a code without an opening sentence has none to look up. */
export function hasRichMessage(code: string, params: MessageParams): boolean {
  return lacksSummary(code) || isMessageKey(`daemon.${richMessageCode(code, params)}`);
}

/** Every referenced agent and product uses the same name treatment, including messages whose only
 * parameter is a global product name. */
export function referenceNameParams(code: string, params: MessageParams): string[] {
  const key = `daemon.${messageCode(code, params)}`;
  if (!isMessageKey(key)) return [];
  const richKey = `daemon.${richMessageCode(code, params)}`;
  const text = messageText("en", key, params) + (isMessageKey(richKey) ? messageText("en", richKey, params) : "");
  return ["agent", "appName", "codex", "claude", "grok", "grokBuild", "git", "macOS"].filter((name) => text.includes(`{${name}}`));
}

/** The code a message is worded under: the daemon's, except where one code has a message of its own
 * for a variant its params name (`unsupported_file_type` of an `unmerged` file, worded as
 * `daemon.unsupported_file_type.unmerged`, like `daemon.trust_reason.<code>`; `account_quota_exhausted`
 * with a `resets_at` time, worded as `daemon.account_quota_exhausted.resets`; `account_in_use` while the
 * account is being signed in, worded as `daemon.account_in_use.signingIn`, or `.both` with sessions too; a refused
 * avatar of an imported console, which names it, worded as `daemon.icon_too_large.import`; `path_link_refused` by its
 * `cause`, worded as `daemon.path_link_refused.<cause>`, an unknown one as `other`; `project_directory_trash_failed` and
 * `account_directory_trash_failed` with `cause` `unanswered`, where a folder it depends on did not respond in time, or
 * `unreadable`, where the folder itself could not be read (for an account also `other_unlooked`, where whether the
 * directory holds or overlaps another account's or a project's could not be checked, and `signing_in`, where the
 * account's sign-in did not end in time), worded as `daemon.<code>.<cause>`; `account_directory_shared` by its `kind`
 * (`account`, `default_account` or `project`), worded as `daemon.account_directory_shared.<kind>`; `outside_scope` of a
 * refused link click, with `action` `open`, worded as `daemon.outside_scope.open`; `terminal_input_lost` of a
 * session's terminal or of a shell terminal, naming it by its `session` or `shell_terminal` param, worded as
 * `daemon.terminal_input_lost.session` or `.shell`; `queued_messages_lost` of a session that holds later writes behind the
 * fragment, with `fragment` `held`, worded as `daemon.queued_messages_lost.held`). */
export function messageCode(code: string, params: MessageParams): string {
  if (code === "queued_messages_lost" && params.fragment === "held") return "queued_messages_lost.held";
  if (code === "message_not_sent" && params.submit_retry === "once") return "message_not_sent.retry";
  if (code === "terminal_input_lost" && params.session) return "terminal_input_lost.session";
  if (code === "terminal_input_lost" && params.shell_terminal) return "terminal_input_lost.shell";
  if (code === "field_too_long" && (params.field === "name" || params.field === "description")) return `${code}.${params.field}`;
  if (code === "account_quota_exhausted" && /^\d+$/.test(params.resets_at ?? "")) return "account_quota_exhausted.resets";
  if (code === "sharing_resume_failed" && params.still_running === "true") return "sharing_resume_failed.running";
  if (code === "relocation_failed" && (params.stage === "thread_record" || params.stage === "kept_staging")) return `${code}.${params.stage}`;
  if (code === "outside_scope" && params.action === "open") return "outside_scope.open";
  if (code === "path_link_refused") return `${code}.${pathLinkRefusedCause(params.cause)}`;
  if (code === "account_directory_shared" && (params.kind === "account" || params.kind === "default_account" || params.kind === "project")) return `${code}.${params.kind}`;
  if ((code === "project_directory_trash_failed" || code === "account_directory_trash_failed") && (params.cause === "unanswered" || params.cause === "unreadable" || (code === "account_directory_trash_failed" && (params.cause === "other_unlooked" || params.cause === "signing_in")))) return `${code}.${params.cause}`;
  if ((code === "icon_not_an_image" || code === "icon_too_large") && params.console_name) return `${code}.import`;
  if (code === "account_in_use" && params.signing_in === "true") return /^[1-9]\d*$/.test(params.sessions ?? "") ? "account_in_use.both" : "account_in_use.signingIn";
  return code === "unsupported_file_type" && params.file_type === "unmerged" ? "unsupported_file_type.unmerged" : code;
}

/** A message's params as the UI shapes them: the daemon's over the UI's own (`uiParams` in `codes.ts`). */
export function withUiParams(code: string, params: MessageParams): MessageParams {
  return { ...codeRecord(messageCode(code, params))?.uiParams, ...params };
}

/** The kinds a refusal can name: a model's own weekly window never refuses. */
const REFUSED_WINDOW_KINDS: readonly QuotaWindow["kind"][] = ["session", "weekly", "credit_period"];

/** The window an `account_quota_exhausted` refusal names, with what names it (`QuotaNaming`): its `window_kind`,
 * `window_minutes` and `model_weekly` params, and the `agent` label. Undefined for a refusal without them. */
function refusedQuotaWindow(params: MessageParams): { naming: QuotaNaming; window: QuotaWindow } | undefined {
  const kind = REFUSED_WINDOW_KINDS.find((candidate) => candidate === params.window_kind);
  const agent = AGENT_OPTIONS.find((option) => option.label === params.agent)?.value;
  if (!kind || !agent) return undefined;
  const minutes = /^\d+$/.test(params.window_minutes ?? "") ? Number(params.window_minutes) : null;
  const window: QuotaWindow = { kind, label: null, used_percent: 100, resets_at: null, window_minutes: minutes };
  const modelWeekly: QuotaWindow = { kind: "weekly_model", label: null, used_percent: 0, resets_at: null, window_minutes: null };
  return { naming: { agent, windows: params.model_weekly === "true" ? [window, modelWeekly] : [window] }, window };
}

/** Whether the refusal's window can be worded from its structured params; if not, from its English name. */
export function hasRefusedQuotaWindow(params: MessageParams): boolean {
  return refusedQuotaWindow(params) !== undefined;
}

/** An account's name as the application shows it: the default account, which the daemon names by its fixed English
 * `DEFAULT_ACCOUNT_NAME`, by its name in `language` (as the accounts section does), any other account by its own. */
export function accountNameText(language: Language, name: string): string {
  return name === DEFAULT_ACCOUNT_NAME ? format(language, "settings.accounts.defaultName", {}) : name;
}

/** The name of the record a param `name` holds the id of, or `undefined` once the record is gone or has no name to
 * show: the caller then shows the id, never an empty name. An `owner` is a session, named as the session list
 * shows it. */
function recordName(records: RecordNames, name: string, id: string, language: Language): string | undefined {
  if (name !== "owner") return RECORD_NAMES[name]?.(records, id);
  const session = records.sessions.get(id);
  const title = session && sessionDisplayTitle((key, ...args) => format(language, key, args[0]), session);
  return title?.trim() ? title : undefined;
}

/** A param as plain text: a record id as that record's name, anything else as it came, isolated. */
function paramText(records: RecordNames, name: string, value: string, code: string, language: Language): string {
  if (name === "session" && (code === "conversation_held_in_project" || code === "conversation_held_in_console")) {
    const session = records.sessions.get(value);
    if (session) return isolate(sessionDisplayTitle((key, ...args) => format(language, key, args[0]), session));
  }
  return isolate(codeRecord(code)?.recordIds?.includes(name) ? recordName(records, name, value, language) ?? value : value);
}

/** The values a message `code` is filled in with: the daemon's `params` over the UI's own
 * (`uiParams`), each shown as plain text (see `paramText`). A numeric `count` param is a number,
 * which selects the message's plural form. A `reason_code` param is worded from the catalog's
 * `daemon.trust_reason.<code>` and replaces `reason`, which stays the daemon's English account for a
 * reason code the catalog lacks. A `launch_failed` or `account_switch_automatically_failed` notice's
 * `reason_code` is the code of the error it failed with, so its `reason` is worded from that code's own
 * `daemon.<code>` message, with the failure's params that arrive beside it and that code's `uiParams`. With
 * `mark`, a param the message sets apart (`codeParams` or `nameParams`) is what `mark` makes of its displayed
 * value instead. */
export function daemonParams<V = never>(
  language: Language,
  code: string,
  params: MessageParams,
  records: RecordNames,
  mark?: (value: string, monospace: boolean) => V,
): Record<string, string | number | V> {
  // A failed launch or move carries its failure's params, which take the UI's own for the failure's code.
  const nested = codeRecord(code)?.nested !== undefined;
  const all = nested && params.reason_code ? withUiParams(params.reason_code, params) : withUiParams(code, params);
  // The default account is shown by its name in the current language, as the accounts section does.
  for (const accountParam of codeRecord(code)?.defaultAccountParams ?? []) {
    if (accountParam in all) all[accountParam] = accountNameText(language, all[accountParam]);
  }
  const shown: Record<string, string | number> = Object.fromEntries(
    Object.entries(all).map(([name, value]) => [name, paramText(records, name, value, code, language)]),
  );
  const running = code === "session_has_running_sessions" ? runningSessionIds(all) : undefined;
  if (running) shown.sessions = joinPhrases(language, running.map((id) => paramText(records, "session", id, "unknown_session", language)));
  const left = leftBranchItems(code, all);
  if (left) {
    // Each group's lead says why once. A name or a detail may hold commas of its own, so the
    // catalog sets the items of a group apart, and the groups, with marks of its own.
    const joined = (key: "branchCleanup.plain.items" | "branchCleanup.plain.groups", parts: string[]) =>
      parts.reduce((before, after) => format(language, key, { before, after }));
    const groups = leftBranchGroups(left).map((group) => format(language, "branchCleanup.plain.group", {
      lead: format(language, group.lead, { count: group.items.length }),
      items: joined("branchCleanup.plain.items", group.items.map((item) => leftBranchText(language, item, records.homeDir))),
    }));
    // A group's note is a sentence of its own, after the list.
    const notes = leftBranchGroups(left).flatMap((group) => group.note ? [format(language, group.note, { count: group.items.length })] : []);
    shown.items = [format(language, "branchCleanup.plain.end", { groups: joined("branchCleanup.plain.groups", groups) }), ...notes]
      .join(format(language, "common.sentenceSeparator", {}));
  }
  // A count arrives as text; as a number it selects the singular or plural wording.
  if (/^\d+$/.test(all.count ?? "")) shown.count = Number(all.count);
  if ((code === "account_in_use" || code === "account_has_running_sessions") && /^\d+$/.test(all.sessions ?? "")) shown.count = Number(all.sessions);
  if (code === "field_too_long" && /^\d+$/.test(all.limit ?? "")) shown.limit = Number(all.limit);
  // The window is worded in the UI's language from its structured params; a refusal without them, as a quota of
  // its English `window` name.
  if (code === "account_quota_exhausted") {
    const refused = refusedQuotaWindow(all);
    shown.quota = refused
      ? quotaWindowPhrase((key, ...args) => format(language, key, args[0]), refused.naming, refused.window)
      : format(language, "quota.windowPhrase.named", { window: shown.window });
  }
  // The time left until the account's quota resets, counted from when the message is worded.
  if (messageCode(code, all) === "account_quota_exhausted.resets") {
    shown.time = countdown((key, ...args) => format(language, key, args[0]), Number(all.resets_at), Date.now());
  }
  const reasonKey = `daemon.trust_reason.${all.reason_code}`;
  if (isMessageKey(reasonKey)) shown.reason = format(language, reasonKey, shown);
  const failureKey = `daemon.${messageCode(all.reason_code ?? "", all)}`;
  if (nested && all.reason_code !== code && isMessageKey(failureKey)) shown.reason = daemonMessage(language, all.reason_code, all, String(shown.reason ?? shown.detail ?? ""), records);
  if (!mark) return shown;
  const marked: Record<string, string | number | V> = { ...shown };
  const facets = messageFacets(messageCode(code, all));
  for (const name of facets?.codeParams ?? []) if (name in all) marked[name] = mark(all[name], true);
  for (const name of facets?.nameParams ?? []) {
    if (!(name in all)) continue;
    const recordId = codeRecord(code)?.recordIds?.includes(name) ?? false;
    const shownName = recordId ? recordName(records, name, all[name], language) : undefined;
    marked[name] = mark(shownName ?? all[name], recordId && shownName === undefined);
  }
  for (const name of referenceNameParams(code, all)) {
    const value = all[name] ?? globalParam(name);
    if (value !== undefined) marked[name] = mark(value, false);
  }
  return marked;
}

/** What the daemon's `error` or `session_notice` says, in `language`: the catalog's text
 * for its `code` with its params filled in, a record id shown as that record's name (the raw id only
 * when the record is unknown). A code the catalog does not have — a daemon newer than this client —
 * shows the daemon's own English `fallback`. The params are shaped by `daemonParams`. With `summary`
 * a code whose rich sentence leaves out a raw diagnostic (`richMessageCode`) is worded as that sentence
 * alone, for a plain-text line that has no Details behind it for the raw text; a code with no summary
 * (`noSummary`) then says nothing, its plain message being the title again with the raw text. */
export function daemonMessage(
  language: Language,
  code: string,
  params: MessageParams,
  fallback: string,
  records: RecordNames,
  summary = false,
): string {
  if (summary && lacksSummary(code)) return "";
  const summaryKey = `daemon.${richMessageCode(code, params)}`;
  const key = summary && isMessageKey(summaryKey) ? summaryKey : `daemon.${messageCode(code, params)}`;
  if (!isMessageKey(key)) return fallback;
  if (code === "shell_environment_timeout" && !["shell", "command", "timeout"].every((name) => !!params[name])) return fallback;
  const shown = daemonParams(language, code, params, records);
  // A single group of leftovers is said by its own lead, without the line over the groups.
  const left = leftBranchItems(code, params);
  if (left && leftBranchGroups(left).length === 1) return String(shown.items);
  const text = format(language, key, shown);
  // Each entry the change could not finish follows on a line of its own, in this language. The
  // daemon's English summary of the same list is not repeated under it.
  const sharing = sharingFailureItems(code, params);
  if (sharing) return `${text}\n${sharingItemsText(language, sharing, records)}`;
  const reason = codeRecord(code)?.sharingItems ? sharingReasonKey(params.cause) : undefined;
  if (reason) return `${text}\n${format(language, reason, {})}${code === "account_sharing_failed" && params.detail ? `\n${params.detail}` : ""}`;
  // A repair that names no entry keeps its reason only in the daemon's English message.
  if (code === "account_sharing_repair_failed" && fallback) return `${text}\n${fallback}`;
  // As plain text the diagnostic follows on a line of its own.
  return (code === "project_branches_not_determined" || code === "account_sharing_failed" || code === "path_link_refused") && params.detail ? `${text}\n${params.detail}` : text;
}

/** Why the entries were left, one short line per cause ("You do not have permission to change skills/ and 1 more."),
 * then what each was left as. */
function sharingItemsText(language: Language, items: SharingFailureItem[], records: RecordNames): string {
  const title = (id: string) => records.sessions.get(id)?.title;
  const causes = sharingCauseGroups(items).map((group) => {
    const names = group.items.map((item) => isolate(sharingItemLabel(item, title).text));
    const phrase = names.length === 1 ? names[0] : format(language, "daemon.sharing.items", { first: names[0], count: names.length - 1 });
    return format(language, sharingCauseKey(group.cause), { items: phrase, count: names.length } as never);
  });
  const states = items.map((item) => {
    const state = format(language, sharingStateKey(item), {});
    if (item.sessions.length === 0) return format(language, "daemon.sharing.item.summary", { item: isolate(item.item), state });
    const sessions = joinPhrases(language, item.sessions.map((id) => paramText(records, "session", id, "unknown_session", language)));
    return format(language, "daemon.sharing.conversation.summary", { item: isolate(item.item), sessions, state });
  });
  return [...causes, ...states].join("\n");
}
