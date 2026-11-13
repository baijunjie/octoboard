import type { ThirdPartyLicenses } from "../about/thirdPartyLicenses";
import type { Language } from "../i18n/languages";
import type { AvailableUpdate, HostFacts, LaunchAtLoginStatus, MacosPermissionsStatus, UpdateProgress } from "../platform";
import type { Fixture } from "../fixtures/fixture";
import type { Step } from "../fixtures/interact";
import type { Preferences } from "../fixtures/preferences";
import type { ShellSettings } from "../lifecycle/shellSettings";
import type { ViewerSubject } from "../viewer/content";
import type { GroupTitle } from "./fixtures/groups";

/** What the window shows in place of the app, for the screens that have no daemon behind them. A
 * crash shows `stack` as the error's stack, which is otherwise the browser's own and so as long as
 * the engine makes it; it is what gives the stack block a length the scenario chooses. */
export type Startup = { kind: "noAddress" } | { kind: "daemonFailed"; error: string } | { kind: "crash"; stack?: string };

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

/** What a check answers: a newer version, `"upToDate"`, `"pending"` for a check that is never answered, or the
 * updater's own failure text. */
export type UpdaterCheck = AvailableUpdate | "upToDate" | "pending" | { error: string };

/** What the desktop shell's updater answers, for a window that can update itself: the Settings rows and the
 * update notice exist only with one. */
export interface UpdaterFixture {
  /** The version running. */
  currentVersion?: string;
  /** What a check answers, `"upToDate"` by default, whichever channel is followed. */
  check?: UpdaterCheck;
  /** Whether a check has already run when the window opens, as the automatic one does shortly after launch, so
   * its result (a notice, a Settings row) is on screen without a press. Without it the window has not looked
   * yet, and stays so: the gallery's provider makes no automatic checks of its own. */
  checked?: boolean;
  /** What an install does: `progress` is reported first, then it ends as `ends` says, by default with the update
   * in place; `"pending"` is a download that never finishes and `"gone"` the shell having nothing to install any more. */
  install?: { progress?: UpdateProgress; ends?: "pending" | "gone" };
}

export interface Scenario extends Fixture {
  id: string;
  /** The heading it is listed under. */
  group: GroupTitle;
  /** Deliberately classified; empty, loading and constrained layouts can be normal. */
  category: "normal" | "exceptional";
  title: string;
  /** Said under the title in the gallery; what to look at. */
  description?: string;
  /** The window width the gallery page opens it at, over a width chosen for an earlier scenario.
   * Left out for `DEFAULT_WIDTH`. Only for a target that can be reached in a band of widths the
   * default is outside of, where the steps cannot get there at whatever width the page's control picks (a swipe, the
   * open drawer card itself): a width inside that band, and the title or description says what it buys. Never for the
   * same state seen at another width, which the control shows; see "The UI state gallery" in
   * `packages/ui/README.md`. */
  width?: number;
  /** The narrowest window its target exists in: a width chosen with the page's control, or by the sweep's `--width`,
   * below it is raised to it. For what exists only from a breakpoint up (a card floated in by hovering a toggle,
   * above 768px; a docked pane's width grip or Maximize panel, from the width its visible panes need), which no width
   * chosen below it shows. A comment beside it says why. A scenario carries this or `width`, not both. */
  minWidth?: number;
  /** The window height the gallery page opens it at, as `width` is for the width; left out for a
   * window that fits the page. For a scenario that rests on a window too short for its dialog,
   * whose body then scrolls: that cannot be left to the height of whatever browser opens it. */
  height?: number;
  /** The shortcut modifier (⌘ / Ctrl) is held down when it opens, for a view that exists only while it is: the
   * quick-create form of the new-session buttons. Not a copy of another scenario made to see it held: the
   * shell's Modifier control holds it for any scenario. The page's own choice (`held` in the URL) stands over
   * it, and like a width stays on scenarios that carry none and is dropped on picking one that does. It is pressed
   * after the steps, which find the buttons by their plain names; `"from-start"` presses it before them instead, for a
   * view the steps open with it held (a menu locks its form as it opens). Its steps then find the buttons by their
   * quick names, or by either name where the viewer may choose Released. */
  held?: boolean | "from-start";
  /** Shown in place of the app; without it the real `App` is rendered over `state`. */
  startup?: Startup;
  /** Shows the real code wrappers against the alert tones instead of the application. */
  codePalette?: boolean;
  /** Shows the application's semantic colours as tokens, as real components and on each surface, instead of the application. */
  semanticColours?: boolean;
  /** Shows the shared button in its pending state, in every size and variant and as the icon-only avatar field, instead of the application. */
  pendingButtons?: boolean;
  preferences?: Preferences;
  /** The choices the desktop shell holds (Keep running in the background, Show menu bar icon) as the window starts, as
   * the shell would hand them over; the app's defaults (off, on) for one left out. The window keeps them in memory only. */
  shellSettings?: Partial<ShellSettings>;
  /** The desktop shell asks the window to quit (Cmd+Q) once the steps have run: with a busy session or shell terminal
   * the confirmation they hold the quit up with is on screen, and with none the quit goes straight to the shell. The
   * platform has no exit capability otherwise, except in a `desktopApp`; what the shell does with the quit is nothing
   * (confirming ends nothing) unless `exitFailure` fails it. */
  quitRequested?: boolean;
  /** The text the desktop shell fails the quit or restart with, so a quit (`quitRequested`) or a restart the steps press
   * ends in the failure alert instead of nothing. The platform has the exit capability, as with `quitRequested`. */
  exitFailure?: string;
  /** The daemon exits by itself, as the desktop shell reports with this text, once the steps have run, so the alert
   * saying so is on screen. The platform has the exit capability, as with `quitRequested`. */
  daemonExited?: string;
  /** What the shell answers about the macOS privacy grants, for the Permissions section.
   * `"unreadable"` is a shell that refuses the read; without this the browser platform's own
   * answer stands, which is the section's "only in the macOS app" line. A stub `request` answers
   * that the permission was not granted, which is
   * how a scenario reaches the row's System Settings guidance — nothing in a browser can raise a
   * real prompt. */
  macosPermissions?: MacosPermissionsStatus | "unreadable";
  /** With `macosPermissions`, the reason the system gives for not opening the System Settings pane a row's
   * Open System Settings points at; without it the pane opens (nothing is shown). */
  macosSettingsOpenError?: string;
  /** What the shell reports about the machine (`PlatformAdapter.host`): which of the system's own words the copy uses,
   * which Settings sections exist, whether the window controls overlay the top bar. Left out, the browser's own facts
   * stand, as for every scenario that is not about a host. Give the whole set the real shell reports
   * (`MAC_HOST`, `WINDOWS_HOST` of `test/hosts.ts`), with the snapshot's `state.host` and `state.homeDir` saying the
   * same of the daemon's machine. A host whose `chrome` is `"native"` has no `windowChrome`, whatever the frame URL's
   * `chrome=1` says, and one without an `appMenu` gives a `desktopApp` scenario no native menu. The key caps follow it
   * (Ctrl, Alt and Shift as text on a host whose modifier is not ⌘), and so do the key presses of the steps (`ui.chord`)
   * and the held modifier. */
  host?: HostFacts;
  /** The platform has what the desktop app's window has, so a view that lists or uses it shows the app's: the
   * window chrome (the `chrome=1` of the frame URL, which a view that exists only in the app cannot leave to
   * the address), the native menu bar and the desktop platform kind, by which the app's shortcuts exist (the
   * console-session switch on any system, Back and Forward on macOS; `chrome=1` alone does not give those), and the
   * exit capability the shell always has (a quit confirms nothing and ends nothing; see `quitRequested`). */
  desktopApp?: boolean;
  /** The platform offers the desktop app's run-in-background capability, so the Settings switch for
   * it is shown; the browser platform has none. */
  backgroundRun?: boolean;
  /** The platform has the desktop app's menu bar icon (the notification area icon on Windows), so the Settings switch
   * for showing it exists. Nothing is drawn. */
  menuBarIcon?: boolean;
  /** The system lets the app register a login entry, as the desktop app's does on macOS 13 and later and on Windows, so
   * the Launch at login switch is shown, the system's entry standing at this status when the window opens. Changing the
   * switch succeeds and leaves the entry enabled or disabled as asked. */
  launchAtLogin?: LaunchAtLoginStatus;
  /** With `launchAtLogin`, the reason the system gives for not opening its list of login items, which the Open System
   * Settings button of a login entry that `requiresApproval` goes to; without it the list opens (nothing is shown). */
  launchAtLoginOpenError?: string;
  /** The platform has the desktop shell's notifications, which it grants without a prompt, so the app keeps no switch of
   * its own. Whether the Settings row also links to the system's notification pane follows `host`
   * (`notificationSettings: "system-pane"`, as the real shell does). Nothing is shown or opened. */
  desktopNotifications?: boolean;
  /** Opening an external link fails with this text, as a refused desktop opener does. Without it the browser
   * platform's own opener stands and opens the address in a new tab. */
  externalLinkError?: string;
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
