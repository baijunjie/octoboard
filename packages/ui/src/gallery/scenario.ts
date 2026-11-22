import type { ThirdPartyLicenses } from "../about/thirdPartyLicenses";
import type { StartupCause } from "../daemon";
import type { Language } from "../i18n/languages";
import type { AvailableUpdate, CommandLineError, CommandLineState, HostFacts, LaunchAtLoginStatus, LinkForwardError, MacosPermissionsStatus, RemoteDaemonConnectOutcome, RemoteDaemonEntry, RemoteDaemonStatus, RemoteDeviceKeyRemoval, RemoteForward, ShellSystemChange, ShellSystemChangeResult, ShellSystemChangesFailure, UpdateProgress } from "../platform";
import type { Fixture } from "../fixtures/fixture";
import type { Step } from "../fixtures/interact";
import type { Preferences } from "../fixtures/preferences";
import type { ShellSettings } from "../lifecycle/shellSettings";
import type { ViewerSubject } from "../viewer/content";
import type { GroupTitle } from "./fixtures/groups";

/** What the window shows in place of the app, for the screens that have no daemon behind them. A
 * crash shows `stack` as the error's stack, which is otherwise the browser's own and so as long as
 * the engine makes it; it is what gives the stack block a length the scenario chooses. */
export type Startup = { kind: "noAddress" } | { kind: "daemonFailed"; error: string; cause?: StartupCause } | { kind: "crash"; stack?: string };

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

/** The remote daemons the desktop shell keeps, for a window that offers them: the rail's Remote devices button and
 * the page it shows exist only with this. A connection never ends unless `outcomes` says how, so pressing a Connect that has no
 * password to ask for leaves it connecting. */
export interface RemoteDaemonsFixture {
  /** The saved entries, in the order the page lists them. */
  entries: RemoteDaemonEntry[];
  /** The entries whose window is open; any other entry is not connected. */
  status?: RemoteDaemonStatus[];
  /** What a press on an entry ends in, by its id; an entry without one never finishes connecting. A press that asks to
   * hold the connection for the browser (`browser`) is answered with its address, and the entry then counts as held. */
  outcomes?: Record<string, RemoteDaemonConnectOutcome>;
  /** What taking back the key of an entry that has this computer's key installed (`auth` "device") comes to, by its id:
   * the line removed from its server and the private key gone from this computer unless this says otherwise, for
   * example the line left on its server (`line`) or the private key left in the credential store (`privateKeyLeft`). */
  keyRemovals?: Record<string, RemoteDeviceKeyRemoval>;
}

/** One daemon's tab of the notification-area flyout. */
export interface TrayFlyoutTabFixture {
  /** The remote daemon's name, as its tab is labelled; none for this computer's tab, which comes first. */
  remote?: string;
  /** A remote daemon whose connection is held for the browser: no window sends its menu, so its tab offers only Open in
   * browser and Disconnect. */
  held?: true;
  /** The window state the daemon's menu is built from (its sessions, consoles, accounts and quota readings), by the
   * app's own menu builder: the sessions waiting for the user raise the hand on the tab. Left out for a held daemon. */
  state?: Scenario["state"];
  /** The session whose quota gives the icon its capsule, and the console whose account the quota rows' openers show. */
  selectedSessionId?: string;
  currentConsoleId?: string;
}

/** The page of the Windows notification-area flyout (`TrayFlyout`), which has no daemon behind it: the model the shell
 * would hand it, laid out as the shell lays out the macOS menu's lines (a tab per daemon, the chosen tab's lines, then
 * the lines every tab shares), in the window's language. */
export interface TrayFlyoutFixture {
  /** The daemons' tabs in connection order; the first is this computer's. With one tab there is no strip of tabs. */
  tabs: TrayFlyoutTabFixture[];
  /** The tab the flyout opens on; the first by default. */
  chosen?: number;
  /** The version a known newer release has, which the Check for updates line then names; without it the line only
   * offers the check. */
  updateVersion?: string;
  /** The shell cannot be handed a choice: the flyout's `choose` rejects, as it does when the shell is unreachable, so a
   * chosen line leaves the flyout open with the error Alert at its top. */
  chooseFails?: true;
}

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

/** What the macOS application's command-line tool answers (`PlatformAdapter.commandLine`): the Settings row for it exists
 * only with one. */
export interface CommandLineFixture {
  /** Where the tool stands when the window opens, which follows a successful install or remove; `"unreadable"` is a shell
   * that cannot tell (the read is refused). */
  state: CommandLineState | "unreadable";
  /** What Install or Reinstall does: by default it makes the link in `/usr/local/bin`; `"pending"` is a change that never
   * finishes, so the row stays with the pressed button pending; `{ error }` refuses as the shell does, with the
   * `CommandLineError` it gives. */
  install?: "pending" | { error: CommandLineError };
}

/** What the desktop shell lists as in effect and answers to clearing it (`PlatformAdapter.systemChanges`): the
 * application's own items and this computer's daemon's (the accounts that share) in one list. Only the local daemon's
 * window of the desktop application has one, so it is for a `desktopApp` scenario that is not a `remoteWindow`. */
export interface ShellSystemChangesFixture {
  /** What is in effect when the window opens, in the order the shell lists it (the saved remote daemons' device keys,
   * launch at login, the command-line link, the accounts that share); none is the empty state. */
  changes: ShellSystemChange[];
  /** The parts that cannot tell what they have in effect when listing, as the shell answers beside `changes`: the
   * `daemon` kind is this computer's daemon not answering, any other a feature of the application. */
  listFailed?: ShellSystemChangesFailure[];
  /** What clearing answers, one result per item as the shell does; by default every item is cleared. An item whose
   * outcome is not `cleared` stays listed afterwards, and so does one that has no result. */
  clear?: ShellSystemChangeResult[];
  /** The parts that failed as a whole when clearing, beside `clear`, whose items `clear` then leaves out. */
  clearFailed?: ShellSystemChangesFailure[];
  /** The shell does not answer the clearing at all: it is refused with this text, and nothing is cleared. */
  clearUnanswered?: string;
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
  /** The macOS application can link its `octoboardd` into the PATH, so the Command line group of General is shown, the
   * tool standing as this says when the window opens. Needs a `host` that offers it (`MAC_HOST`). */
  commandLine?: CommandLineFixture;
  /** The window is the local daemon's in the desktop application, which lists and clears what its own features and its
   * daemon have in effect outside their directories (Settings > Cleanup, the "Clean up before uninstalling" row), the
   * one place the action is offered. Without it the window is a browser's or a remote daemon's, and the row is one
   * sentence saying where the cleanup is done, which in a browser `state.host.application` decides (a desktop
   * application's own daemon, or one that runs on its own). */
  shellSystemChanges?: ShellSystemChangesFixture;
  /** The platform offers the desktop shell's remote daemons, listing these, so the rail has its Remote devices
   * button under the consoles (alone, and in a `desktopApp`, which is the window that has it). Saving and removing
   * change the list. */
  remoteDaemons?: RemoteDaemonsFixture;
  /** The window is a remote daemon's, as the shell opens one (`?remote=<name>`): the title bar names the daemon, and
   * what is the application's own rather than a daemon's is absent (the rail's Remote devices button, the Access,
   * Permissions and Updates sections), as in the shell's window of that kind. Give the `host` the shell gives such a
   * window (`REMOTE_WINDOW_HOST` of `fixtures/remoteWindow.ts`) and leave out the capabilities a remote window is not
   * given (`backgroundRun`, `menuBarIcon`, `launchAtLogin`, `commandLine`, `shellSystemChanges`, `updater`). The daemon's host is its own, as `state.host`
   * says. */
  remoteWindow?: { name: string };
  /** The ports of this computer the remote window's shell forwards to its daemon's machine, in the order they were made:
   * the rail has its Forwarded ports entry while there is one, and Stop lets a port go. Only for a `remoteWindow`. */
  remoteForwards?: RemoteForward[];
  /** The remote window's shell can copy a file of its daemon's machine to this computer and open the copy (Download and
   * open); the shell takes every part and shows nothing, and how many parts the daemon answers is the file's own
   * (`answeredParts`). Only for a `remoteWindow`. */
  remoteFiles?: boolean;
  /** Whether macOS Remote Login (Windows: OpenSSH Server) is on, as the shell reports it for the Access section:
   * `"unknown"` is a shell that cannot tell. Opening the system's settings succeeds and shows nothing. Only a window
   * that has it offers the section (with `state.remoteAccess` set). */
  remoteLogin?: "on" | "off" | "unknown";
  /** The window is the notification-area flyout's page in place of the app, over the model of this. */
  trayFlyout?: TrayFlyoutFixture;
  /** The platform has the desktop shell's notifications, which it grants without a prompt, so the app keeps no switch of
   * its own. Whether the Settings row also links to the system's notification pane follows `host`
   * (`notificationSettings: "system-pane"`, as the real shell does). Nothing is shown or opened. */
  desktopNotifications?: boolean;
  /** Opening an external link fails with this text, as a refused desktop opener does. Without it the browser
   * platform's own opener stands and opens the address in a new tab. */
  externalLinkError?: string;
  /** Opening an address in a remote window fails as the shell's forwarding does, by this kind, naming the port of the
   * address: nothing listens on it, the SSH server refuses forwarding, or the connection is down. Only for a
   * `remoteWindow`. */
  forwardFailure?: Exclude<LinkForwardError["kind"], "failed">;
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
   * first. The viewer's Quote item is offered over a session that can take a quote; `quote: "unavailable"` stages
   * a window with none (no session ready for input), where it is disabled with its reason. */
  viewer?: { subjects: (language: Language) => Promise<ViewerSubject[]>; loadDelay?: number; quote?: "unavailable" };
}
