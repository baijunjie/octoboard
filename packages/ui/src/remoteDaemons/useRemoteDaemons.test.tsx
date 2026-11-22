// @vitest-environment jsdom
import { act, useEffect } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { LanguageProvider } from "../i18n/react";
import type { RemoteDaemonEntry, RemoteDaemonsCapability, RemoteDaemonStatus } from "../platform";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { mountHost } from "../test/mountHost";
import { useRemoteDaemons } from "./useRemoteDaemons";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const entryOf = (id: string): RemoteDaemonEntry => ({ id, name: id, host: `${id}.example.com`, port: 22, user: "u", auth: "device", keyPath: null, fingerprint: null, octoboarddPath: null, deviceKey: { declined: false, file: null, mark: null } });
const STAGING = entryOf("staging");
const LAB = entryOf("lab");

const list = vi.fn(async (): Promise<RemoteDaemonEntry[]> => [STAGING]);
const status = vi.fn(async (): Promise<RemoteDaemonStatus[]> => []);
const unsubscribe = vi.fn();
let changed: (() => void) | undefined;
beforeEach(() => {
  acknowledgeAllAlerts();
  list.mockReset().mockResolvedValue([STAGING]);
  status.mockReset().mockResolvedValue([]);
  unsubscribe.mockClear();
  changed = undefined;
});

const CAPABILITY: RemoteDaemonsCapability = {
  list: () => list(),
  status: () => status(),
  onChanged: (listener) => {
    changed = listener;
    return unsubscribe;
  },
  save: async (entry) => ({ entry: { ...entry, id: entry.id ?? "new", deviceKey: { declined: false, file: null, mark: null } }, keyRemoval: { line: { result: "not_installed" as const }, privateKeyLeft: null } }),
  remove: async () => ({ line: { result: "not_installed" as const }, privateKeyLeft: null }),
  setDeviceKey: async () => ({ line: { result: "not_installed" as const }, privateKeyLeft: null }),
  connect: async () => ({ outcome: "opened" }),
  disconnect: async () => {},
  onPageRequested: () => () => {},
  onHandedOver: () => () => {},
  showMainWindow: async () => {},
  showUpdates: async () => {},
};

type Result = ReturnType<typeof useRemoteDaemons>;
let latest: Result;
function Probe({ capability, onResult }: { capability: RemoteDaemonsCapability | undefined; onResult: (result: Result) => void }) {
  const result = useRemoteDaemons(capability);
  useEffect(() => {
    onResult(result);
  });
  return null;
}

async function mount(capability: RemoteDaemonsCapability | null = CAPABILITY) {
  let container!: ReturnType<typeof mountHost>;
  await act(async () => {
    container = mountHost(<LanguageProvider><Probe capability={capability ?? undefined} onResult={(result) => (latest = result)} /></LanguageProvider>);
  });
  return container;
}

it("reads the saved entries and which are connected once mounted", async () => {
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
  await mount();
  expect(latest.entries).toEqual([STAGING]);
  expect(latest.statuses).toEqual([{ id: "staging", state: "connected", via: "window" }]);
});

it("reads both again whenever the shell says they changed, and when reload is called", async () => {
  await mount();
  list.mockResolvedValue([STAGING, LAB]);
  status.mockResolvedValue([{ id: "lab", state: "reconnecting", via: "window" }]);
  await act(async () => changed!());
  expect(latest.entries).toEqual([STAGING, LAB]);
  expect(latest.statuses).toEqual([{ id: "lab", state: "reconnecting", via: "window" }]);
  list.mockResolvedValue([LAB]);
  await act(async () => latest.reload());
  expect(latest.entries).toEqual([LAB]);
});

it("stops listening to the shell once unmounted", async () => {
  const container = await mount();
  expect(unsubscribe).not.toHaveBeenCalled();
  await act(async () => container.render(null));
  expect(unsubscribe).toHaveBeenCalledOnce();
});

it("raises an alert when the list cannot be read, and leaves the list as it was", async () => {
  await mount();
  list.mockRejectedValueOnce(new Error("store unreadable"));
  await act(async () => changed!());
  expect(errorAlerts.getSnapshot()[0]!.title).toBe("Could not read the remote daemons");
  expect(latest.entries).toEqual([STAGING]);
});

it("leaves the connection states as they were, with no alert, when only their read fails", async () => {
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
  await mount();
  status.mockRejectedValueOnce(new Error("busy"));
  await act(async () => changed!());
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  expect(latest.statuses).toEqual([{ id: "staging", state: "connected", via: "window" }]);
});

it("has no entries and reads nothing where the platform has no remote daemons", async () => {
  await mount(null);
  expect(latest).toMatchObject({ entries: [], statuses: [] });
  expect(list).not.toHaveBeenCalled();
});

it("answers a reload once both reads have ended, whether or not they succeeded", async () => {
  await mount();
  let answerList!: (entries: RemoteDaemonEntry[]) => void;
  let answerStatus!: (statuses: RemoteDaemonStatus[]) => void;
  list.mockImplementationOnce(() => new Promise((resolve) => (answerList = resolve)));
  status.mockImplementationOnce(() => new Promise((resolve) => (answerStatus = resolve)));
  let ended = false;
  let reloading!: Promise<void>;
  await act(async () => {
    reloading = latest.reload().then(() => void (ended = true));
  });
  await act(async () => answerList([LAB]));
  expect(ended).toBe(false);
  await act(async () => answerStatus([]));
  await reloading;
  expect(ended).toBe(true);
  expect(latest.entries).toEqual([LAB]);

  // A read that fails ends the reload as well, which does not throw.
  list.mockRejectedValueOnce(new Error("store unreadable"));
  status.mockRejectedValueOnce(new Error("busy"));
  await act(async () => latest.reload());
  expect(latest.entries).toEqual([LAB]);
});
