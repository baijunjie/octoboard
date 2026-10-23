// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { windowAttention } from "../components/windowTestSupport";
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
  expect(container.textContent).toContain("Could not read which permissions");
  expect(container.textContent).not.toContain("only be managed in the macOS app");
  // Permissions that cannot be read are the state of the section: an Alert, not a muted line.
  expect(container.querySelector('[data-slot="alert-root"]')?.textContent).toContain("could not read");
  // Found so on the section's first read, it is what the section opened with, so it is not announced.
  expect(container.querySelector('[role="alert"]')).toBeNull();
});

it("offers nothing for a permission that is granted, and shows a success badge at the row's end instead", async () => {
  const { container, buttons } = await mount({ statuses: [GRANTED_AT_LAUNCH] });
  expect(buttons()).toEqual([]);
  const badges = Array.from(container.querySelectorAll('[data-color="success"][data-variant="soft"]'));
  expect(badges.map((badge) => badge.textContent)).toEqual(["Granted", "Granted"]);
  // The badge is the granted state, so the description no longer repeats it.
  expect(container.querySelector(".groove-t")!.textContent).not.toContain("Granted.");
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

/** What the section's status regions say, one entry per region. */
const statuses = (container: HTMLElement) => [...container.querySelectorAll('[role="status"]')].map((region) => region.textContent);
const elapse = (ms: number) => act(() => vi.advanceTimersByTime(ms));

// The states are in place as the section appears, which is as focus moves onto its tab: said with
// the section, they would go unsaid, so they are said a moment later, from one region.
it("says where every permission stands a moment after the section appears, from one region of its own", async () => {
  windowAttention();
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { container } = await mount({ statuses: [SCREEN_RECORDING_MISSING] });
  expect(statuses(container)).toEqual([""]);
  expect(container.querySelector('[role="status"]')!.className).toContain("sr-only");
  expect(container.textContent).toContain("Not granted.");
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(statuses(container)).toEqual([""]);
  await elapse(1);
  expect(statuses(container)).toEqual(["Screen Recording Not granted. Accessibility Granted."]);
});

// The way back from System Settings has VoiceOver speak the focused control, which a status written
// as the window comes back would lose.
it("says a grant picked up on the way back from System Settings a moment after the user is back, once", async () => {
  const attend = windowAttention();
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { container } = await mount({ statuses: [SCREEN_RECORDING_MISSING, GRANTED_SINCE_LAUNCH] });
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS);
  attend(false);
  attend(true);
  await act(async () => {});
  // The rows show it at once; only the region waits.
  expect(container.querySelector(".groove-t")!.textContent).not.toContain("Not granted.");
  expect(statuses(container)).toEqual(["Screen Recording Not granted. Accessibility Granted."]);
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(statuses(container)).toEqual(["Screen Recording Not granted. Accessibility Granted."]);
  await elapse(1);
  const restart = "Restart Octoboard for it to take effect.";
  expect(statuses(container)).toEqual([`Screen Recording Granted. ${restart} Accessibility Granted.`]);
});

it("holds what it has to say while the window is not attended", async () => {
  const attend = windowAttention();
  attend(false);
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { container } = await mount({ statuses: [SCREEN_RECORDING_MISSING] });
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(statuses(container)).toEqual([""]);
  attend(true);
  await act(async () => {});
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(statuses(container)).toEqual(["Screen Recording Not granted. Accessibility Granted."]);
});

it.each([
  ["a browser, without the capability", { absent: true }],
  ["a shell that cannot be asked", { unreadable: true }],
])("says nothing from its status region for %s", async (_name, options) => {
  windowAttention();
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { container } = await mount(options);
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(statuses(container)).toEqual([""]);
});

it("says only the latest of readings that follow one another within the wait, and nothing for a reading that changes nothing", async () => {
  const attend = windowAttention();
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { container, refocus } = await mount({ statuses: [SCREEN_RECORDING_MISSING, GRANTED_AT_LAUNCH, SCREEN_RECORDING_MISSING, GRANTED_AT_LAUNCH] });
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS);
  const region = container.querySelector('[role="status"]')!;
  const writes: string[] = [];
  const observer = new MutationObserver(() => writes.push(region.textContent ?? ""));
  observer.observe(region, { childList: true, characterData: true, subtree: true });
  onTestFinished(() => observer.disconnect());
  // Granted, then not again: the region ends where it was and says nothing.
  await refocus();
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS / 2);
  await refocus();
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(writes).toEqual([]);
  // Granted, with the window left and come back while it waits: said once, after the user is back.
  await refocus();
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS / 2);
  attend(false);
  attend(true);
  await act(async () => {});
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(writes).toEqual([]);
  await elapse(1);
  expect(writes).toEqual(["Screen Recording Granted. Accessibility Granted."]);
  await refocus();
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(writes).toEqual(["Screen Recording Granted. Accessibility Granted."]);
});
