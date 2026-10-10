import type { Agent, DirEntry, MessageParams } from "../protocol";
import type { FixtureError, FixtureFiles } from "./fixtures/projectFiles";
import type { GroupTitle } from "./fixtures/groups";
import type { FixtureGit } from "./fixtures/projectGit";
import type { ViewerSubject } from "../viewer/content";
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
}

/** What the window shows in place of the app, for the screens that have no daemon behind them. */
export type Startup = { kind: "noAddress" } | { kind: "daemonFailed"; error: string } | { kind: "crash" };

/** A toast the scenario raises once the app is mounted. With a `code` the text is worded from the
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

export interface Scenario {
  id: string;
  /** The heading it is listed under. */
  group: GroupTitle;
  title: string;
  /** Said under the title in the gallery; what to look at. */
  description?: string;
  /** The window width it opens at unless the URL says otherwise; the `docked` breakpoint is 1148. */
  width?: number;
  /** Shown in place of the app; without it the real `App` is rendered over `state`. */
  startup?: Startup;
  state?: Partial<State>;
  preferences?: Preferences;
  toasts?: ToastFixture[];
  /** How a session's terminal behaves: `output` connects and draws a sample (default), `refuse`
   * never connects, which is a terminal in trouble. */
  terminal?: TerminalBehaviour;
  /** The terminal's automatic reconnect attempts follow one another at once, so a `refuse`d
   * terminal reaches its disconnected state without the backoff's wait. */
  terminalRetriesImmediate?: boolean;
  /** What `list_dir` answers per path; a path not listed gets a generic listing. */
  directories?: Record<string, DirEntry[]>;
  /** What `detect_directory_agent` and `probe_git_remote` answer per path or URL: an agent (or null
   * for none), a daemon error, or `"pending"` for a request that is never answered. One not listed
   * answers with no agent. */
  detections?: Record<string, { agent: Agent | null } | FixtureError | "pending">;
  /** The files every project's browser shows (`list_project_dir`, `read_project_file`); without it,
   * `SAMPLE_FILES`. */
  files?: FixtureFiles;
  /** The repository every project's Git mode shows (`get_project_source`, `list_project_changes`,
   * `read_project_change`); without it, `SAMPLE_GIT`; `null` for a project in no repository. */
  git?: FixtureGit | null;
  steps?: Step[];
  /** Renders the file viewer over the subjects `subjects` builds, in place of the app; built only
   * when the scenario opens. `loadDelay` shows each one's loading state for that many milliseconds
   * first. */
  viewer?: { subjects: () => Promise<ViewerSubject[]>; loadDelay?: number };
}
