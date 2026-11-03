// The request-to-reply contract of the daemon connection, shared by the client that enforces it and by everything
// that stands in for the daemon.

import type { Event, RequestBody } from "./protocol";

/** The reply each request is answered with when it succeeds; a failure is an `error` for any of them. A request with
 * nothing to return is answered with `ack`; `answer_session_request` with the session it started, or `ack` when it
 * started none. A new request has to name its reply here, which `DaemonClient.request` checks every reply against. */
export const REPLY_TYPES = {
  create_console: "ack",
  update_console: "ack",
  delete_console: "ack",
  add_project: "ack",
  update_project: "ack",
  delete_project: "ack",
  inspect_project_removal: "project_removal_inspected",
  list_dir: "dir_listing",
  detect_directory_agent: "agent_detected",
  probe_git_remote: "agent_detected",
  list_agent_models: "agent_models",
  locate_conversation_to_resume: "conversation_located",
  refresh_account_quotas: "ack",
  start_account_login: "ack",
  use_codex_reset_credit: "reset_credit_used",
  cancel_account_login: "ack",
  export_configuration: "configuration_exported",
  inspect_configuration: "configuration_inspected",
  import_configuration: "configuration_imported",
  open_session: "session_opened",
  resume_session: "ack",
  archive_session: "ack",
  switch_session_account: "ack",
  set_session_model: "session_model_set",
  handoff_session: "session_opened",
  inspect_session_records: "session_records_inspected",
  read_session_history: "session_history",
  inspect_session_branches: "session_branches_inspected",
  list_processes: "processes_listed",
  list_all_processes: "all_processes_listed",
  stop_processes: "processes_stopped",
  keep_processes: "processes_kept",
  delete_session: "ack",
  delete_archived_sessions: "ack",
  set_session_pinned: "ack",
  set_session_locked: "ack",
  set_session_colour: "ack",
  send_message: "ack",
  interrupt_session: "session_interrupted",
  rename_session: "ack",
  release_session: "ack",
  list_pages: "page_list",
  submit_page: "ack",
  confirm_trust: "ack",
  remove_trusted_directory: "ack",
  answer_session_request: ["session_opened", "ack"],
  update_settings: "ack",
  refresh_git_status: "ack",
  pull_project_branch: "ack",
  create_shell_terminal: "shell_terminal_created",
  rename_shell_terminal: "ack",
  close_shell_terminal: "ack",
  open_project_terminal: "ack",
  open_project_directory: "ack",
  reveal_project_path: "ack",
  open_project_path: "ack",
  probe_path_links: "path_links",
  open_path_link: "ack",
  push_project_branch: "ack",
  switch_project_branch: "ack",
  restore_project_changes: "ack",
  create_account: "ack",
  update_account: "ack",
  delete_account: "ack",
  inspect_account_trash: "account_trash_inspected",
  add_agent: "ack",
  reorder_agents: "ack",
  reorder_accounts: "ack",
  remove_agent: "ack",
  set_default_account_name: "ack",
  set_codex_approval_reviewer: "ack",
  set_claude_approval_mode: "ack",
  set_account_disabled: "ack",
  set_account_sharing: "ack",
  get_project_source: "project_source",
  search_project_files: "project_file_matches",
  list_project_dir: "project_dir",
  read_project_file: "project_file",
  list_project_changes: "project_changes",
  read_project_change: "project_change",
  list_project_branches: "project_branches",
  compare_project_branches: "project_comparison",
  read_project_comparison_change: "project_comparison_change",
  read_project_change_bodies: "project_change_bodies",
  read_project_comparison_change_bodies: "project_comparison_change_bodies",
  list_project_commits: "project_commits",
  read_project_commit_change: "project_commit_change",
  read_project_commit_change_bodies: "project_commit_change_bodies",
  shutdown: "ack",
} as const satisfies Record<RequestBody["type"], Event["type"] | readonly Event["type"][]>;

type ReplyTypes = typeof REPLY_TYPES;

/** A request's `type`. */
export type RequestType = RequestBody["type"];

/** Each request type's request. */
type RequestsByType = { [K in RequestType]: Extract<RequestBody, { type: K }> };

/** The request of `type`. An indexed type rather than an `Extract`, so a call infers `type` from the body. */
export type RequestOf<T extends RequestType> = RequestsByType[T];

/** The event types one entry of `REPLY_TYPES` names. */
type Named<V> = V extends readonly (infer R)[] ? R : V;

/** The reply a request of `type` resolves with: `ReplyOf<"list_processes">` is the `processes_listed` event. */
export type ReplyOf<T extends RequestType> = Extract<Event, { type: Named<ReplyTypes[T]> }>;

/** Sends a request and resolves with its reply, of the type the request is answered with. */
export type RequestSender = <T extends RequestType>(body: RequestsByType[T] & { type: T }) => Promise<ReplyOf<T>>;

/** A request handler typed as a plain union: what `looseRequestSender` turns into a `RequestSender`. */
export type RequestHandler = (body: RequestBody) => Promise<Event>;

/** A `RequestSender` over a handler that answers requests from somewhere other than the daemon (the fixture daemon, the
 * website preview, a test's stand-in), which narrows a request by its `type` as a plain union. Nothing checks that it
 * answers each request with its reply type, as `DaemonClient` does. */
export function looseRequestSender(send: RequestHandler): RequestSender {
  return send as RequestSender;
}
