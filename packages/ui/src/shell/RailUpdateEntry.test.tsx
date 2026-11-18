// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { activate, closeOpenTooltip, CONTROL_TOOLTIP_DELAY_MS, enter, leave, openTooltip } from "../components/hint/tooltipTestSupport";
import { fakeTimers } from "../test/clock";
import { mountHost } from "../test/mountHost";
import type { UpdateState, UpdateStore } from "../updates/updateStore";
import { RailUpdateEntry } from "./RailUpdateEntry";

const held = vi.hoisted(() => ({ updates: undefined as { state: UpdateState; store: UpdateStore } | undefined }));
vi.mock("../updates/UpdatesProvider", () => ({ useUpdates: () => held.updates }));

beforeEach(() => {
  fakeTimers();
  held.updates = undefined;
});
afterEach(() => closeOpenTooltip());

function mount(change: Partial<UpdateState> = {}) {
  const state: UpdateState = { phase: "idle", channel: "stable", autoCheck: true, noticeDismissed: false, restartNoticeHidden: false, restarting: false, update: { version: "0.2.0" }, ...change };
  const store: UpdateStore = {
    getState: () => state, subscribe: () => () => {}, check: vi.fn(async () => {}), install: vi.fn(async () => {}),
    setChannel: vi.fn(), setAutoCheck: vi.fn(), dismissNotice: vi.fn(), hideRestartNotice: vi.fn(), restart: vi.fn(async () => {}),
    onRestartRequested: () => () => {}, start: () => () => {},
  };
  held.updates = { state, store };
  const focusTerminal = vi.fn();
  const onOpenSettingsAt = vi.fn();
  const ui = () => <RailUpdateEntry focusTerminal={focusTerminal} onOpenSettingsAt={onOpenSettingsAt} />;
  const container = mountHost(ui());
  const render = (change: Partial<UpdateState>) => {
    held.updates = { state: { ...held.updates!.state, ...change }, store };
    container.render(ui());
  };
  return { container, store, focusTerminal, onOpenSettingsAt, render };
}

it.each([0, CONTROL_TOOLTIP_DELAY_MS])("offers download despite a dismissed notice and names its version after an extra clock advance of %s", async (extraDelay) => {
  const { container, store } = mount({ noticeDismissed: true });
  const button = container.querySelector("button")!;
  expect(button.getAttribute("aria-label")).toBe("Download and install");
  enter(button);
  await act(async () => vi.advanceTimersByTimeAsync(CONTROL_TOOLTIP_DELAY_MS));
  await act(async () => vi.advanceTimersByTimeAsync(extraDelay));
  expect(openTooltip()?.textContent).toContain("0.2.0");
  activate(button);
  expect(store.install).toHaveBeenCalledOnce();
});

it.each([undefined, 100])("holds the download pending with a circular progress indicator when total is %s", (total) => {
  const { container, store } = mount({ phase: "downloading", progress: { downloaded: 45, total } });
  const button = container.querySelector("button")!;
  expect(button.getAttribute("aria-busy")).toBe("true");
  const ring = button.querySelector('[role="progressbar"]')!;
  expect(ring).not.toBeNull();
  if (total) expect(ring.getAttribute("aria-valuenow")).toBe("45");
  else expect(ring.hasAttribute("aria-valuenow")).toBe(false);
  activate(button);
  expect(store.install).not.toHaveBeenCalled();
});

it("keeps the restart entry after Later and starts the existing restart flow on press", () => {
  const { container, store } = mount({ phase: "installed", installedVersion: "0.2.0", update: undefined, restartNoticeHidden: true });
  const button = container.querySelector("button")!;
  expect(button.getAttribute("aria-label")).toBe("Restart now");
  activate(button);
  expect(store.restart).toHaveBeenCalledOnce();
  expect(store.install).not.toHaveBeenCalled();
});

it("opens update settings for an update that must be installed by hand", () => {
  const { container, store, onOpenSettingsAt } = mount({ manual: { path: "/usr/bin/octoboardd", command: "install" } });
  activate(container.querySelector("button")!);
  expect(onOpenSettingsAt).toHaveBeenCalledExactlyOnceWith("updates");
  expect(store.install).not.toHaveBeenCalled();
});

it("offers no action without an update, where reload is needed, or without an update store", () => {
  const { container, render } = mount({ update: undefined });
  expect(container.querySelector("button")).toBeNull();
  render({ update: { version: "0.2.0" }, reloadNeeded: true });
  expect(container.querySelector("button")).toBeNull();
  held.updates = undefined;
  container.render(<RailUpdateEntry focusTerminal={() => {}} onOpenSettingsAt={() => {}} />);
  expect(container.querySelector("button")).toBeNull();
});

it("hands keyboard focus back when an offered update disappears", () => {
  const { container, render, focusTerminal } = mount();
  act(() => container.querySelector("button")!.focus());
  render({ update: undefined });
  expect(focusTerminal).toHaveBeenCalledOnce();
});


it.each([
  ["hover", 100, 0], ["hover", undefined, 0], ["focus", 100, 0], ["focus", undefined, 0],
  ["hover", 100, CONTROL_TOOLTIP_DELAY_MS], ["focus", undefined, CONTROL_TOOLTIP_DELAY_MS],
] as const)("shows download detail through %s when total is %s and extra delay is %s, and leaves its action pending", async (interaction, total, extraDelay) => {
  const { container, store, render } = mount({ phase: "downloading", progress: { downloaded: 45, total } });
  const button = container.querySelector("button")!;
  const description = document.getElementById(button.getAttribute("aria-describedby")!);
  expect(description?.textContent).toContain("0.2.0");
  if (total) expect(description?.textContent).toContain("45%");
  else expect(description?.textContent).not.toContain("%");
  if (interaction === "focus") act(() => button.focus());
  else enter(button);
  await act(async () => vi.advanceTimersByTimeAsync(CONTROL_TOOLTIP_DELAY_MS));
  await act(async () => vi.advanceTimersByTimeAsync(extraDelay));
  const tooltip = openTooltip();
  expect(tooltip?.textContent).toContain("Downloading version");
  expect(tooltip?.textContent).toContain("0.2.0");
  if (total) expect(tooltip?.textContent).toContain("45%");
  else expect(tooltip?.textContent).not.toContain("%");
  activate(button);
  activate(button);
  expect(store.install).not.toHaveBeenCalled();
  expect(button.getAttribute("aria-busy")).toBe("true");
  if (interaction === "focus") act(() => button.blur());
  else leave(button);
  await act(async () => vi.advanceTimersByTimeAsync(CONTROL_TOOLTIP_DELAY_MS));
  expect(openTooltip()).toBeNull();
  render({ phase: "installed", installedVersion: "0.2.0", update: undefined, progress: undefined });
  expect(container.querySelector("button")).toBe(button);
  expect(button.getAttribute("aria-describedby")).not.toBe(description?.id);
  expect(openTooltip()).toBeNull();
});

it("withdraws the download tooltip when the download ends while the pointer stays on the control", async () => {
  const { container, render } = mount({ phase: "downloading", progress: { downloaded: 45, total: 100 } });
  enter(container.querySelector("button")!);
  await act(async () => vi.advanceTimersByTimeAsync(CONTROL_TOOLTIP_DELAY_MS));
  expect(openTooltip()?.textContent).toContain("45%");
  render({ phase: "installed", installedVersion: "0.2.0", update: undefined, progress: undefined });
  await act(async () => vi.advanceTimersByTimeAsync(CONTROL_TOOLTIP_DELAY_MS));
  expect(document.querySelector("[data-anchored-tooltip]")).toBeNull();
  expect(openTooltip()?.textContent ?? "").not.toContain("Downloading version");
});

it.each(["pen", "touch"])("does not open a mouse-hover download tooltip from a %s pointer", async (pointerType) => {
  const { container } = mount({ phase: "downloading", progress: { downloaded: 45, total: 100 } });
  act(() => container.querySelector("button")!.dispatchEvent(new PointerEvent("pointerover", { bubbles: true, pointerType })));
  await act(async () => vi.advanceTimersByTimeAsync(CONTROL_TOOLTIP_DELAY_MS));
  expect(openTooltip()).toBeNull();
});

it("reopens download detail for keyboard focus after Tab closes its hover tooltip", async () => {
  const { container } = mount({ phase: "downloading", progress: { downloaded: 45, total: 100 } });
  const button = container.querySelector("button")!;
  enter(button);
  await act(async () => vi.advanceTimersByTimeAsync(CONTROL_TOOLTIP_DELAY_MS));
  expect(openTooltip()?.textContent).toContain("45%");
  act(() => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })));
  await act(async () => vi.advanceTimersByTimeAsync(CONTROL_TOOLTIP_DELAY_MS));
  expect(openTooltip()).toBeNull();
  act(() => button.focus());
  await act(async () => vi.advanceTimersByTimeAsync(0));
  expect(openTooltip()?.textContent).toContain("45%");
  expect(document.activeElement).toBe(button);
});
