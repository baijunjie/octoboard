// @vitest-environment jsdom
// Loaded first, as `main.tsx` loads it, so a guard that swallows the window's focus events fails this test.
import "../input/focusGuard";
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { ActionableError } from "../errors/actionableError";
import { LanguageProvider } from "../i18n/react";
import type { LaunchAtLoginCapability, LaunchAtLoginStatus, PlatformAdapter } from "../platform";
import { BROWSER_HOST, MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { LaunchAtLoginSetting } from "./LaunchAtLoginSetting";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});

beforeEach(() => report.mockClear());

/** A capability whose `openSystemSettings` does nothing unless `capability` has one of its own. */
const login = (capability: Pick<LaunchAtLoginCapability, "status" | "set"> & Partial<LaunchAtLoginCapability>): LaunchAtLoginCapability => ({ openSystemSettings: vi.fn(async () => {}), ...capability });

function mount(launchAtLogin?: LaunchAtLoginCapability) {
  const platform: PlatformAdapter = { kind: "tauri", host: MAC_HOST, launchAtLogin };
  return mountHost(
    <LanguageProvider>
      <LaunchAtLoginSetting />
    </LanguageProvider>,
    { platform },
  );
}

const toggleOf = (container: HTMLElement) => container.querySelector<HTMLInputElement>("input[type=checkbox]");

it("has no row without the capability, as in a browser", async () => {
  const container = mountHost(
    <LanguageProvider>
      <LaunchAtLoginSetting />
    </LanguageProvider>,
    { platform: { kind: "browser", host: BROWSER_HOST } },
  );
  await act(async () => {});
  expect(toggleOf(container)).toBeNull();
});

it.each<{ status: LaunchAtLoginStatus; checked: boolean; text: string }>([
  { status: "disabled", checked: false, text: "Once this is on" },
  { status: "enabled", checked: true, text: "opens when you log in" },
  { status: "requiresApproval", checked: true, text: "waiting for your approval" },
])("shows the system's own state when it is $status", async ({ status, checked, text }) => {
  const container = mount(login({ status: () => Promise.resolve(status), set: vi.fn() }));
  await act(async () => {});
  expect(toggleOf(container)!.checked).toBe(checked);
  expect(container.textContent).toContain(text);
});

it("reads the state again when the window is focused, as the user can change it in the system's list", async () => {
  const status = vi.fn<() => Promise<LaunchAtLoginStatus>>(() => Promise.resolve("disabled"));
  const container = mount(login({ status, set: vi.fn() }));
  await act(async () => {});
  expect(toggleOf(container)!.checked).toBe(false);
  status.mockImplementation(() => Promise.resolve("enabled"));
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
  });
  expect(toggleOf(container)!.checked).toBe(true);
});

it("asks the system to register and shows what it answers", async () => {
  const set = vi.fn<(enabled: boolean) => Promise<LaunchAtLoginStatus>>(() => Promise.resolve("requiresApproval"));
  const container = mount(login({ status: () => Promise.resolve("disabled"), set }));
  await act(async () => {});
  await act(async () => {
    toggleOf(container)!.click();
  });
  expect(set).toHaveBeenCalledExactlyOnceWith(true);
  expect(toggleOf(container)!.checked).toBe(true);
  expect(container.textContent).toContain("waiting for your approval");
});

it("raises an error alert when the system refuses, and leaves the switch as it was", async () => {
  const set = vi.fn(() => Promise.reject("the system said no"));
  const container = mount(login({ status: () => Promise.resolve("disabled"), set }));
  await act(async () => {});
  await act(async () => {
    toggleOf(container)!.click();
  });
  expect(toggleOf(container)!.checked).toBe(false);
  expect(report).toHaveBeenCalledOnce();
  expect(report.mock.calls[0]![0]).toMatchObject({ title: "Could not change Launch at login" });
});

it("raises the refusal as an unexpected failure, keeping what the system said as the error to report", async () => {
  const container = mount(login({ status: () => Promise.resolve("disabled"), set: vi.fn(() => Promise.reject(new Error("SMAppService said no"))) }));
  await act(async () => {});
  await act(async () => {
    toggleOf(container)!.click();
  });
  const { error } = report.mock.calls[0]![0];
  expect(error).not.toBeInstanceOf(ActionableError);
  expect((error as Error).message).toBe("SMAppService said no");
});

it("reads the state again after a refusal, as the system may have changed before it refused", async () => {
  const status = vi.fn<() => Promise<LaunchAtLoginStatus>>(() => Promise.resolve("disabled"));
  const container = mount(login({ status, set: vi.fn(() => Promise.reject("no")) }));
  await act(async () => {});
  status.mockImplementation(() => Promise.resolve("requiresApproval"));
  await act(async () => {
    toggleOf(container)!.click();
  });
  expect(toggleOf(container)!.checked).toBe(true);
  expect(container.textContent).toContain("waiting for your approval");
});

it("does not let a read from a window focus show the state from before a change that is still out", async () => {
  let answer!: (status: LaunchAtLoginStatus) => void;
  const set = vi.fn(() => new Promise<LaunchAtLoginStatus>((resolve) => (answer = resolve)));
  const status = vi.fn<() => Promise<LaunchAtLoginStatus>>(() => Promise.resolve("disabled"));
  const container = mount(login({ status, set }));
  await act(async () => {});
  await act(async () => {
    toggleOf(container)!.click();
  });
  status.mockClear();
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
  });
  expect(status).not.toHaveBeenCalled();
  await act(async () => answer("enabled"));
  expect(toggleOf(container)!.checked).toBe(true);
  // Once the change is done, a focus reads again.
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
  });
  expect(status).toHaveBeenCalledOnce();
});

it("drops a read that began before a change and answers after it began", async () => {
  let late!: (status: LaunchAtLoginStatus) => void;
  const status = vi.fn<() => Promise<LaunchAtLoginStatus>>(() => Promise.resolve("disabled"));
  const container = mount(login({ status, set: vi.fn((): Promise<LaunchAtLoginStatus> => Promise.resolve("enabled")) }));
  await act(async () => {});
  status.mockImplementationOnce(() => new Promise<LaunchAtLoginStatus>((resolve) => (late = resolve)));
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
  });
  await act(async () => {
    toggleOf(container)!.click();
  });
  await act(async () => late("disabled"));
  expect(toggleOf(container)!.checked).toBe(true);
});

const openButton = (container: HTMLElement) => Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "Open System Settings");

it("offers a button that opens the system's list of login items only while the registration awaits approval", async () => {
  const openSystemSettings = vi.fn(async () => {});
  const container = mount(login({ status: () => Promise.resolve("enabled"), set: vi.fn(), openSystemSettings }));
  await act(async () => {});
  expect(openButton(container)).toBeUndefined();
  const waiting = mount(login({ status: () => Promise.resolve("requiresApproval"), set: vi.fn(), openSystemSettings }));
  await act(async () => {});
  await act(async () => openButton(waiting)!.click());
  expect(openSystemSettings).toHaveBeenCalledOnce();
});

it("raises an error alert when the list does not open", async () => {
  const container = mount(login({ status: () => Promise.resolve("requiresApproval"), set: vi.fn(), openSystemSettings: vi.fn(() => Promise.reject(new Error("no pane"))) }));
  await act(async () => {});
  await act(async () => openButton(container)!.click());
  expect(report.mock.calls[0]![0]).toMatchObject({ title: "Could not open System Settings" });
});

it("hands focus to the switch when the button it was on goes with the approval", async () => {
  const status = vi.fn<() => Promise<LaunchAtLoginStatus>>(() => Promise.resolve("requiresApproval"));
  const container = mount(login({ status, set: vi.fn() }));
  await act(async () => {});
  act(() => openButton(container)!.focus());
  status.mockImplementation(() => Promise.resolve("enabled"));
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
  });
  expect(openButton(container)).toBeUndefined();
  expect(document.activeElement).toBe(container.querySelector('input[role="switch"]'));
});
