import { format, isMessageKey, messageText, globalParam, PROVIDER_PARAM_NAMES, translatorFor, type MessageKey } from "../i18n/catalog";
import { joinPhrases } from "../i18n/joinPhrases";
import { isolate } from "../i18n/isolate";
import { daemonOs, daemonOsName } from "../i18n/platformTerms";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { countdown, quotaWindowPhrase, type QuotaNaming } from "../accounts/quotaFormat";
import type { Language } from "../i18n/languages";
import { hostPathsOf } from "../format/hostPath";
import { droppedFileEntries, droppedFilesText } from "./droppedFileEntries";
import { leftBranchesText, leftBranchGroups, leftBranchItems } from "./branchCleanupItems";
import { providerLabel } from "../agents/providers";
import { CODE, codeRecord, messageFacets, messageKey } from "./codes";
import { sharingFailureCause, sharingFailureItems, sharingItemsText, sharingReasonKey } from "./sharingFailure";
import { type Console, type Project, type ProviderInfo, type QuotaWindow, type Session, type Settings, type ShellTerminal } from "../protocol";
import { nestedFailureParams, type MessageParams } from "./codedError";
import { DEFAULT_ACCOUNT_NAME } from "../accounts/defaultAccount";

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
  host?: { path_style: string } | null;
  shellTerminals?: Map<string, ShellTerminal>;
  accounts?: Settings["accounts"];
  settings?: Settings;
  /** The providers the daemon serves, which a quota refusal names its window by. */
  providers?: readonly ProviderInfo[];
}

/** No records to name anything by, for a message about none of them (a quota reading, a login) or worded where the
 * store is out of reach: a record id is then shown as it came. Each of its maps also stands for one a reader of the store
 * does not subscribe to. */
export const NO_RECORDS: Required<Pick<RecordNames, "consoles" | "projects" | "sessions" | "shellTerminals">> = { consoles: new Map(), projects: new Map(), sessions: new Map(), shellTerminals: new Map() };

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

/** These trust reasons carry an OS write/read diagnostic, not another daemon error code. */
export function hasTrustDiagnostic(code: string, params: MessageParams): boolean {
  return (code === CODE.trust_answer_failed || code === CODE.trust_not_carried_over) && ["terminal_write_failed", "store_io_failed"].includes(params.reason_code) && !!params.detail;
}

/** The catalog key of a failure's further explanation in the application's own words, which leads its Details: the
 * `.details` of the message it is worded under, else of its code, so a variant can replace its code's explanation. */
export function furtherKey(code: string, params: MessageParams): MessageKey | undefined {
  return [`${messageKey(messageCode(code, params))}.details`, `${messageKey(code)}.details`].find(isMessageKey);
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
  return lacksSummary(code) || isMessageKey(messageKey(richMessageCode(code, params)));
}

/** The product names a message names (an agent's label, a provider's product name, Git, the application, the
 * daemon's operating system), each one plain text with no chip, including messages whose only parameter is a global
 * product name. */
export function referenceNameParams(code: string, params: MessageParams): string[] {
  const key = messageKey(messageCode(code, params));
  if (!isMessageKey(key)) return [];
  const richKey = messageKey(richMessageCode(code, params));
  const text = messageText("en", key, params) + (isMessageKey(richKey) ? messageText("en", richKey, params) : "");
  return ["agent", "appName", ...PROVIDER_PARAM_NAMES, "git", "osName"].filter((name) => text.includes(`{${name}}`));
}

/** The catalog suffix for a daemon code and its variant, selected by its params and by the operating system of the
 * daemon's host. Corrected meanings use a new suffix across plain messages, summaries and remedies, so delivered
 * translations keep their original keys. */
export function messageCode(code: string, params: MessageParams): string {
  const variant = codeRecord(code)?.variantOf?.(params, daemonOs());
  const message = variant === undefined ? code : `${code}.${variant}`;
  if (code === CODE.project_directory_shared || (code === CODE.project_directory_trash_failed && (variant === "unanswered" || variant === "unreadable"))) return `${message}.retryTrash`;
  // A launch no longer waits on the snapshot (it falls back), so what a timeout refuses is the request that needed it.
  if (code === CODE.shell_environment_timeout && ["shell", "command", "timeout"].every((name) => !!params[name])) return `${code}.refused`;
  return message;
}

/** A message's params as the UI shapes them: the daemon's over the UI's own (`uiParams` in `codes.ts`), and the name of
 * the operating system of the daemon's host, where what the message tells of happened, which a message names as a
 * reference. A variant of a code takes the UI params of the code itself. */
export function withUiParams(code: string, params: MessageParams): MessageParams {
  return { osName: daemonOsName(), ...codeRecord(code)?.uiParams, ...params };
}

/** The names of the providers the daemon serves, in their order, for a message that lists the supported agents. */
export function servedAgentLabels(language: Language, providers: readonly ProviderInfo[]): string[] {
  const t = translatorFor(language);
  return providers.map(({ id }) => providerLabel(t, providers, id));
}

/** The kinds a refusal can name: a model's own weekly window never refuses. */
const REFUSED_WINDOW_KINDS: readonly QuotaWindow["kind"][] = ["session", "weekly", "credit_period"];

/** The window an `account_quota_exhausted` refusal names, with what names it (`QuotaNaming`): its `window_kind`,
 * `window_minutes` and `model_weekly` params, and the `agent` label, which names one of the served `providers` by its
 * label. Undefined for a refusal without them, or for a provider not served. */
function refusedQuotaWindow(params: MessageParams, providers: readonly ProviderInfo[] | undefined): { naming: QuotaNaming; window: QuotaWindow } | undefined {
  const kind = REFUSED_WINDOW_KINDS.find((candidate) => candidate === params.window_kind);
  const provider = providers?.find((candidate) => candidate.label === params.agent);
  if (!kind || !provider) return undefined;
  const minutes = /^\d+$/.test(params.window_minutes ?? "") ? Number(params.window_minutes) : null;
  const window: QuotaWindow = { kind, label: null, used_percent: 100, resets_at: null, window_minutes: minutes };
  const modelWeekly: QuotaWindow = { kind: "weekly_model", label: null, used_percent: 0, resets_at: null, window_minutes: null };
  return { naming: { windowNaming: provider.quota_windows, windows: params.model_weekly === "true" ? [window, modelWeekly] : [window] }, window };
}

/** Whether the refusal's window can be worded from its structured params; if not, from its English name. */
export function hasRefusedQuotaWindow(params: MessageParams, providers: readonly ProviderInfo[] | undefined): boolean {
  return refusedQuotaWindow(params, providers) !== undefined;
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
  const title = session && sessionDisplayTitle(translatorFor(language), session);
  return title?.trim() ? title : undefined;
}

/** A param as plain text: a record id as that record's name, anything else as it came, isolated. */
function paramText(records: RecordNames, name: string, value: string, code: string, language: Language): string {
  if (name === "session" && (code === CODE.conversation_held_in_project || code === CODE.conversation_held_in_console)) {
    const session = records.sessions.get(value);
    if (session) return isolate(sessionDisplayTitle(translatorFor(language), session));
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
 * value instead, and a product name the message names (see `referenceNameParams`) what it makes of it as `plain` text:
 * isolated as an element, with no chip. */
export function daemonParams<V = never>(
  language: Language,
  code: string,
  params: MessageParams,
  records: RecordNames,
  mark?: (value: string, monospace: boolean, plain?: boolean) => V,
): Record<string, string | number | V> {
  // A failed launch or move carries its failure's params, which take the UI's own for the failure's code, and the
  // code's own beside them for a name the failure's lack (a `binary_not_found` nesting a snapshot failure shows its
  // `pathVar`).
  const nested = codeRecord(code)?.nested !== undefined;
  const all = nested && params.reason_code ? withUiParams(code, withUiParams(params.reason_code, params)) : withUiParams(code, params);
  // The provider a refusal names arrives as its wire id, shown by its name.
  if (code === CODE.provider_unsupported && all.agent) all.agent = providerLabel(translatorFor(language), records.providers ?? [], all.agent);
  // The supported agents are the providers the daemon serves.
  if (code === CODE.agent_not_available) all.agents = joinPhrases(language, servedAgentLabels(language, records.providers ?? []).map(isolate));
  // The default account is shown by its name in the current language, as the accounts section does.
  for (const accountParam of codeRecord(code)?.defaultAccountParams ?? []) {
    if (accountParam in all) all[accountParam] = accountNameText(language, all[accountParam]);
  }
  const shown: Record<string, string | number> = Object.fromEntries(
    Object.entries(all).map(([name, value]) => [name, paramText(records, name, value, code, language)]),
  );
  const running = code === CODE.session_has_running_sessions ? runningSessionIds(all) : undefined;
  if (running) shown.sessions = joinPhrases(language, running.map((id) => paramText(records, "session", id, CODE.unknown_session, language)));
  const left = leftBranchItems(code, all);
  if (left) shown.items = leftBranchesText(language, left, records.homeDir, hostPathsOf(records).style);
  // A count arrives as text; as a number it selects the singular or plural wording.
  if (/^\d+$/.test(all.count ?? "")) shown.count = Number(all.count);
  if ((code === CODE.account_in_use || code === CODE.account_has_running_sessions) && /^\d+$/.test(all.sessions ?? "")) shown.count = Number(all.sessions);
  if (code === CODE.field_too_long && /^\d+$/.test(all.limit ?? "")) shown.limit = Number(all.limit);
  if ((code === CODE.dropped_file_too_large || code === CODE.dropped_file_images_only) && /^\d+$/.test(all.limit_mib ?? "")) shown.limit_mib = Number(all.limit_mib);
  // Where a refused regular expression fails, as the 1-based character it is shown as. The daemon's `offset` counts
  // UTF-16 units; with the `query` the refusal was worded with (`useContentSearch` adds it), characters are counted as
  // the user sees them, an emoji as one.
  if (code === CODE.invalid_search_pattern && /^\d+$/.test(all.offset ?? "")) {
    const offset = Number(all.offset);
    shown.position = (all.query === undefined ? offset : [...all.query.slice(0, offset)].length) + 1;
  }
  // The window is worded in the UI's language from its structured params; a refusal without them, as a quota of
  // its English `window` name.
  if (code === CODE.account_quota_exhausted) {
    const refused = refusedQuotaWindow(all, records.providers);
    shown.quota = refused
      ? quotaWindowPhrase(translatorFor(language), refused.naming, refused.window)
      : format(language, "quota.windowPhrase.named", { window: shown.window });
  }
  // The time left until the account's quota resets, counted from when the message is worded.
  if (messageCode(code, all) === `${CODE.account_quota_exhausted}.resets`) {
    shown.time = countdown(translatorFor(language), Number(all.resets_at), Date.now());
  }
  const reasonKey = `daemon.trust_reason.${all.reason_code}`;
  if (isMessageKey(reasonKey)) shown.reason = format(language, reasonKey, shown);
  const failure = nestedFailureParams(all.reason_code ?? "", all);
  const failureKey = messageKey(messageCode(all.reason_code ?? "", failure));
  if (nested && all.reason_code !== code && isMessageKey(failureKey)) {
    shown.reason = timedOutSnapshot(all.reason_code, failure)
      ? timedOutSnapshotReason(language, failure)
      : daemonMessage(language, all.reason_code, failure, String(shown.reason ?? shown.detail ?? ""), records);
  }
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
  // A product name the host supplies (an agent's label, Git, the application's own name) is plain text, isolated, with
  // no chip (the UI standard, "Badge, reference or plain name").
  for (const name of referenceNameParams(code, all)) {
    const value = all[name] ?? globalParam(name);
    if (value !== undefined) marked[name] = mark(value, false, true);
  }
  return marked;
}

/** Why a file of a drop was failed, in the language: the failure's own sentence where the application words it, else
 * the generic one that stands for a failure it has no words for, which `uncatalogued` words as the alert's list does
 * (it knows the cause the system's text names, which this module does not) and which defaults to the plain generic one. */
function droppedReason(language: Language, entry: { code: string; params: MessageParams; message: string }, records: RecordNames, uncatalogued?: (entry: { code: string; params: MessageParams; message: string }) => string | undefined): string {
  // The client words its own failures (a closed connection, a timeout) in the language already.
  if (codeRecord(entry.code)?.origin === "client") return entry.message;
  const generic = uncatalogued?.(entry);
  if (generic !== undefined) return generic;
  if (isMessageKey(messageKey(messageCode(entry.code, entry.params)))) return daemonMessage(language, entry.code, entry.params, entry.message, records, true);
  return format(language, entry.code === CODE.unexpected_error ? "error.unexpected.summary" : "error.systemFailure.other", {});
}

/** Whether the nested failure `code`, with its own `params`, is a timed-out login-shell snapshot with what timed out. The
 * code is the one being worded: a failure that nests a timeout in turn (a CLI not found on the fallback environment)
 * carries the timeout's code in its own params, and is worded as itself. */
function timedOutSnapshot(code: string | undefined, params: MessageParams): boolean {
  return code === CODE.shell_environment_timeout && ["shell", "command", "timeout"].every((name) => !!params[name]);
}

/** A timed-out snapshot as the reason of a failure that nests it (a CLI not found on the fallback environment a launch
 * took instead): what did not finish in time and its likely cause, without what a request refused for want of the
 * environment cannot do, which the nesting failure says differently. */
function timedOutSnapshotReason(language: Language, params: MessageParams): string {
  const command = format(language, "daemon.shell_environment_timeout.command", { command: isolate(`${params.shell} -l -i -c '${params.command}'`), timeout: isolate(params.timeout) });
  return `${command}${format(language, "common.sentenceSeparator")}${format(language, "daemon.shell_environment_timeout.cause")}`;
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
  uncatalogued?: (entry: { code: string; params: MessageParams; message: string }) => string | undefined,
): string {
  if (summary && lacksSummary(code)) return "";
  const summaryKey = messageKey(richMessageCode(code, params));
  const key = summary && isMessageKey(summaryKey) ? summaryKey : messageKey(messageCode(code, params));
  if (!isMessageKey(key)) return fallback;
  if (code === CODE.shell_environment_timeout && !["shell", "command", "timeout"].every((name) => !!params[name])) return fallback;
  const shown = daemonParams(language, code, params, records);
  // A single group of leftovers is said by its own lead, without the line over the groups.
  const left = leftBranchItems(code, params);
  if (left && leftBranchGroups(left).length === 1) return String(shown.items);
  const text = format(language, key, shown);
  // Each entry the change could not finish follows on a line of its own, in this language. The
  // daemon's English summary of the same list is not repeated under it.
  // The plain-text list is kept for parity with the other list codes (sharing failures, branches left behind), though no route
  // reaches it today: a refusal is raised through `raiseError` and shown as `DaemonErrorMessage`'s structured list.
  const dropped = droppedFileEntries(code, params);
  if (dropped) return `${text}\n${droppedFilesText(language, dropped, (entry) => droppedReason(language, entry, records, uncatalogued))}`;
  const sharing = sharingFailureItems(code, params);
  if (sharing) {
    const title = (id: string) => records.sessions.get(id)?.title;
    const sessions = (ids: string[]) => joinPhrases(language, ids.map((id) => paramText(records, "session", id, CODE.unknown_session, language)));
    return `${text}\n${sharingItemsText(language, sharing, title, sessions)}`;
  }
  const reason = codeRecord(code)?.sharingItems ? sharingReasonKey(sharingFailureCause(params)) : undefined;
  if (reason) return `${text}\n${format(language, reason, {})}${code === CODE.account_sharing_failed && params.detail ? `\n${params.detail}` : ""}`;
  // A repair that names no entry keeps its reason only in the daemon's English message.
  if (code === CODE.account_sharing_repair_failed && fallback) return `${text}\n${fallback}`;
  // As plain text the diagnostic, or the user's own text (`detailLabel`), follows on a line of its own.
  return (code === CODE.project_branches_not_determined || code === CODE.account_sharing_failed || code === CODE.path_link_refused || codeRecord(code)?.detailLabel) && params.detail ? `${text}\n${params.detail}` : text;
}
