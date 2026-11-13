// @vitest-environment jsdom
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers } from "../test/clock";

import { CodedError } from "../errors/codedError";
import type { DaemonUpdateStatus } from "../protocol";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { createStateStore } from "../store/state";
import { applyDaemonEvent, setRestartAwaited } from "../store/actions";
import { createDaemonUpdateStore } from "./daemonUpdateStore";
import type { UpdateFailure } from "./updateStore";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };
const AVAILABLE = { version: "0.2.0", notes: null, holder_floor: "0.1.0", channel: "stable" } as const;

const UI_VERSION = "0.1.0";

function setup(status: DaemonUpdateStatus | null = IDLE, version = UI_VERSION, broadcasts = true, uiVersion = UI_VERSION) {
  const store = createStateStore({ daemon: { version, generation: 1 }, daemonUpdate: status, settings: { ...DEFAULT_SETTINGS, versions: { daemon_update_check: 3, daemon_update_channel: 4 } } });
  const requests: { type: string; [key: string]: unknown }[] = [];
  /** What the daemon answers: a refusal, the reply to a check and to an install, a promise the reply waits for, and
   * what happens (a status arriving) just before the reply does. */
  const answers: { fail?: unknown; check: { started: boolean; last_check_at: number | null }; install: { started_at: number }; hold?: Promise<unknown>; beforeReply?: () => void } = {
    check: { started: true, last_check_at: null },
    install: { started_at: 3 },
  };
  const request = vi.fn(async (body: { type: string; base_versions?: Record<string, number> }) => {
    requests.push(body);
    await answers.hold;
    answers.beforeReply?.();
    if (answers.fail) throw answers.fail;
    // The daemon's broadcast of the setting it stored, with the version it moved to.
    if (body.type === "update_settings" && broadcasts) {
      const moved = Object.fromEntries(Object.entries(body.base_versions ?? {}).map(([path, version]) => [path, version + 1]));
      store.setState((state) => ({ settings: { ...state.settings, versions: { ...state.settings.versions, ...moved } } }));
    }
    if (body.type === "check_daemon_update") return { type: "daemon_update_check", id: null, ...answers.check };
    if (body.type === "update_daemon") return { type: "daemon_update_started", id: null, ...answers.install };
    return {};
  });
  const failures: UpdateFailure[] = [];
  const refusals: { error: unknown; action?: UpdateFailure["action"] }[] = [];
  const updates = createDaemonUpdateStore({
    daemon: { store, request: request as never },
    reportFailure: (failure) => failures.push(failure),
    reportRequestFailure: (error, action) => refusals.push({ error, action }),
    reportSaveFailure: (error) => refusals.push({ error }),
    uiVersion,
  });
  const stop = updates.start();
  onTestFinished(stop);
  const send = (next: DaemonUpdateStatus | null) => store.setState({ daemonUpdate: next });
  /** The daemon coming back: its greeting and its snapshot, which the connection delivers together in one update. */
  const comesBack = (to: string, status: DaemonUpdateStatus) => {
    store.setState((state) => ({ daemon: { version: to, generation: 1 }, daemonUpdate: status, daemonUpdateStale: false, snapshotEpoch: state.snapshotEpoch + 1, restartAwaited: false }));
  };
  return { updates, store, requests, answers, failures, refusals, send, stop, comesBack };
}

const CHECKED = (at: number, outcome: "up_to_date" | "available" | "failed" = "up_to_date") => ({ outcome, at, error: outcome === "failed" ? "daemon_update_check_failed" : null, detail: outcome === "failed" ? "offline" : null });
const INSTALL = (state: "running" | "succeeded" | "failed", startedAt: number) => ({
  state,
  started_at: startedAt,
  finished_at: state === "running" ? null : startedAt + 1,
  error: state === "failed" ? "daemon_update_unverified" : null,
  detail: state === "failed" ? "bad" : null,
});

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
    send({ ...IDLE, available: AVAILABLE, manual: { reason: "not_writable", path: "/usr/bin/octoboardd", command: "curl -fsSL https://example.test/install.sh | sh -s -- --bin-dir /usr/bin" } });
    expect(updates.getState().manual).toEqual({ path: "/usr/bin/octoboardd", command: "curl -fsSL https://example.test/install.sh | sh -s -- --bin-dir /usr/bin" });
  });

  it("asks for a reload exactly while the daemon runs another version than the page's own build, whichever daemon greeted first", () => {
    const { updates, store } = setup(IDLE, "0.2.0", true, "0.2.0");
    expect(updates.getState().reloadNeeded).toBe(false);
    store.setState({ daemon: null });
    expect(updates.getState().reloadNeeded).toBe(false);
    applyDaemonEvent(store, { type: "hello", daemon_version: "0.3.0", generation: 1 } as never);
    expect(updates.getState().reloadNeeded).toBe(true);
    applyDaemonEvent(store, { type: "hello", daemon_version: "0.2.0", generation: 1 } as never);
    expect(updates.getState().reloadNeeded).toBe(false);
  });

  it("takes the page's own build version from the build when none is given", () => {
    const store = createStateStore({ daemon: { version: __OCTOBOARD_VERSION__, generation: 1 }, daemonUpdate: IDLE });
    const updates = createDaemonUpdateStore({ daemon: { store, request: vi.fn() as never }, reportFailure: () => {}, reportRequestFailure: () => {}, reportSaveFailure: () => {} });
    expect(updates.getState().reloadNeeded).toBe(false);
    applyDaemonEvent(store, { type: "hello", daemon_version: `${__OCTOBOARD_VERSION__}-other`, generation: 1 } as never);
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
    const manual = setup({ ...IDLE, available: AVAILABLE, manual: { reason: "not_writable", path: "/x", command: "install /x" } });
    await manual.updates.install();
    expect(none.requests).toEqual([]);
    expect(manual.requests).toEqual([]);
  });

  it("says nothing of a failed check it did not ask for", async () => {
    const { updates, send, failures } = setup();
    send({ ...IDLE, last_check: CHECKED(2, "failed") });
    await updates.check("automatic");
    send({ ...IDLE, last_check: CHECKED(3, "failed") });
    expect(failures).toEqual([]);
    expect(updates.getState().checkAsked).toBeUndefined();
  });
});

describe("a check the user asked for", () => {
  it("is pending until the daemon has replied, and is no longer once it refuses, reporting the refusal as a check's", async () => {
    let release!: () => void;
    const { updates, answers, refusals } = setup();
    answers.hold = new Promise<void>((resolve) => (release = resolve));
    answers.fail = new CodedError("unavailable", "daemon_update_unavailable", {});
    const done = updates.check("manual");
    expect(updates.getState().checkAsked).toBe("pending");
    release();
    await done;
    expect(updates.getState().checkAsked).toBeUndefined();
    expect(refusals).toEqual([{ error: answers.fail, action: "check" }]);
  });

  it("stays pending after the daemon replied that it started, until a check ends, and ends with no word for one that went well", async () => {
    const { updates, send, failures } = setup({ ...IDLE, last_check: CHECKED(5) });
    await updates.check("manual");
    expect(updates.getState().checkAsked).toBe("pending");
    send({ ...IDLE, phase: "checking", last_check: CHECKED(5) });
    expect(updates.getState().checkAsked).toBe("pending");
    send({ ...IDLE, last_check: CHECKED(6) });
    expect(updates.getState().checkAsked).toBeUndefined();
    expect(failures).toEqual([]);
  });

  it("is ended by the first check after the one that had ended before it started, with no timer deciding whose it is", async () => {
    fakeTimers();
    const { updates, send, answers } = setup(IDLE);
    answers.check = { started: true, last_check_at: 5 };
    send({ ...IDLE, last_check: CHECKED(5) });
    await updates.check("manual");
    await vi.advanceTimersByTimeAsync(10 * 60 * 1000);
    expect(updates.getState().checkAsked).toBe("pending");
    send({ ...IDLE, last_check: CHECKED(5) });
    expect(updates.getState().checkAsked).toBe("pending");
    send({ ...IDLE, last_check: CHECKED(9) });
    expect(updates.getState().checkAsked).toBeUndefined();
  });

  it("reports a failed one once, with the daemon's text and code", async () => {
    const { updates, send, failures, answers } = setup({ ...IDLE, last_check: CHECKED(5) });
    answers.check = { started: true, last_check_at: 5 };
    await updates.check("manual");
    send({ ...IDLE, last_check: CHECKED(6, "failed") });
    send({ ...IDLE, available: AVAILABLE, last_check: CHECKED(6, "failed") });
    expect(failures).toEqual([{ action: "check", reason: "failed", detail: "offline", code: "daemon_update_check_failed" }]);
    expect(updates.getState().checkAsked).toBeUndefined();
  });

  it("reports a failed one whose status arrived ahead of the reply", async () => {
    const { updates, send, failures, answers } = setup({ ...IDLE, last_check: CHECKED(5) });
    answers.check = { started: true, last_check_at: 5 };
    answers.beforeReply = () => send({ ...IDLE, last_check: CHECKED(6, "failed") });
    await updates.check("manual");
    expect(failures).toEqual([{ action: "check", reason: "failed", detail: "offline", code: "daemon_update_check_failed" }]);
    expect(updates.getState().checkAsked).toBeUndefined();
  });

  it("is ended with nothing said when an update installed meanwhile", async () => {
    const { updates, send, failures, answers } = setup({ ...IDLE, last_check: CHECKED(5) });
    answers.check = { started: true, last_check_at: 5 };
    await updates.check("manual");
    send({ ...IDLE, phase: "installed", installed: { version: "0.2.0", holder_floor: null }, last_check: CHECKED(5) });
    expect(updates.getState().checkAsked).toBeUndefined();
    expect(failures).toEqual([]);
  });

  it("drops the wait, with nothing said, when a fresh snapshot shows no check running, and keeps it for one that does", async () => {
    const running = setup({ ...IDLE, last_check: CHECKED(5) });
    running.answers.check = { started: true, last_check_at: 5 };
    await running.updates.check("manual");
    running.comesBack("0.1.0", { ...IDLE, phase: "checking", last_check: CHECKED(5) });
    expect(running.updates.getState().checkAsked).toBe("pending");
    running.send({ ...IDLE, last_check: CHECKED(6, "failed") });
    expect(running.failures).toHaveLength(1);

    const gone = setup({ ...IDLE, last_check: CHECKED(5) });
    gone.answers.check = { started: true, last_check_at: 5 };
    await gone.updates.check("manual");
    gone.comesBack("0.1.0", { ...IDLE, last_check: CHECKED(5) });
    expect(gone.updates.getState().checkAsked).toBeUndefined();
    gone.send({ ...IDLE, last_check: CHECKED(6, "failed") });
    expect(gone.failures).toEqual([]);
  });

  it("ends, with nothing said, a check the daemon was seen running that stopped without recording an end", async () => {
    const { updates, answers, send, failures } = setup({ ...IDLE, last_check: CHECKED(5) });
    answers.check = { started: true, last_check_at: 5 };
    await updates.check("manual");
    send({ ...IDLE, phase: "checking", last_check: CHECKED(5) });
    expect(updates.getState().checkAsked).toBe("pending");
    send({ ...IDLE, last_check: CHECKED(5) });
    expect(updates.getState().checkAsked).toBeUndefined();
    send({ ...IDLE, last_check: CHECKED(6, "failed") });
    expect(failures).toEqual([]);
  });

  it("keeps the button pending for a check the daemon let pass, then says the last check stands", async () => {
    fakeTimers();
    const { updates, answers, send, failures } = setup({ ...IDLE, last_check: CHECKED(5) });
    answers.check = { started: false, last_check_at: null };
    const done = updates.check("manual");
    expect(updates.getState().checkAsked).toBe("pending");
    await vi.advanceTimersByTimeAsync(0);
    expect(updates.getState().checkAsked).toBe("unchanged");
    await vi.runOnlyPendingTimersAsync();
    await done;
    expect(updates.getState().checkAsked).toBeUndefined();
    // A check it never started is no check of this press: a later failed one is the section's mark alone.
    send({ ...IDLE, last_check: CHECKED(6, "failed") });
    expect(failures).toEqual([]);
  });

  it("is not cleared by the end of an earlier press's hold when a later press has asked again meanwhile", async () => {
    fakeTimers();
    const { updates, answers } = setup({ ...IDLE, last_check: CHECKED(5) });
    answers.check = { started: false, last_check_at: null };
    const first = updates.check("manual");
    await vi.advanceTimersByTimeAsync(0);
    expect(updates.getState().checkAsked).toBe("unchanged");
    answers.check = { started: true, last_check_at: 5 };
    await updates.check("manual");
    expect(updates.getState().checkAsked).toBe("pending");
    await vi.runOnlyPendingTimersAsync();
    await first;
    expect(updates.getState().checkAsked).toBe("pending");
  });
});

describe("an install the user asked for", () => {
  it("shows the download begun at once, before any status, and sends one request for a double press", async () => {
    let release!: () => void;
    const { updates, answers, requests } = setup({ ...IDLE, available: AVAILABLE });
    answers.hold = new Promise<void>((resolve) => (release = resolve));
    const first = updates.install();
    expect(updates.getState()).toMatchObject({ phase: "downloading", progress: { downloaded: 0 } });
    await updates.install();
    expect(requests.filter((request) => request.type === "update_daemon")).toHaveLength(1);
    release();
    await first;
    // The daemon has replied and no status has shown the download yet.
    expect(updates.getState()).toMatchObject({ phase: "downloading", progress: { downloaded: 0 } });
    expect(requests.filter((request) => request.type === "update_daemon")).toHaveLength(1);
  });

  it("reports the failure of the install the daemon named in its reply, once, and none that is another window's", async () => {
    const { updates, send, failures } = setup({ ...IDLE, available: AVAILABLE });
    await updates.install();
    send({ ...IDLE, available: AVAILABLE, phase: "downloading", install: INSTALL("running", 3) });
    expect(failures).toEqual([]);
    send({ ...IDLE, available: AVAILABLE, install: INSTALL("failed", 9) });
    expect(failures).toEqual([]);
    send({ ...IDLE, available: AVAILABLE, install: INSTALL("failed", 3) });
    send({ ...IDLE, available: AVAILABLE, install: { ...INSTALL("failed", 3), finished_at: 99 } });
    expect(failures).toEqual([{ action: "install", reason: "failed", detail: "bad", code: "daemon_update_unverified" }]);
    expect(updates.getState().phase).toBe("idle");
  });

  it("says nothing when the install it asked for succeeds", async () => {
    const { updates, send, failures } = setup({ ...IDLE, available: AVAILABLE });
    await updates.install();
    send({ ...IDLE, phase: "installed", installed: { version: "0.2.0", holder_floor: null }, install: INSTALL("succeeded", 3) });
    expect(failures).toEqual([]);
    expect(updates.getState().phase).toBe("installed");
  });

  it("reports a failure whose status arrived ahead of the reply", async () => {
    const { updates, send, failures, answers } = setup({ ...IDLE, available: AVAILABLE });
    answers.beforeReply = () => send({ ...IDLE, available: AVAILABLE, install: INSTALL("failed", 3) });
    await updates.install();
    expect(failures).toEqual([{ action: "install", reason: "failed", detail: "bad", code: "daemon_update_unverified" }]);
  });

  it("reports an update the daemon holds none of any more as gone, and any other refusal as an install's, each ending the pending phase", async () => {
    const { updates, answers, refusals, failures } = setup({ ...IDLE, available: AVAILABLE });
    answers.fail = new CodedError("none", "daemon_update_nothing_to_install", {});
    await updates.install();
    expect(failures).toEqual([{ action: "install", reason: "gone", detail: "" }]);
    expect(refusals).toEqual([]);
    expect(updates.getState()).toMatchObject({ phase: "idle", progress: undefined });

    answers.fail = new CodedError("running", "daemon_update_running", {});
    await updates.install();
    expect(refusals).toEqual([{ error: answers.fail, action: "install" }]);
    expect(failures).toHaveLength(1);
    expect(updates.getState().phase).toBe("idle");
    // Not pending any more: the next press asks again.
    delete answers.fail;
    await updates.install();
    expect(updates.getState().phase).toBe("downloading");
  });

  it("drops the wait, with nothing said, when a fresh snapshot shows no download, and keeps it for one that does", async () => {
    const running = setup({ ...IDLE, available: AVAILABLE });
    await running.updates.install();
    running.comesBack("0.1.0", { ...IDLE, phase: "downloading", available: AVAILABLE, install: INSTALL("running", 3) });
    running.send({ ...IDLE, available: AVAILABLE, install: INSTALL("failed", 3) });
    expect(running.failures).toHaveLength(1);

    const gone = setup({ ...IDLE, available: AVAILABLE });
    await gone.updates.install();
    gone.comesBack("0.1.0", { ...IDLE, available: AVAILABLE });
    expect(gone.updates.getState().phase).toBe("idle");
    gone.send({ ...IDLE, available: AVAILABLE, install: INSTALL("failed", 3) });
    expect(gone.failures).toEqual([]);
  });

  it("reports an install that failed under another window's press only as the section's state, not as an alert", () => {
    const { send, failures, updates } = setup({ ...IDLE, available: AVAILABLE });
    send({ ...IDLE, available: AVAILABLE, install: INSTALL("failed", 3) });
    expect(failures).toEqual([]);
    expect(updates.getState().phase).toBe("idle");
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
    const { updates, requests } = setup(IDLE, UI_VERSION, false);
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

  it("is pending while the daemon's restart is awaited, which the exit flow having answered does not end, and the next hello does", async () => {
    const { updates, store } = setup(INSTALLED);
    updates.onRestartRequested(async () => {
      setRestartAwaited(store, true);
    });
    await updates.restart();
    expect(updates.getState().restarting).toBe(true);
    // A press while the restart is pending asks for nothing more.
    const handler = vi.fn(async () => {});
    updates.onRestartRequested(handler);
    await updates.restart();
    expect(handler).not.toHaveBeenCalled();
    applyDaemonEvent(store, { type: "hello", daemon_version: "0.2.0", generation: 1 } as never);
    expect(updates.getState().restarting).toBe(false);
  });

  it("stops being pending when the restart is no longer awaited, as when the daemon refused it", () => {
    const { updates, store } = setup(INSTALLED);
    setRestartAwaited(store, true);
    expect(updates.getState().restarting).toBe(true);
    setRestartAwaited(store, false);
    expect(updates.getState().restarting).toBe(false);
  });

  it("hides the restart notice for the installed version only: another version installed brings it back, and the same stays hidden", () => {
    const { updates, send } = setup(INSTALLED);
    expect(updates.getState().restartNoticeHidden).toBe(false);
    updates.hideRestartNotice();
    expect(updates.getState().restartNoticeHidden).toBe(true);
    send({ ...INSTALLED, last_check: CHECKED(5) });
    expect(updates.getState().restartNoticeHidden).toBe(true);
    send({ ...IDLE, phase: "installed", installed: { version: "0.3.0", holder_floor: "0.1.0" } });
    expect(updates.getState().restartNoticeHidden).toBe(false);
  });

  it("has the restart notice hidden only while an update is installed", () => {
    const { updates, send } = setup(INSTALLED);
    updates.hideRestartNotice();
    send(IDLE);
    expect(updates.getState().restartNoticeHidden).toBe(false);
  });
});
