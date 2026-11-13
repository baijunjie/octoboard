import { describe, expect, it } from "vitest";

import type { DaemonUpdateStatus, Event } from "../protocol";
import { applyDaemonEvent } from "./actions";
import { createStateStore } from "./state";

const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };
const hello = { type: "hello", daemon_version: "0.2.0", generation: 1 } as unknown as Event;
const status = (next: DaemonUpdateStatus): Event => ({ type: "daemon_update_status_updated", status: next });

describe("the daemon's update status after a greeting", () => {
  it("is held as the previous connection's from the hello until the status of the one that greeted", () => {
    const store = createStateStore({ daemonUpdate: IDLE });
    expect(store.getState().daemonUpdateStale).toBe(false);
    applyDaemonEvent(store, hello);
    expect(store.getState().daemonUpdateStale).toBe(true);
    expect(store.getState().daemonUpdate).toBe(IDLE);
  });

  it("is ended by a status event, even one that says what was held", () => {
    const store = createStateStore({ daemonUpdate: IDLE });
    applyDaemonEvent(store, hello);
    applyDaemonEvent(store, status({ ...IDLE }));
    expect(store.getState().daemonUpdateStale).toBe(false);
    expect(store.getState().daemonUpdate).toEqual(IDLE);
  });

  it("is ended by a snapshot, whatever it says of the update", () => {
    const store = createStateStore({ daemonUpdate: IDLE });
    applyDaemonEvent(store, hello);
    applyDaemonEvent(store, {
      type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
      agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [], daemon_update: null,
      home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
    } as unknown as Event);
    expect(store.getState()).toMatchObject({ daemonUpdateStale: false, daemonUpdate: null });
  });

  it("keeps the state it holds when a status event repeats it outside a greeting", () => {
    const store = createStateStore({ daemonUpdate: IDLE });
    const before = store.getState();
    applyDaemonEvent(store, status({ ...IDLE }));
    expect(store.getState()).toBe(before);
  });
});
