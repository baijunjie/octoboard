// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { BOOLEAN_SETTINGS } from "../configuration/booleanSettings";
import { errorAlerts } from "../components/alerts/errorAlerts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);
const raiseError = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
let settings: Record<string, boolean | undefined> = {};

vi.mock("../store", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../store")>()),
  useDaemon: () => ({ request, raiseError }),
  useDaemonStore: (selector: (state: { settings: typeof settings }) => unknown) => selector({ settings }),
}));

beforeEach(() => {
  request.mockClear();
  raiseError.mockClear();
  request.mockImplementation(() => Promise.resolve({}) as Promise<never>);
  settings = {};
});

it("lists every switch the settings page covers once", () => {
  expect(BOOLEAN_SETTINGS.map(({ field }) => field).sort()).toEqual(switches.map(({ key }) => key).sort());
});

// What each switch says while it is on; every one says the same while it is off.
const OFF_TEXT = "Once this is on";
const switches = [
  { key: "auto_delete_unresumable_archives", label: "Automatically delete archived sessions whose conversation is gone", onText: "is deleted automatically" },
  { key: "auto_approve_resume_requests", label: "Automatically approve resume requests", onText: "without asking you first" },
  { key: "auto_approve_account_switches", label: "Automatically approve account switches", onText: "without asking you first" },
  { key: "auto_approve_handoffs", label: "Automatically approve handoffs", onText: "without asking you first" },
  { key: "ask_questions_as_choices", label: "Ask questions as choices", onText: "Octoboard asks sessions to put a question" },
  { key: "auto_approve_session_requests", label: "Automatically approve session requests", onText: "without asking you first" },
  { key: "auto_select_account", label: "Automatically select an account", onText: "whose weekly limit resets soonest" },
  { key: "auto_switch_on_usage_limit", label: "Automatically switch accounts at the usage limit", onText: "switched to another account with usage left" },
  { key: "sidebar_order_by_status", label: "Order the sidebar by status", onText: "move as their status changes" },
  { key: "auto_pull_repositories", label: "Automatically pull repositories", help: "About automatic pulling", onText: "A project's branch is pulled" },
  { key: "auto_start_quota_windows", label: "Start usage windows automatically", help: "About starting usage windows automatically", onText: "starts counting once it resets" },
];

function mount(field: string) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <LanguageProvider>
        <DaemonSwitchSetting field={field as never} />
      </LanguageProvider>,
    ),
  );
  return container.querySelector<HTMLInputElement>("input[type=checkbox]")!;
}

describe.each(switches)("$key", ({ key, label, help, onText }) => {
  it.each([
    { name: "off while it is on", on: true },
    { name: "on while it is off", on: false },
  ])("shows the daemon's value and turns the switch $name", async ({ on }) => {
    settings = { [key]: on };
    const toggle = mount(key);
    expect(toggle.checked).toBe(on);
    await act(async () => {
      toggle.click();
    });
    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", [key]: !on });
  });

  it.each([
    { name: "what is happening now while it is on", on: true, shown: onText, hidden: OFF_TEXT },
    { name: "what turning it on does while it is off", on: false, shown: OFF_TEXT, hidden: undefined },
  ])("describes $name", ({ on, shown, hidden }) => {
    settings = { [key]: on };
    mount(key);
    expect(document.body.textContent).toContain(shown);
    if (hidden) expect(document.body.textContent).not.toContain(hidden);
  });

  it("names the switch by its visible label and offers the help tip only where it has one", () => {
    const toggle = mount(key);
    expect(toggle.getAttribute("aria-label")).toBe(label);
    expect(document.body.textContent).toContain(label);
    expect(document.querySelector(`button[aria-label="${help}"]`) !== null).toBe(help !== undefined);
  });

  it("reports a failed change to the user", async () => {
    request.mockImplementation(() => Promise.reject(new Error("daemon refused")) as Promise<never>);
    const toggle = mount(key);
    await act(async () => {
      toggle.click();
    });
    expect(raiseError).toHaveBeenCalledOnce();
    expect(raiseError.mock.calls[0][0]).toMatchObject({ title: "Could not save the setting" });
  });
});
