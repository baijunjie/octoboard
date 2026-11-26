// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { flushPromises } from "../test/clock";
import type { MacosPermissionsCapability, MacosPermissionsStatus, PlatformAdapter } from "../platform";
import { MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { preloadMacosPermissions, useMacosPermissionsRead, type MacosPermissionsRead } from "./useMacosPermissions";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const BOTH_GRANTED = { screenRecording: true, accessibility: true } satisfies MacosPermissionsStatus;
const SCREEN_RECORDING_MISSING = { screenRecording: false, accessibility: true } satisfies MacosPermissionsStatus;

/** A capability of its own per test: the last report each one gave is kept for the module's life, by capability. */
function capability(status: () => Promise<MacosPermissionsStatus | undefined>): MacosPermissionsCapability {
  return { status: vi.fn(status), request: vi.fn(async () => true), openSystemSettings: vi.fn(async () => {}) };
}

/** A read of the shell that has not answered, and does not. */
const unanswered = () => new Promise<MacosPermissionsStatus | undefined>(() => {});

/** Mounts the dialog's read as the Settings dialog does, and returns what it holds now, with a way to close the dialog
 * again. */
function openDialog(macosPermissions: MacosPermissionsCapability | undefined) {
  let read!: MacosPermissionsRead;
  function Probe() {
    read = useMacosPermissionsRead();
    return null;
  }
  const platform: PlatformAdapter = { kind: "tauri", host: MAC_HOST, macosPermissions };
  const container = mountHost(<Probe />, { platform });
  return { read: () => read, close: () => { container.render(null); } };
}

beforeEach(() => {
  vi.clearAllMocks();
});

it("starts from the report a preload read, so the dialog has the rows on its first render", async () => {
  const shell = capability(() => Promise.resolve(SCREEN_RECORDING_MISSING));
  preloadMacosPermissions(shell);
  await flushPromises();
  // The dialog's own read has not answered: what it shows on the first render is from the preload.
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read()).toMatchObject({ shown: true, loaded: true, status: SCREEN_RECORDING_MISSING });
});

it("has no rows on the first render of a shell never read, which show once it answers", async () => {
  const dialog = openDialog(capability(() => Promise.resolve(BOTH_GRANTED)));
  expect(dialog.read()).toMatchObject({ shown: false, loaded: false, status: undefined });
  await flushPromises();
  expect(dialog.read()).toMatchObject({ shown: true, loaded: true, status: BOTH_GRANTED });
});

it("has nothing to start from, and nothing loaded, where the platform has no capability", async () => {
  const dialog = openDialog(undefined);
  expect(dialog.read()).toMatchObject({ shown: false, loaded: false, status: undefined });
  await flushPromises();
  expect(dialog.read()).toMatchObject({ shown: false, status: undefined });
});

it("starts from the report the dialog read at an earlier opening", async () => {
  const shell = capability(() => Promise.resolve(SCREEN_RECORDING_MISSING));
  const first = openDialog(shell);
  await flushPromises();
  first.close();
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read()).toMatchObject({ shown: true, loaded: true, status: SCREEN_RECORDING_MISSING });
});

it("reads again as the dialog opens, though it starts from a report", async () => {
  const shell = capability(() => Promise.resolve(BOTH_GRANTED));
  preloadMacosPermissions(shell);
  await flushPromises();
  expect(shell.status).toHaveBeenCalledTimes(1);
  openDialog(shell);
  await flushPromises();
  expect(shell.status).toHaveBeenCalledTimes(2);
});

it("moves the rows when the read of a later opening finds the grants changed", async () => {
  const shell = capability(() => Promise.resolve(SCREEN_RECORDING_MISSING));
  const first = openDialog(shell);
  await flushPromises();
  first.close();
  vi.mocked(shell.status).mockImplementation(() => Promise.resolve(BOTH_GRANTED));
  const second = openDialog(shell);
  expect(second.read().status).toEqual(SCREEN_RECORDING_MISSING);
  await flushPromises();
  expect(second.read().status).toEqual(BOTH_GRANTED);
});

it("keeps the report a later read found for the opening after it", async () => {
  const shell = capability(() => Promise.resolve(SCREEN_RECORDING_MISSING));
  const first = openDialog(shell);
  await flushPromises();
  first.close();
  vi.mocked(shell.status).mockImplementation(() => Promise.resolve(BOTH_GRANTED));
  const second = openDialog(shell);
  await flushPromises();
  second.close();
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read().status).toEqual(BOTH_GRANTED);
});

it("keeps the report a refresh found, for the opening after it", async () => {
  const shell = capability(() => Promise.resolve(SCREEN_RECORDING_MISSING));
  const first = openDialog(shell);
  await flushPromises();
  vi.mocked(shell.status).mockImplementation(() => Promise.resolve(BOTH_GRANTED));
  await act(async () => first.read().refresh());
  first.close();
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read().status).toEqual(BOTH_GRANTED);
});

it("counts a report of nothing as loaded: no rows, but not a read still in flight", async () => {
  const shell = capability(() => Promise.resolve(undefined));
  const first = openDialog(shell);
  await flushPromises();
  expect(first.read()).toMatchObject({ shown: false, loaded: true, status: undefined });
  first.close();
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read()).toMatchObject({ shown: false, loaded: true, status: undefined });
});

it("counts a preloaded report of nothing as loaded on the first render", async () => {
  const shell = capability(() => Promise.resolve(undefined));
  preloadMacosPermissions(shell);
  await flushPromises();
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read()).toMatchObject({ shown: false, loaded: true, status: undefined });
});

it("keeps nothing of a read that failed: the next opening has no rows and is not loaded until the shell answers", async () => {
  const shell = capability(() => Promise.reject(new Error("no such command")));
  const first = openDialog(shell);
  await flushPromises();
  expect(first.read()).toMatchObject({ unreadable: true, loaded: true, shown: false });
  first.close();
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read()).toMatchObject({ shown: false, loaded: false, status: undefined });
});

it("keeps nothing of a read that threw as it started", async () => {
  const shell = capability(() => { throw new Error("no such command"); });
  const first = openDialog(shell);
  await flushPromises();
  expect(first.read()).toMatchObject({ unreadable: true, loaded: true, shown: false });
  first.close();
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read()).toMatchObject({ shown: false, loaded: false });
});

it("drops the earlier report when the read of a later opening fails: the next opening has no rows until the shell answers", async () => {
  const shell = capability(() => Promise.resolve(SCREEN_RECORDING_MISSING));
  const first = openDialog(shell);
  await flushPromises();
  first.close();
  vi.mocked(shell.status).mockImplementation(() => Promise.reject(new Error("no")));
  const second = openDialog(shell);
  // Until the failing read answers, the opening starts from the earlier report.
  expect(second.read()).toMatchObject({ shown: true, status: SCREEN_RECORDING_MISSING });
  await flushPromises();
  expect(second.read()).toMatchObject({ unreadable: true, shown: false });
  second.close();
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read()).toMatchObject({ shown: false, loaded: false, status: undefined });
});

it("drops the earlier report when a refresh fails", async () => {
  const shell = capability(() => Promise.resolve(SCREEN_RECORDING_MISSING));
  const first = openDialog(shell);
  await flushPromises();
  vi.mocked(shell.status).mockImplementation(() => Promise.reject(new Error("no")));
  await act(async () => first.read().refresh());
  first.close();
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read()).toMatchObject({ shown: false, loaded: false });
});

it("keeps the earlier report when a failed read comes after the dialog was closed", async () => {
  let fail!: (error: Error) => void;
  const shell = capability(() => Promise.resolve(SCREEN_RECORDING_MISSING));
  const first = openDialog(shell);
  await flushPromises();
  first.close();
  vi.mocked(shell.status).mockImplementation(() => new Promise<MacosPermissionsStatus | undefined>((_resolve, reject) => (fail = reject)));
  const second = openDialog(shell);
  await flushPromises();
  second.close();
  await act(async () => fail(new Error("no")));
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read()).toMatchObject({ shown: true, status: SCREEN_RECORDING_MISSING });
});

it("does not let a failed read replace the report of a newer read", async () => {
  let fail!: (error: Error) => void;
  const shell = capability(() => Promise.resolve(SCREEN_RECORDING_MISSING));
  const dialog = openDialog(shell);
  await flushPromises();
  vi.mocked(shell.status).mockImplementation(() => new Promise<MacosPermissionsStatus | undefined>((_resolve, reject) => (fail = reject)));
  let older!: Promise<void>;
  await act(async () => { older = dialog.read().refresh(); });
  vi.mocked(shell.status).mockImplementation(() => Promise.resolve(BOTH_GRANTED));
  await act(async () => dialog.read().refresh());
  await act(async () => { fail(new Error("no")); await older; });
  dialog.close();
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read()).toMatchObject({ shown: true, status: BOTH_GRANTED });
});

it.each([
  ["rejected", () => Promise.reject(new Error("no such command"))],
  ["threw as it started", () => { throw new Error("no such command"); }],
])("keeps nothing of a preload that %s: the dialog then reads as it opens", async (_name, failure) => {
  const shell = capability(failure);
  preloadMacosPermissions(shell);
  await flushPromises();
  vi.mocked(shell.status).mockImplementation(() => Promise.resolve(BOTH_GRANTED));
  const dialog = openDialog(shell);
  expect(dialog.read()).toMatchObject({ shown: false, loaded: false, status: undefined });
  await flushPromises();
  expect(dialog.read()).toMatchObject({ shown: true, loaded: true, status: BOTH_GRANTED });
});

it("reads once per capability however often it is asked to preload, and once more for another", async () => {
  const shell = capability(() => Promise.resolve(BOTH_GRANTED));
  const other = capability(() => Promise.resolve(SCREEN_RECORDING_MISSING));
  preloadMacosPermissions(shell);
  preloadMacosPermissions(shell);
  await flushPromises();
  preloadMacosPermissions(shell);
  preloadMacosPermissions(other);
  await flushPromises();
  expect(shell.status).toHaveBeenCalledOnce();
  expect(other.status).toHaveBeenCalledOnce();
  vi.mocked(shell.status).mockImplementation(unanswered);
  vi.mocked(other.status).mockImplementation(unanswered);
  expect(openDialog(shell).read().status).toEqual(BOTH_GRANTED);
  expect(openDialog(other).read().status).toEqual(SCREEN_RECORDING_MISSING);
});

it("reads once for a preload asked for again while its first read is unanswered", async () => {
  const shell = capability(unanswered);
  preloadMacosPermissions(shell);
  await flushPromises();
  preloadMacosPermissions(shell);
  await flushPromises();
  expect(shell.status).toHaveBeenCalledOnce();
});

it("does not read when it is asked to preload after the dialog has recorded a report", async () => {
  const shell = capability(() => Promise.resolve(BOTH_GRANTED));
  const dialog = openDialog(shell);
  await flushPromises();
  dialog.close();
  preloadMacosPermissions(shell);
  await flushPromises();
  expect(shell.status).toHaveBeenCalledOnce();
});

it("drops a preload's answer that comes after the dialog recorded a report", async () => {
  let late!: (status: MacosPermissionsStatus) => void;
  const shell = capability(() => new Promise<MacosPermissionsStatus>((resolve) => (late = resolve)));
  preloadMacosPermissions(shell);
  await flushPromises();
  // The dialog opens while the preload is out, and its own read answers first.
  vi.mocked(shell.status).mockImplementation(() => Promise.resolve(BOTH_GRANTED));
  const first = openDialog(shell);
  await flushPromises();
  expect(first.read().status).toEqual(BOTH_GRANTED);
  first.close();
  await act(async () => late(SCREEN_RECORDING_MISSING));
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read().status).toEqual(BOTH_GRANTED);
});

it("leaves the report the dialog recorded when a preload that was out meanwhile fails", async () => {
  let fail!: (error: Error) => void;
  const shell = capability(() => new Promise<MacosPermissionsStatus>((_resolve, reject) => (fail = reject)));
  preloadMacosPermissions(shell);
  await flushPromises();
  vi.mocked(shell.status).mockImplementation(() => Promise.resolve(BOTH_GRANTED));
  const first = openDialog(shell);
  await flushPromises();
  first.close();
  await act(async () => fail(new Error("no")));
  vi.mocked(shell.status).mockImplementation(unanswered);
  expect(openDialog(shell).read()).toMatchObject({ shown: true, status: BOTH_GRANTED });
});
