import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { UpdaterError, type AvailableUpdate, type UpdateChannel, type UpdaterCapability, type UpdateProgress } from "../platform";
import { CHECK_INTERVAL_MS, createUpdateStore, FIRST_CHECK_DELAY_MS, type UpdateFailure } from "./updateStore";

const FOUND: AvailableUpdate = { version: "0.2.0", notes: "Fixes." };

/** An updater whose calls the test settles itself. */
function fakeUpdater() {
  const checks: Array<{ channel: UpdateChannel; settle: (result: AvailableUpdate | undefined | Error) => void }> = [];
  const installs: Array<{ progress: (progress: UpdateProgress) => void; settle: (error?: Error) => void }> = [];
  const updater: UpdaterCapability = {
    currentVersion: async () => "0.1.0",
    check: (channel) =>
      new Promise((resolve, reject) => {
        checks.push({ channel, settle: (result) => (result instanceof Error ? reject(result) : resolve(result)) });
      }),
    install: (onProgress) =>
      new Promise((resolve, reject) => {
        installs.push({ progress: onProgress, settle: (error) => (error ? reject(error) : resolve()) });
      }),
  };
  return { updater, checks, installs };
}

function setup(initial: UpdateChannel = "beta") {
  const fake = fakeUpdater();
  const failures: UpdateFailure[] = [];
  let channel = initial;
  const store = createUpdateStore({ updater: fake.updater, getChannel: () => channel, reportFailure: (failure) => failures.push(failure), now: () => 1000 });
  return { ...fake, failures, store, switchTo: (next: UpdateChannel) => (channel = next) };
}

const flush = () => Promise.resolve().then(() => Promise.resolve());

describe("checking", () => {
  it("reads the running version and checks the chosen channel", async () => {
    const { store, checks } = setup("stable");
    await flush();
    expect(store.getState().currentVersion).toBe("0.1.0");
    void store.check("manual");
    expect(store.getState().phase).toBe("checking");
    expect(checks.map((check) => check.channel)).toEqual(["stable"]);
  });

  it("offers the update it found and remembers how the check ended", async () => {
    const { store, checks } = setup();
    const done = store.check("automatic");
    checks[0].settle(FOUND);
    await done;
    expect(store.getState()).toMatchObject({ phase: "idle", update: FOUND, lastCheck: { outcome: "available", at: 1000 } });
  });

  it("says up to date, and withdraws an update a later check no longer finds", async () => {
    const { store, checks } = setup();
    let done = store.check("automatic");
    checks[0].settle(FOUND);
    await done;
    done = store.check("automatic");
    checks[1].settle(undefined);
    await done;
    expect(store.getState()).toMatchObject({ update: undefined, lastCheck: { outcome: "upToDate" } });
  });

  it("does not start a second check while one is under way", async () => {
    const { store, checks } = setup();
    void store.check("manual");
    await store.check("automatic");
    expect(checks).toHaveLength(1);
  });

  it("reports a failed manual check and only records a failed automatic one", async () => {
    const { store, checks, failures } = setup();
    let done = store.check("automatic");
    checks[0].settle(new UpdaterError("failed", "offline"));
    await done;
    expect(failures).toEqual([]);
    expect(store.getState()).toMatchObject({ phase: "idle", lastCheck: { outcome: "failed" } });
    done = store.check("manual");
    checks[1].settle(new UpdaterError("failed", "offline"));
    await done;
    expect(failures).toEqual([{ action: "check", reason: "failed", detail: "offline" }]);
  });
});

describe("a channel change during a check", () => {
  it("drops the result for the old channel and checks the new one", async () => {
    const { store, checks, switchTo } = setup("beta");
    const done = store.check("automatic");
    switchTo("stable");
    checks[0].settle({ version: "0.3.0-beta.1" });
    await flush();
    expect(checks.map((check) => check.channel)).toEqual(["beta", "stable"]);
    expect(store.getState().phase).toBe("checking");
    expect(store.getState().update).toBeUndefined();
    expect(store.getState().lastCheck).toBeUndefined();
    checks[1].settle(undefined);
    await done;
    expect(store.getState()).toMatchObject({ phase: "idle", update: undefined, lastCheck: { outcome: "upToDate" } });
  });

  it("does not report the failure of a check that asked about the old channel", async () => {
    const { store, checks, failures, switchTo } = setup("beta");
    const done = store.check("manual");
    switchTo("stable");
    checks[0].settle(new UpdaterError("failed", "offline"));
    await flush();
    checks[1].settle(FOUND);
    await done;
    expect(failures).toEqual([]);
    expect(store.getState().update).toEqual(FOUND);
  });
});

describe("an update offered from another channel", () => {
  it("is withdrawn, with its notice, as soon as the channel changes, and stays withdrawn when the new check fails", async () => {
    const { store, checks, switchTo } = setup("beta");
    let done = store.check("automatic");
    checks[0].settle(FOUND);
    await done;
    store.dismissNotice();
    switchTo("stable");
    done = store.check("automatic");
    expect(store.getState()).toMatchObject({ phase: "checking", update: undefined, updateChannel: undefined, noticeDismissed: false });
    checks[1].settle(new UpdaterError("failed", "offline"));
    await done;
    expect(store.getState().update).toBeUndefined();
  });

  it("is withdrawn even while a check on the old channel is still running", async () => {
    const { store, checks, switchTo } = setup("beta");
    let done = store.check("automatic");
    checks[0].settle(FOUND);
    await done;
    done = store.check("automatic");
    switchTo("stable");
    void store.check("automatic");
    expect(store.getState().update).toBeUndefined();
    checks[1].settle(undefined);
    await flush();
    checks[2].settle(undefined);
    await done;
  });

  it("stays offered while it is being downloaded, and remembers the channel it came from", async () => {
    const { store, checks, installs, switchTo } = setup("beta");
    const done = store.check("automatic");
    checks[0].settle(FOUND);
    await done;
    expect(store.getState().updateChannel).toBe("beta");
    void store.install();
    switchTo("stable");
    await store.check("automatic");
    expect(store.getState()).toMatchObject({ phase: "downloading", update: FOUND });
    installs[0].settle();
    await flush();
  });
});

describe("what the shell refuses", () => {
  it("takes a check refused as already installed as waiting for a restart", async () => {
    const { store, checks, failures } = setup();
    const done = store.check("manual");
    checks[0].settle(new UpdaterError("installed"));
    await done;
    expect(store.getState()).toMatchObject({ phase: "installed", installedVersion: undefined });
    expect(failures).toEqual([]);
  });

  it("treats a check refused as busy as nothing learned, and reports nothing", async () => {
    const { store, checks, failures } = setup();
    const done = store.check("manual");
    checks[0].settle(new UpdaterError("busy"));
    await done;
    expect(store.getState().phase).toBe("idle");
    expect(store.getState().lastCheck).toBeUndefined();
    expect(failures).toEqual([]);
  });
});

describe("the notice", () => {
  it("stays dismissed for the version dismissed and comes back for another", async () => {
    const { store, checks } = setup();
    let done = store.check("automatic");
    checks[0].settle(FOUND);
    await done;
    store.dismissNotice();
    done = store.check("automatic");
    checks[1].settle(FOUND);
    await done;
    expect(store.getState().noticeDismissed).toBe(true);
    done = store.check("automatic");
    checks[2].settle({ version: "0.3.0" });
    await done;
    expect(store.getState().noticeDismissed).toBe(false);
  });
});

describe("installing", () => {
  async function found() {
    const context = setup();
    const done = context.store.check("automatic");
    context.checks[0].settle(FOUND);
    await done;
    return context;
  }

  it("does nothing without an update found", async () => {
    const { store, installs } = setup();
    await store.install();
    expect(installs).toEqual([]);
  });

  it("shows the download's progress, then waits for a restart", async () => {
    const { store, installs } = await found();
    const done = store.install();
    expect(store.getState()).toMatchObject({ phase: "downloading", progress: { downloaded: 0 } });
    installs[0].progress({ downloaded: 50, total: 100 });
    expect(store.getState().progress).toEqual({ downloaded: 50, total: 100 });
    installs[0].settle();
    await done;
    expect(store.getState()).toMatchObject({ phase: "installed", installedVersion: "0.2.0", update: undefined, progress: undefined });
  });

  it("neither installs twice nor checks while downloading", async () => {
    const { store, installs, checks } = await found();
    void store.install();
    await store.install();
    await store.check("manual");
    expect(installs).toHaveLength(1);
    expect(checks).toHaveLength(1);
  });

  it("offers nothing more once an update is installed", async () => {
    const { store, installs, checks } = await found();
    const done = store.install();
    installs[0].settle();
    await done;
    await store.check("manual");
    await store.install();
    expect(checks).toHaveLength(1);
    expect(installs).toHaveLength(1);
  });

  it("keeps the update on offer after a failed install, and reports it", async () => {
    const { store, installs, failures } = await found();
    const done = store.install();
    installs[0].settle(new UpdaterError("failed", "disk full"));
    await done;
    expect(store.getState()).toMatchObject({ phase: "idle", update: FOUND, progress: undefined });
    expect(failures).toEqual([{ action: "install", reason: "failed", detail: "disk full" }]);
  });

  it("takes an install the shell says is already done as waiting for a restart", async () => {
    const { store, installs, failures } = await found();
    const done = store.install();
    installs[0].settle(new UpdaterError("installed"));
    await done;
    expect(store.getState()).toMatchObject({ phase: "installed", installedVersion: "0.2.0" });
    expect(failures).toEqual([]);
  });
});

describe("an install the shell refuses", () => {
  async function found() {
    const context = setup();
    const done = context.store.check("automatic");
    context.checks[0].settle(FOUND);
    await done;
    const install = context.store.install();
    return { ...context, install };
  }

  it("withdraws the update when the shell has nothing to install, and says so rather than blaming the connection", async () => {
    const { store, installs, failures, install } = await found();
    installs[0].settle(new UpdaterError("nothing_to_install"));
    await install;
    expect(store.getState()).toMatchObject({ phase: "idle", update: undefined, progress: undefined });
    expect(failures).toEqual([{ action: "install", reason: "gone", detail: "" }]);
  });

  it("keeps the update on offer without a report when the shell is busy", async () => {
    const { store, installs, failures, install } = await found();
    installs[0].settle(new UpdaterError("busy"));
    await install;
    expect(store.getState()).toMatchObject({ phase: "idle", update: FOUND });
    expect(failures).toEqual([]);
  });

  it("ignores progress that arrives once the install has settled", async () => {
    const { store, installs, install } = await found();
    installs[0].settle(new UpdaterError("failed", "disk full"));
    await install;
    installs[0].progress({ downloaded: 90, total: 100 });
    expect(store.getState().progress).toBeUndefined();
  });
});

describe("restarting", () => {
  async function installed() {
    const context = setup();
    let done = context.store.check("automatic");
    context.checks[0].settle(FOUND);
    await done;
    done = context.store.install();
    context.installs[0].settle();
    await done;
    return context;
  }

  it("asks the window's exit flow, which is the only thing that restarts", async () => {
    const { store } = await installed();
    const handler = vi.fn();
    store.onRestartRequested(handler);
    store.restart();
    expect(handler).toHaveBeenCalledOnce();
  });

  it("asks for nothing before an update is installed, or once the handler is gone", async () => {
    const { store } = setup();
    const handler = vi.fn();
    const off = store.onRestartRequested(handler);
    store.restart();
    expect(handler).not.toHaveBeenCalled();
    const ready = await installed();
    const later = vi.fn();
    ready.store.onRestartRequested(later)();
    ready.store.restart();
    expect(later).not.toHaveBeenCalled();
    off();
  });

  it("hides the restart notice with Later", async () => {
    const { store } = await installed();
    store.hideRestartNotice();
    expect(store.getState().restartNoticeHidden).toBe(true);
  });
});

describe("the schedule", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("checks shortly after it starts and then on every interval, until stopped", async () => {
    const { store, checks } = setup();
    const stop = store.start();
    expect(checks).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(FIRST_CHECK_DELAY_MS);
    expect(checks).toHaveLength(1);
    checks[0].settle(undefined);
    await vi.advanceTimersByTimeAsync(CHECK_INTERVAL_MS);
    expect(checks).toHaveLength(2);
    stop();
    checks[1].settle(undefined);
    await vi.advanceTimersByTimeAsync(CHECK_INTERVAL_MS * 2);
    expect(checks).toHaveLength(2);
  });

  it("starts the first check after the delay it is given", async () => {
    const { store, checks } = setup();
    const stop = store.start(1000);
    await vi.advanceTimersByTimeAsync(999);
    expect(checks).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(1);
    expect(checks).toHaveLength(1);
    stop();
  });

  it("checks automatically never when the delay is null", async () => {
    const { store, checks } = setup();
    store.start(null)();
    await vi.advanceTimersByTimeAsync(CHECK_INTERVAL_MS * 2);
    expect(checks).toHaveLength(0);
  });
});
