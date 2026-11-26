// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { flushPromises } from "../test/clock";
import { LanguageProvider } from "../i18n/react";
import type { LaunchAtLoginCapability, LaunchAtLoginStatus, PlatformAdapter } from "../platform";
import { MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { preloadLaunchAtLogin, useLaunchAtLogin, type LaunchAtLoginRead } from "./useLaunchAtLogin";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});

beforeEach(() => report.mockClear());

/** A capability of its own per test: the last answer each one gave is kept for the module's life, by capability. */
function capability(status: () => Promise<LaunchAtLoginStatus>, set: LaunchAtLoginCapability["set"] = vi.fn()): LaunchAtLoginCapability {
  return { status: vi.fn(status), set, openSystemSettings: vi.fn(async () => {}) };
}

/** A read of the system that has not answered, and does not. */
const unanswered = () => new Promise<LaunchAtLoginStatus>(() => {});

/** Mounts the hook as the Settings dialog does, and returns what it holds now, with a way to close the dialog again. */
function openDialog(launchAtLogin: LaunchAtLoginCapability) {
  let read!: LaunchAtLoginRead;
  function Probe() {
    read = useLaunchAtLogin();
    return null;
  }
  const platform: PlatformAdapter = { kind: "tauri", host: MAC_HOST, launchAtLogin };
  const container = mountHost(<LanguageProvider><Probe /></LanguageProvider>, { platform });
  return { read: () => read, close: () => { container.render(null); } };
}

it("starts from the state a preload read, so the dialog has the row on its first render", async () => {
  const system = capability(() => Promise.resolve("enabled"));
  preloadLaunchAtLogin(system);
  await flushPromises();
  // The dialog's own read has not answered: what it shows on the first render is from the preload.
  vi.mocked(system.status).mockImplementation(unanswered);
  expect(openDialog(system).read()).toMatchObject({ shown: true, status: "enabled" });
});

it("has no row on the first render of a system never read, which shows once it answers", async () => {
  const dialog = openDialog(capability(() => Promise.resolve("disabled")));
  expect(dialog.read()).toMatchObject({ shown: false, status: undefined });
  await flushPromises();
  expect(dialog.read()).toMatchObject({ shown: true, status: "disabled" });
});

it("starts from the state the dialog read at an earlier opening", async () => {
  let answer: LaunchAtLoginStatus = "requiresApproval";
  const system = capability(() => Promise.resolve(answer));
  const first = openDialog(system);
  await flushPromises();
  first.close();
  answer = "enabled";
  vi.mocked(system.status).mockImplementation(unanswered);
  expect(openDialog(system).read()).toMatchObject({ shown: true, status: "requiresApproval" });
});

it("starts from the state the dialog set at an earlier opening", async () => {
  const set = vi.fn<LaunchAtLoginCapability["set"]>(() => Promise.resolve("enabled"));
  const system = capability(() => Promise.resolve("disabled"), set);
  const first = openDialog(system);
  await flushPromises();
  await act(async () => first.read().change(true));
  first.close();
  vi.mocked(system.status).mockImplementation(unanswered);
  expect(openDialog(system).read()).toMatchObject({ shown: true, status: "enabled" });
});

it("starts from the state the dialog read after the system refused a change", async () => {
  const system = capability(() => Promise.resolve("disabled"), vi.fn(() => Promise.reject(new Error("no"))));
  const first = openDialog(system);
  await flushPromises();
  vi.mocked(system.status).mockImplementation(() => Promise.resolve("requiresApproval"));
  await act(async () => first.read().change(true));
  first.close();
  vi.mocked(system.status).mockImplementation(unanswered);
  expect(openDialog(system).read()).toMatchObject({ shown: true, status: "requiresApproval" });
});

it("moves the switch when the read of a later opening finds the state changed", async () => {
  const system = capability(() => Promise.resolve("disabled"));
  const first = openDialog(system);
  await flushPromises();
  first.close();
  vi.mocked(system.status).mockImplementation(() => Promise.resolve("enabled"));
  const second = openDialog(system);
  expect(second.read().status).toBe("disabled");
  await flushPromises();
  expect(second.read().status).toBe("enabled");
});

it("keeps the state a later read found for the opening after it", async () => {
  const system = capability(() => Promise.resolve("disabled"));
  const first = openDialog(system);
  await flushPromises();
  first.close();
  vi.mocked(system.status).mockImplementation(() => Promise.resolve("enabled"));
  const second = openDialog(system);
  await flushPromises();
  second.close();
  vi.mocked(system.status).mockImplementation(unanswered);
  expect(openDialog(system).read().status).toBe("enabled");
});

it("keeps nothing of a read that failed: the next opening has no row until the system answers", async () => {
  const system = capability(() => Promise.reject(new Error("no such command")));
  const first = openDialog(system);
  await flushPromises();
  // The dialog shows the switch off meanwhile, which a press then tries to change; that is not the system's answer.
  expect(first.read()).toMatchObject({ shown: true, status: "disabled" });
  first.close();
  vi.mocked(system.status).mockImplementation(unanswered);
  expect(openDialog(system).read()).toMatchObject({ shown: false, status: undefined });
});

it("keeps nothing of a preload that failed: the dialog then reads as it opens", async () => {
  const system = capability(() => Promise.reject(new Error("no such command")));
  preloadLaunchAtLogin(system);
  await flushPromises();
  vi.mocked(system.status).mockImplementation(() => Promise.resolve("enabled"));
  const dialog = openDialog(system);
  expect(dialog.read()).toMatchObject({ shown: false, status: undefined });
  await flushPromises();
  expect(dialog.read()).toMatchObject({ shown: true, status: "enabled" });
});

it("reads once per capability however often it is asked to preload, and once more for another", async () => {
  const system = capability(() => Promise.resolve("enabled"));
  const other = capability(() => Promise.resolve("disabled"));
  preloadLaunchAtLogin(system);
  preloadLaunchAtLogin(system);
  await flushPromises();
  preloadLaunchAtLogin(system);
  preloadLaunchAtLogin(other);
  await flushPromises();
  expect(system.status).toHaveBeenCalledOnce();
  expect(other.status).toHaveBeenCalledOnce();
  vi.mocked(system.status).mockImplementation(unanswered);
  vi.mocked(other.status).mockImplementation(unanswered);
  expect(openDialog(system).read().status).toBe("enabled");
  expect(openDialog(other).read().status).toBe("disabled");
});

it("does not read again when it is asked to preload after the dialog has recorded the state", async () => {
  const system = capability(() => Promise.resolve("enabled"));
  const dialog = openDialog(system);
  await flushPromises();
  dialog.close();
  preloadLaunchAtLogin(system);
  await flushPromises();
  expect(system.status).toHaveBeenCalledOnce();
});

it("drops a preload's answer that comes after the dialog recorded a newer state", async () => {
  let late!: (status: LaunchAtLoginStatus) => void;
  const system = capability(() => new Promise<LaunchAtLoginStatus>((resolve) => (late = resolve)));
  preloadLaunchAtLogin(system);
  // The dialog opens while the preload is out, and its own read answers first.
  vi.mocked(system.status).mockImplementation(() => Promise.resolve("enabled"));
  const first = openDialog(system);
  await flushPromises();
  expect(first.read().status).toBe("enabled");
  first.close();
  await act(async () => late("disabled"));
  vi.mocked(system.status).mockImplementation(unanswered);
  expect(openDialog(system).read().status).toBe("enabled");
});
