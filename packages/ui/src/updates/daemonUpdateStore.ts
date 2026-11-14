import type { StoreApi } from "zustand";

import { CodedError } from "../errors/codedError";
import type { DaemonUpdateStatus } from "../protocol";
import type { RequestSender } from "../requests";
import { createSettingsWriter, type State } from "../store";
import { sameValue } from "../values/sameValue";
import type { UpdateChannel } from "../platform";
import { createUpdateStoreBase, type UpdateFailure, type UpdateState, type UpdateStore } from "./updateStore";

/** How long a check the daemon did not start keeps the button pending, so that the press is seen to have been heard. */
const CHECK_UNCHANGED_HOLD_MS = 1200;

/**
 * The update store of a daemon that updates itself (`snapshot.daemon_update` is not null), kept in step with the daemon's
 * status as the store of the connection holds it. It is the same interface as the desktop application's (`createUpdateStore`),
 * so the notice and the Settings section serve both: a check and a download are requests, whose end arrives as the
 * status; a restart goes through the window's exit flow, which asks about the sessions it would interrupt and sends
 * `restart_daemon`. Nothing is downloaded or restarted but by the user's press.
 *
 * What only this window knows stays here: whether its notice was dismissed or its restart notice hidden (for the version
 * installed), whether its restart is pending (until the daemon that follows greets, `restartAwaited`), and the check and
 * the install it asked for, whose failure it alone reports, as an action the user started (another window's failure is
 * the section's mark alone). The daemon's reply tells which they are: whether the check started, with the last check's
 * `at` it started after, and the install's `started_at`. A daemon of another version than the page's own build asks for
 * a reload. A restart is held back, and said to be, while an agent's CLI is being updated (`restartBlocked`), which the
 * daemon would refuse. The channel and the automatic check are the daemon's settings, written as any setting is, based
 * on the version this window holds.
 */
export function createDaemonUpdateStore({
  daemon,
  reportFailure,
  reportRequestFailure,
  reportSaveFailure,
  uiVersion = __OCTOBOARD_VERSION__,
}: {
  daemon: { store: StoreApi<State>; request: RequestSender };
  /** An end of a check or an install this window asked for that went wrong. */
  reportFailure: (failure: UpdateFailure) => void;
  /** A refusal of one of the requests. */
  reportRequestFailure: (error: unknown, action: UpdateFailure["action"]) => void;
  /** A setting the daemon would not take. */
  reportSaveFailure: (error: unknown) => void;
  /** The release version of the page's own build. */
  uiVersion?: string;
}): UpdateStore {
  const { store, request } = daemon;

  // Client state: what this window has done about the daemon's update.
  let dismissedVersion: string | undefined;
  /** The installed version (null where unknown) whose restart notice was hidden. */
  let restartNoticeHiddenFor: string | null | undefined;
  let restarting = false;
  /** A check this window asked for that the daemon started, after the last check that had ended at `after` (null for
   * none): its end is the status whose last check is another, or, once a status showed it `checking`, one that no longer
   * does (a check the daemon gave up on records nothing). */
  let askedCheck: { after: number | null; seenChecking: boolean } | undefined;
  /** An install this window asked for: `startedAt` once the daemon's reply has named it. */
  let askedInstall: { startedAt?: number } | undefined;
  let checkAsked: UpdateState["checkAsked"];
  const save = createSettingsWriter({ store, request, onFailure: reportSaveFailure });

  const derive = (held: State, previous: UpdateState | undefined): UpdateState => {
    // What was held from the daemon before this one greeted is not this one's: read nothing from it until its own status.
    const status: DaemonUpdateStatus | null = held.daemonUpdateStale ? null : held.daemonUpdate;
    const version = held.daemon?.version;
    // Asked for and not yet begun as the status sees it: shown as begun, so that the press is seen to have taken and a
    // second one finds the download under way.
    const installing = askedInstall !== undefined && (status?.phase ?? "idle") === "idle";
    const installedVersion = status?.installed?.version ?? null;
    const available = status?.available ?? undefined;
    const progress = status?.progress ?? undefined;
    const installed = status?.installed ?? undefined;
    const last = status?.last_check ?? undefined;
    const next: UpdateState = {
      currentVersion: version,
      phase: installing ? "downloading" : status?.phase ?? "idle",
      update: available && { version: available.version, holderFloor: available.holder_floor },
      updateChannel: available?.channel,
      progress: progress ? { downloaded: progress.downloaded, total: progress.total ?? undefined } : installing ? { downloaded: 0 } : undefined,
      installedVersion: installed?.version ?? undefined,
      installedHolderFloor: installed ? installed.holder_floor : undefined,
      // Only the restart into an installed update is held back; read nothing otherwise, so a CLI update that starts or
      // ends changes nothing the row or the strip shows before then.
      restartBlocked:
        status?.phase === "installed" && [...held.agentCliStatuses.values()].some((cli) => cli.update?.state === "running")
          ? "agentUpdate"
          : undefined,
      lastCheck: last && { outcome: last.outcome === "up_to_date" ? "upToDate" : last.outcome, at: last.at },
      channel: held.settings.daemon_update_channel,
      autoCheck: held.settings.daemon_update_check,
      manual: status?.manual ? { path: status.manual.path, command: status.manual.command } : undefined,
      checkAsked,
      reloadNeeded: version !== undefined && version !== uiVersion,
      noticeDismissed: available !== undefined && dismissedVersion === available.version,
      restartNoticeHidden: status?.phase === "installed" && restartNoticeHiddenFor === installedVersion,
      restarting: restarting || held.restartAwaited,
    };
    // A state that says what the last said is the same state, so what reads it does not render again.
    return previous && sameValue(previous, next) ? previous : next;
  };

  let state = derive(store.getState(), undefined);
  const base = createUpdateStoreBase(() => state, (value) => {
    restarting = value;
    refresh();
  });

  function refresh() {
    const next = derive(store.getState(), state);
    if (next === state) return;
    state = next;
    base.notify();
  }

  /** Reports the end of what this window asked for, once. `fresh` is a status a new `snapshot` brought (a new connection,
   * or a recovery from lag): work it does not show running ended unseen, or was another daemon's, and is no longer
   * waited for. */
  const settle = (held: State, fresh: boolean) => {
    const status = held.daemonUpdateStale ? null : held.daemonUpdate;
    if (!status) return;
    const last = status.last_check;
    if (askedCheck && last && last.at !== askedCheck.after) {
      askedCheck = undefined;
      checkAsked = undefined;
      if (last.outcome === "failed") reportFailure({ action: "check", reason: "failed", detail: last.detail ?? "", code: last.error ?? undefined });
    } else if (askedCheck && (status.phase === "installed" || ((fresh || askedCheck.seenChecking) && status.phase !== "checking"))) {
      // An update learned of meanwhile ends the check with nothing to say, as does a check that ended unrecorded or unseen.
      askedCheck = undefined;
      checkAsked = undefined;
    } else if (askedCheck && status.phase === "checking") {
      askedCheck.seenChecking = true;
    }
    const install = status.install;
    if (askedInstall?.startedAt !== undefined && install?.started_at === askedInstall.startedAt && install.state !== "running") {
      askedInstall = undefined;
      if (install.state === "failed") reportFailure({ action: "install", reason: "failed", detail: install.detail ?? "", code: install.error ?? undefined });
    } else if (askedInstall?.startedAt !== undefined && fresh && status.phase !== "downloading") {
      askedInstall = undefined;
    }
  };

  return {
    getState: () => state,
    subscribe: base.subscribe,
    async check(trigger) {
      // The daemon looks by itself; what a press asks for is followed, to report its failure.
      if (trigger !== "manual") {
        await request({ type: "check_daemon_update" }).catch(() => {});
        return;
      }
      checkAsked = "pending";
      refresh();
      let reply;
      try {
        reply = await request({ type: "check_daemon_update" });
      } catch (error) {
        checkAsked = undefined;
        refresh();
        reportRequestFailure(error, "check");
        return;
      }
      if (reply.started) {
        askedCheck = { after: reply.last_check_at, seenChecking: false };
        // Its end may have arrived ahead of the reply.
        settle(store.getState(), false);
        refresh();
        return;
      }
      checkAsked = "unchanged";
      refresh();
      await new Promise((resolve) => setTimeout(resolve, CHECK_UNCHANGED_HOLD_MS));
      // A later press may have asked again meanwhile.
      if (checkAsked === "unchanged") checkAsked = undefined;
      refresh();
    },
    async install() {
      if (state.phase !== "idle" || !state.update || state.manual || askedInstall) return;
      const asked: { startedAt?: number } = {};
      askedInstall = asked;
      refresh();
      try {
        asked.startedAt = (await request({ type: "update_daemon" })).started_at;
      } catch (error) {
        if (askedInstall === asked) askedInstall = undefined;
        refresh();
        // An update the daemon holds none of any more is withdrawn at its next status; the others are said as they are.
        if (error instanceof CodedError && error.code === "daemon_update_nothing_to_install") reportFailure({ action: "install", reason: "gone", detail: "" });
        else reportRequestFailure(error, "install");
        return;
      }
      // Its end may have arrived ahead of the reply.
      settle(store.getState(), false);
      refresh();
    },
    setChannel: (channel: UpdateChannel) => save({ daemon_update_channel: channel }, ["daemon_update_channel"]),
    setAutoCheck: (value) => save({ daemon_update_check: value }, ["daemon_update_check"]),
    dismissNotice() {
      dismissedVersion = state.update?.version;
      refresh();
    },
    hideRestartNotice() {
      restartNoticeHiddenFor = state.installedVersion ?? null;
      refresh();
    },
    restart: base.restart,
    onRestartRequested: base.onRestartRequested,
    // The daemon checks by itself, on its own schedule: what starts here is following its status.
    start() {
      refresh();
      return store.subscribe((held, previous) => {
        if (held.daemonUpdate !== previous.daemonUpdate || held.daemonUpdateStale !== previous.daemonUpdateStale || held.snapshotEpoch !== previous.snapshotEpoch) {
          // `hello` and `snapshot` usually arrive in one batch, so the stale step between them is seldom seen.
          settle(held, held.snapshotEpoch !== previous.snapshotEpoch);
        }
        refresh();
      });
    },
  };
}
