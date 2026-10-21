// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { AutoSelectAccountSetting } from "./AutoSelectAccountSetting";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);
const toastError = vi.fn();
let enabled = false;

vi.mock("../store", () => ({
  useDaemon: () => ({ request, toastError }),
  useDaemonStore: (selector: (state: { settings: { auto_select_account: boolean } }) => unknown) =>
    selector({ settings: { auto_select_account: enabled } }),
}));

beforeEach(() => {
  request.mockClear();
  toastError.mockClear();
  request.mockImplementation(() => Promise.resolve({}) as Promise<never>);
  enabled = false;
});

function mount() {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <LanguageProvider>
        <AutoSelectAccountSetting />
      </LanguageProvider>,
    ),
  );
  return container.querySelector<HTMLInputElement>("input[type=checkbox]")!;
}

it.each([
  { name: "turns the switch off while it is on", on: true },
  { name: "turns the switch on while it is off", on: false },
])("shows the daemon's value and $name", async ({ on }) => {
  enabled = on;
  const toggle = mount();
  expect(toggle.checked).toBe(on);
  await act(async () => {
    toggle.click();
  });
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", auto_select_account: !on });
});

it.each([
  { name: "what is happening now while it is on", on: true, shown: "whose weekly quota resets soonest", hidden: "Once this is on" },
  { name: "what turning it on does while it is off", on: false, shown: "Once this is on", hidden: undefined },
])("describes $name", ({ on, shown, hidden }) => {
  enabled = on;
  mount();
  expect(document.body.textContent).toContain(shown);
  if (hidden) expect(document.body.textContent).not.toContain(hidden);
});

it("reports a failed change to the user", async () => {
  request.mockImplementation(() => Promise.reject(new Error("daemon refused")) as Promise<never>);
  const toggle = mount();
  await act(async () => {
    toggle.click();
  });
  expect(toastError).toHaveBeenCalledOnce();
});
