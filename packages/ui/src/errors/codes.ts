import { EXAMPLE_CONVERSATION_ID } from "../agents/conversationId";
import type { MessageParams } from "./codedError";
import { pathLinkRefusedCause } from "./causes";
import { PROVIDER_CODES } from "../agents/providers";
import type { ErrorRecovery } from "./errorRecovery";
import type { PlatformHost } from "../i18n/platformTerms";

/** Where a code comes from: the daemon declares it (`apps/daemon/src/protocol/codes.rs`), `daemon-client.ts` raises it
 * itself, or only the UI words it. */
export type CodeOrigin = "daemon" | "client" | "ui";

/** What the UI says about the params of one message: the code's own, or those of one variant it is worded under. */
export interface MessageFacets {
  /** Params shown as code: a technical literal (an ID, a path) that reads better set apart from the sentence than
   * quoted inside it. The plain text of such a message carries the value bare; `DaemonErrorMessage` sets it as
   * `InlineReference`. A code the browser panes show (`browse/failure.ts`) has its params reshaped first (`shownParams`),
   * so a structured render of it must be given those, never the daemon's raw ones. */
  codeParams?: readonly string[];
  /** Params shown as a name, which keeps the surrounding prose's typeface rather than the monospace used for paths. */
  nameParams?: readonly string[];
}

/** Everything the UI knows about one code, whichever consumer asks: how its message is worded, what the alert shows for
 * it and which remedy goes with it. A facet that is absent is false or empty. */
export interface CodeRecord extends MessageFacets {
  origin: CodeOrigin;
  /** The remedy the alert gives (`classifyError`). A code with none, nor `causeOnly` or `unexpected`, is not classified
   * by its code: a failure shown where it is read (a quota reading, a models list, a login), one the client raises
   * itself and the alert words from its message (`unexpected_reply`), or a JavaScript error that is no `CodedError`,
   * which `errorPresentation` wraps as `unexpected_error` and the alert shows as unexpected. */
  recovery?: ErrorRecovery;
  /** Actionable with no remedy: the alert says why it was refused or failed, never what to do, and names the system's
   * own cause after the sentence. */
  causeOnly?: true;
  /** Why the window shows this failure as unexpected on purpose, or classifies it by its cause or by another code it
   * carries. */
  unexpected?: string;
  /** Carries a nested failure as `reason_code` and its params, so its reason is worded from that failure's message;
   * `failureAndRemedy` also shows that failure's own remedy sentence (a failed launch has none of its own, a failed
   * automatic switch says what to do in its own sentence). A handoff that failed after the work moved uses `failure`
   * although the nested failure has a remedy: that remedy would say to retry, so the alert's remedy comes from
   * `classifyError` instead, with the retry removed. */
  nested?: "failure" | "failureAndRemedy";
  /** A session notice that is a failure, shown as an error alert rather than a toast (`noticeKind`). */
  failureNotice?: true;
  /** `daemon.<code>.alertTitle` exists: the alert's own title, which wins over the caller's except in a dialog's
   * submit or confirmation (and in a dialog's closing, after-the-fact alert for `handoff_archive_failed` and
   * `handoff_move_failed`, whose title is the code's: "Could not finish the handoff"). Checked against the catalog by
   * `daemonCodes.test.ts`, not read at run time. */
  alertTitle?: true;
  /** Producer-confirmed CLI, shell and sharing diagnostics: its `detail` is shown as a block of its own, and the rich
   * sentence (`daemon.<code>.summary`) leaves it out. */
  detailBlock?: true;
  /** `false` where `detailBlock` is set but the catalog has no `.summary`, so the plain sentence stays. */
  summaryForDetail?: false;
  /** Has a step of its own: `daemon.<code>.summary` says what happened and `.recovery` that step. */
  recoveryLine?: true;
  /** No `daemon.<code>.summary`: raised only by an action whose alert is titled for what failed, whose summary could
   * only have said that again. The alert has no opening sentence and the body starts with the remedy. A code also
   * shown where no title says what failed (a pane's failure, a mark's popover, a field's message) needs its summary. */
  noSummary?: true;
  /** Carries a list of sharing failure entries as its `items` param. */
  sharingItems?: true;
  /** A file or folder in a project that is not there any more, a folder that has become a file included
   * (`hostPathRecovery`). */
  hostPathGone?: true;
  /** Params that are record ids, shown as the record's current name. Code-specific: shared-folder errors already carry
   * a project name, while switch failures carry session IDs, and an account quota carries a name, not an account ID. */
  recordIds?: readonly string[];
  /** Params that may hold the default account's name, which the daemon gives in its fixed English
   * `DEFAULT_ACCOUNT_NAME`. */
  defaultAccountParams?: readonly string[];
  /** Params the UI supplies itself (Git, PATH, `~/`, Enter, product names), beneath what the daemon sends. */
  uiParams?: MessageParams;
  /** The variant of the code a message with `params` is worded as, `daemon.<code>.<variant>` (see `messageCode`), or
   * nothing for the code's own message. `os` is the operating system of the daemon's host, for a message whose wording
   * is that of one system. Each variant it can name has its message in every catalog. */
  variantOf?: (params: MessageParams, os: PlatformHost["os"]) => string | undefined;
  /** The message's params under a variant `variantOf` names, where they differ from the code's own. */
  variants?: Record<string, MessageFacets>;
}

/** The operations `provider_unsupported` is worded under, as its variants. */
const UNSUPPORTED_OPERATIONS: Record<string, MessageFacets> = { sign_in: {}, kick: {}, reset_credit: {}, config_sharing: {} };

/** The variant of a message about a directory the daemon cannot read or reach, on a host that is not a Mac: the
 * per-volume file access prompts its own wording explains are macOS's, and elsewhere the daemon reads with the
 * permissions of the account it runs as. */
function notMacOS(os: PlatformHost["os"]): string | undefined {
  return os === "macos" ? undefined : "accountPermissions";
}

/** The codes of the core: every code the UI has anything to say about except a provider's own, which its presentation
 * module owns (`providers/`). Keep a daemon code here in the same change that has the daemon send it:
 * `daemonCodes.test.ts` fails for one missing, and for one the daemon no longer declares. */
const CORE_CODES = {
  account_being_removed: { origin: "daemon", causeOnly: true, nameParams: ["name"] },
  account_directory_protected: { origin: "daemon", causeOnly: true },
  account_directory_shared: { origin: "daemon", causeOnly: true, variants: { account: { nameParams: ["name"] }, project: { nameParams: ["name"] } }, variantOf: (params) => params.kind === "account" || params.kind === "default_account" || params.kind === "project" ? params.kind : undefined },
  // Removing an account with its directory, and a login refused because the account is being removed
  // (`causeOnly`): the alert says only why it was refused or failed, never what to do.
  account_directory_trash_failed: { origin: "daemon", causeOnly: true, detailBlock: true, variantOf: (params) => ["unanswered", "unreadable", "other_unlooked", "signing_in"].includes(params.cause) ? params.cause : undefined },
  account_disabled: { origin: "daemon", recovery: "accountDisabled", defaultAccountParams: ["account"], nameParams: ["account"] },
  account_has_running_sessions: { origin: "daemon", causeOnly: true, nameParams: ["name"] },
  account_in_use: { origin: "daemon", recovery: "accountBusy", nameParams: ["name"], variants: { signingIn: { nameParams: ["name"] }, both: { nameParams: ["name"] } }, variantOf: (params) => params.signing_in !== "true" ? undefined : /^[1-9]\d*$/.test(params.sessions ?? "") ? "both" : "signingIn" },
  account_name_taken: { origin: "daemon", recovery: "uniqueName", defaultAccountParams: ["name"], nameParams: ["name"] },
  account_not_signed_in: { origin: "daemon", recovery: "accountNotSignedIn", defaultAccountParams: ["account"], nameParams: ["account"] },
  account_order_invalid: { origin: "daemon", recovery: "refresh" },
  account_quota_exhausted: { origin: "daemon", recovery: "quotaUsedUp", defaultAccountParams: ["account"], nameParams: ["account"], variants: { resets: { nameParams: ["account"] } }, variantOf: (params) => /^\d+$/.test(params.resets_at ?? "") ? "resets" : undefined },
  account_sharing_failed: { origin: "daemon", recovery: "accountSharing", detailBlock: true, summaryForDetail: false, sharingItems: true, nameParams: ["name"] },
  account_sharing_in_progress: { origin: "daemon", recovery: "accountChanging", nameParams: ["name"] },
  account_sharing_repair_failed: { origin: "daemon", recovery: "none", failureNotice: true, alertTitle: true, sharingItems: true, nameParams: ["name"] },
  account_still_sharing: { origin: "daemon", recovery: "accountBusy", nameParams: ["name"] },
  account_switch_automatically_failed: { origin: "daemon", unexpected: "classified by the failure its `reason_code` names", nested: "failure", failureNotice: true, alertTitle: true, defaultAccountParams: ["from", "account"], nameParams: ["from", "account"] },
  account_switched_automatically: { origin: "daemon", recordIds: ["session"], defaultAccountParams: ["from", "account"], nameParams: ["from", "account", "session"] },
  added_agents_undecided: { origin: "daemon", recovery: "availability" },
  agent_already_added: { origin: "daemon", recovery: "addedAgents" },
  agent_availability_pending: { origin: "daemon", recovery: "availability" },
  agent_not_added: { origin: "daemon", recovery: "agentNotAdded" },
  agent_not_available: { origin: "daemon", recovery: "tool", recoveryLine: true, codeParams: ["pathVar"], uiParams: { pathVar: "PATH" } },
  agent_not_installed: { origin: "daemon", recovery: "tool" },
  agent_order_invalid: { origin: "daemon", recovery: "addedAgents" },
  all_projects_already_added: { origin: "daemon", recovery: "existingProject" },
  binary_not_found: { origin: "daemon", recovery: "tool", codeParams: ["binary", "pathVar"], uiParams: { pathVar: "PATH" } },
  clean_filtered: { origin: "daemon", recovery: "scope", codeParams: ["path", "filter"] },
  config_dir_not_absolute: { origin: "daemon", recovery: "path", codeParams: ["tilde"], uiParams: { tilde: "~/" } },
  config_dir_taken: { origin: "daemon", recovery: "accountDirectory", defaultAccountParams: ["name"], nameParams: ["name"] },
  config_dir_unreachable: { origin: "daemon", recovery: "path", recoveryLine: true, codeParams: ["path"] },
  configuration_not_recognised: { origin: "daemon", recovery: "configuration" },
  configuration_unreadable: { origin: "daemon", recovery: "configuration", detailBlock: true },
  configuration_version_unsupported: { origin: "daemon", recovery: "configuration", codeParams: ["version", "oldest", "supported"] },
  conflicting_fields: { origin: "daemon", unexpected: "the window never sends two fields that exclude each other", codeParams: ["first", "second"] },
  connection_closed: { origin: "client", recovery: "connection" },
  console_has_running_sessions: { origin: "daemon", recovery: "stopSessions", recoveryLine: true },
  // `console_session_never_bound` and `session_not_bound_to_owner` cannot come from the UI: the window sends
  // release_session without an owner, and offers Unbind only on a bound project session.
  console_session_never_bound: { origin: "daemon", recovery: "none" },
  console_session_trust_not_asked: { origin: "daemon", unexpected: "the window never offers to answer a console session's trust confirmation" },
  conversation_held_in_console: { origin: "ui", recordIds: ["session", "console"], nameParams: ["session", "console"] },
  conversation_held_in_project: { origin: "ui", recordIds: ["session", "project"], nameParams: ["session", "project"] },
  conversation_in_session: { origin: "daemon", recovery: "conversation", recoveryLine: true, recordIds: ["session"], nameParams: ["session"] },
  conversation_not_found: { origin: "daemon", recovery: "conversation", codeParams: ["path"] },
  conversation_outside_project: { origin: "daemon", recovery: "conversation" },
  conversation_to_resume_not_found: { origin: "daemon", recovery: "conversation", recoveryLine: true, codeParams: ["path"] },
  conversation_unresumable: { origin: "daemon", recovery: "unresumable" },
  daemon_shutting_down: { origin: "daemon", recovery: "none" },
  default_account_not_shareable: { origin: "daemon", recovery: "accountNotShareable" },
  directory_unavailable: { origin: "daemon", recovery: "openFileManager", detailBlock: true, noSummary: true },
  directory_unreachable: { origin: "daemon", recovery: "path", codeParams: ["path"], variants: { accountPermissions: { codeParams: ["path"] } }, variantOf: (_params, os) => notMacOS(os) },
  directory_unreadable: { origin: "daemon", recovery: "access", detailBlock: true, recoveryLine: true, codeParams: ["path"], variants: { accountPermissions: { codeParams: ["path"] } }, variantOf: (_params, os) => notMacOS(os) },
  draft_restore_unconfirmed: { origin: "daemon", recovery: "none", failureNotice: true, alertTitle: true, recoveryLine: true },
  draft_save_unconfirmed: { origin: "daemon", recovery: "none", failureNotice: true, alertTitle: true, recoveryLine: true },
  // The export dialog keeps Export unavailable while nothing is chosen, so only a client that does not guard it
  // meets this.
  export_empty: { origin: "daemon", recovery: "exportSelection" },
  field_required: { origin: "daemon", unexpected: "the window always sends the fields a request needs", codeParams: ["field"] },
  field_too_long: { origin: "daemon", unexpected: "the forms that send a project's name and description hold them to the daemon's limits", codeParams: ["field"], variantOf: (params) => params.field === "name" || params.field === "description" ? params.field : undefined },
  file_not_found: { origin: "daemon", recovery: "path", hostPathGone: true, codeParams: ["path"] },
  git_clone_failed: { origin: "daemon", recovery: "gitFailure", detailBlock: true, codeParams: ["cloneCommand"], uiParams: { git: "Git", cloneCommand: "git clone" } },
  git_failed: { origin: "daemon", recovery: "gitFailure", detailBlock: true, uiParams: { git: "Git" } },
  // Expected failures whose sentence already says what follows and what to do, with no remedy of their kind to
  // add. `git_operation_in_progress` is only a restore refused before it starts, and says to finish or abort the
  // operation.
  git_operation_in_progress: { origin: "daemon", recovery: "none" },
  git_push_failed: { origin: "daemon", recovery: "gitFailure", detailBlock: true, noSummary: true, uiParams: { git: "Git" } },
  git_remote_unreachable: { origin: "daemon", recovery: "connection", detailBlock: true },
  git_restore_failed: { origin: "daemon", recovery: "gitRestoreFailure", detailBlock: true, noSummary: true, uiParams: { git: "Git" } },
  // Switching a branch and restoring files are local: their remedies do not point at remote access. A restore
  // discards uncommitted changes by design, so its remedy names what can really refuse it.
  git_switch_failed: { origin: "daemon", recovery: "gitSwitchFailure", detailBlock: true, noSummary: true, uiParams: { git: "Git" } },
  git_unavailable: { origin: "daemon", recovery: "tool", detailBlock: true, uiParams: { git: "Git" } },
  handoff_messages_dropped: { origin: "daemon", recovery: "lostMessages", failureNotice: true, alertTitle: true },
  // The archive of the original failed after the work moved: the failure it carries as `reason_code` is worded as a
  // failed launch's is. Its own remedy would say to retry, though the work is done, so `nested` is `failure` and the
  // remedy comes from `classifyError` with the retry removed; the receiving session rides as `receiving_session`,
  // leaving the failure's own `session` param as it is. One with no words of its own is shown as unexpected.
  handoff_archive_failed: { origin: "daemon", unexpected: "shown as the failure its `reason_code` names", nested: "failure", alertTitle: true, recordIds: ["receiving_session"], nameParams: ["receiving_session"] },
  // The move of the place failed after the receiving session was running, which stays running: worded and given its
  // remedy as the archive's failure is, from the failure it carries.
  handoff_move_failed: { origin: "daemon", unexpected: "shown as the failure its `reason_code` names", nested: "failure", alertTitle: true, recordIds: ["receiving_session"], nameParams: ["receiving_session"] },
  handoff_to_same_agent: { origin: "daemon", unexpected: "the handoff dialog never offers the session's own agent" },
  // The console dialog draws a picked image into a data URL scaled down to the limit, so only an imported
  // configuration carries an avatar that is not an image or is too large.
  icon_not_an_image: { origin: "daemon", recovery: "avatarImage", variants: { import: { nameParams: ["console_name"] } }, variantOf: (params) => params.console_name ? "import" : undefined },
  icon_too_large: { origin: "daemon", recovery: "avatarSize", variants: { import: { nameParams: ["console_name"] } }, variantOf: (params) => params.console_name ? "import" : undefined },
  import_accounts_missing: { origin: "daemon", recovery: "configuration" },
  import_choice_unavailable: { origin: "daemon", recovery: "configuration" },
  import_resolution_invalid: { origin: "daemon", recovery: "configuration" },
  import_settings_missing: { origin: "daemon", recovery: "configuration" },
  internal_error: { origin: "daemon", unexpected: "a failure the daemon did not foresee, which is what the unexpected alert is for" },
  invalid_branch_name: { origin: "daemon", recovery: "branch", codeParams: ["branch"] },
  invalid_change: { origin: "daemon", unexpected: "the window names only sides the change's group can read" },
  invalid_commit: { origin: "daemon", unexpected: "the window names only commits the daemon gave it", codeParams: ["commit"] },
  invalid_launch_setting: { origin: "daemon", recovery: "launchSetting", codeParams: ["field", "value"] },
  invalid_path: { origin: "daemon", recovery: "path", codeParams: ["path"] },
  invalid_remote_url: { origin: "daemon", recovery: "remote", codeParams: ["url"] },
  launch_failed: { origin: "daemon", unexpected: "shown as the failure its `reason_code` names", nested: "failureAndRemedy", failureNotice: true, alertTitle: true, nameParams: ["title"], variants: { title: { nameParams: ["title"] } } },
  limit_exceeded: { origin: "daemon", recovery: "limit" },
  login_already_running: { origin: "daemon", recovery: "none" },
  // The login dialog takes up a login already running and ignores a cancel of one that has ended.
  login_failed: { origin: "daemon", detailBlock: true },
  login_not_confirmed: { origin: "daemon" },
  login_not_running: { origin: "daemon", recovery: "none" },
  message_discarded_by_dialog: { origin: "daemon" },
  message_diverted_by_view: { origin: "daemon" },
  message_not_sent: { origin: "daemon", recovery: "unsentMessage", failureNotice: true, alertTitle: true, variantOf: (params) => params.submit_retry === "once" ? "retry" : undefined },
  // A relaunch that ends at once with a new model most likely does not take that model, and leaves the session not
  // running.
  model_change_did_not_come_up: { origin: "daemon", recovery: "modelChange" },
  model_change_did_not_stop: { origin: "daemon", recovery: "stopProcess" },
  models_binary_not_found: { origin: "daemon", recoveryLine: true, codeParams: ["binary", "pathVar"], uiParams: { pathVar: "PATH" } },
  models_command_failed: { origin: "daemon", detailBlock: true },
  models_shell_environment_failed: { origin: "daemon", detailBlock: true },
  models_timed_out: { origin: "daemon" },
  models_unreadable_output: { origin: "daemon" },
  no_commits_to_push: { origin: "daemon", recovery: "noCommits" },
  no_commits_to_restore: { origin: "daemon", recovery: "noCommits" },
  no_merge_base: { origin: "daemon", recovery: "gitHistory", codeParams: ["left", "right"] },
  no_merge_base_shallow: { origin: "daemon", recovery: "gitHistory", codeParams: ["left", "right"] },
  no_push_remote: { origin: "daemon", recovery: "gitFailure" },
  no_repositories_found: { origin: "daemon", recovery: "path", codeParams: ["path"] },
  not_a_git_repository: { origin: "daemon", recovery: "repository", recordIds: ["project"], nameParams: ["project"] },
  not_on_a_branch: { origin: "daemon", recovery: "branch" },
  outside_scope: { origin: "daemon", recovery: "scope", codeParams: ["path"], variants: { open: { codeParams: ["path"] } }, variantOf: (params) => params.action === "open" ? "open" : undefined },
  page_not_current: { origin: "daemon", recovery: "refresh" },
  path_already_exists: { origin: "daemon", recovery: "pathExists", codeParams: ["path"] },
  // A path link the daemon will not open; its `cause` picks the remedy (`PATH_LINK_REFUSED_RECOVERY`), which is
  // none for the causes whose sentence says it all.
  path_link_refused: { origin: "daemon", recovery: "none", detailBlock: true, variants: { ambiguous: { codeParams: ["path"] }, launchable: { codeParams: ["path"] }, not_private: { codeParams: ["path"] }, not_a_file_or_folder: { codeParams: ["path"] }, unreadable: { codeParams: ["path"] }, other: { codeParams: ["path"] } }, variantOf: (params) => pathLinkRefusedCause(params.cause) },
  path_not_a_directory: { origin: "daemon", recovery: "path", hostPathGone: true, codeParams: ["path"] },
  path_not_absolute: { origin: "daemon", recovery: "path", recoveryLine: true, codeParams: ["path", "tilde"], uiParams: { tilde: "~/" } },
  path_not_found: { origin: "daemon", recovery: "path", hostPathGone: true, codeParams: ["path"] },
  path_open_failed: { origin: "daemon", recovery: "openDefaultApplication", detailBlock: true, noSummary: true },
  // A read of the project's files the system denied, where there is no folder to choose instead and nothing is written.
  permission_denied: { origin: "daemon", recovery: "readAccess", detailBlock: true, codeParams: ["path"] },
  process_count_timed_out: { origin: "daemon", recovery: "none" },
  // A stop the daemon answered with processes it left running (`processes_stopped`'s `not_stopped`), worded by why
  // the first of them was left.
  process_not_stopped: { origin: "ui", recovery: "none", recoveryLine: true, variantOf: (params) => params.reason === "access_denied" ? "accessDenied" : undefined },
  process_shares_agent_console: { origin: "daemon", recovery: "none", recoveryLine: true },
  project_branches_not_determined: { origin: "daemon", recovery: "projectBranches", alertTitle: true, detailBlock: true, summaryForDetail: false, nameParams: ["name"] },
  project_directory_protected: { origin: "daemon", recovery: "trash" },
  project_directory_shared: { origin: "daemon", recovery: "trash", recoveryLine: true, nameParams: ["project"], variants: { retryTrash: { nameParams: ["project"] } } },
  project_directory_trash_failed: { origin: "daemon", recovery: "trash", detailBlock: true, recoveryLine: true, variantOf: (params) => params.cause === "unanswered" || params.cause === "unreadable" ? params.cause : undefined },
  project_has_running_sessions: { origin: "daemon", recovery: "stopSessions", recoveryLine: true },
  provider_unsupported: { origin: "daemon", recovery: "none", variants: UNSUPPORTED_OPERATIONS, variantOf: (params) => params.operation !== undefined && Object.hasOwn(UNSUPPORTED_OPERATIONS, params.operation) ? params.operation : undefined },
  // A push refused because the project's previous one is still running, and says so.
  push_in_progress: { origin: "daemon", recovery: "none" },
  queued_messages_dropped: { origin: "daemon", recovery: "refreshMessages", failureNotice: true, alertTitle: true, recoveryLine: true },
  queued_messages_lost: { origin: "daemon", recovery: "refreshMessages", recoveryLine: true, variants: { held: {} }, variantOf: (params) => params.fragment === "held" ? "held" : undefined },
  quick_create_owner_gone: { origin: "ui", recovery: "ownerGone", nameParams: ["name"] },
  quick_create_unavailable: { origin: "ui", recovery: "none" },
  quota_binary_not_found: { origin: "daemon", recoveryLine: true, codeParams: ["binary", "pathVar"], uiParams: { pathVar: "PATH" } },
  quota_command_failed: { origin: "daemon", detailBlock: true },
  quota_config_dir_missing: { origin: "daemon", recoveryLine: true },
  quota_no_windows: { origin: "daemon" },
  quota_not_signed_in: { origin: "daemon", recoveryLine: true },
  quota_shell_environment_failed: { origin: "daemon", detailBlock: true },
  quota_timed_out: { origin: "daemon" },
  quota_unreadable_reply: { origin: "daemon" },
  relocation_failed: { origin: "daemon", unexpected: "classified by its `cause`; one the window has no remedy for is unexpected", detailBlock: true, codeParams: ["path"], variants: { thread_record: { codeParams: ["path"] }, kept_staging: { codeParams: ["path", "kept_in"] } }, variantOf: (params) => params.stage === "thread_record" || params.stage === "kept_staging" ? params.stage : undefined },
  repository_name_missing: { origin: "daemon", recovery: "path", codeParams: ["url"] },
  request_superseded: { origin: "daemon", unexpected: "a browse request given up for a newer one, which the window shows nothing for" },
  request_timed_out: { origin: "client", recovery: "connection" },
  reset_credit_failed: { origin: "daemon", recovery: "resetCredit", detailBlock: true, noSummary: true },
  reset_credit_in_progress: { origin: "daemon", recovery: "none" },
  reset_credit_not_signed_in: { origin: "daemon", recovery: "resetCreditLogin", defaultAccountParams: ["account"], nameParams: ["account"] },
  reset_credit_timed_out: { origin: "daemon", recovery: "none" },
  restore_not_on_branch: { origin: "daemon", recovery: "branch", recoveryLine: true },
  resume_for_console_session: { origin: "daemon", unexpected: "the window never asks to resume a conversation for a console session" },
  resume_id_invalid: { origin: "daemon", recovery: "conversation", codeParams: ["example"], uiParams: { example: EXAMPLE_CONVERSATION_ID } },
  session_already_on_account: { origin: "daemon", recovery: "session" },
  session_already_running: { origin: "daemon", recovery: "wait" },
  session_already_starting: { origin: "daemon", recovery: "wait" },
  session_archived: { origin: "daemon", recovery: "session", recoveryLine: true },
  session_binding_changed: { origin: "daemon", recovery: "ownerMoved" },
  session_branches_not_removed: { origin: "daemon", recovery: "sessionBranches", alertTitle: true },
  session_busy: { origin: "daemon", recovery: "wait" },
  session_changing: { origin: "daemon", recovery: "wait" },
  session_did_not_stop: { origin: "daemon", recovery: "stopProcess", recordIds: ["session"], nameParams: ["session"] },
  session_has_running_sessions: { origin: "daemon", recovery: "stopSessions", recoveryLine: true },
  session_lock_busy: { origin: "daemon", recovery: "wait", recoveryLine: true },
  session_locked: { origin: "daemon", recovery: "unlock", recoveryLine: true },
  session_model_session_archived: { origin: "ui", nameParams: ["title"] },
  session_model_session_gone: { origin: "ui", nameParams: ["title"] },
  session_not_bound: { origin: "daemon", recovery: "session" },
  session_not_bound_to_owner: { origin: "daemon", recovery: "none", recordIds: ["owner"], nameParams: ["owner"] },
  session_not_deletable: { origin: "daemon", recovery: "stopSessions" },
  session_not_running: { origin: "daemon", recovery: "session" },
  session_owner_changing: { origin: "daemon", recovery: "ownerChanging", recordIds: ["owner"], nameParams: ["owner"] },
  session_request_answered: { origin: "daemon", recovery: "session" },
  session_request_not_waiting: { origin: "daemon", recovery: "session", alertTitle: true },
  session_trust_pending: { origin: "daemon", recovery: "trust", recoveryLine: true },
  session_waiting_for_user: { origin: "daemon", recovery: "trust", recoveryLine: true },
  session_working: { origin: "daemon", recovery: "wait" },
  sharing_messages_dropped: { origin: "daemon", recovery: "sharingLostMessages", failureNotice: true, alertTitle: true },
  sharing_resume_failed: { origin: "daemon", recovery: "none", failureNotice: true, alertTitle: true, detailBlock: true, nameParams: ["name"], variants: { running: { nameParams: ["name"] } }, variantOf: (params) => params.still_running === "true" ? "running" : undefined },
  sharing_session_did_not_stop: { origin: "daemon", recovery: "sharingStop", recordIds: ["session"], nameParams: ["session", "name"] },
  sharing_unavailable_on_host: { origin: "daemon", causeOnly: true },
  shell_environment_timeout: { origin: "daemon", recovery: "shell", detailBlock: true },
  source_changed: { origin: "daemon", recovery: "refresh", recoveryLine: true, codeParams: ["path"] },
  // The project's directory, or its place in a worktree, that a request is scoped to: nothing the user typed or
  // picked with Browse, so it is put back rather than corrected. Its `cause` picks a closer remedy
  // (`SOURCE_CAUSE_RECOVERY`).
  source_unavailable: { origin: "daemon", recovery: "sourceGone", detailBlock: true, codeParams: ["path"] },
  // Any request that reads or writes the application's own database can raise it, and so can a background step (a
  // failed switch's restore, a launch), so its `cause` picks the remedy (`STORE_WRITE_CAUSE_RECOVERY`) wherever it
  // comes from, nested or not, and its summary says the application's own data could not be read or written, which no
  // call site's title says. The `recovery` here is the one for the cause `other`, which `classifyError` never reaches
  // for a failure that carries a cause, as every one does.
  store_write_failed: { origin: "daemon", recovery: "appDataOther", detailBlock: true },
  submit_retry_failed: { origin: "daemon", recovery: "none", failureNotice: true, alertTitle: true, recoveryLine: true, detailBlock: true, codeParams: ["enter"], uiParams: { enter: "Enter" } },
  // A switch whose relaunch ended at once leaves the session interrupted on the account it had, resumable as before.
  switch_did_not_come_up: { origin: "daemon", recovery: "switchRelaunch", recordIds: ["session"], nameParams: ["session"] },
  terminal_input_lost: { origin: "daemon", recovery: "none", alertTitle: true, detailBlock: true, recoveryLine: true, recordIds: ["session", "shell_terminal"], variants: { session: { nameParams: ["session"] }, shell: { nameParams: ["shell_terminal"] } }, variantOf: (params) => params.session ? "session" : params.shell_terminal ? "shell" : undefined },
  // One remedy per application the host opens, so each names only the one that failed. `openExternal` is the
  // browser link the UI itself fails to open, raised as an explicit recovery, so no code maps to it.
  terminal_unavailable: { origin: "daemon", recovery: "openTerminal", detailBlock: true, noSummary: true },
  trust_answer_failed: { origin: "daemon", recovery: "trust", failureNotice: true, alertTitle: true, recoveryLine: true, codeParams: ["enter"], uiParams: { enter: "Enter" } },
  trust_directory_too_broad: { origin: "daemon", recovery: "trust", recoveryLine: true, codeParams: ["path"] },
  trust_directory_unreachable: { origin: "daemon", recovery: "trust", recoveryLine: true, codeParams: ["path"] },
  trust_home_unknown: { origin: "daemon", recovery: "trust" },
  trust_not_carried_over: { origin: "daemon", recovery: "none", alertTitle: true, codeParams: ["path", "enter"], uiParams: { enter: "Enter" } },
  trust_not_waiting: { origin: "daemon", recovery: "session" },
  trust_path_not_absolute: { origin: "daemon", recovery: "trust", codeParams: ["path"] },
  typing_blocked: { origin: "daemon", recovery: "none", failureNotice: true, alertTitle: true, recoveryLine: true },
  typing_held: { origin: "daemon", recovery: "none", failureNotice: true, alertTitle: true, recoveryLine: true },
  typing_lost: { origin: "daemon", recovery: "none", failureNotice: true, alertTitle: true },
  unexpected_error: { origin: "ui" },
  unexpected_reply: { origin: "client" },
  unknown_account: { origin: "daemon", recovery: "account", recordIds: ["account"], nameParams: ["account"] },
  unknown_branch: { origin: "daemon", recovery: "branch", codeParams: ["branch"] },
  unknown_commit: { origin: "daemon", recovery: "refresh", codeParams: ["commit"] },
  unknown_console: { origin: "daemon", recovery: "refresh", recordIds: ["console"], nameParams: ["console"] },
  unknown_page: { origin: "daemon", recovery: "refresh", recordIds: ["page"], nameParams: ["page"] },
  unknown_project: { origin: "daemon", recovery: "refresh", recordIds: ["project"], nameParams: ["project"] },
  unknown_session: { origin: "daemon", recovery: "session", recordIds: ["session"], nameParams: ["session"] },
  unknown_shell_terminal: { origin: "daemon", recovery: "refresh", recordIds: ["shell_terminal"], nameParams: ["shell_terminal"] },
  unreadable_request: { origin: "daemon", unexpected: "the window sends only requests of the shapes it knows", detailBlock: true },
  unshared_state_database: { origin: "daemon", codeParams: ["path"], nameParams: ["name"] },
  unsupported_file_type: { origin: "daemon", recovery: "scope", codeParams: ["path"], variants: { unmerged: { codeParams: ["path"] } }, variantOf: (params) => params.file_type === "unmerged" ? "unmerged" : undefined },
  upstream_not_a_remote: { origin: "daemon", recovery: "gitFailure" },
  worktree_unavailable: { origin: "daemon", recovery: "branch" },
} satisfies Record<string, CodeRecord>;

/** Every code the UI has anything to say about, daemon, client and UI-only: the core's and each provider's. */
export const CODES: Readonly<Record<string, CodeRecord>> = { ...CORE_CODES, ...(PROVIDER_CODES satisfies Readonly<Record<string, CodeRecord>>) };

/** A code of the core, which the registry names; a provider's codes are looked up by their strings. */
export type Code = keyof typeof CORE_CODES;

// The codes the client branches on as a group; a single one is named by `CODE`.

/** Codes meaning a launch was refused because the session was already running or already being
 * started. A double click produces them and is not worth showing. */
export const ALREADY_RUNNING_CODES: readonly string[] = ["session_already_running", "session_already_starting"] satisfies Code[];

/** Codes meaning no parent directory can be offered to trust, or it could not be checked in time:
 * nothing was answered, and the dialog stays open. */
export const TRUST_REFUSED_CODES: readonly string[] = [
  "trust_directory_too_broad",
  "trust_path_not_absolute",
  "trust_home_unknown",
  "trust_directory_unreachable",
] satisfies Code[];

/** The codes a failed `confirm_trust` can be answered with that the daemon also broadcasts as a `session_notice`
 * of the same code. */
export const TRUST_FAILURE_CODES: ReadonlySet<string> = new Set<Code>(["trust_answer_failed", "trust_not_carried_over"]);

/** Every code by its own name, for logic that checks for one: `CODE.trust_answer_failed` fails to type check once the
 * code is gone from the registry, where a string would go on comparing quietly. */
export const CODE = Object.fromEntries(Object.keys(CORE_CODES).map((code) => [code, code])) as { readonly [C in Code]: C };

/** The record of `code`, or nothing for a code the UI does not know (a daemon newer than this client). */
export function codeRecord(code: string): CodeRecord | undefined {
  return Object.hasOwn(CODES, code) ? CODES[code] : undefined;
}

/** The prefix of a provider's code and of its messages: `provider.<id>.<code>`. */
const PROVIDER_PREFIX = "provider.";

/** The catalog key of a message worded under `messageCode` (`<code>`, `<code>.<variant>` and the like): a core code's is
 * under `daemon.`, a provider's own code's is its code, which sits in the provider's namespace
 * (`provider.<id>.<code>`) in the catalog the provider's presentation module supplies. */
export function messageKey(messageCode: string): string {
  return messageCode.startsWith(PROVIDER_PREFIX) ? messageCode : `daemon.${messageCode}`;
}

/** The params facets of a message worded under `messageCode`: the code's own, or those of the variant it names
 * (`<code>.<variant>`). A provider's code has two dots of its own (`provider.<id>.<code>`). */
export function messageFacets(messageCode: string): MessageFacets | undefined {
  const parts = messageCode.split(".");
  const length = messageCode.startsWith(PROVIDER_PREFIX) ? 3 : 1;
  if (parts.length <= length) return codeRecord(messageCode);
  return codeRecord(parts.slice(0, length).join("."))?.variants?.[parts.slice(length).join(".")];
}
