// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import type { NotificationCapability, PlatformAdapter } from "../platform";
import { errorAlerts } from "../errorAlerts";
import { PlatformProvider } from "../platform/react";
import { NotificationsSection } from "./NotificationsSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Mounts the section over a platform with `notifications`. */
async function mount(notifications: NotificationCapability) {
  const platform: PlatformAdapter = { kind: "tauri", notifications };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => {
    root.render(
      <PlatformProvider value={platform}>
        <NotificationsSection />
      </PlatformProvider>,
    );
  });
  const buttons = () => [...container.querySelectorAll("button")].map((button) => button.textContent);
  return { container, buttons };
}

afterEach(() => act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); }));

const base = {
  ensurePermission: vi.fn(() => Promise.resolve(true)),
  notify: vi.fn(() => Promise.resolve()),
} satisfies NotificationCapability;

it("explains that the system manages the notifications and opens its settings on press, in the macOS app", async () => {
  const openSystemSettings = vi.fn(() => Promise.resolve());
  const { container, buttons } = await mount({ ...base, openSystemSettings });
  expect(container.textContent).toContain("has no switch of its own");
  expect(buttons()).toEqual(["Open System Settings"]);
  await act(async () => {
    container.querySelector("button")!.click();
  });
  expect(openSystemSettings).toHaveBeenCalledTimes(1);
});

it("offers no link to the system's settings where the capability cannot open them", async () => {
  const { buttons } = await mount({ ...base });
  expect(buttons()).toEqual([]);
});

it("offers no link to the system's settings where the page asks for the permission itself", async () => {
  const { buttons } = await mount({
    ...base,
    openSystemSettings: vi.fn(() => Promise.resolve()),
    permissionPrompt: { status: () => "undecided", request: () => Promise.resolve() },
  });
  expect(buttons()).toEqual(["Enable"]);
});

it("raises an error alert when the system's settings did not open", async () => {
  const cause = "the system did not open the pane";
  const { container } = await mount({ ...base, openSystemSettings: vi.fn(() => Promise.reject(cause)) });
  await act(async () => {
    container.querySelector("button")!.click();
  });
  const [raised] = errorAlerts.getSnapshot();
  expect(raised.title).toBe("Could not open System Settings");
  expect(raised.message).toContain("Open it yourself and choose Notifications.");
  expect(raised.error).toMatchObject({ recovery: "none", cause });
});
