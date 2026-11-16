// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { activate, mountForTooltip, press } from "../components/hint/tooltipTestSupport";
import { setLanguage } from "../i18n/language";
import type { PlatformAdapter, RemoteForward } from "../platform";
import { LinkForwardError } from "../platform/linkForwardError";
import { PlatformProvider } from "../platform/react";
import { fakeTimers } from "../test/clock";
import { BROWSER_HOST } from "../test/hosts";
import { RailForwardsEntry } from "./RailForwardsEntry";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SAME: RemoteForward = { remotePort: 3000, localPort: 3000, address: "http://localhost:3000/app", localAddress: "http://localhost:3000/app" };
const THIRD: RemoteForward = { remotePort: 8080, localPort: 8080, address: "http://localhost:8080/", localAddress: "http://localhost:8080/" };
const MOVED: RemoteForward = { remotePort: 5173, localPort: 53012, address: "http://127.0.0.1:5173/", localAddress: "http://127.0.0.1:53012/" };

beforeEach(() => {
  act(() => setLanguage("en"));
  HTMLElement.prototype.animate = vi.fn(() => ({ cancel: vi.fn(), finished: Promise.resolve(), onfinish: null })) as unknown as typeof HTMLElement.prototype.animate;
  // The daemon's name, as the shell gives a remote window.
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  window.history.replaceState(null, "", "/?remote=build-server&daemon=me%40build-server");
  onTestFinished(() => {
    vi.unstubAllGlobals();
    window.history.replaceState(null, "", "/");
    while (errorAlerts.getSnapshot().length > 0) act(() => errorAlerts.acknowledge());
  });
});

async function mount(initial: RemoteForward[], withCapability = true) {
  fakeTimers();
  const state = { forwards: initial, listener: () => {} };
  const stop = vi.fn(async (remotePort: number) => {
    state.forwards = state.forwards.filter((forward) => forward.remotePort !== remotePort);
    state.listener();
  });
  const open = vi.fn(async (_address: string) => {});
  const platform: PlatformAdapter = {
    kind: "tauri",
    host: BROWSER_HOST,
    links: { open },
    remoteForwards: withCapability ? { list: async () => state.forwards, stop, onChange: (listener) => ((state.listener = listener), () => {}) } : undefined,
  };
  const container = mountForTooltip(<PlatformProvider value={platform}><nav><RailForwardsEntry /><button id="next">Next</button></nav></PlatformProvider>);
  await act(async () => {});
  const change = async (forwards: RemoteForward[]) => {
    state.forwards = forwards;
    await act(async () => state.listener());
  };
  const advance = (ms: number) => act(async () => void vi.advanceTimersByTime(ms));
  return { container, state, stop, open, change, advance };
}

const panel = () => document.querySelector<HTMLElement>('[role="dialog"]');
const button = (container: HTMLElement) => container.querySelector<HTMLButtonElement>("#rail-forwards")!;

it("shows nothing outside a remote window, and nothing while no port is forwarded", async () => {
  expect((await mount([SAME], false)).container.querySelector("#rail-forwards")).toBeNull();
  expect((await mount([])).container.querySelector("#rail-forwards")).toBeNull();
});

it("counts the forwards in the button's name", async () => {
  const { container, change } = await mount([SAME]);
  expect(button(container).getAttribute("aria-label")).toBe("1 port is forwarded. Show it.");
  await change([SAME, MOVED]);
  expect(button(container).getAttribute("aria-label")).toBe("2 ports are forwarded. Show them.");
});

it("shows the address on this computer and the remote port with the daemon's name for each forward, in the order made", async () => {
  const { container } = await mount([SAME, MOVED]);
  press(button(container), "touch");
  expect(panel()!.getAttribute("aria-label")).toBe("Forwarded ports");
  expect(panel()!.querySelector("h2")?.textContent).toBe("Forwarded ports");
  const rows = Array.from(panel()!.querySelectorAll("li"), (row) => row.querySelector("button")!.textContent!.replace(/[⁨⁩]/g, ""));
  expect(rows).toEqual(["localhost:3000Port 3000 on build-server", "127.0.0.1:53012Port 5173 on build-server"]);
  expect(panel()!.querySelector("li button")!.getAttribute("aria-label")?.replace(/[⁨⁩]/g, "")).toBe("Open localhost:3000 in the browser. It reaches port 3000 on build-server.");
});

it("opens the address as it was written, not the one on this computer, when a row is pressed", async () => {
  const { container, open } = await mount([SAME, MOVED]);
  press(button(container), "touch");
  const rows = Array.from(panel()!.querySelectorAll<HTMLButtonElement>("li button"));
  act(() => rows.find((row) => row.textContent?.includes("5173"))!.click());
  expect(open).toHaveBeenCalledExactlyOnceWith("http://127.0.0.1:5173/");
});

it("raises the failure of opening a row's address as the alert of an address that could not be forwarded", async () => {
  const { container, open } = await mount([SAME]);
  open.mockRejectedValueOnce(new LinkForwardError("nothing_listening", 3000));
  press(button(container), "touch");
  await act(async () => panel()!.querySelector<HTMLButtonElement>("li button")!.click());
  expect(errorAlerts.getSnapshot()[0].message).toContain("Nothing is listening on port 3000");
});

it("stops a forward with Stop and drops its row when the shell says the list changed, the popover going with the last", async () => {
  const { container, stop, advance } = await mount([SAME, MOVED]);
  press(button(container), "touch");
  const stops = () => Array.from(panel()!.querySelectorAll<HTMLButtonElement>("li button")).filter((control) => control.textContent === "Stop");
  expect(stops().map((control) => control.getAttribute("aria-label"))).toEqual(["Stop forwarding port 3000", "Stop forwarding port 5173"]);
  await act(async () => stops()[0].click());
  expect(stop).toHaveBeenCalledExactlyOnceWith(3000);
  // The row leaves with the collection motion, which leaves an inert ghost of it until it has played.
  await advance(1000);
  expect(panel()!.querySelectorAll("li:not([data-list-motion-ghost])")).toHaveLength(1);
  await act(async () => stops()[0].click());
  expect(stop).toHaveBeenLastCalledWith(5173);
  expect(button(container)).toBeNull();
  expect(panel()).toBeNull();
});

it("hands focus to the next row's Stop, else the previous one's, when a forward is stopped from its focused Stop", async () => {
  const { container, advance } = await mount([SAME, MOVED]);
  activate(button(container));
  const stops = () => Array.from(panel()!.querySelectorAll<HTMLButtonElement>("li:not([data-list-motion-ghost]) button")).filter((control) => control.textContent === "Stop");
  act(() => stops()[0].focus());
  await act(async () => stops()[0].click());
  await advance(1000);
  expect(stops().map((control) => control.getAttribute("aria-label"))).toEqual(["Stop forwarding port 5173"]);
  expect(document.activeElement).toBe(stops()[0]);
});

it("leaves focus alone when a forward is stopped by a press that did not leave it on Stop", async () => {
  const { container, advance } = await mount([SAME, MOVED]);
  activate(button(container));
  const first = panel()!.querySelector<HTMLButtonElement>("li button")!;
  await act(async () => Array.from(panel()!.querySelectorAll<HTMLButtonElement>("li button")).find((control) => control.textContent === "Stop")!.click());
  await advance(1000);
  expect(document.activeElement).not.toBe(Array.from(panel()!.querySelectorAll<HTMLButtonElement>("li button")).find((control) => control.textContent === "Stop"));
  expect(first.isConnected).toBe(false);
});

it("leaves focus on a Stop that stays when another row's Stop is pressed", async () => {
  const { container, advance } = await mount([SAME, MOVED, THIRD]);
  activate(button(container));
  const stops = () => Array.from(panel()!.querySelectorAll<HTMLButtonElement>("li:not([data-list-motion-ghost]) button")).filter((control) => control.textContent === "Stop");
  const kept = stops()[0];
  act(() => kept.focus());
  await act(async () => stops()[2].click());
  await advance(1000);
  expect(stops()).toHaveLength(2);
  expect(document.activeElement).toBe(kept);
});

it("focuses the first row when the keyboard opens it", async () => {
  const { container } = await mount([SAME]);
  activate(button(container));
  expect(document.activeElement).toBe(panel()!.querySelector("li button"));
});

it("hands focus to the next control when the last forward goes while it holds focus", async () => {
  const { container, change } = await mount([SAME]);
  button(container).focus();
  await change([]);
  expect(document.activeElement).toBe(container.querySelector("#next"));
});

it("keeps the popover and the button while a forward is left, listing only what is left", async () => {
  const { container, change } = await mount([SAME, MOVED]);
  press(button(container), "touch");
  await change([MOVED]);
  expect(button(container)).not.toBeNull();
  expect(button(container).getAttribute("aria-label")).toBe("1 port is forwarded. Show it.");
  // The stopped forward's row leaves with the collection motion, which leaves an inert ghost of it until it has played.
  await act(async () => void vi.advanceTimersByTime(1000));
  const rows = Array.from(panel()!.querySelectorAll("li:not([data-list-motion-ghost])"), (row) => row.querySelector("button")!.textContent!.replace(/[⁨⁩]/g, ""));
  expect(rows).toEqual(["127.0.0.1:53012Port 5173 on build-server"]);
});

it("hands focus to the next control when the last forward goes while focus is in its popover", async () => {
  const { container, change } = await mount([SAME]);
  press(button(container), "touch");
  const row = panel()!.querySelector<HTMLButtonElement>("li button")!;
  row.focus();
  expect(panel()!.contains(document.activeElement)).toBe(true);
  await change([]);
  expect(document.activeElement).toBe(container.querySelector("#next"));
});
