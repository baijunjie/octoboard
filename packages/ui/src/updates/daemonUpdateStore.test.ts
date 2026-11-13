// @vitest-environment jsdom
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../errors/codedError";
import type { DaemonUpdateStatus } from "../protocol";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { createStateStore } from "../store/state";
import { applyDaemonEvent } from "../store/actions";
import { createDaemonUpdateStore } from "./daemonUpdateStore";
import type { UpdateFailure } from "./updateStore";

const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };
const AVAILABLE = { version: "0.2.0", notes: null, holder_floor: "0.1.0", channel: "stable" } as const;

function setup(status: DaemonUpdateStatus | null = IDLE, version = "0.1.0", clock = { now: 1000 }, broadcasts = true) {
  const store = createStateStore({ daemon: { version, generation: 1 }, daemonUpdate: status, settings: { ...DEFAULT_SETTINGS, versions: { daemon_update_check: 3, daemon_update_channel: 4 } } });
  const requests: { type: string; [key: string]: unknown }[] = [];
  const answers: { fail?: unknown } = {};
  const request = vi.fn(async (body: { type: string; base_versions?: Record<string, number> }) => {
    requests.push(body);
    if (answers.fail) throw answers.fail;
    // The daemon's broadcast of the setting it stored, with the version it moved to.
    if (body.type === "update_settings" && broadcasts) {
      const moved = Object.fromEntries(Object.entries(body.base_versions ?? {}).map(([path, version]) => [path, version + 1]));
      store.setState((state) => ({ settings: { ...state.settings, versions: { ...state.settings.versions, ...moved } } }));
    }
    return {};
  });
  const failures: UpdateFailure[] = [];
  const refusals: unknown[] = [];
  const updates = createDaemonUpdateStore({
    daemon: { store, request: request as never },
    reportFailure: (failure) => failures.push(failure),
    reportRequestFailure: (error) => refusals.push(error),
    reportSaveFailure: (error) => refusals.push(error),
    now: () => clock.now,
  });
  const stop = updates.start();
  const send = (next: DaemonUpdateStatus | null) => store.setState({ daemonUpdate: next });
  return { updates, store, requests, answers, failures, refusals, send, stop };
}

describe("the state", () => {
  it("reads the daemon's status in the shape of the desktop application's", () => {
    const { updates } = setup({ ...IDLE, available: AVAILABLE, last_check: { outcome: "up_to_date", at: 5, error: null, detail: null } });
    expect(updates.getState()).toMatchObject({
      currentVersion: "0.1.0",
      phase: "idle",
      update: { version: "0.2.0", holderFloor: "0.1.0" },
      updateChannel: "stable",
      lastCheck: { outcome: "upToDate", at: 5 },
      channel: "stable",
      autoCheck: true,
      reloadNeeded: false,
    });
  });

  it("follows the status as it changes, and keeps the state when nothing in it changed", () => {
    const { updates, send } = setup();
    const listener = vi.fn();
    updates.subscribe(listener);
    send({ ...IDLE, phase: "downloading", available: AVAILABLE, progress: { downloaded: 10, total: 100 } });
    expect(updates.getState()).toMatchObject({ phase: "downloading", progress: { downloaded: 10, total: 100 } });
    expect(listener).toHaveBeenCalledTimes(1);
    send({ ...IDLE, phase: "downloading", available: AVAILABLE, progress: { downloaded: 10, total: 100 } });
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("knows an installed update of no known version, and a manual one", () => {
    const { updates, send } = setup();
    send({ ...IDLE, phase: "installed", installed: { version: null, holder_floor: null } });
    expect(updates.getState()).toMatchObject({ phase: "installed", installedVersion: undefined, installedHolderFloor: null });
    send({ ...IDLE, available: AVAILABLE, manual: { reason: "not_writable", path: "/usr/bin/octoboardd" } });
    expect(updates.getState().manual).toEqual({ path: "/usr/bin/octoboardd" });
  });

  it("asks for a reload once the daemon runs another version than the page was loaded for", () => {
    const { updates, store } = setup();
    expect(updates.getState().reloadNeeded).toBe(false);
    store.setState({ daemon: null });
    expect(updates.getState().reloadNeeded).toBe(false);
    store.setState({ daemon: { version: "0.2.0", generation: 1 } });
    expect(updates.getState().reloadNeeded).toBe(true);
  });

  it("reads nothing from the status held before the daemon that greeted sent its own", () => {
    const { updates, store, send } = setup({ ...IDLE, available: AVAILABLE });
    expect(updates.getState().update?.version).toBe("0.2.0");
    applyDaemonEvent(store, { type: "hello", daemon_version: "0.2.0", generation: 1 } as never);
    expect(updates.getState()).toMatchObject({ currentVersion: "0.2.0", update: undefined, phase: "idle" });
    send({ ...IDLE, phase: "downloading", available: { ...AVAILABLE, version: "0.3.0" }, progress: { downloaded: 1, total: 2 } });
    applyDaemonEvent(store, { type: "daemon_update_status_updated", status: { ...IDLE, available: { ...AVAILABLE, version: "0.3.0" } } });
    expect(updates.getState().update?.version).toBe("0.3.0");
  });

  it("brings a dismissed notice back for another version", () => {
    const { updates, send } = setup({ ...IDLE, available: AVAILABLE });
    updates.dismissNotice();
    expect(updates.getState().noticeDismissed).toBe(true);
    send({ ...IDLE, available: { ...AVAILABLE, version: "0.3.0" } });
    expect(updates.getState().noticeDismissed).toBe(false);
  });
});

describe("requests", () => {
  it("asks the daemon to check, and to install the update it found", async () => {
    const { updates, requests } = setup({ ...IDLE, available: AVAILABLE });
    await updates.check("manual");
    await updates.install();
    expect(requests.map((request) => request.type)).toEqual(["check_daemon_update", "update_daemon"]);
  });

  it("installs nothing it was not offered, nor where the daemon cannot replace itself", async () => {
    const none = setup();
    await none.updates.install();
    const manual = setup({ ...IDLE, available: AVAILABLE, manual: { reason: "not_writable", path: "/x" } });
    await manual.updates.install();
    expect(none.requests).toEqual([]);
    expect(manual.requests).toEqual([]);
  });

  it("reports a refusal of an install, and an update that is no longer there as gone", async () => {
    const { updates, answers, refusals, failures } = setup({ ...IDLE, available: AVAILABLE });
    answers.fail = new CodedError("running", "daemon_update_running", {});
    await updates.install();
    expect(refusals).toHaveLength(1);
    answers.fail = new CodedError("none", "daemon_update_nothing_to_install", {});
    await updates.install();
    expect(failures).toEqual([{ action: "install", reason: "gone", detail: "" }]);
  });

  it("reports the failure of the check and of the install this window asked for, once", async () => {
    const { updates, send, failures } = setup({ ...IDLE, available: AVAILABLE });
    await updates.check("manual");
    send({ ...IDLE, available: AVAILABLE, last_check: { outcome: "failed", at: 2, error: "daemon_update_check_failed", detail: "offline" } });
    await updates.install();
    send({ ...IDLE, available: AVAILABLE, install: { state: "failed", started_at: 3, finished_at: 4, error: "daemon_update_unverified", detail: "bad" } });
    send({ ...IDLE, available: AVAILABLE, install: { state: "failed", started_at: 3, finished_at: 5, error: "daemon_update_unverified", detail: "bad" } });
    expect(failures).toEqual([
      { action: "check", reason: "failed", detail: "offline", code: "daemon_update_check_failed" },
      { action: "install", reason: "failed", detail: "bad", code: "daemon_update_unverified" },
    ]);
  });

  it("keeps the button pending for a check the daemon let pass, then says the last check stands", async () => {
    vi.useFakeTimers();
    onTestFinished(() => void vi.useRealTimers());
    const { updates } = setup({ ...IDLE, last_check: { outcome: "up_to_date", at: 5, error: null, detail: null } });
    const done = updates.check("manual");
    expect(updates.getState().checkAsked).toBe("pending");
    await vi.advanceTimersByTimeAsync(700);
    expect(updates.getState().checkAsked).toBe("unchanged");
    await vi.advanceTimersByTimeAsync(1300);
    await done;
    expect(updates.getState().checkAsked).toBeUndefined();
  });

  it("stops being pending at once when the daemon shows the check started", async () => {
    vi.useFakeTimers();
    onTestFinished(() => void vi.useRealTimers());
    const { updates, send } = setup();
    const done = updates.check("manual");
    send({ ...IDLE, phase: "checking" });
    await vi.advanceTimersByTimeAsync(700);
    await done;
    expect(updates.getState().checkAsked).toBeUndefined();
  });

  it("says nothing of a failed check it did not ask for", () => {
    const { send, failures } = setup();
    send({ ...IDLE, last_check: { outcome: "failed", at: 2, error: "daemon_update_check_failed", detail: "offline" } });
    expect(failures).toEqual([]);
  });
});

describe("settings", () => {
  it("writes the channel and the automatic check as settings, based on the versions held", async () => {
    const { updates, requests } = setup();
    await updates.setChannel("beta");
    await updates.setAutoCheck(false);
    expect(requests).toEqual([
      { type: "update_settings", daemon_update_channel: "beta", base_versions: { daemon_update_channel: 4 } },
      { type: "update_settings", daemon_update_check: false, base_versions: { daemon_update_check: 3 } },
    ]);
  });
});

describe("restarting", () => {
  it("hands the installed update's floor and version to the exit flow, and does nothing before one is installed", async () => {
    const { updates, send } = setup();
    const handler = vi.fn(async () => {});
    updates.onRestartRequested(handler);
    await updates.restart();
    expect(handler).not.toHaveBeenCalled();
    send({ ...IDLE, phase: "installed", installed: { version: "0.2.0", holder_floor: "0.1.0" } });
    await updates.restart();
    expect(handler).toHaveBeenCalledWith("0.1.0", "0.2.0");
  });
});

describe("what the daemon refuses or does not answer", () => {
  it("reports a refused check as the user's own only when the user asked for it", async () => {
    const asked = setup();
    asked.answers.fail = new CodedError("unavailable", "daemon_update_unavailable", {});
    await asked.updates.check("manual");
    expect(asked.refusals).toHaveLength(1);
    const automatic = setup();
    automatic.answers.fail = new CodedError("unavailable", "daemon_update_unavailable", {});
    await automatic.updates.check("automatic");
    expect(automatic.refusals).toEqual([]);
  });

  it("does not install while a check or a download runs, or after one is installed", async () => {
    for (const phase of ["checking", "downloading", "installed"] as const) {
      const { updates, requests } = setup({ ...IDLE, phase, available: AVAILABLE });
      await updates.install();
      expect(requests, phase).toEqual([]);
    }
  });

  it("leaves a failed check to the section's mark once the press that asked for it is old", async () => {
    const clock = { now: 1000 };
    const { updates, send, failures } = setup(IDLE, "0.1.0", clock);
    await updates.check("manual");
    // A request the daemon let pass without checking, and a failure of the automatic check a minute later.
    clock.now += 61_000;
    send({ ...IDLE, last_check: { outcome: "failed", at: 2, error: "daemon_update_check_failed", detail: "offline" } });
    expect(failures).toEqual([]);
  });

  it("reports an install that failed under another window's press only as the section's state, not as an alert", () => {
    const { send, failures, updates } = setup({ ...IDLE, available: AVAILABLE });
    send({ ...IDLE, available: AVAILABLE, install: { state: "failed", started_at: 3, finished_at: 4, error: "daemon_update_replace_failed", detail: "x" } });
    expect(failures).toEqual([]);
    expect(updates.getState().phase).toBe("idle");
  });
});

describe("saving the settings", () => {
  it("reports a write the daemon refused, and takes the next one afterwards", async () => {
    const { updates, answers, refusals, requests } = setup();
    answers.fail = new CodedError("stale", "write_conflict", {});
    await updates.setAutoCheck(false);
    expect(refusals).toHaveLength(1);
    delete answers.fail;
    await updates.setAutoCheck(false);
    expect(requests).toHaveLength(2);
  });

  it("drops a write of a setting while the store still holds the version the last one was based on", async () => {
    const { updates, requests } = setup(IDLE, "0.1.0", { now: 1000 }, false);
    const first = updates.setChannel("beta");
    const second = updates.setChannel("stable");
    await Promise.all([first, second]);
    expect(requests).toHaveLength(1);
    expect(requests[0]).toMatchObject({ daemon_update_channel: "beta" });
  });

  it("bases the next write on the version the daemon's broadcast moved the setting to", async () => {
    const { updates, requests } = setup();
    await updates.setChannel("beta");
    await updates.setChannel("stable");
    expect(requests.map((request) => request.base_versions)).toEqual([{ daemon_update_channel: 4 }, { daemon_update_channel: 5 }]);
  });

  it("shows the channel and the switch as the daemon holds them", () => {
    const { updates, store } = setup();
    store.setState((state) => ({ settings: { ...state.settings, daemon_update_channel: "beta", daemon_update_check: false } }));
    expect(updates.getState()).toMatchObject({ channel: "beta", autoCheck: false });
  });
});

describe("the restart", () => {
  const INSTALLED: DaemonUpdateStatus = { ...IDLE, phase: "installed", installed: { version: "0.2.0", holder_floor: "0.1.0" } };

  it("is pending while the exit flow runs and is no longer once it ends, however it ends", async () => {
    const { updates } = setup(INSTALLED);
    let finish!: () => void;
    updates.onRestartRequested(() => new Promise<void>((resolve) => (finish = resolve)));
    const pressed = updates.restart();
    expect(updates.getState().restarting).toBe(true);
    // A second press while one is pending asks for nothing more.
    await updates.restart();
    finish();
    await pressed;
    expect(updates.getState().restarting).toBe(false);

    updates.onRestartRequested(async () => {
      throw new Error("cancelled");
    });
    await updates.restart();
    expect(updates.getState().restarting).toBe(false);
  });

  it("does nothing once its handler is taken away, and hides only the notice with Later", async () => {
    const { updates } = setup(INSTALLED);
    const handler = vi.fn(async () => {});
    const off = updates.onRestartRequested(handler);
    off();
    await updates.restart();
    expect(handler).not.toHaveBeenCalled();
    updates.hideRestartNotice();
    expect(updates.getState()).toMatchObject({ restartNoticeHidden: true, phase: "installed" });
  });
});
