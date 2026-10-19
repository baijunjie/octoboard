import type { Language } from "../i18n/languages";
import type { Imported } from "../configuration/ImportResult";
import type { Inspection } from "../configuration/ImportReview";
import type { MacosPermissionsStatus } from "../platform";
import type { Agent, AgentModel, DirEntry, GitStatus, MessageParams, ModelsUnavailable, PathLinkKind, PathLinkView, SessionBranchItem, SessionRecordsPresence } from "../protocol";
import type { FixtureError, FixtureFiles } from "./fixtures/projectFiles";
import type { GroupTitle } from "./fixtures/groups";
import type { FixtureGit } from "./fixtures/changeFixtures";
import type { ViewerSubject } from "../viewer/content";
import type { MarkdownStyle } from "../viewer/markdownStyle";
import type { State, ToastRequest } from "../store";
import type { TerminalBehaviour } from "./fakeTerminal";
import type { Ui } from "./interact";

/** The user preferences a scenario starts from, each persisted in `localStorage` by the module the
 * key names. Anything not listed is unset, so the app's own default applies. */
export interface Preferences {
  /** The docked sidebar is shown (default) or hidden by the user. */
  sidebarVisible?: boolean;
  asideVisible?: boolean;
  /** Chosen docked widths in pixels. */
  sidebarWidth?: number;
  asideWidth?: number;
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
}

/** What the window shows in place of the app, for the screens that have no daemon behind them. A
 * crash shows `stack` as the error's stack, which is otherwise the browser's own and so as long as
 * the engine makes it; it is what gives the stack block a length the scenario chooses. */
export type Startup = { kind: "noAddress" } | { kind: "daemonFailed"; error: string } | { kind: "crash"; stack?: string };

/** Feedback the scenario raises once the app is mounted: errors enter the required-acknowledgement
 * alert queue, while notices remain toasts. With a `code` the text is worded from the
 * catalog as the daemon's own errors and notices are, with `message` as the fallback for a code the
 * catalog lacks; without one `message` is shown as is, like a client-side error. */
export interface ToastFixture {
  kind: ToastRequest["kind"];
  message: string;
  code?: string;
  params?: MessageParams;
  session?: string;
}

/** An interaction run once the app is mounted, to reach state that lives in a component (a dialog,
 * the archive view, which session is selected) rather than in the store. */
export type Step = (ui: Ui) => Promise<void>;

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

export interface Scenario {
  id: string;
  /** The heading it is listed under. */
  group: GroupTitle;
  /** Deliberately classified; empty, loading and constrained layouts can be normal. */
  category: "normal" | "exceptional";
  title: string;
  /** Said under the title in the gallery; what to look at. */
  description?: string;
  /** The window width the gallery page opens it at, over a width chosen for an earlier scenario; the
   * `docked` breakpoint is 1168. Left out for `DEFAULT_WIDTH`; see "The UI state gallery" in
   * `packages/ui/README.md` for when a scenario earns a width of its own. */
  width?: number;
  /** The window height the gallery page opens it at, as `width` is for the width; left out for a
   * window that fits the page. For a scenario that rests on a window too short for its dialog,
   * whose body then scrolls: that cannot be left to the height of whatever browser opens it. */
  height?: number;
  /** Shown in place of the app; without it the real `App` is rendered over `state`. */
  startup?: Startup;
  /** Shows the real code wrappers against the alert tones instead of the application. */
  codePalette?: boolean;
  state?: Partial<State>;
  preferences?: Preferences;
  toasts?: ToastFixture[];
  /** How a session's terminal behaves: `output` draws scrolling sample output (default),
   * `fullscreen` draws a response in the alternate screen, `paths` draws output naming files and
   * folders (see `pathLinks`), and `refuse` never connects. */
  terminal?: TerminalBehaviour;
  /** The files and folders that exist for `probe_path_links`, by absolute path: a path the terminal
   * shows, in whatever form, is resolved against the home directory `/Users/dev` or the asking
   * project's folder first. A file in the project or a worktree carries its `view`, where
   * `read_project_file` reads it (a worktree's files are `worktreeFiles`), and `viewerOnly` when the
   * system could launch it; every other path is answered as `none`. */
  pathLinks?: Record<string, Exclude<PathLinkKind, "none"> | { view: PathLinkView; viewerOnly?: true }>;
  /** What `read_project_file` answers for a request that names a worktree, by the worktree's id and
   * then as `files` does by wire path; a worktree not listed has no file. */
  worktreeFiles?: Record<string, FixtureFiles>;
  /** The terminal's automatic reconnect attempts follow one another at once, so a `refuse`d
   * terminal reaches its disconnected state without the backoff's wait. */
  terminalRetriesImmediate?: boolean;
  /** What `list_dir` answers per path, `~` being asked as the home directory `/Users/dev`; a path not
   * listed gets a generic listing of five directories. */
  directories?: Record<string, DirEntry[]>;
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
  /** A runtime refusal after the user confirms archiving; no session changes on failure. */
  sessionArchiveError?: FixtureError;
  /** Native session-file inspection per session. Unlisted or missing sessions are unknown;
   * has_conversation does not establish whether files exist. Aggregate presence is present if
   * any inspected session has files, otherwise unknown if any cannot be established. */
  sessionRecords?: Record<string, SessionRecordsPresence>;
  /** The branches and worktrees bound to sessions, as `inspect_session_branches` finds them: those
   * whose `session` is among the sessions asked about and still exists. Without it none are bound,
   * so an archive or deletion shows no section about them; `"pending"` is an inspection that is never
   * answered, so Confirm stays pending; an error is an inspection the daemon fails. */
  sessionBranches?: SessionBranchItem[] | "pending" | FixtureError;
  /** What `inspect_project_removal` finds per project: the worktrees and branches its removal with
   * the folder in the Trash takes along, or the error the inspection fails with. A project not
   * listed has none. */
  projectRemovalBranches?: Record<string, SessionBranchItem[] | FixtureError>;
  /** Per-session account-switch refusals or a request kept pending. Unlisted sessions switch
   * normally, updating their account and recorded configuration directory. */
  sessionAccountSwitches?: Record<string, FixtureError | "pending">;
  /** Per-session handoff failures or a request kept pending, ahead of the refusals the fixture
   * daemon works out from the session itself: for one the window could not have seen coming (the
   * daemon finding a session mid-turn that the window still lists as idle). Unlisted sessions are
   * handed off normally. */
  sessionHandoffs?: Record<string, FixtureError | "pending">;
  /** A refusal to save an account's approval mode, Codex's or Claude Code's; successful requests update settings. */
  accountApprovalError?: FixtureError;
  /** A refused request to open a project's directory in the host's file manager. The fixture
   * otherwise acknowledges the request without opening any host application. */
  projectDirectoryOpenError?: FixtureError;
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
   * answer stands, which is the section's "only in the macOS app" line. A stub `request` always
   * answers that the permission was not granted, which is how a scenario reaches the row's System
   * Settings guidance — nothing in a browser can raise a real prompt. */
  macosPermissions?: MacosPermissionsStatus | "unreadable";
  /** The platform offers the desktop app's run-in-background capability, so the Settings switch for
   * it is shown; the browser platform has none. */
  backgroundRun?: boolean;
  /** Stub external links without opening a host browser; an error models a refused desktop opener. */
  externalLinks?: "available" | { error: string };
  configuration?: ConfigurationFixture;
  /** `"unavailable"` for a platform with no clipboard; `text` gives a private, mutable clipboard
   * for interactive copy and paste previews. Without either, the browser's clipboard stands. */
  clipboard?: "unavailable" | { text: string };
  steps?: Step[];
  /** Renders the file viewer over the subjects `subjects` builds, in place of the app; built only
   * when the scenario opens. `loadDelay` shows each one's loading state for that many milliseconds
   * first. */
  viewer?: { subjects: (language: Language) => Promise<ViewerSubject[]>; loadDelay?: number };
}
