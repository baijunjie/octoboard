// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { AccountLogin } from "../protocol";
import { createStateStore, DaemonProvider, trackAccountLogin, type Daemon } from "../store";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { windowAttention } from "../components/windowTestSupport";
import { AccountLoginAnnouncer, useOpenLoginPages } from "./AccountLogin";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const login = (patch: Partial<AccountLogin> = {}): AccountLogin => ({
  agent: "claude", account: "work", status: "waiting", authorization_url: "https://claude.test/authorize", device_code: null, opened_by_agent: false, failure: null, ...patch,
});

function Opener(): null {
  useOpenLoginPages();
  return null;
}

function setup() {
  const open = vi.fn(() => Promise.resolve());
  const store = createStateStore();
  const daemon = { store } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const mount = () => act(() => root.render(
    <DaemonProvider value={daemon}>
      <PlatformProvider value={{ links: { open } } as unknown as PlatformAdapter}><Opener /></PlatformProvider>
    </DaemonProvider>,
  ));
  const unmount = () => act(() => root.render(null));
  const set = (logins: AccountLogin[]) => act(() => store.setState({ accountLogins: logins }));
  return { store, open, mount, unmount, set };
}

it("opens the authorization page of a login this window started, once, however often the opener is mounted again", () => {
  const { store, open, mount, unmount, set } = setup();
  mount();
  trackAccountLogin(store, "claude", "work", true);
  set([login({ status: "starting", authorization_url: null })]);
  expect(open).not.toHaveBeenCalled();
  set([login()]);
  expect(open).toHaveBeenCalledExactlyOnceWith("https://claude.test/authorize");
  unmount();
  mount();
  set([login({ device_code: "ABCD" })]);
  expect(open).toHaveBeenCalledOnce();
});

it("leaves a login another window started alone", () => {
  const { open, mount, set } = setup();
  mount();
  set([login()]);
  expect(open).not.toHaveBeenCalled();
});

it("opens nothing for a login whose own CLI opens the page, and does not keep waiting to", () => {
  const { store, open, mount, set } = setup();
  mount();
  trackAccountLogin(store, "claude", "work", true);
  set([login({ opened_by_agent: true })]);
  expect(open).not.toHaveBeenCalled();
  expect(store.getState().loginsStartedHere).toEqual([]);
});

function announcer() {
  const attend = windowAttention();
  vi.useFakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    try {
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });
  const show = (value: AccountLogin | undefined) => act(() => root.render(<AccountLoginAnnouncer login={value} name="Work" />));
  const advance = (ms: number) => act(() => vi.advanceTimersByTime(ms));
  const said = () => container.querySelector('[role="status"]')?.textContent?.replace(/[\u2068\u2069]/g, "");
  return { container, show, advance, said, attend };
}

it("keeps one empty status region through a login, and says nothing for a state other than success", () => {
  const { container, show, advance, said } = announcer();
  show(undefined);
  const region = container.querySelector('[role="status"]');
  show(login({ status: "failed" }));
  advance(10_000);
  expect(said()).toBe("");
  expect(container.querySelector('[role="status"]')).toBe(region);
});

it("stays silent for a success already there when it mounts, and names the next one", () => {
  const { show, advance, said } = announcer();
  show(login({ status: "succeeded" }));
  advance(10_000);
  expect(said()).toBe("");
  show(undefined);
  show(login({ status: "succeeded" }));
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe("Work: logged in.");
});

it("names the account a focus move's delay after the login succeeded, once, and clears with the record", () => {
  const { container, show, advance, said } = announcer();
  show(undefined);
  const region = container.querySelector('[role="status"]');
  advance(1000);
  show(login({ status: "succeeded" }));
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBe("");
  // A new record object for the same success neither restarts the wait nor says it again.
  show(login({ status: "succeeded" }));
  advance(1);
  expect(said()).toBe("Work: logged in.");
  const text = region!.firstChild;
  show(login({ status: "succeeded" }));
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(region!.firstChild).toBe(text);
  show(undefined);
  expect(said()).toBe("");
  expect(container.querySelector('[role="status"]')).toBe(region);
});

it("holds a success that came while the window was not attended until the user is back, then says it once", () => {
  const { container, show, advance, said, attend } = announcer();
  show(undefined);
  attend(false);
  show(login({ status: "succeeded" }));
  advance(10_000);
  expect(said()).toBe("");
  attend(true);
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  // Leaving again before it is due starts the wait over on the way back.
  attend(false);
  attend(true);
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBe("");
  advance(1);
  expect(said()).toBe("Work: logged in.");
  const text = container.querySelector('[role="status"]')!.firstChild;
  attend(false);
  attend(true);
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(container.querySelector('[role="status"]')!.firstChild).toBe(text);
});
