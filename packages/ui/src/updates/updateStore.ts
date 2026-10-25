import { UpdaterError, type AvailableUpdate, type UpdateChannel, type UpdaterCapability, type UpdateProgress } from "../platform";

/** How long after launch the first automatic check waits, so it never competes with the window opening. */
export const FIRST_CHECK_DELAY_MS = 30_000;

/** How often the running application looks again. */
export const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

export type UpdatePhase = "idle" | "checking" | "downloading" | "installed";

export interface UpdateState {
  /** The version running, once the shell has said. */
  currentVersion?: string;
  phase: UpdatePhase;
  /** The newer version the last check found, offered until it is installed or a later check finds none, or the
   * channel it was found on is left. */
  update?: AvailableUpdate;
  /** The channel `update` was found on. */
  updateChannel?: UpdateChannel;
  /** The download's progress, while `phase` is `downloading`. */
  progress?: UpdateProgress;
  /** The version installed and waiting for a restart, once `phase` is `installed`. */
  installedVersion?: string;
  /** How the last check ended, and when. */
  lastCheck?: { outcome: "upToDate" | "available" | "failed"; at: number };
  /** The notice about `update` was dismissed; a different version brings it back. Settings still offers it. */
  noticeDismissed: boolean;
  /** The notice that a restart is needed was hidden with Later; Settings still shows the pending restart. */
  restartNoticeHidden: boolean;
}

/** What went wrong in something the user started, for the UI to report; an automatic check's failure is not
 * reported, only recorded in `lastCheck`. */
export interface UpdateFailure {
  action: "check" | "install";
  /** `gone` is an install the shell had nothing to install for: the update it offered is no longer there. */
  reason: "failed" | "gone";
  /** The updater's own text. */
  detail: string;
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
  /** Hides the notice about the update found. */
  dismissNotice(): void;
  /** Hides the notice that a restart is needed. */
  hideRestartNotice(): void;
  /** Asks the window to restart, through its exit flow; does nothing until an update is installed. */
  restart(): void;
  /** Registers what answers `restart`, returning the function that unregisters it. */
  onRestartRequested(handler: () => void): () => void;
  /** Starts the automatic checks, the first `firstCheckDelayMs` after the call (`FIRST_CHECK_DELAY_MS` unless
   * given) and then every `CHECK_INTERVAL_MS`; returns the function that stops them. With `null` there are none. */
  start(firstCheckDelayMs?: number | null): () => void;
}

/**
 * The state of the application's updates, and the one place that decides what may happen next: a check and a
 * download never overlap, and once an update is installed nothing more is offered until the restart that applies
 * it. Restarting is only ever asked for, never done here: it goes through the window's exit confirmation
 * (`onRestartRequested`, answered by `useAppExit`), which asks the user first about the sessions it ends.
 */
export function createUpdateStore({
  updater,
  getChannel,
  reportFailure,
  now = Date.now,
}: {
  updater: UpdaterCapability;
  getChannel: () => UpdateChannel;
  reportFailure: (failure: UpdateFailure) => void;
  now?: () => number;
}): UpdateStore {
  let state: UpdateState = { phase: "idle", noticeDismissed: false, restartNoticeHidden: false };
  const listeners = new Set<() => void>();
  let restartHandler: (() => void) | undefined;

  const set = (change: Partial<UpdateState>) => {
    state = { ...state, ...change };
    for (const listener of listeners) listener();
  };

  void updater.currentVersion().then(
    (currentVersion) => set({ currentVersion }),
    () => {},
  );

  /** The state once an update is installed and waits for a restart; `version` is unknown when the shell only says
   * that one is already installed and this window never saw which. */
  const installed = (version: string | undefined): Partial<UpdateState> => ({
    phase: "installed",
    installedVersion: version,
    update: undefined,
    updateChannel: undefined,
    progress: undefined,
    restartNoticeHidden: false,
  });

  async function check(trigger: "manual" | "automatic"): Promise<void> {
    const channel = getChannel();
    // What another channel offered is no news about this one, and stays withdrawn if the check below fails. A download
    // under way keeps its update, which `install` holds on to and which is gone once it ends.
    if (state.update && state.updateChannel !== channel && state.phase !== "downloading") {
      set({ update: undefined, updateChannel: undefined, noticeDismissed: false });
    }
    if (state.phase !== "idle") return;
    set({ phase: "checking" });
    try {
      const found = await updater.check(channel);
      if (getChannel() !== channel) {
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
      if (getChannel() !== channel) {
        set({ phase: "idle" });
        return check(trigger);
      }
      if (error instanceof UpdaterError && error.kind === "installed") {
        // An update was installed before this window knew of it: it only awaits the restart.
        set(installed(state.update?.version));
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
      set(installed(update.version));
    } catch (error) {
      const kind = error instanceof UpdaterError ? error.kind : "failed";
      if (kind === "installed") {
        // Installed already, by an earlier press this window lost track of: it only awaits the restart.
        set(installed(update.version));
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
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    check,
    install,
    dismissNotice: () => set({ noticeDismissed: true }),
    hideRestartNotice: () => set({ restartNoticeHidden: true }),
    restart() {
      if (state.phase === "installed") restartHandler?.();
    },
    onRestartRequested(handler) {
      restartHandler = handler;
      return () => {
        if (restartHandler === handler) restartHandler = undefined;
      };
    },
    start(firstCheckDelayMs = FIRST_CHECK_DELAY_MS) {
      if (firstCheckDelayMs === null) return () => {};
      let interval: ReturnType<typeof setInterval> | undefined;
      const first = setTimeout(() => {
        void check("automatic");
        interval = setInterval(() => void check("automatic"), CHECK_INTERVAL_MS);
      }, firstCheckDelayMs);
      return () => {
        clearTimeout(first);
        clearInterval(interval);
      };
    },
  };
}
