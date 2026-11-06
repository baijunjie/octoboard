// @vitest-environment jsdom
import { act, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { FocusScope } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import type { AccountQuota } from "../../protocol";
import { TestHost } from "../../test/TestHost";
import { testDaemon } from "../../test/testDaemon";
import { AccountLoginButton, type ReturnLoginFocus } from "./AccountLoginButton";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const signedOut: AccountQuota = {
  agent: "claude", account: "work", fetched_at: 0, plan: null, windows: [], email: null,
  unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "Not signed in" },
};

function mount() {
  const callbacks = new Map<number, FrameRequestCallback>();
  let sequence = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    callbacks.set(++sequence, callback);
    return sequence;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => callbacks.delete(id));
  onTestFinished(() => { vi.unstubAllGlobals(); });
  const frame = () => act(() => {
    const pending = [...callbacks.values()];
    callbacks.clear();
    for (const callback of pending) callback(performance.now());
  });
  let succeed!: (removeOpener: boolean) => void;
  const daemon = testDaemon();
  function View() {
    const [open, setOpen] = useState(false);
    const [quota, setQuota] = useState(signedOut);
    const returnFocus = useRef<ReturnLoginFocus>(undefined);
    const fallback = useRef<HTMLButtonElement>(null);
    succeed = (removeOpener) => {
      if (removeOpener) setQuota({ ...signedOut, unavailable: null });
      returnFocus.current!(true);
      setOpen(false);
    };
    return <FocusScope>
      <button type="button">Default account actions</button>
      <AccountLoginButton agent="claude" account="work" name="Work" quota={quota} turnedOff={false}
        fallback={() => fallback.current} onLogin={(restore) => { returnFocus.current = restore; setOpen(true); }} />
      <button ref={fallback} type="button">Work account actions</button>
      {open && <FocusScope contain restoreFocus autoFocus><button type="button">Cancel login</button></FocusScope>}
    </FocusScope>;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<TestHost daemon={daemon}><View /></TestHost>));
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const first = container.querySelector<HTMLButtonElement>("button")!;
  const login = container.querySelector<HTMLButtonElement>('button[aria-label^="Log in to"]')!;
  const actions = [...container.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "Work account actions")!;
  return { frame, first, login, actions, succeed: (removeOpener: boolean) => act(() => succeed(removeOpener)) };
}

it.each([false, true])("returns to its own account when the focus scope's opener is removed: %s", (removeOpener) => {
  const { frame, login, actions, succeed } = mount();
  act(() => { login.focus(); login.click(); });
  frame();
  succeed(removeOpener);
  for (let index = 0; index < 4; index++) frame();
  expect(document.activeElement).toBe(actions);
});

it("preserves a deliberate focus move while the dialog's restoration is pending", () => {
  const { frame, first, login, succeed } = mount();
  act(() => { login.focus(); login.click(); });
  frame();
  succeed(true);
  act(() => first.focus());
  for (let index = 0; index < 4; index++) frame();
  expect(document.activeElement).toBe(first);
});

it("leaves restoration to the control that held focus when the login press took none", () => {
  const { frame, first, login, succeed } = mount();
  act(() => { first.focus(); login.click(); });
  frame();
  succeed(true);
  for (let index = 0; index < 4; index++) frame();
  expect(document.activeElement).toBe(first);
});
