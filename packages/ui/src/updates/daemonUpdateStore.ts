import type { StoreApi } from "zustand";

import { CodedError } from "../errors/codedError";
import type { DaemonUpdateStatus } from "../protocol";
import type { RequestSender } from "../requests";
import { createSettingsWriter, type State } from "../store";
import { sameValue } from "../values/sameValue";
import type { UpdateChannel } from "../platform";
import type { UpdateFailure, UpdateState, UpdateStore } from "./updateStore";

/** How long a check this window asked for stays the one whose failure is reported, so that a request the daemon let
 * pass without checking (it answers nothing for one within 30 seconds of the last) does not leave a later automatic
 * check's failure to be taken for it. */
const ASKED_CHECK_LIFETIME_MS = 60_000;

/** How long after the daemon answered a check the page waits for a sign that it started (its status turning to checking,
 * or a last check that ended): the daemon answers one it lets pass, within 30 seconds of the last, as it answers any, and
 * says nothing more. */
const CHECK_STARTED_WAIT_MS = 600;

/** How long a check that did not start keeps the button pending, so that the press is seen to have been heard. */
const CHECK_UNCHANGED_HOLD_MS = 1200;

/**
 * The update store of a daemon that updates itself (`snapshot.daemon_update` is not null), kept in step with the daemon's
 * status as the store of the connection holds it. It is the same interface as the desktop application's (`createUpdateStore`),
 * so the notice and the Settings section serve both: a check and a download are requests, whose end arrives as the
 * status; a restart goes through the window's exit flow, which asks about the sessions it would interrupt and sends
 * `restart_daemon`. Nothing is downloaded or restarted but by the user's press.
 *
 * What only this window knows stays here: whether its notice was dismissed or hidden, whether its restart is pending, the
 * check and the install it asked for (whose failure it alone reports, as an action the user started; another window's
 * failure is the section's mark alone), and the version of the daemon the page was loaded for, so that a restart into
 * another version asks for a reload. The channel and the automatic check are the daemon's settings, written as any
 * setting is, based on the version this window holds.
 */
export function createDaemonUpdateStore({
  daemon,
  reportFailure,
  reportRequestFailure,
  reportSaveFailure,
  now = Date.now,
}: {
  daemon: { store: StoreApi<State>; request: RequestSender };
  /** An end of a check or an install this window asked for that went wrong. */
  reportFailure: (failure: UpdateFailure) => void;
  /** A refusal of one of the requests. */
  reportRequestFailure: (error: unknown, action: UpdateFailure["action"]) => void;
  /** A setting the daemon would not take. */
  reportSaveFailure: (error: unknown) => void;
  now?: () => number;
}): UpdateStore {
  const { store, request } = daemon;
  const listeners = new Set<() => void>();
  let restartHandler: ((holderFloor: string | null, targetVersion: string | null) => Promise<void>) | undefined;

  // Client state: what this window has done about the daemon's update.
  let dismissedVersion: string | undefined;
  let restartNoticeHidden = false;
  let restarting = false;
  /** The version of the daemon the page was loaded for: the first one that greeted it. */
  let loadedVersion: string | undefined;
  /** The `at` of the last check when this window asked for another (`null` for none yet) and when it asked. */
  let askedCheck: { from: number | null; at: number } | undefined;
  /** The `started_at` of the last install when this window asked for another (`null` for none yet). */
  let askedInstall: { from: number | null } | undefined;
  let checkAsked: UpdateState["checkAsked"];
  const save = createSettingsWriter({ store, request, onFailure: reportSaveFailure });

  const derive = (held: State, previous: UpdateState | undefined): UpdateState => {
    // What was held from the daemon before this one greeted is not this one's: read nothing from it until its own status.
    const status: DaemonUpdateStatus | null = held.daemonUpdateStale ? null : held.daemonUpdate;
    const version = held.daemon?.version;
    loadedVersion ??= version;
    const available = status?.available ?? undefined;
    const progress = status?.progress ?? undefined;
    const installed = status?.installed ?? undefined;
    const last = status?.last_check ?? undefined;
    const next: UpdateState = {
      currentVersion: version,
      phase: status?.phase ?? "idle",
      update: available && { version: available.version, holderFloor: available.holder_floor, notes: available.notes ?? undefined },
      updateChannel: available?.channel,
      progress: progress && { downloaded: progress.downloaded, total: progress.total ?? undefined },
      installedVersion: installed?.version ?? undefined,
      installedHolderFloor: installed ? installed.holder_floor : undefined,
      lastCheck: last && { outcome: last.outcome === "up_to_date" ? "upToDate" : last.outcome, at: last.at },
      channel: held.settings.daemon_update_channel,
      autoCheck: held.settings.daemon_update_check,
      manual: status?.manual ? { path: status.manual.path } : undefined,
      checkAsked,
      reloadNeeded: version !== undefined && loadedVersion !== undefined && version !== loadedVersion,
      noticeDismissed: available !== undefined && dismissedVersion === available.version,
      restartNoticeHidden,
      restarting,
    };
    // A state that says what the last said is the same state, so what reads it does not render again.
    return previous && sameValue(previous, next) ? previous : next;
  };

  let state = derive(store.getState(), undefined);

  const refresh = () => {
    const next = derive(store.getState(), state);
    if (next === state) return;
    state = next;
    for (const listener of listeners) listener();
  };

  /** Reports the end of what this window asked for, once. */
  const settle = (held: State) => {
    const status = held.daemonUpdateStale ? null : held.daemonUpdate;
    if (!status) return;
    const last = status.last_check;
    if (askedCheck && last && last.at !== askedCheck.from) {
      const asked = askedCheck;
      askedCheck = undefined;
      if (last.outcome === "failed" && now() - asked.at <= ASKED_CHECK_LIFETIME_MS) {
        reportFailure({ action: "check", reason: "failed", detail: last.detail ?? "", code: last.error ?? undefined });
      }
    }
    const install = status.install;
    if (askedInstall && install && install.started_at !== askedInstall.from && install.state !== "running") {
      askedInstall = undefined;
      if (install.state === "failed") reportFailure({ action: "install", reason: "failed", detail: install.detail ?? "", code: install.error ?? undefined });
    }
  };

  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async check(trigger) {
      // The daemon looks by itself; what a press asks for is remembered, to report its failure.
      if (trigger === "manual") {
        askedCheck = { from: store.getState().daemonUpdate?.last_check?.at ?? null, at: now() };
        checkAsked = "pending";
        refresh();
      }
      try {
        await request({ type: "check_daemon_update" });
      } catch (error) {
        askedCheck = undefined;
        checkAsked = undefined;
        refresh();
        if (trigger === "manual") reportRequestFailure(error, "check");
        return;
      }
      if (trigger !== "manual") return;
      // Whether the daemon looked is only told by its status: a check it let pass changes nothing.
      const asked = askedCheck;
      await new Promise((resolve) => setTimeout(resolve, CHECK_STARTED_WAIT_MS));
      const held = store.getState().daemonUpdate;
      if (held && (held.phase !== "idle" || held.last_check?.at !== asked?.from)) {
        checkAsked = undefined;
        refresh();
        return;
      }
      checkAsked = "unchanged";
      refresh();
      await new Promise((resolve) => setTimeout(resolve, CHECK_UNCHANGED_HOLD_MS));
      checkAsked = undefined;
      refresh();
    },
    async install() {
      if (state.phase !== "idle" || !state.update || state.manual) return;
      askedInstall = { from: store.getState().daemonUpdate?.install?.started_at ?? null };
      try {
        await request({ type: "update_daemon" });
      } catch (error) {
        askedInstall = undefined;
        // An update the daemon holds none of any more is withdrawn at its next status; the others are said as they are.
        if (error instanceof CodedError && error.code === "daemon_update_nothing_to_install") reportFailure({ action: "install", reason: "gone", detail: "" });
        else reportRequestFailure(error, "install");
      }
    },
    setChannel: (channel: UpdateChannel) => save({ daemon_update_channel: channel }, ["daemon_update_channel"]),
    setAutoCheck: (value) => save({ daemon_update_check: value }, ["daemon_update_check"]),
    dismissNotice() {
      dismissedVersion = state.update?.version;
      refresh();
    },
    hideRestartNotice() {
      restartNoticeHidden = true;
      refresh();
    },
    async restart() {
      if (state.phase !== "installed" || restarting || !restartHandler) return;
      restarting = true;
      refresh();
      try {
        await restartHandler(state.installedHolderFloor ?? null, state.installedVersion ?? null);
      } catch {
        // The exit flow reports its own failures; the press only stops being pending.
      } finally {
        restarting = false;
        refresh();
      }
    },
    onRestartRequested(handler) {
      restartHandler = handler;
      return () => {
        if (restartHandler === handler) restartHandler = undefined;
      };
    },
    // The daemon checks by itself, on its own schedule: what starts here is following its status.
    start() {
      refresh();
      return store.subscribe((held, previous) => {
        if (held.daemonUpdate !== previous.daemonUpdate || held.daemonUpdateStale !== previous.daemonUpdateStale) settle(held);
        refresh();
      });
    },
  };
}
