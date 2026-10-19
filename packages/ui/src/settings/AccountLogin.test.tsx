// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { AccountLogin } from "../protocol";
import { createStateStore, DaemonProvider, trackAccountLogin, type Daemon } from "../store";
import { AccountLoginAnnouncement, useOpenLoginPages } from "./AccountLogin";

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

it("announces a login that succeeded with the account's name, and says nothing for any other state", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const show = (value: AccountLogin | undefined) => act(() => root.render(<AccountLoginAnnouncement login={value} name="Work" />));
  show(login({ status: "waiting" }));
  expect(container.textContent).toBe("");
  show(login({ status: "failed" }));
  expect(container.textContent).toBe("");
  show(login({ status: "succeeded" }));
  expect(container.textContent?.replace(/[\u2066-\u2069]/g, "")).toBe("Work: logged in.");
  expect([...container.querySelectorAll("[data-inline-chip]")].map((chip) => chip.textContent?.replace(/[\u2066-\u2069]/g, ""))).toEqual(["Work"]);
});
