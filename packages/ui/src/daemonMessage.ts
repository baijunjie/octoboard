import { EXAMPLE_CONVERSATION_ID } from "./conversationId";
import { format, isMessageKey, messageText, globalParam } from "./i18n/catalog";
import { joinPhrases } from "./i18n/joinPhrases";
import { isolate } from "./i18n/isolate";
import { abbreviateHome } from "./pathDisplay";
import { sessionDisplayTitle } from "./sessionLabel";
import { countdown } from "./quota/quotaFormat";
import type { Language } from "./i18n/languages";
import { DEFAULT_ACCOUNT_NAME, type MessageParams, type SessionBranchState } from "./protocol";
import type { State } from "./store";

/** The params that are record ids, and what each is shown as: the record's current name. */
const RECORD_NAMES: Record<string, (state: RecordNames, id: string) => string | undefined> = {
  console: (state, id) => state.consoles.get(id)?.name,
  project: (state, id) => state.projects.get(id)?.name,
  session: (state, id) => state.sessions.get(id)?.title,
  shell_terminal: (state, id) => state.shellTerminals?.get(id)?.title,
  account: (state, id) => (state.accounts ?? state.settings?.accounts)?.find((account) => account.id === id)?.name,
};

type RecordNames = Pick<State, "consoles" | "projects" | "sessions"> & { homeDir?: State["homeDir"]; shellTerminals?: State["shellTerminals"]; accounts?: State["settings"]["accounts"]; settings?: State["settings"] };

/** The params a message shows as code, by message code: a technical literal (an ID, a path) that
 * reads better set apart from the sentence than quoted inside it. The plain text of such a message
 * carries the value bare; `DaemonErrorMessage` sets it as `InlineChip`. A code the browser panes show
 * (`browseError.ts`) has its params reshaped first (`shownParams`), so a structured render of it
 * must be given those, never the daemon's raw ones. */
export const CODE_PARAMS: Record<string, readonly string[]> = {
  git_clone_failed: ["cloneCommand"],
  conversation_to_resume_not_found: ["path"],
  resume_id_invalid: ["example"],
  conversation_not_found: ["path"],
  path_not_absolute: ["path", "tilde"],
  path_not_found: ["path"],
  path_not_a_directory: ["path"],
  path_already_exists: ["path"],
  directory_unreadable: ["path"],
  directory_unreachable: ["path"],
  no_repositories_found: ["path"],
  invalid_path: ["path"],
  source_unavailable: ["path"],
  file_not_found: ["path"],
  permission_denied: ["path"],
  outside_scope: ["path"],
  unsupported_file_type: ["path"],
  // The UI's wording of `unsupported_file_type` with `file_type` `unmerged` (see `messageCode`); not a code
  // the daemon sends.
  "unsupported_file_type.unmerged": ["path"],
  source_changed: ["path"],
  trust_path_not_absolute: ["path"],
  relocation_failed: ["path"],
  repository_name_missing: ["url"],
  config_dir_unreachable: ["path"],
  grok_home_not_initialized: ["path"],
  trust_directory_too_broad: ["path"],
  trust_not_carried_over: ["path", "enter"],
  trust_answer_failed: ["enter"],
  clean_filtered: ["path", "filter"],
  invalid_branch_name: ["branch"],
  unknown_branch: ["branch"],
  invalid_commit: ["commit"],
  unknown_commit: ["commit"],
  no_merge_base: ["left", "right"],
  no_merge_base_shallow: ["left", "right"],
  binary_not_found: ["binary", "pathVar"],
  models_binary_not_found: ["binary", "pathVar"],
  quota_binary_not_found: ["binary", "pathVar"],
  config_dir_not_absolute: ["tilde"],
  field_too_long: ["field"],
  field_required: ["field"],
  conflicting_fields: ["first", "second"],
  configuration_version_unsupported: ["version", "supported"],
  invalid_launch_setting: ["field", "value"],
  claude_workspace_untrusted: ["allow", "deny"],
  agent_not_available: ["pathVar"],
};

/** Record lookup is code-specific: shared-folder errors already carry a project name, while
 * switch failures carry session IDs. An account quota carries a name, not an account ID. */
export const RECORD_ID_PARAMS: Record<string, readonly string[]> = {
  unknown_console: ["console"], unknown_project: ["project"], unknown_session: ["session"],
  unknown_shell_terminal: ["shell_terminal"], unknown_page: ["page"], unknown_account: ["account"],
  not_a_git_repository: ["project"], conversation_in_session: ["session"],
  conversation_held_in_project: ["session", "project"], conversation_held_in_console: ["session", "console"],
  session_did_not_stop: ["session"], switch_did_not_come_up: ["session"],
};

/** Names retain the surrounding prose's typeface rather than the monospace used for paths. */
export const NAME_PARAMS: Record<string, readonly string[]> = {
  unknown_console: ["console"],
  unknown_project: ["project"],
  unknown_session: ["session"],
  unknown_shell_terminal: ["shell_terminal"],
  unknown_page: ["page"],
  unknown_account: ["account"],
  not_a_git_repository: ["project"],
  quick_create_quota_refused: ["action"],
  quick_create_owner_gone: ["name"],
  account_name_taken: ["name"],
  config_dir_taken: ["name"],
  project_directory_shared: ["project"],
  conversation_in_session: ["session"],
  conversation_held_in_project: ["session", "project"],
  conversation_held_in_console: ["session", "console"],
  launch_failed: ["title"],
  "launch_failed.title": ["title"],
  session_did_not_stop: ["session"],
  project_branches_not_determined: ["name"],
  switch_did_not_come_up: ["session"],
  account_quota_exhausted: ["account"],
  "account_quota_exhausted.resets": ["account"],
};

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
 * uncommitted changes, `detail` the reason given for a `failed` one. */
export type LeftBranchItem = { kind: "worktree" | "branch"; name: string; branch: string | null; state: SessionBranchState; uncommitted?: boolean; detail: string | null };

/** The groups a `session_branches_not_removed` alert sorts its items into, in the order shown, each
 * with the lead line that says once why its items are still there; what could not be removed at
 * all comes first. */
type LeftBranchLead =
  | "branchCleanup.group.notRemoved" | "branchCleanup.group.uncommitted" | "branchCleanup.group.submodules"
  | "branchCleanup.group.locked" | "branchCleanup.group.unmerged" | "branchCleanup.group.checkedOut"
  | "branchCleanup.group.projectFolder";
const LEFT_BRANCH_GROUPS: Array<{ key: string; lead: LeftBranchLead; states: SessionBranchState[] }> = [
  { key: "notRemoved", lead: "branchCleanup.group.notRemoved", states: ["failed", "removable"] },
  { key: "uncommitted", lead: "branchCleanup.group.uncommitted", states: ["uncommitted_changes"] },
  { key: "submodules", lead: "branchCleanup.group.submodules", states: ["submodules"] },
  { key: "locked", lead: "branchCleanup.group.locked", states: ["locked"] },
  { key: "unmerged", lead: "branchCleanup.group.unmerged", states: ["unmerged"] },
  { key: "checkedOut", lead: "branchCleanup.group.checkedOut", states: ["checked_out"] },
  { key: "projectFolder", lead: "branchCleanup.group.projectFolder", states: ["project_folder"] },
];
const LEFT_BRANCH_STATES = new Set(LEFT_BRANCH_GROUPS.flatMap((group) => group.states));

/** `items` sorted into their groups, leaving out the empty ones. */
export function leftBranchGroups(items: LeftBranchItem[]): Array<{ key: string; lead: LeftBranchLead; items: LeftBranchItem[] }> {
  return LEFT_BRANCH_GROUPS
    .map(({ key, lead, states }) => ({ key, lead, items: items.filter((item) => states.includes(item.state)) }))
    .filter((group) => group.items.length > 0);
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
  return item.detail ? format(language, "branchCleanup.failed", { name, detail: isolate(item.detail) }) : name;
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

/** Producer-confirmed CLI and shell diagnostics are displayed separately from the explanation. */
export const DETAIL_BLOCK_CODES = new Set([
  "project_branches_not_determined", "git_failed", "git_clone_failed", "git_remote_unreachable",
  "git_push_failed", "git_switch_failed", "git_restore_failed", "git_unavailable",
  "models_command_failed", "models_shell_environment_failed", "quota_command_failed", "quota_shell_environment_failed", "codex_unarchive_failed", "login_failed", "shell_environment_timeout",
]);

/** These trust reasons carry an OS write/read diagnostic, not another daemon error code. */
export function hasTrustDiagnostic(code: string, params: MessageParams): boolean {
  return (code === "trust_answer_failed" || code === "trust_not_carried_over") && ["terminal_write_failed", "store_io_failed"].includes(params.reason_code) && !!params.detail;
}

export const RECOVERY_CODES = new Set([
  "console_has_running_sessions", "project_has_running_sessions", "project_directory_shared", "path_not_absolute", "directory_unreadable",
  "models_binary_not_found", "quota_binary_not_found", "quota_not_signed_in", "quota_config_dir_missing", "login_codex_busy",
  "config_dir_unreachable", "grok_home_not_initialized", "session_archived", "conversation_to_resume_not_found", "conversation_in_session",
  "session_waiting_for_user", "session_trust_pending", "queued_messages_lost", "source_changed", "trust_directory_too_broad",
  "agent_not_available", "queued_messages_dropped", "trust_answer_failed", "session_lock_busy", "session_locked", "restore_not_on_branch",
  "session_has_running_sessions",
]);

/** The rich sentence omits a raw diagnostic that is rendered separately. Plain messages keep the
 * complete catalog sentence for native notifications and other text-only consumers. */
export function richMessageCode(code: string, params: MessageParams): string {
  return RECOVERY_CODES.has(code) || (DETAIL_BLOCK_CODES.has(code) && code !== "project_branches_not_determined" && params.detail) ? `${code}.summary` : messageCode(code, params);
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

/** The params of a message that the UI supplies itself, by message code, beneath what the daemon
 * sends. */
export const UI_PARAMS: Record<string, MessageParams> = {
  trust_answer_failed: { enter: "Enter" },
  trust_not_carried_over: { enter: "Enter" },
  directory_unreadable: { macOS: "macOS" },
  git_failed: { git: "Git" },
  git_clone_failed: { git: "Git", cloneCommand: "git clone" },
  git_push_failed: { git: "Git" },
  git_switch_failed: { git: "Git" },
  git_restore_failed: { git: "Git" },
  git_unavailable: { git: "Git" },

  login_codex_busy: { codex: "Codex" },
  codex_unarchive_failed: { codex: "Codex" },
  grok_home_not_initialized: { grok: "Grok" },
  resume_id_invalid: { example: EXAMPLE_CONVERSATION_ID },
  path_not_absolute: { tilde: "~/" },
  config_dir_not_absolute: { tilde: "~/" },
  binary_not_found: { pathVar: "PATH" },
  models_binary_not_found: { pathVar: "PATH" },
  quota_binary_not_found: { pathVar: "PATH" },
  claude_workspace_untrusted: { allow: "allow", deny: "deny", claude: "Claude Code" },
  agent_not_available: { pathVar: "PATH", claude: "Claude Code", codex: "Codex", grokBuild: "Grok Build" },
};

/** The code a message is worded under: the daemon's, except where one code has a message of its own
 * for a variant its params name (`unsupported_file_type` of an `unmerged` file, worded as
 * `daemon.unsupported_file_type.unmerged`, like `daemon.trust_reason.<code>`; `account_quota_exhausted`
 * with a `resets_at` time, worded as `daemon.account_quota_exhausted.resets`). */
export function messageCode(code: string, params: MessageParams): string {
  if (code === "field_too_long" && (params.field === "name" || params.field === "description")) return `${code}.${params.field}`;
  if (code === "account_quota_exhausted" && /^\d+$/.test(params.resets_at ?? "")) return "account_quota_exhausted.resets";
  return code === "unsupported_file_type" && params.file_type === "unmerged" ? "unsupported_file_type.unmerged" : code;
}

/** A message's params as the UI shapes them: the daemon's over the UI's own (`UI_PARAMS`). */
export function withUiParams(code: string, params: MessageParams): MessageParams {
  return { ...UI_PARAMS[messageCode(code, params)], ...params };
}

/** The English window names `account_quota_exhausted` carries, and the messages that word them. */
const QUOTA_WINDOW_KEYS: Record<string, "quota.windowPhrase.session" | "quota.windowPhrase.weekly" | "quota.windowPhrase.monthly" | "quota.windowPhrase.credits"> = {
  "5-hour": "quota.windowPhrase.session",
  Weekly: "quota.windowPhrase.weekly",
  Monthly: "quota.windowPhrase.monthly",
  Credits: "quota.windowPhrase.credits",
};

/** The codes whose account name param may be the default account's, which the daemon names by its fixed
 * English `DEFAULT_ACCOUNT_NAME`, and the param that holds it. */
const DEFAULT_ACCOUNT_NAME_PARAMS: Record<string, string> = {
  account_name_taken: "name",
  config_dir_taken: "name",
  account_quota_exhausted: "account",
};

/** A param as plain text: a record id as that record's name, anything else as it came, isolated. */
function paramText(records: RecordNames, name: string, value: string, code: string, language: Language): string {
  if (name === "session" && (code === "conversation_held_in_project" || code === "conversation_held_in_console")) {
    const session = records.sessions.get(value);
    if (session) return isolate(sessionDisplayTitle((key, ...args) => format(language, key, args[0]), session));
  }
  return isolate(RECORD_ID_PARAMS[code]?.includes(name) ? RECORD_NAMES[name]?.(records, value) ?? value : value);
}

/** The values a message `code` is filled in with: the daemon's `params` over the UI's own
 * (`UI_PARAMS`), each shown as plain text (see `paramText`). A numeric `count` param is a number,
 * which selects the message's plural form. A `reason_code` param is worded from the catalog's
 * `daemon.trust_reason.<code>` and replaces `reason`, which stays the daemon's English account for a
 * reason code the catalog lacks. A `launch_failed` notice's `reason_code` is the code of the error
 * the launch failed with, so its `reason` is worded from that code's own `daemon.<code>` message, with
 * the failure's params that arrive beside it and that code's `UI_PARAMS`. With `mark`, a param the
 * message sets apart (`CODE_PARAMS` or `NAME_PARAMS`) is what `mark` makes of its displayed value instead. */
export function daemonParams<V = never>(
  language: Language,
  code: string,
  params: MessageParams,
  records: RecordNames,
  mark?: (value: string, monospace: boolean) => V,
): Record<string, string | number | V> {
  // A failed launch carries its failure's params, which take the UI's own for the failure's code.
  const all = code === "launch_failed" && params.reason_code ? withUiParams(params.reason_code, params) : withUiParams(code, params);
  // The default account is shown by its name in the current language, as the accounts section does.
  const accountParam = DEFAULT_ACCOUNT_NAME_PARAMS[code];
  if (accountParam && all[accountParam] === DEFAULT_ACCOUNT_NAME) {
    all[accountParam] = format(language, "settings.accounts.defaultName", {});
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
    shown.items = format(language, "branchCleanup.plain.end", { groups: joined("branchCleanup.plain.groups", groups) });
  }
  // A count arrives as text; as a number it selects the singular or plural wording.
  if (/^\d+$/.test(all.count ?? "")) shown.count = Number(all.count);
  if (code === "field_too_long" && /^\d+$/.test(all.limit ?? "")) shown.limit = Number(all.limit);
  // The daemon names the window in English; the standard names are worded in the UI's language, any other
  // (a window's own label) as a quota of that name.
  if (code === "account_quota_exhausted") {
    shown.quota = Object.hasOwn(QUOTA_WINDOW_KEYS, all.window)
      ? format(language, QUOTA_WINDOW_KEYS[all.window], {})
      : format(language, "quota.windowPhrase.named", { window: shown.window });
  }
  // The time left until the account's quota resets, counted from when the message is worded.
  if (messageCode(code, all) === "account_quota_exhausted.resets") {
    shown.time = countdown((key, ...args) => format(language, key, args[0]), Number(all.resets_at), Date.now());
  }
  const reasonKey = `daemon.trust_reason.${all.reason_code}`;
  if (isMessageKey(reasonKey)) shown.reason = format(language, reasonKey, shown);
  const failureKey = `daemon.${messageCode(all.reason_code ?? "", all)}`;
  if (code === "launch_failed" && all.reason_code !== "launch_failed" && isMessageKey(failureKey)) shown.reason = daemonMessage(language, all.reason_code, all, String(shown.reason ?? shown.detail ?? ""), records);
  if (!mark) return shown;
  const marked: Record<string, string | number | V> = { ...shown };
  for (const name of CODE_PARAMS[messageCode(code, all)] ?? []) if (name in all) marked[name] = mark(all[name], true);
  for (const name of NAME_PARAMS[messageCode(code, all)] ?? []) {
    if (!(name in all)) continue;
    const recordId = RECORD_ID_PARAMS[code]?.includes(name) ?? false;
    const recordName = recordId ? RECORD_NAMES[name]?.(records, all[name]) : undefined;
    marked[name] = mark(recordName ?? all[name], recordId && recordName === undefined);
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
 * shows the daemon's own English `fallback`. The params are shaped by `daemonParams`. */
export function daemonMessage(
  language: Language,
  code: string,
  params: MessageParams,
  fallback: string,
  records: RecordNames,
): string {
  const key = `daemon.${messageCode(code, params)}`;
  if (!isMessageKey(key)) return fallback;
  if (code === "shell_environment_timeout" && !["shell", "command", "timeout"].every((name) => !!params[name])) return fallback;
  const shown = daemonParams(language, code, params, records);
  // A single group of leftovers is said by its own lead, without the line over the groups.
  const left = leftBranchItems(code, params);
  if (left && leftBranchGroups(left).length === 1) return String(shown.items);
  const text = format(language, key, shown);
  // As plain text the diagnostic follows on a line of its own.
  return code === "project_branches_not_determined" && params.detail ? `${text}\n${params.detail}` : text;
}
