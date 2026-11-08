import type { ErrorRecovery } from "../errors/errorRecovery";
import type { Translate } from "../i18n/catalog";
import type { ReplyOf } from "../requests";
import type { Imported } from "../configuration/ImportResult";
import type { Inspection } from "../configuration/importPlan";
import type { AccountMcpInventory, AccountTrashOutcome, McpServerAuthOutcome, Agent, AgentModel, ConversationLocation, DirEntry, GitStatus, HistoryMessage, ImportFailure, ImportItem, ModelsUnavailable, PathLinkKind, PathLinkView, ProcessTree, QuotaUnavailable, ResetCreditOutcome, SessionBranchItem, SessionRecordPresence, UnstoppedReason } from "../protocol";
import type { MessageParams } from "../errors/codedError";
import type { PathLinkRefusedCause } from "../errors/causes";
import type { State } from "../store";
import type { NotifiedBroadcast } from "../store/notifications";
import type { FixtureGit } from "./changeFixtures";
import type { TerminalBehaviour } from "./fakeTerminal";
import type { FixtureError, FixtureFiles } from "./sampleFiles";

/** Feedback the scenario raises once the app is mounted, by the route the real feedback takes to the stack: a
 * broadcast of the daemon, a failure an action raised itself, or a notice the UI words itself. Errors enter the
 * required-acknowledgement alert queue, while notices remain toasts. */
export type NotificationFixture =
  | BroadcastNotificationFixture
  | RaisedNotificationFixture
  | NoticeNotificationFixture;

/** An event the daemon sends unasked, shown as the app shows it (`broadcastNotification`): a failure or a notice carrying raw
 * text becomes an error alert and any other notice a toast. `event.message` is the daemon's English fallback for a code
 * the catalog lacks, as in the real event; the text shown is worded from the catalog. */
export interface BroadcastNotificationFixture {
  kind: "broadcast";
  event: NotifiedBroadcast;
}

/** An error alert an action of the app raises for a failure of its own request (`raiseError`). `title` is the
 * plain name of what failed that the action gives it, worded in the viewing language when a function; a code's own
 * `alertTitle` still wins over it, as it does there. `message` is the fallback for a `code` the catalog lacks; without
 * a `code` the failure is an unexpected one, shown as the unexpected-failure sentence with `message` under its error
 * details. A function words it from the catalog in the viewing language, for a failure the client words itself. */
export interface RaisedNotificationFixture {
  kind: "raised";
  title: string | ((t: Translate) => string);
  message: string | ((t: Translate) => string);
  code?: string;
  params?: MessageParams;
  /** The session the call site knows the failure is about. */
  session?: string;
  /** The remedy the call site gives its error alert in place of its failure's own (`NotificationRequest.recovery`). */
  recovery?: ErrorRecovery;
}

/** A timed notice the UI words itself (`Daemon.showNotice`), worded in the viewing language when a function. */
export interface NoticeNotificationFixture {
  kind: "notice";
  message: string | ((t: Translate) => string);
}

/** One answer of `inspect_account_trash`: an outcome, a failure of the request, or `"pending"` for one never answered. */
export type AccountTrashAnswer = AccountTrashOutcome | FixtureError | "pending";

/** One answer of `read_account_mcp_inventory`: the inventory the daemon read (`ageMinutes` is how long ago, left out for
 * just now; `unavailable` is a reading that could not be taken, with both lists empty), a failure of the request, or
 * `"pending"` for one never answered. */
export type AccountMcpAnswer =
  | (Partial<Pick<AccountMcpInventory, "connectors" | "servers" | "unavailable">> & { ageMinutes?: number })
  | FixtureError
  | "pending";

/** How a sign-in to or sign-out of an MCP server ends: the outcome the CLI came to, a refusal of the request, or `"pending"`
 * for a run that stays under way, which a sign-in's cancel (`cancel_account_mcp_sign_in`) ends as `cancelled`. */
export type McpServerAuthAnswer = McpServerAuthOutcome | FixtureError | "pending";

/** The daemon's process ledger as a scenario sets it, for the Processes panel and the archive
 * confirmation's stop options (`fixtureProcesses.ts` answers `list_processes`, `list_all_processes` (the rail's counts
 * of every project's), `stop_processes`, `keep_processes` and the processes of `inspect_session_branches` from it, the
 * last in `fixtureSessions.ts`).
 * Stopping a tree, a project process included, takes it out of the list the panel reads; keeping a session's tree
 * makes it a kept project process, which stays listed, and archiving a session ends its trees but the ones kept. */
export interface ProcessesFixture {
  /** The trees of running launches, each naming its session. A session that is not running has no
   * process, so its trees are never listed. */
  trees?: (ProcessTree & { session_id: string })[];
  /** The project processes of a project, by project id: trees no session owns, kept ones and ones left behind in a
   * removed directory, which a project's removal stops. A project the state does not have stands in for a removed one,
   * which the daemon leaves out of its counts: such a project's trees are counted by no request. */
  projectProcesses?: Record<string, Omit<ProcessTree, "session_id">[]>;
  /** What `list_processes` and `list_all_processes` answer instead of the trees: `"pending"` is a list that is never
   * answered (the panel's loading line; the rail has no button until it is), an error one the daemon refuses. */
  list?: "pending" | FixtureError;
  /** What `list_processes` answers for a project's own list (the request a removal confirmation makes per project)
   * instead of its trees, by project id: the refusal, while the other projects' lists, and the session lists, are
   * answered as usual. */
  projectList?: Record<string, FixtureError>;
  /** What `list_processes` and `list_all_processes` answer instead of the trees once a second has passed since the
   * first ask of each: the list is answered at first and its next poll (the panel, and the rail's list while it is
   * open, ask again every few seconds) is refused, so the panel shows the trees with the refresh failure over them. */
  listLater?: FixtureError;
  /** What `stop_processes` answers instead of stopping: never answered, refused, or answered with the trees left
   * running for the reason given (`not_stopped` names each one, `ids` leaves them out, and they stay listed). */
  stop?: "pending" | FixtureError | { leavesRunning: UnstoppedReason };
  /** What `keep_processes` answers instead of keeping: never answered, or refused. */
  keep?: "pending" | FixtureError;
}

/** Where one item of a running import stands, as the daemon's last progress report for it says (see
 * `ConfigurationFixture.progress`). */
export type ImportProgressFixture = { item: ImportItem } & (
  | { state: "creating" | "cloning" | "done" | "not_attempted" }
  | { state: "failed"; error: ImportFailure }
);

/** What the platform and the daemon answer to exporting and importing configuration. Exporting needs
 * nothing from a scenario: the document is built from its state, as the daemon builds one. What an
 * inspection or an import answers is the scenario's to give, and has to agree with the scenario's
 * state and steps (`fixtureConfiguration.ts` and the test beside `gallery/fixtures/configuration.ts` hold it
 * to that). */
export interface ConfigurationFixture {
  /** The file the platform's open dialog hands back, `"dismissed"` for a dialog the user closes, or
   * `"pending"` for one that is never answered (a browser too old to report a dismissed picker).
   * Without it, an empty document of the right format. */
  file?: { name: string; text: string } | "dismissed" | "pending";
  /** For a platform that hands the file to a browser's download rather than saving it where the
   * user chose. */
  downloads?: boolean;
  /** For a platform that can neither save nor open a file, so that nothing offers to. */
  unavailable?: boolean;
  /** What `inspect_configuration` answers: the inspection, or the daemon error refusing the
   * document. A scenario that opens a file gives it. */
  inspection?: Omit<Inspection, "type"> | FixtureError;
  /** What `import_configuration` answers: the result, a daemon error refusing the request, or
   * `"pending"` for an import that is never answered (a long clone). A scenario that applies an
   * import gives it. */
  imported?: Omit<Imported, "type"> | FixtureError | "pending";
  /** The progress reports that precede the reply to `import_configuration`: the items that have left `waiting`, as the
   * last report of each says. Every other item the import handles is announced `waiting`, as the daemon announces them
   * all before it starts, and the fixture daemon refuses a list the real order of work cannot have produced (see
   * `importProgressDisagreement`). Give it with `imported: "pending"` to hold the import part-way. Without it the import
   * reports nothing, as a request that carries no `import_id` does. */
  progress?: ImportProgressFixture[];
}

/** What the fixture daemon and the platform around it answer: the state the app starts from and the replies and
 * refusals it is given. `createFixtureDaemon` reads exactly these fields. */
export interface Fixture {
  state?: Partial<State>;
  notifications?: NotificationFixture[];
  /** How a session's terminal behaves: `output` draws scrolling sample output (default), `mouse`
   * draws it from a program with mouse reporting on, `fullscreen` draws a response in the
   * alternate screen, `cursor-selection` draws reversed text beside a block cursor, `paths` draws output naming files and folders (see `pathLinks`), and
   * `refuse` never connects. It does not reach the Terminal panel's shells (`shellTerminal`). */
  terminal?: Exclude<TerminalBehaviour, "shell" | "exited">;
  /** How the Terminal panel's shell terminals behave, as `terminal` does for a session's: `shell`
   * (default) draws a shell at its prompt, and `refuse` is a running shell whose socket never
   * connects. A shell that has exited always draws its last screen and closes. */
  shellTerminal?: Extract<TerminalBehaviour, "shell" | "refuse">;
  /** `"pending"`: `create_shell_terminal` is never answered, so a start stays in flight and the
   * Terminal panel keeps its "Starting terminal…" state. */
  shellTerminalStart?: "pending";
  /** `"pending"`: `close_shell_terminal` is never answered, so a close stays in flight and the tab being closed
   * keeps its dimmed, closing look. */
  shellTerminalClose?: "pending";
  /** A refusal of `create_shell_terminal`, for a start the window could not have seen coming (a folder that is gone
   * or no longer a folder, a place the system denies); left out, a start succeeds, in the folder it asked for. */
  shellTerminalStartError?: FixtureError;
  /** The files and folders that exist for `probe_path_links`, by absolute path: a path the terminal shows, in
   * whatever form, is resolved against the home directory `/Users/dev` or the asking project's folder first. A file
   * in the project or a worktree carries its `view`, where `read_project_file` reads it (a worktree's files are
   * `worktreeFiles`), and is answered `viewer_only`, as the daemon answers every such file: a click on it goes to the
   * viewer and never to `open_path_link`, which refuses it with `path_link_refused` and cause `launchable`. A bare
   * `"file"` is one outside the projects, in a temp root. A folder in the project is a bare `"directory"`, or
   * `{ kind: "directory", view }` when it has a README or index file, which `view` names; every other path is
   * answered as `none`. `{ kind, gone: true }` is a path the probe found but that is gone when `open_path_link` opens it,
   * which the daemon refuses with `file_not_found`. `{ kind, refused: cause }` is one the probe found but
   * `open_path_link` will not open, which the daemon refuses with `path_link_refused` and that `cause`.
   * `{ kind, outside: true }` is one the probe found but that lies outside every root when `open_path_link` opens it (a link
   * that was redirected since), which the daemon refuses with `outside_scope` and `action: "open"`. A click reaches
   * `open_path_link` only for a path that is no file of a project (a folder with no entry file or one whose entry
   * file cannot be read, a path in a temp root) or a folder opened on the host. Cause `unreadable` carries the
   * system's text as `detail`, which the daemon sends only for it. `reveal_path_link` shows the same path in the file
   * manager and answers an entry as `open_path_link` does, except that it accepts a file the viewer shows with an
   * ack, including a `refused: "launchable"` one that has a `view`; a path that is gone, outside or refused for any
   * other cause, a `launchable` one with no `view` (a path in a temp root, a folder in a project) included, is still
   * refused. A package folder, which the daemon opens only by revealing it, has no entry. */
  pathLinks?: Record<
    string,
    | Exclude<PathLinkKind, "none">
    | { view: PathLinkView }
    | { kind: "directory"; view: PathLinkView }
    | { kind: Exclude<PathLinkKind, "none">; gone: true }
    | { kind: Exclude<PathLinkKind, "none">; outside: true }
    | { kind: Exclude<PathLinkKind, "none">; refused: Exclude<PathLinkRefusedCause, "unreadable">; view?: PathLinkView }
    | { kind: Exclude<PathLinkKind, "none">; refused: "unreadable"; detail: string; view?: PathLinkView }
  >;
  /** What `read_project_file` answers for a request that names a worktree, by the worktree's id and
   * then as `files` does by wire path; a worktree not listed has no file. */
  worktreeFiles?: Record<string, FixtureFiles>;
  /** The terminal's automatic reconnect attempts follow one another at once, so a `refuse`d
   * terminal reaches its disconnected state without the backoff's wait. It drives the Terminal
   * panel's shell views' backoff as well as a session's terminal's. */
  terminalRetriesImmediate?: boolean;
  /** What `list_dir` answers per path, `~` being asked as the home directory `/Users/dev`: a listing, or
   * the daemon error that refuses it. A path not listed gets a generic listing of five directories. */
  directories?: Record<string, DirEntry[] | FixtureError>;
  /** What `detect_directory_agent` and `probe_git_remote` answer per path or URL: an agent (or null
   * for none), a daemon error, or `"pending"` for a request that is never answered. One not listed
   * answers with no agent. Local paths exist unless `directory_exists` is false; remote probes omit
   * directory existence. */
  detections?: Record<string, Pick<ReplyOf<"detect_directory_agent">, "agent" | "directory_exists"> | FixtureError | "pending">;
  /** What `list_agent_models` answers per agent: a list, a reason there is none, or `"pending"` for a
   * request that is never answered. An agent not listed gets `SAMPLE_CLAUDE_MODELS` for Claude Code
   * and an empty list for the others. The reply's `default_model` is, for Claude Code, the first listed
   * model known to be usable (`defaultClaudeModel`), as the daemon picks it when no Claude Code
   * configuration names a model, sent with `default_model_source: "best_available"`; the other
   * agents have none. */
  agentModels?: Partial<Record<Agent, { models: AgentModel[]; unavailable?: ModelsUnavailable } | "pending">>;
  /** A refused session launch, such as a shell environment that could not be read. */
  sessionOpenError?: FixtureError;
  /** What `locate_conversation_to_resume` answers for any ID: where the conversation is held, a daemon
   * error refusing the lookup, or `"pending"` for a request that is never answered. Left out, no
   * conversation was found. */
  conversationLocation?: ConversationLocation | FixtureError | "pending";
  /** A runtime refusal after the user confirms archiving; no session changes on failure. */
  sessionArchiveError?: FixtureError;
  /** A coded refusal of releasing a session from its owner after the user confirms; the session stays bound. */
  sessionReleaseError?: FixtureError;
  /** With `sessionReleaseError`: the session another owner takes the moment the refusal fires, as when
   * the binding moved between the check and the write. */
  sessionReleaseMovedTo?: string;
  /** Native session-file inspection per session. Unlisted or missing sessions are unknown;
   * has_conversation does not establish whether files exist. Aggregate presence is present if
   * any inspected session has files, otherwise unknown if any cannot be established. */
  sessionRecords?: Record<string, SessionRecordPresence>;
  /** What `read_session_history` answers per archived session: the messages its record holds (none
   * for an empty history), `"unavailable"` for a record that is missing or unreadable, or `"pending"`
   * for a read that is never answered, or a daemon error refusing the request. A session not listed has
   * an empty history. */
  sessionHistory?: Record<string, HistoryMessage[] | "unavailable" | "pending" | FixtureError>;
  /** Console sessions whose `list_pages` the daemon refuses, by id, with that error; every other one is listed. */
  pageListErrors?: Record<string, FixtureError>;
  /** Sessions whose `resume_session` is never answered, so a resume stays in flight and the terminal
   * keeps its "Resuming session…" cover. Other sessions resume at once. */
  sessionResumes?: Record<string, "pending">;
  /** The branches and worktrees bound to sessions, as `inspect_session_branches` finds them: those
   * whose `session` is among the sessions asked about and still exists. Without it none are bound,
   * so an archive or deletion shows no section about them; `"pending"` is an inspection that is never
   * answered, so Confirm stays pending; an error is an inspection the daemon fails. */
  sessionBranches?: SessionBranchItem[] | "pending" | FixtureError;
  /** The process ledger behind the Processes panel and the archive confirmation's processes. Without it
   * no session has a process. */
  processes?: ProcessesFixture;
  /** What `inspect_project_removal` finds per project: the worktrees and branches its removal with
   * the folder in the Trash takes along, or the error the inspection fails with. A project not
   * listed has none. `{ items, inspection }` is an answer that holds worktrees back (items with a `cause`), carrying the
   * token the removal sends back so that they stay. */
  projectRemovalBranches?: Record<string, SessionBranchItem[] | { items: SessionBranchItem[]; inspection: string } | FixtureError>;
  /** Per-session account-switch refusals or a request kept pending. A failure with `switch_did_not_come_up` leaves the
   * session interrupted on the account it had, any other puts it back as it was. Unlisted sessions switch
   * normally, updating their account and recorded configuration directory. */
  sessionAccountSwitches?: Record<string, FixtureError | "pending">;
  /** Per-session `set_session_model` failures or a request kept pending, ahead of the refusals the fixture
   * daemon works out from the session itself. A running session is shown starting meanwhile; a failure
   * with `model_change_did_not_come_up` leaves it interrupted with the values it had, any other puts it
   * back as it was (the process did not stop). Unlisted sessions take the new values normally. */
  sessionModelChanges?: Record<string, FixtureError | "pending">;
  /** Per-session handoff failures or a request kept pending, ahead of the refusals the fixture
   * daemon works out from the session itself: for one the window could not have seen coming (the
   * daemon finding a session mid-turn that the window still lists as idle). Unlisted sessions are
   * handed off normally. */
  sessionHandoffs?: Record<string, FixtureError | "pending">;
  /** A refusal to save a provider setting (a `set_provider_setting`: an agent's permission approval or its Claude compatibility); successful requests update settings. */
  accountApprovalError?: FixtureError;
  /** A refusal of `update_agent_cli` for what the window could not have seen coming (`agent_cli_update_unsupported` when the
   * `npm` or `brew` that installed the CLI is not on the shell's `PATH`, `agent_cli_update_running` for an update another
   * window started). Left out, an update that the agent's status allows is started. */
  agentCliUpdateError?: FixtureError;
  /** How an `update_agent_cli` that was started ends: `failed` with the code the daemon gives (`agent_cli_update_failed`
   * with the tool's last line as `detail`, `agent_cli_update_timed_out` with none) and the CLI as it was. Left out, the
   * update succeeds and the CLI is at its latest version. */
  agentCliUpdateFailure?: { error: "agent_cli_update_failed" | "agent_cli_update_timed_out"; detail: string | null };
  /** A refusal to save an agent's tags, an `update_settings` that carries `agent_tags`; other settings updates and successful tag saves update settings as usual. */
  agentTagsError?: FixtureError;
  /** How a `set_account_sharing` that is not turned away at once ends. An `account_sharing_failed` error is a change that
   * fails part-way: the choice is recorded, the account stays incomplete and the failure (its items and detail from the
   * error's params) is kept in the account's sharing state until the next change starts. Any other error is a refusal
   * with nothing recorded. The default account, turning sharing on for a directory that is the default account's, and
   * an account that `state.accountSharing` marks as in use or changing are turned away whether or not this is set.
   * Left out, a change is carried out at once. */
  accountSharingError?: FixtureError;
  /** A refusal of `delete_account` that asks for the account's directory to be moved to the Trash (`trash_directory`),
   * as the daemon refuses a directory that must stay or cannot be moved, or an account with running sessions. A removal
   * that does not ask for it is not refused. */
  accountDeleteError?: FixtureError;
  /** What `inspect_account_trash` answers for any account, which the removal confirmation asks each time its Trash
   * option is ticked and again after a refusal of the removal that the directory's state decides: an outcome, a
   * `FixtureError` that fails the request, or `"pending"` for one never answered. A list answers the asks in order, the
   * last one for every ask after it (a refusal that the check then finds, a retry that has not answered). Left out, `ok`:
   * the directory may be moved. */
  accountTrashInspection?: AccountTrashAnswer | readonly AccountTrashAnswer[];
  /** What `read_account_mcp_inventory` answers, for every account or, as a map, per account (the key is the account's id,
   * `null` for an agent's default account; an account the map does not name gets an empty inventory). A list answers an
   * account's reads in order (the dialog's opening read first, then each refresh), the last one for every read after it,
   * so a refresh can answer differently from the first reading, or never. Left out, an empty inventory at once. */
  accountMcpInventory?: AccountMcpAnswer | readonly AccountMcpAnswer[] | ReadonlyMap<string | null, AccountMcpAnswer | readonly AccountMcpAnswer[]>;
  /** What `sign_in_account_mcp_server` and `sign_out_account_mcp_server` answer, for every server or, as a map, per
   * server (the key is the server's name; a server the map does not name succeeds). Left out, both succeed at once and
   * the server's sign-in state in the inventory the scenario stages turns to `signed_in` (a sign-in) or `needs_sign_in`
   * (a sign-out), as the rereading shows it; any other answer leaves the state as it was. */
  accountMcpServerAuth?: McpServerAuthAnswer | ReadonlyMap<string, McpServerAuthAnswer>;
  /** How a login started from Settings ends, a moment after it reaches `waiting` with its page (Grok Build's with its
   * device code); `failed` carries `login_failed`. Left out, a started login stays waiting until it is cancelled. A
   * state shown from the start is set in `state.accountLogins` instead. */
  loginOutcome?: "succeeded" | "failed" | "timed_out";
  /** How `use_reset_credit` ends for any Codex account: the outcome Codex answers with, a refusal as the daemon
   * words one, or `"pending"` for a use never answered, which keeps the confirmation busy. The quota is read again
   * after the answer and after a failure of Codex's run, but not after a refusal that comes before Codex runs
   * (`binary_not_found`, `reset_credit_in_progress`, `daemon_shutting_down`, `unknown_account`,
   * `agent_not_available`, `shell_environment_timeout`). Only `reset` spends a credit: it takes the first one that
   * has not expired off the held reading, windows as they were, before the answer; `already_redeemed` and
   * `nothing_to_reset` spend nothing, and `no_credit` drops the credits at the next read. Left out, a usable held
   * credit is used (`reset`) and an account holding none gets `no_credit`. */
  resetCreditUse?: ResetCreditOutcome | FixtureError | "pending";
  /** The quota reading the fixture daemon takes after it answered a use of a reset credit (or Codex's run failed), as
   * it does after the real one: `after` milliseconds later (default 100), with the windows full after a `reset` and
   * only its time moved after any other answer; with `unavailable`, a read that failed, which keeps the windows and
   * the credits and carries that failure; or `"never"`, so the window keeps waiting for it. */
  resetCreditReading?: { after?: number; unavailable?: QuotaUnavailable } | "never";
  /** A refusal of `start_account_login`, as the daemon words one. */
  loginStartError?: FixtureError;
  /** A refusal of `add_agent`, `remove_agent` and `reorder_agents`, for what the window could not have seen coming
   * (an agent uninstalled since the list was drawn, a second window that changed the order). Left out, they are
   * answered by the added list's own rules. */
  agentListError?: FixtureError;
  /** `"pending"`: `add_agent` is never answered, so an Add stays in flight and the Add agent dialog stays busy. */
  agentAdd?: "pending";
  /** A refusal of `confirm_trust` that asks for the project's parent folder to be trusted (`trust_parent_dir`), as the
   * daemon refuses a folder it cannot vouch for; nothing is answered, so the dialog stays open. A go-ahead for the
   * project alone is not refused, and without this every go-ahead is acknowledged. */
  trustParentDirError?: FixtureError;
  /** A refusal of `answer_session_request`, whether the dialog approves or refuses, as the daemon refuses an answer that
   * cannot be carried out; the request is left as it was. Without it every answer is acknowledged. */
  sessionRequestAnswerError?: FixtureError;
  /** A refused request to open a project's directory in the host's file manager. The fixture
   * otherwise acknowledges the request without opening any host application. */
  projectDirectoryOpenError?: FixtureError;
  /** A refused request to open a project's directory in the host's system terminal (`open_project_terminal`), acknowledged
   * otherwise without opening any host application. */
  projectTerminalOpenError?: FixtureError;
  /** A refused push of a project's current branch (`push_project_branch`), acknowledged otherwise. */
  projectPushError?: FixtureError;
  /** A refusal of `open_project_path` and `reveal_project_path`, which a row of the project pane and the file viewer's
   * header ask for to open a file or folder in the default application or the file manager; acknowledged otherwise. */
  projectPathActionError?: FixtureError;
  /** A refusal of `delete_project` that asks for the folder to be moved to the Trash (`trash_directory`),
   * as the daemon refuses a folder shared with another project or one it cannot move. A removal that
   * does not ask for it is not refused. */
  projectDeleteError?: FixtureError;
  /** A restore refusal or explicit post-restore repository and file tree, keyed by project.
   * Without a result the fixture refuses to acknowledge a destructive change it cannot model. */
  projectRestores?: Record<string, FixtureError | { git: FixtureGit; files: FixtureFiles }>;
  /** Per-project local branch listings kept pending or refused; otherwise the repository's branches. */
  projectBranches?: Record<string, FixtureError | "pending">;
  /** Per-project, per-branch switch refusals or explicit post-switch repository, files and badge.
   * A switch without an outcome is refused rather than acknowledged without modeling its effect. */
  projectBranchSwitches?: Record<string, Record<string, FixtureError | { git: FixtureGit; files: FixtureFiles; status: GitStatus }>>;
  /** The files every project's browser shows (`list_project_dir`, `read_project_file`); without it,
   * `SAMPLE_FILES`. */
  files?: FixtureFiles;
  /** What a file search of the project browser (`search_project_files`) is refused with, in place of its matches. */
  searchError?: FixtureError;
  /** Per-project file trees for comparing project switches; entries fall back to `files`. */
  filesByProject?: Record<string, FixtureFiles>;
  /** The repository every project's Git mode shows (`get_project_source`, `list_project_changes`,
   * `read_project_change`); without it, `SAMPLE_GIT`; `null` for a project in no repository. */
  git?: FixtureGit | null;
  configuration?: ConfigurationFixture;
}
