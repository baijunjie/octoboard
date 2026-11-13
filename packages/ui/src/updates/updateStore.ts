import { UpdaterError, type AvailableUpdate, type UpdateChannel, type UpdaterCapability, type UpdateProgress } from "../platform";

/** How long after launch the first automatic check waits, so it never competes with the window opening. */
export const FIRST_CHECK_DELAY_MS = 30_000;

/** How often the running application looks again. */
export const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

export type UpdatePhase = "idle" | "checking" | "downloading" | "installed";

export interface UpdateState {
  /** The version running, once the shell (or, for a daemon, its greeting) has said. */
  currentVersion?: string;
  phase: UpdatePhase;
  /** The newer version the last check found, offered until it is installed or a later check finds none, or the
   * channel it was found on is left. */
  update?: AvailableUpdate;
  /** The channel `update` was found on. */
  updateChannel?: UpdateChannel;
  /** The download's progress, while `phase` is `downloading`. */
  progress?: UpdateProgress;
  /** The version installed and waiting for a restart, once `phase` is `installed`; it is the version of the daemon the
   * restart starts, which the restart's compatibility check measures holders against. */
  installedVersion?: string;
  /** Why the restart that applies the installed update cannot be asked for now, which the daemon would refuse: `agentUpdate`
   * is an update of an agent's CLI running, which a restart would cut off. Only a daemon that updates itself knows
   * it (the desktop application's store never sets it); the window shows it ahead of the press, and the refusal that
   * still arrives (the update began after the press) is reported as an error. */
  restartBlocked?: "agentUpdate";
  /** The holder floor of the installed update (see `AvailableUpdate`); `null` when unknown (this window never saw it,
   * or the update gave none). */
  installedHolderFloor?: string | null;
  /** How the last check ended, and when. */
  lastCheck?: { outcome: "upToDate" | "available" | "failed"; at: number };
  /** The channel the next check looks on, as the user chose it or the running build defaults to. */
  channel: UpdateChannel;
  /** Whether the application looks for updates by itself; off, only a check the user asks for runs. */
  autoCheck: boolean;
  /** Set while the update found cannot be installed from here, the daemon's executable being a file it cannot replace
   * (`path`): it is upgraded by hand, by whoever may write to its folder, with `command` (the install script told that
   * folder and the channel), and no download is offered. */
  manual?: { path: string; command: string };
  /** A check the user asked of the daemon: `pending` until it has ended, or until the daemon said it did not start one
   * (it looks again at most every 30 seconds), then `unchanged` for a moment, so the button can say that it was heard
   * and the last check stands. */
  checkAsked?: "pending" | "unchanged";
  /** The daemon this page talks to runs a version other than the page's own build, which a reload takes up. */
  reloadNeeded?: boolean;
  /** The notice about `update` was dismissed; a different version brings it back. Settings still offers it. */
  noticeDismissed: boolean;
  /** The notice that a restart is needed was hidden with Later; Settings still shows the pending restart. */
  restartNoticeHidden: boolean;
  /** A restart was asked for and the window's exit flow has not yet opened its confirmation or started the
   * relaunch, or, for a daemon, the restart was sent and no daemon has greeted since; a button that asks holds itself
   * pending meanwhile. */
  restarting: boolean;
}

/** What went wrong in something the user started, for the UI to report; an automatic check's failure is not
 * reported, only recorded in `lastCheck`. */
export interface UpdateFailure {
  action: "check" | "install";
  /** `gone` is an install the shell (or the daemon) had nothing to install for: the update it offered is no longer there. */
  reason: "failed" | "gone";
  /** The updater's own text. */
  detail: string;
  /** The daemon's code of the failure, where it names one (`daemon_update_unverified`): its copy words the alert. */
  code?: string;
}

export interface UpdateStore {
  getState(): UpdateState;
  subscribe(listener: () => void): () => void;
  /** Looks for an update on the chosen channel. An update offered from another channel is withdrawn first, with its
   * notice, even when the check that follows cannot start (except while it is being downloaded). That check does
   * nothing while another check or a download is under way or once an update is installed. A `manual` check reports a
   * failure; an `automatic` one only records it. A check that settles after the channel was changed says nothing about
   * the new one, so its result is dropped and the new channel is checked in its place. */
  check(trigger: "manual" | "automatic"): Promise<void>;
  /** Downloads and installs the update the last check found. Does nothing without one, while anything else is
   * under way or once an update is installed, so nothing is ever installed twice. */
  install(): Promise<void>;
  /** Chooses the channel to look on, withdrawing an update another channel offered, and, where the automatic checks are on, looks again at once on it. For the daemon's update it is the setting the daemon
   * holds, so the promise is the save, which reports its own failure. */
  setChannel(channel: UpdateChannel): void | Promise<unknown>;
  /** Switches the automatic checks on or off; switched on, it looks at once. For the daemon's update it is the setting
   * the daemon holds, so the promise is the save, which reports its own failure. */
  setAutoCheck(value: boolean): void | Promise<unknown>;
  /** Hides the notice about the update found. */
  dismissNotice(): void;
  /** Hides the notice that a restart is needed. */
  hideRestartNotice(): void;
  /** Asks the window to restart, through its exit flow, and settles once that has opened its confirmation or started
   * the relaunch (`restarting` is set until then), telling it the installed update's holder floor and version (`null`
   * for either this window never saw); does nothing until an update is installed, or while an earlier
   * ask is still being answered. */
  restart(): Promise<void>;
  /** Registers what answers `restart`, returning the function that unregisters it. The handler's promise settles when
   * the confirmation is open or the relaunch is under way. */
  onRestartRequested(handler: (holderFloor: string | null, targetVersion: string | null) => Promise<void>): () => void;
  /** Starts the automatic checks, the first `firstCheckDelayMs` after the call (`FIRST_CHECK_DELAY_MS` unless
   * given) and then every `CHECK_INTERVAL_MS`, while the automatic checks are on; returns the function that stops them.
   * With `null` there are none. */
  start(firstCheckDelayMs?: number | null): () => void;
}

/** What every update store does alike: tells its listeners, and asks the window's exit flow for the restart that
 * applies an installed update (`restart` and `onRestartRequested` of `UpdateStore`), with `setRestarting` holding the
 * press pending until the flow has answered. */
export function createUpdateStoreBase(
  getState: () => UpdateState,
  setRestarting: (restarting: boolean) => void,
): Pick<UpdateStore, "subscribe" | "restart" | "onRestartRequested"> & { notify: () => void } {
  const listeners = new Set<() => void>();
  let restartHandler: ((holderFloor: string | null, targetVersion: string | null) => Promise<void>) | undefined;
  return {
    notify() {
      for (const listener of listeners) listener();
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async restart() {
      const state = getState();
      if (state.phase !== "installed" || state.restarting || !restartHandler) return;
      setRestarting(true);
      try {
        await restartHandler(state.installedHolderFloor ?? null, state.installedVersion ?? null);
      } catch {
        // The exit flow reports its own failures; the press only stops being pending.
      } finally {
        setRestarting(false);
      }
    },
    onRestartRequested(handler) {
      restartHandler = handler;
      return () => {
        if (restartHandler === handler) restartHandler = undefined;
      };
    },
  };
}

/**
 * The state of the application's updates, and the one place that decides what may happen next: a check and a
 * download never overlap, and once an update is installed nothing more is offered until the restart that applies
 * it. Restarting is only ever asked for, never done here: it goes through the window's exit confirmation
 * (`onRestartRequested`, answered by `useAppExit`), which asks the user first only when the restart would interrupt
 * a session or end a busy terminal.
 */
export function createUpdateStore({
  updater,
  getChannel,
  setChannel,
  getAutoCheck,
  setAutoCheck,
  reportFailure,
  now = Date.now,
}: {
  updater: UpdaterCapability;
  /** The channel to look on, given the running version (`undefined` until the shell has said). */
  getChannel: (currentVersion: string | undefined) => UpdateChannel;
  /** Keeps the user's choice of channel, which `getChannel` then answers. */
  setChannel: (channel: UpdateChannel) => void;
  /** Whether the automatic checks are on, and keeps the user's choice. */
  getAutoCheck: () => boolean;
  setAutoCheck: (value: boolean) => void;
  reportFailure: (failure: UpdateFailure) => void;
  now?: () => number;
}): UpdateStore {
  let state: UpdateState = {
    phase: "idle",
    channel: getChannel(undefined),
    autoCheck: getAutoCheck(),
    noticeDismissed: false,
    restartNoticeHidden: false,
    restarting: false,
  };
  const set = (change: Partial<UpdateState>) => {
    state = { ...state, ...change };
    base.notify();
  };
  const base = createUpdateStoreBase(() => state, (restarting) => set({ restarting }));

  void updater.currentVersion().then(
    (currentVersion) => set({ currentVersion, channel: getChannel(currentVersion) }),
    () => {},
  );

  /** The state once an update is installed and waits for a restart; `found` is unknown when the shell only says
   * that one is already installed and this window never saw which. */
  const installed = (found: AvailableUpdate | undefined): Partial<UpdateState> => ({
    phase: "installed",
    installedVersion: found?.version,
    installedHolderFloor: found?.holderFloor ?? null,
    update: undefined,
    updateChannel: undefined,
    progress: undefined,
    restartNoticeHidden: false,
  });

  /** What another channel offered is no news about `channel`, and stays withdrawn if a check on it fails. A download under
   * way keeps its update, which `install` holds on to and which is gone once it ends. */
  function withdrawOtherChannel(channel: UpdateChannel): void {
    if (state.update && state.updateChannel !== channel && state.phase !== "downloading") {
      set({ update: undefined, updateChannel: undefined, noticeDismissed: false });
    }
  }

  async function check(trigger: "manual" | "automatic"): Promise<void> {
    const channel = getChannel(state.currentVersion);
    withdrawOtherChannel(channel);
    if (state.phase !== "idle") return;
    set({ phase: "checking" });
    try {
      const found = await updater.check(channel);
      if (getChannel(state.currentVersion) !== channel) {
        set({ phase: "idle" });
        return check(trigger);
      }
      if (found) {
        // Another version than the one dismissed is news again.
        set({
          phase: "idle",
          update: found,
          updateChannel: channel,
          noticeDismissed: state.update?.version === found.version && state.noticeDismissed,
          lastCheck: { outcome: "available", at: now() },
        });
      } else {
        set({ phase: "idle", update: undefined, updateChannel: undefined, lastCheck: { outcome: "upToDate", at: now() } });
      }
    } catch (error) {
      if (getChannel(state.currentVersion) !== channel) {
        set({ phase: "idle" });
        return check(trigger);
      }
      if (error instanceof UpdaterError && error.kind === "installed") {
        // An update was installed before this window knew of it: it only awaits the restart.
        set(installed(state.update));
        return;
      }
      // The shell is busy with something of its own: nothing was learned, and nothing went wrong.
      if (error instanceof UpdaterError && error.kind === "busy") {
        set({ phase: "idle" });
        return;
      }
      set({ phase: "idle", lastCheck: { outcome: "failed", at: now() } });
      if (trigger === "manual") reportFailure({ action: "check", reason: "failed", detail: (error as Error).message });
    }
  }

  async function install(): Promise<void> {
    const update = state.update;
    if (state.phase !== "idle" || !update) return;
    set({ phase: "downloading", progress: { downloaded: 0 } });
    try {
      await updater.install((progress) => {
        if (state.phase === "downloading") set({ progress });
      });
      set(installed(update));
    } catch (error) {
      const kind = error instanceof UpdaterError ? error.kind : "failed";
      if (kind === "installed") {
        // Installed already, by an earlier press this window lost track of: it only awaits the restart.
        set(installed(update));
      } else if (kind === "busy") {
        // The shell is busy with something of its own; the update stays offered for another press.
        set({ phase: "idle", progress: undefined });
      } else if (kind === "nothing_to_install") {
        // The shell no longer holds the update this window offers, so it is not offered either.
        set({ phase: "idle", update: undefined, updateChannel: undefined, progress: undefined });
        reportFailure({ action: "install", reason: "gone", detail: "" });
      } else {
        // The update stays offered, so a failed install can be tried again.
        set({ phase: "idle", progress: undefined });
        reportFailure({ action: "install", reason: "failed", detail: (error as Error).message });
      }
    }
  }

  return {
    getState: () => state,
    subscribe: base.subscribe,
    check,
    install,
    setChannel(channel) {
      setChannel(channel);
      set({ channel: getChannel(state.currentVersion) });
      // With the automatic checks off nothing looks until the user asks, but what the other channel offered is withdrawn.
      withdrawOtherChannel(state.channel);
      if (state.autoCheck) void check("automatic");
    },
    setAutoCheck(value) {
      setAutoCheck(value);
      set({ autoCheck: value });
      if (value) void check("automatic");
    },
    dismissNotice: () => set({ noticeDismissed: true }),
    hideRestartNotice: () => set({ restartNoticeHidden: true }),
    restart: base.restart,
    onRestartRequested: base.onRestartRequested,
    start(firstCheckDelayMs = FIRST_CHECK_DELAY_MS) {
      if (firstCheckDelayMs === null) return () => {};
      let interval: ReturnType<typeof setInterval> | undefined;
      // The switch is read at each check, so turning it off stops the checks without stopping the timer.
      const automatic = () => {
        if (getAutoCheck()) void check("automatic");
      };
      const first = setTimeout(() => {
        automatic();
        interval = setInterval(automatic, CHECK_INTERVAL_MS);
      }, firstCheckDelayMs);
      return () => {
        clearTimeout(first);
        clearInterval(interval);
      };
    },
  };
}
