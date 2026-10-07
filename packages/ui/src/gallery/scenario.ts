import type { DirEntry, MessageParams } from "../protocol";
import type { State, ToastRequest } from "../store";
import type { TerminalBehaviour } from "./fakeTerminal";
import type { Ui } from "./interact";

/** The user preferences a scenario starts from, each persisted in `localStorage` by the module the
 * key names. Anything not listed is unset, so the app's own default applies. */
export interface Preferences {
  /** The docked sidebar is shown (default) or hidden by the user. */
  sidebarVisible?: boolean;
  reportVisible?: boolean;
  /** Chosen docked widths in pixels. */
  sidebarWidth?: number;
  reportWidth?: number;
  /** The console the sidebar shows, and the project in focus mode. */
  sidebarConsole?: string;
  sidebarFocusProject?: string;
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
  group: string;
  title: string;
  /** Said under the title in the gallery; what to look at. */
  description?: string;
  /** The window width it opens at unless the URL says otherwise; the `docked` breakpoint is 1100. */
  width?: number;
  /** Shown in place of the app; without it the real `App` is rendered over `state`. */
  startup?: Startup;
  state?: Partial<State>;
  preferences?: Preferences;
  toasts?: ToastFixture[];
  /** How a session's terminal behaves: `output` connects and draws a sample (default), `refuse`
   * never connects, which is a terminal in trouble. */
  terminal?: TerminalBehaviour;
  /** What `list_dir` answers per path; a path not listed gets a generic listing. */
  directories?: Record<string, DirEntry[]>;
  steps?: Step[];
}
