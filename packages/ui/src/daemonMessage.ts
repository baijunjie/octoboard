import { EXAMPLE_CONVERSATION_ID } from "./conversationId";
import { format, isMessageKey } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import type { Language } from "./i18n/languages";
import type { MessageParams } from "./protocol";
import type { State } from "./store";

/** The params that are record ids, and what each is shown as: the record's current name. */
const RECORD_NAMES: Record<string, (state: RecordNames, id: string) => string | undefined> = {
  console: (state, id) => state.consoles.get(id)?.name,
  project: (state, id) => state.projects.get(id)?.name,
  session: (state, id) => state.sessions.get(id)?.title,
};

type RecordNames = Pick<State, "consoles" | "projects" | "sessions">;

/** The params a message shows as code, by message code: a technical literal (an ID, a path) that
 * reads better set apart from the sentence than quoted inside it. The plain text of such a message
 * carries the value bare; `DaemonErrorMessage` sets it as `InlineCode`. A code the browser panes show
 * (`browseError.ts`) has its params reshaped first (`shownParams`), so a structured render of it
 * must be given those, never the daemon's raw ones. */
export const CODE_PARAMS: Record<string, readonly string[]> = {
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
  trust_not_carried_over: ["path"],
  clean_filtered: ["path", "filter"],
  invalid_branch_name: ["branch"],
  unknown_branch: ["branch"],
  invalid_commit: ["commit"],
  unknown_commit: ["commit"],
  no_merge_base: ["left", "right"],
  no_merge_base_shallow: ["left", "right"],
  binary_not_found: ["binary", "pathVar"],
  models_binary_not_found: ["binary", "pathVar"],
  config_dir_not_absolute: ["tilde"],
  invalid_launch_setting: ["value"],
  claude_workspace_untrusted: ["allow", "deny"],
  agent_not_available: ["pathVar"],
};

/** The params of a message that the UI supplies itself, by message code, beneath what the daemon
 * sends. */
export const UI_PARAMS: Record<string, MessageParams> = {
  resume_id_invalid: { example: EXAMPLE_CONVERSATION_ID },
  path_not_absolute: { tilde: "~/" },
  config_dir_not_absolute: { tilde: "~/" },
  binary_not_found: { pathVar: "PATH" },
  models_binary_not_found: { pathVar: "PATH" },
  claude_workspace_untrusted: { allow: "allow", deny: "deny" },
  agent_not_available: { pathVar: "PATH" },
};

/** The code a message is worded under: the daemon's, except where one code has a message of its own
 * for a variant its params name (`unsupported_file_type` of an `unmerged` file, worded as
 * `daemon.unsupported_file_type.unmerged`, like `daemon.trust_reason.<code>`). */
export function messageCode(code: string, params: MessageParams): string {
  return code === "unsupported_file_type" && params.file_type === "unmerged" ? "unsupported_file_type.unmerged" : code;
}

/** A message's params as the UI shapes them: the daemon's over the UI's own (`UI_PARAMS`). */
export function withUiParams(code: string, params: MessageParams): MessageParams {
  return { ...UI_PARAMS[messageCode(code, params)], ...params };
}

/** A param as plain text: a record id as that record's name, anything else as it came, isolated. */
function paramText(records: RecordNames, name: string, value: string): string {
  return isolate(RECORD_NAMES[name]?.(records, value) ?? value);
}

/** The values a message `code` is filled in with: the daemon's `params` over the UI's own
 * (`UI_PARAMS`), each shown as plain text (see `paramText`). A numeric `count` param is a number,
 * which selects the message's plural form. A `reason_code` param is worded from the catalog's
 * `daemon.trust_reason.<code>` and replaces `reason`, which stays the daemon's English account for a
 * reason code the catalog lacks. A `launch_failed` notice's `reason_code` is the code of the error
 * the launch failed with, so its `reason` is worded from that code's own `daemon.<code>` message, with
 * the failure's params that arrive beside it and that code's `UI_PARAMS`. With `mark`, a param the
 * message shows as code (`CODE_PARAMS`) is what `mark` makes of its raw value instead. */
export function daemonParams<V = never>(
  language: Language,
  code: string,
  params: MessageParams,
  records: RecordNames,
  mark?: (value: string) => V,
): Record<string, string | number | V> {
  // A failed launch carries its failure's params, which take the UI's own for the failure's code.
  const all = code === "launch_failed" && params.reason_code ? withUiParams(params.reason_code, params) : withUiParams(code, params);
  const shown: Record<string, string | number> = Object.fromEntries(
    Object.entries(all).map(([name, value]) => [name, paramText(records, name, value)]),
  );
  // A count arrives as text; as a number it selects the singular or plural wording.
  if (/^\d+$/.test(all.count ?? "")) shown.count = Number(all.count);
  const reasonKey = `daemon.trust_reason.${all.reason_code}`;
  if (isMessageKey(reasonKey)) shown.reason = format(language, reasonKey, shown);
  const failureKey = `daemon.${messageCode(all.reason_code ?? "", all)}`;
  if (code === "launch_failed" && isMessageKey(failureKey)) shown.reason = format(language, failureKey, shown);
  if (!mark) return shown;
  const marked: Record<string, string | number | V> = { ...shown };
  for (const name of CODE_PARAMS[messageCode(code, all)] ?? []) if (name in all) marked[name] = mark(all[name]);
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
  return format(language, key, daemonParams(language, code, params, records));
}
