import type { ThirdPartyLicenses } from "../about/thirdPartyLicenses";
import type { ErrorRecovery } from "../errorRecovery";
import type { Translate } from "../i18n/catalog";
import type { Language } from "../i18n/languages";
import type { Imported } from "../configuration/ImportResult";
import type { Inspection } from "../configuration/ImportReview";
import type { AvailableUpdate, MacosPermissionsStatus, UpdateChannel, UpdateProgress } from "../platform";
import type { OwnerKind, PanelKind } from "../panels/stackState";
import type { Agent, AgentModel, ConversationLocation, DirEntry, GitStatus, HistoryMessage, MessageParams, ModelsUnavailable, PathLinkKind, PathLinkRefusedCause, PathLinkView, SessionBranchItem, SessionRecordsPresence } from "../protocol";
import type { FixtureError, FixtureFiles } from "./fixtures/projectFiles";
import type { GroupTitle } from "./fixtures/groups";
import type { FixtureGit } from "./fixtures/changeFixtures";
import type { ViewerSubject } from "../viewer/content";
import type { MarkdownStyle } from "../viewer/markdownStyle";
import type { Overrides } from "../shortcuts/store";
import type { State, ToastRequest } from "../store";
import type { TerminalBehaviour } from "./fakeTerminal";
import type { Ui } from "./interact";

/** The user preferences a scenario starts from, each persisted in `localStorage` by the module the
 * key names. Anything not listed is unset, so the app's own default applies. */
export interface Preferences {
  /** The docked sidebar is shown (default) or hidden by the user. */
  sidebarVisible?: boolean;
  /** Chosen docked widths in pixels. */
  sidebarWidth?: number;
  asideWidth?: number;
  /** The right pane's panels per owner (a project, a console session's report): which kinds of panel
   * are open (none open is no pane) and their shares of its height. An owner left out has every one of
   * its kinds open, the app's default. */
  panelStacks?: Partial<Record<OwnerKind, { open: PanelKind[]; shares: Record<string, number> }>>;
  /** The project sessions, by id, whose Report panel the user has closed; a session left out has it open. */
  closedReportPanels?: string[];
  /** The console the sidebar shows, and what is in focus mode. */
  sidebarConsole?: string;
  sidebarFocus?: `project:${string}` | `consoleSession:${string}`;
  /** Include console-bound sessions as cards in project focus; off by default. */
  projectFocusIncludeConsoleSessions?: boolean;
  /** How the Git mode's change lists show their changes. */
  changeLayout?: "flat" | "tree";
  /** What each project's browser is left at, by project id: the mode it opens in and whether the Git
   * mode lists what is committed by commit. */
  projectBrowsers?: Record<string, { mode: "files" | "git"; byCommit?: boolean }>;
  /** How a rendered Markdown document is styled; Tailwind, the original look, when unset. */
  markdownStyle?: MarkdownStyle;
  /** Closing the window keeps the app running in the background; off by default. */
  runInBackground?: boolean;
  /** The release channel the application updates from; beta when unset. */
  updateChannel?: UpdateChannel;
  /** The keyboard shortcuts the user has changed, by command; the others keep their defaults. */
  keyboardShortcuts?: Overrides;
}

/** What the window shows in place of the app, for the screens that have no daemon behind them. A
 * crash shows `stack` as the error's stack, which is otherwise the browser's own and so as long as
 * the engine makes it; it is what gives the stack block a length the scenario chooses. */
export type Startup = { kind: "noAddress" } | { kind: "daemonFailed"; error: string } | { kind: "crash"; stack?: string };

/** Feedback the scenario raises once the app is mounted: errors enter the required-acknowledgement
 * alert queue, while notices remain toasts. With a `code` the text is worded from the
 * catalog as the daemon's own errors and notices are, with `message` as the fallback for a code the
 * catalog lacks; without one `message` is shown as is, like a client-side error. A `message` given as a function is
 * worded from the catalog in the viewing language when the toast is raised, for a failure the client words itself. */
export interface ToastFixture {
  /** `"session_notice"` is a notice the daemon sends about a session (needs a `code`): the app routes it as it does
   * the real one (`noticeKind`), a failure or a notice carrying raw text to an error alert and any other to a toast. */
  kind: ToastRequest["kind"] | "session_notice";
  message: string | ((t: Translate) => string);
  /** The plain name of what failed that the action raising the real error gives it (`ToastRequest.title`), worded in
   * the viewing language when a function; a code's own `alertTitle` still wins over it, as it does there. Left out for
   * an error the daemon broadcasts, which no action names. */
  title?: string | ((t: Translate) => string);
  /** The remedy a call site gives its error alert in place of its failure's own (`ToastRequest.recovery`), as the
   * action that raises the real error passes it. */
  recovery?: ErrorRecovery;
  code?: string;
  params?: MessageParams;
  session?: string;
}

/** An interaction run once the app is mounted, to reach state that lives in a component (a dialog,
 * the archive view, which session is selected) rather than in the store. */
export type Step = (ui: Ui) => Promise<void>;

/** A change the daemon makes while the window already shows the scenario's view (an account added by another
 * window, a terminal closed there), for the motion and focus that only a live change shows: the view never
 * changes by itself, the viewer presses `label` among the "Live changes" buttons above the window. Its `steps`
 * are `daemonEvents` steps, which apply the same events the daemon would send. Changes that undo each
 * other let the viewer watch both directions; the undo restores the data, not necessarily what the app selected
 * meanwhile (a reopened terminal is not selected). A change that is already in effect does nothing. */
export interface LiveChange {
  /** Names the change in the frame URL's `live` parameter, which applies it after the steps (the sweep does). */
  id: string;
  /** The button's text, in the gallery's own English. */
  label: string;
  steps: Step[];
}

/** The document event the gallery shell dispatches into a scenario's frame to apply a live change; its detail is
 * the change's `id`. */
export const LIVE_CHANGE_EVENT = "gallery:live-change";

/** The window the gallery page opens a scenario at when the scenario names none and no width has been
 * chosen above the window. */
export const DEFAULT_WIDTH = 1440;

/** The shortest window the gallery page's iframe takes (its `min-height` in `gallery.html`); a
 * scenario's `height` below it would not be the height it is shown at. */
export const MIN_HEIGHT = 400;

/** What the platform and the daemon answer to exporting and importing configuration. Exporting needs
 * nothing from a scenario: the document is built from its state, as the daemon builds one. What an
 * inspection or an import answers is the scenario's to give, and has to agree with the scenario's
 * state and steps (`fixtureConfiguration.ts` and the test beside `fixtures/configuration.ts` hold it
 * to that). */
export interface ConfigurationFixture {
  /** The file the platform's open dialog hands back, `"dismissed"` for a dialog the user closes, or
   * `"pending"` for one that is never answered (a browser too old to report a dismissed picker).
   * Without it, an empty document of the right format. */
  file?: { name: string; text: string } | "dismissed" | "pending";
  /** What the open dialog fails with, for a file that could not be read: the system's own text, or
   * the kind of failure the desktop shell names. */
  readError?: string | { kind: "too_large" | "not_text" };
  /** What the save dialog fails with, for a file that could not be written. */
  saveError?: string;
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
}

/** What a check answers: a newer version, `"upToDate"`, `"pending"` for a check that is never answered,
 * `"installed"` for the shell saying an update is already installed (which this window never saw), or the updater's
 * own failure text. */
export type UpdaterCheck = AvailableUpdate | "upToDate" | "pending" | "installed" | { error: string };

/** What the desktop shell's updater answers, for a window that can update itself: the Settings rows and the
 * update notice exist only with one. */
export interface UpdaterFixture {
  /** The version running. */
  currentVersion?: string;
  /** What a check answers, `"upToDate"` by default: one answer for either channel, or one per channel (a channel
   * left out answers `"upToDate"`). */
  check?: UpdaterCheck | Partial<Record<UpdateChannel, UpdaterCheck>>;
  /** Whether a check has already run when the window opens, as the automatic one does shortly after launch, so
   * its result (a notice, a Settings row) is on screen without a press. Without it the window has not looked
   * yet, and stays so: the gallery's provider makes no automatic checks of its own. */
  checked?: boolean;
  /** What an install does: `progress` is reported first, then it ends as `ends` says, by default with the update
   * in place; `"pending"` is a download that never finishes and `"gone"` the shell having nothing to install any more. */
  install?: { progress?: UpdateProgress; ends?: "pending" | "gone" | { error: string } };
}

export interface Scenario {
  id: string;
  /** The heading it is listed under. */
  group: GroupTitle;
  /** Deliberately classified; empty, loading and constrained layouts can be normal. */
  category: "normal" | "exceptional";
  title: string;
  /** Said under the title in the gallery; what to look at. */
  description?: string;
  /** The window width the gallery page opens it at, over a width chosen for an earlier scenario; the `docked`
   * breakpoint is 1168. Left out for `DEFAULT_WIDTH`. Only for a target that can be reached in a band of widths the
   * default is outside of, where the steps cannot get there at whatever width the page's control picks (a swipe, the
   * open drawer card itself): a width inside that band, and the title or description says what it buys. Never for the
   * same state seen at another width, which the control shows; see "The UI state gallery" in
   * `packages/ui/README.md`. */
  width?: number;
  /** The narrowest window its target exists in: a width chosen with the page's control, or by the sweep's `--width`,
   * below it is raised to it. For what exists only in a docked window (a card floated in by hovering a toggle, a
   * pane's width handle, Maximize panel, the hidden docked sidebar), which no width chosen below it shows. A
   * comment beside it says why. A scenario carries this or `width`, not both. */
  minWidth?: number;
  /** The window height the gallery page opens it at, as `width` is for the width; left out for a
   * window that fits the page. For a scenario that rests on a window too short for its dialog,
   * whose body then scrolls: that cannot be left to the height of whatever browser opens it. */
  height?: number;
  /** The shortcut modifier (⌘ / Ctrl) is held down when it opens, for a view that exists only while it is: the
   * quick-create form of the new-session buttons. Not a copy of another scenario made to see it held: the
   * shell's Modifier control holds it for any scenario. The page's own choice (`held` in the URL) stands over
   * it, and like a width stays on scenarios that carry none and is dropped on picking one that does. */
  held?: boolean;
  /** Shown in place of the app; without it the real `App` is rendered over `state`. */
  startup?: Startup;
  /** Shows the real code wrappers against the alert tones instead of the application. */
  codePalette?: boolean;
  /** Shows the application's semantic colours as tokens, as real components and on each surface, instead of the application. */
  semanticColours?: boolean;
  state?: Partial<State>;
  preferences?: Preferences;
  toasts?: ToastFixture[];
  /** How a session's terminal behaves: `output` draws scrolling sample output (default), `mouse`
   * draws it from a program with mouse reporting on, `fullscreen` draws a response in the
   * alternate screen, `paths` draws output naming files and folders (see `pathLinks`), and
   * `refuse` never connects. It does not reach the Terminal panel's shells (`shellTerminal`). */
  terminal?: Exclude<TerminalBehaviour, "shell" | "exited">;
  /** How the Terminal panel's shell terminals behave, as `terminal` does for a session's: `shell`
   * (default) draws a shell at its prompt, and `refuse` is a running shell whose socket never
   * connects. A shell that has exited always draws its last screen and closes. */
  shellTerminal?: Extract<TerminalBehaviour, "shell" | "refuse">;
  /** `"pending"`: `create_shell_terminal` is never answered, so a start stays in flight and the
   * Terminal panel keeps its "Starting terminal…" state. */
  shellTerminalStart?: "pending";
  /** A refusal of `create_shell_terminal`, for a start the window could not have seen coming (a folder that is gone
   * or no longer a folder, a place the system denies); left out, a start succeeds, in the folder it asked for. */
  shellTerminalStartError?: FixtureError;
  /** The desktop shell asks the window to quit (Cmd+Q) once the steps have run: with a busy session or shell terminal
   * the confirmation they hold the quit up with is on screen, and with none the quit goes straight to the shell. The
   * platform has no exit capability otherwise; what the shell does with the quit is nothing (confirming ends nothing)
   * unless `exitFailure` fails it. */
  quitRequested?: boolean;
  /** The text the desktop shell fails the quit or restart with, so a quit (`quitRequested`) or a restart the steps press
   * ends in the failure alert instead of nothing. The platform has the exit capability, as with `quitRequested`. */
  exitFailure?: string;
  /** The daemon exits by itself, as the desktop shell reports with this text, once the steps have run, so the alert
   * saying so is on screen. The platform has the exit capability, as with `quitRequested`. */
  daemonExited?: string;
  /** The files and folders that exist for `probe_path_links`, by absolute path: a path the terminal
   * shows, in whatever form, is resolved against the home directory `/Users/dev` or the asking
   * project's folder first. A file in the project or a worktree carries its `view`, where
   * `read_project_file` reads it (a worktree's files are `worktreeFiles`), and `viewerOnly` when the
   * system could launch it. A folder in the project is a bare `"directory"`, or `{ kind:
   * "directory", view }` when it has a README or index file, which `view` names; every other path
   * is answered as `none`. `{ kind, gone: true }` is a path the probe found but that is gone when
   * `open_path_link` opens it, which the daemon refuses with `file_not_found`. `{ kind, refused: cause }` is one the
   * probe found but `open_path_link` will not open, which the daemon refuses with `path_link_refused` and that
   * `cause`. `{ kind, outside: true }` is one the probe found but that lies outside every root when `open_path_link`
   * opens it (a link that was redirected since), which the daemon refuses with `outside_scope` and `action: "open"`. A
   * click reaches `open_path_link` only for a path with no `view` (a folder with no entry file, a path in a temp
   * root), a folder opened on the host, or a file the viewer cannot show, so a refused file in the project carries
   * the `view` of a binary one. Cause `unreadable` carries the system's text as `detail`, which the daemon sends only
   * for it. */
  pathLinks?: Record<
    string,
    | Exclude<PathLinkKind, "none">
    | { view: PathLinkView; viewerOnly?: true }
    | { kind: "directory"; view: PathLinkView }
    | { kind: Exclude<PathLinkKind, "none">; gone: true }
    | { kind: Exclude<PathLinkKind, "none">; outside: true; view?: PathLinkView }
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
   * answers with no agent. */
  detections?: Record<string, { agent: Agent | null } | FixtureError | "pending">;
  /** What `list_agent_models` answers per agent: a list, a reason there is none, or `"pending"` for a
   * request that is never answered. An agent not listed gets `SAMPLE_CLAUDE_MODELS` for Claude Code
   * and an empty list for the others. */
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
  sessionRecords?: Record<string, SessionRecordsPresence>;
  /** What `read_session_history` answers per archived session: the messages its record holds (none
   * for an empty history), `"unavailable"` for a record that is missing or unreadable, or `"pending"`
   * for a read that is never answered, or a daemon error refusing the request. A session not listed has
   * an empty history. */
  sessionHistory?: Record<string, HistoryMessage[] | "unavailable" | "pending" | FixtureError>;
  /** Console sessions whose `list_pages` the daemon refuses, by id, with that error; every other one is listed. */
  pageListErrors?: Record<string, FixtureError>;
  /** Sessions whose `resume_session` is never answered, so a Reopen stays in flight and the terminal
   * keeps its "Resuming session…" cover. Other sessions reopen at once. */
  sessionResumes?: Record<string, "pending">;
  /** The branches and worktrees bound to sessions, as `inspect_session_branches` finds them: those
   * whose `session` is among the sessions asked about and still exists. Without it none are bound,
   * so an archive or deletion shows no section about them; `"pending"` is an inspection that is never
   * answered, so Confirm stays pending; an error is an inspection the daemon fails. */
  sessionBranches?: SessionBranchItem[] | "pending" | FixtureError;
  /** What `inspect_project_removal` finds per project: the worktrees and branches its removal with
   * the folder in the Trash takes along, or the error the inspection fails with. A project not
   * listed has none. `{ items, inspection }` is an answer that holds worktrees back (items with a `cause`), carrying the
   * token the removal sends back so that they stay. */
  projectRemovalBranches?: Record<string, SessionBranchItem[] | { items: SessionBranchItem[]; inspection: string } | FixtureError>;
  /** Per-session account-switch refusals or a request kept pending. Unlisted sessions switch
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
  /** A refusal to save an agent's permission approval, Codex's reviewer or Claude Code's mode; successful requests update settings. */
  accountApprovalError?: FixtureError;
  /** A refusal to save an agent's Claude compatibility switch (an `update_settings` carrying `claude_compatibility`);
   * successful requests update settings. */
  claudeCompatibilityError?: FixtureError;
  /** How a `set_account_sharing` that is not turned away at once ends. An `account_sharing_failed` error is a change that
   * fails part-way: the choice is recorded, the account stays incomplete and the failure (its items and detail from the
   * error's params) is kept in the account's sharing state until the next change starts. Any other error is a refusal
   * with nothing recorded. The default account, turning sharing on for a directory that is the default account's, and
   * an account that `state.accountSharing` marks as in use or changing are turned away whether or not this is set.
   * Left out, a change is carried out at once. */
  accountSharingError?: FixtureError;
  /** How a login started from Settings ends, a moment after it reaches `waiting` with its page (Grok Build's with its
   * device code); `failed` carries `login_failed`. Left out, a started login stays waiting until it is cancelled. A
   * state shown from the start is set in `state.accountLogins` instead. */
  loginOutcome?: "succeeded" | "failed" | "timed_out";
  /** A refusal of `start_account_login`, as the daemon words one. */
  loginStartError?: FixtureError;
  /** A refusal of `add_agent`, `remove_agent` and `reorder_agents`, for what the window could not have seen coming
   * (an agent uninstalled since the list was drawn, a second window that changed the order). Left out, they are
   * answered by the added list's own rules. */
  agentListError?: FixtureError;
  /** A refusal of `confirm_trust` that asks for the project's parent folder to be trusted (`trust_parent_dir`), as the
   * daemon refuses a folder it cannot vouch for; nothing is answered, so the dialog stays open. A go-ahead for the
   * project alone is not refused, and without this every go-ahead is acknowledged. */
  trustParentDirError?: FixtureError;
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
  /** Per-project file trees for comparing project switches; entries fall back to `files`. */
  filesByProject?: Record<string, FixtureFiles>;
  /** The repository every project's Git mode shows (`get_project_source`, `list_project_changes`,
   * `read_project_change`); without it, `SAMPLE_GIT`; `null` for a project in no repository. */
  git?: FixtureGit | null;
  /** What the shell answers about the macOS privacy grants, for the Permissions section.
   * `"unreadable"` is a shell that refuses the read; without this the browser platform's own
   * answer stands, which is the section's "only in the macOS app" line. A stub `request` answers
   * that the permission was not granted (unless the scenario gives a `macosRequestError`), which is
   * how a scenario reaches the row's System Settings guidance — nothing in a browser can raise a
   * real prompt. */
  macosPermissions?: MacosPermissionsStatus | "unreadable";
  /** With `macosPermissions`, the reason the system gives for not opening the System Settings pane a row's
   * Open System Settings points at; without it the pane opens (nothing is shown). */
  macosSettingsOpenError?: string;
  /** With `macosPermissions`, the reason a row's Grant request call fails with, so the request itself is
   * rejected; without it the request is answered as not granted. */
  macosRequestError?: string;
  /** The platform has what the desktop app's window has, so a view that lists or uses it shows the app's: the
   * window chrome (the `chrome=1` of the frame URL, which a view that exists only in the app cannot leave to
   * the address) and the native menu bar. */
  desktopApp?: boolean;
  /** The platform offers the desktop app's run-in-background capability, so the Settings switch for
   * it is shown; the browser platform has none. */
  backgroundRun?: boolean;
  /** The platform has the macOS app's notifications: the system manages them, so the app keeps no
   * switch of its own and the Settings row links to System Settings instead. Nothing is shown or
   * opened; `{ openError }` is the reason the system gives for not opening the pane. */
  systemNotifications?: boolean | { openError: string };
  /** Opening an external link fails with this text, as a refused desktop opener does. Without it the browser
   * platform's own opener stands and opens the address in a new tab. */
  externalLinkError?: string;
  configuration?: ConfigurationFixture;
  /** The platform can update the application, as the desktop app does: the Settings rows for the version and
   * channel, and the update notice once a check finds a version. It also has the exit capability that a restart
   * goes through, whose confirmation ends nothing. */
  updater?: UpdaterFixture;
  /** `"unavailable"` for a platform with no clipboard; `text` gives a private, mutable clipboard
   * for interactive copy and paste previews. Without either, the browser's clipboard stands. */
  clipboard?: "unavailable" | { text: string };
  /** Shows the third-party licenses dialog alone, in place of the app, over this list or in its `"loading"` or
   * `"failed"` state, or failing its first load and answering with this list from Try again (`{ failsOnce }`). The
   * dialog reads the list the build generated, which differs between checkouts and which Settings > About cannot be
   * given another of, so it is mounted directly with a loader that answers with this. */
  licensesDialog?: ThirdPartyLicenses | "loading" | "failed" | { failsOnce: ThirdPartyLicenses };
  steps?: Step[];
  /** Changes the viewer can make to what the window shows once it is up, as buttons above it. */
  liveChanges?: LiveChange[];
  /** Renders the file viewer over the subjects `subjects` builds, in place of the app; built only
   * when the scenario opens. `loadDelay` shows each one's loading state for that many milliseconds
   * first. */
  viewer?: { subjects: (language: Language) => Promise<ViewerSubject[]>; loadDelay?: number };
}
