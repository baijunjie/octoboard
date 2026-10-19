// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { MacosPermissionsCapability, MacosPermissionsStatus, PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import { PermissionsSection } from "./PermissionsSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Granted, but only since this process started, so a restart is still owed. */
const GRANTED_SINCE_LAUNCH = { screenRecording: true, accessibility: true, screenRecordingAtLaunch: false } satisfies MacosPermissionsStatus;
const GRANTED_AT_LAUNCH = { screenRecording: true, accessibility: true, screenRecordingAtLaunch: true } satisfies MacosPermissionsStatus;
const SCREEN_RECORDING_MISSING = { screenRecording: false, accessibility: true, screenRecordingAtLaunch: false } satisfies MacosPermissionsStatus;

/**
 * Mounts the section over a platform whose shell answers `statuses` one per read, the last of them
 * for every read after that — the grant changes outside the app, so a case can hand over the
 * sequence the user's trip to System Settings produces. `request` resolves `granted`, `unreadable`
 * makes the read reject, which stands for a shell that cannot be asked, and `absent` leaves out
 * `macosPermissions`, as in a browser.
 */
async function mount({
  statuses = [undefined],
  granted = false,
  absent = false,
  unreadable = false,
}: {
  statuses?: (MacosPermissionsStatus | undefined)[];
  granted?: boolean;
  absent?: boolean;
  unreadable?: boolean;
}) {
  let read = 0;
  const capability = {
    status: vi.fn(() => (unreadable ? Promise.reject(new Error("no such command")) : Promise.resolve(statuses[Math.min(read++, statuses.length - 1)]))),
    request: vi.fn(() => Promise.resolve(granted)),
    openSystemSettings: vi.fn(() => Promise.resolve()),
  } satisfies MacosPermissionsCapability;
  const platform: PlatformAdapter = { kind: "tauri", macosPermissions: absent ? undefined : capability };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => {
    root.render(
      <PlatformProvider value={platform}>
        <PermissionsSection />
      </PlatformProvider>,
    );
  });
  const buttons = () => [...container.querySelectorAll("button")].map((button) => button.textContent);
  /** What the window regaining focus does: the section reads the grants again. */
  const refocus = () => act(async () => void window.dispatchEvent(new Event("focus")));
  return { capability, container, buttons, refocus };
}

it.each([
  ["a browser, without the capability", { absent: true }],
  ["a shell that is not macOS", { statuses: [undefined] }],
])("renders no rows for %s", async (_name, options) => {
  const { container, buttons } = await mount(options);
  expect(buttons()).toEqual([]);
  expect(container.textContent).toContain("only be managed in the macOS app");
});

// A shell that cannot be asked is the one platform that does have the feature, so it must not be
// told it has not.
it("says the grants could not be read when the shell refuses the read", async () => {
  const { container, buttons } = await mount({ unreadable: true });
  expect(buttons()).toEqual([]);
  expect(container.textContent).toContain("could not read");
  expect(container.textContent).not.toContain("only be managed in the macOS app");
});

it("offers nothing for a permission that is granted", async () => {
  const { buttons } = await mount({ statuses: [GRANTED_AT_LAUNCH] });
  expect(buttons()).toEqual([]);
});

it("offers Grant… for one that is not, and asks for it on press", async () => {
  const { capability, container, buttons } = await mount({ statuses: [SCREEN_RECORDING_MISSING] });
  expect(buttons()).toEqual(["Grant…"]);
  await act(async () => {
    container.querySelector("button")!.click();
  });
  expect(capability.request).toHaveBeenCalledWith("screenRecording");
});

it("switches the row to System Settings once a request comes back refused", async () => {
  const { capability, container, buttons } = await mount({ statuses: [SCREEN_RECORDING_MISSING] });
  await act(async () => {
    container.querySelector("button")!.click();
  });
  expect(buttons()).toEqual(["Open System Settings"]);
  await act(async () => {
    container.querySelector("button")!.click();
  });
  expect(capability.openSystemSettings).toHaveBeenCalledWith("screenRecording");
});

// The only way Accessibility is ever granted, and the way Screen Recording is granted whenever the
// user takes the System Settings route rather than the prompt.
it("picks up a grant made outside the app when the window comes back", async () => {
  const { buttons, refocus } = await mount({ statuses: [SCREEN_RECORDING_MISSING, GRANTED_AT_LAUNCH] });
  await refocus();
  expect(buttons()).toEqual([]);
});

// Nothing else tells the user the grant is inert until a relaunch. Both cases read it off the
// shell's own two readings on the section's very first render, which is also what the section sees
// every time it is mounted again — the settings tabs unmount it on a switch away, and a notice
// owed to a grant this section happened to watch arrive would not come back.
it.each([
  ["asks for a restart while the grant is newer than the process", GRANTED_SINCE_LAUNCH, true],
  ["says nothing about restarting for a grant the process started with", GRANTED_AT_LAUNCH, false],
])("%s", async (_name, status, expected) => {
  const { container } = await mount({ statuses: [status] });
  expect(container.textContent?.includes("take effect")).toBe(expected);
});

it("leads with the reason for granting ahead of time once the permissions can be read", async () => {
  const { container } = await mount({ statuses: [SCREEN_RECORDING_MISSING] });
  expect(container.querySelector("p")!.textContent).toContain("stops a system prompt from appearing");
});

it.each([
  ["a browser, without the capability", { absent: true }],
  ["a shell that is not macOS", { statuses: [undefined] }],
  ["a shell that cannot be asked", { unreadable: true }],
])("shows no lead for %s", async (_name, options) => {
  const { container } = await mount(options);
  expect(container.textContent).not.toContain("stops a system prompt");
});
