// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { ArchiveCleanupSetting } from "./ArchiveCleanupSetting";
import { AutoApproveResumeRequestsSetting } from "./AutoApproveResumeRequestsSetting";
import { AutoApproveAccountSwitchesSetting } from "./AutoApproveAccountSwitchesSetting";
import { AskQuestionsAsChoicesSetting } from "./AskQuestionsAsChoicesSetting";
import { ReplyInInterfaceLanguageSetting } from "./ReplyInInterfaceLanguageSetting";
import { AutoApproveHandoffsSetting } from "./AutoApproveHandoffsSetting";
import { AutoApproveSessionRequestsSetting } from "./AutoApproveSessionRequestsSetting";
import { AutoSelectAccountSetting } from "./AutoSelectAccountSetting";
import { SidebarOrderSetting } from "./SidebarOrderSetting";
import { StartQuotaWindowsSetting } from "./StartQuotaWindowsSetting";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);
const toastError = vi.fn();
let settings: Record<string, boolean | undefined> = {};

vi.mock("../store", () => ({
  useDaemon: () => ({ request, toastError }),
  useDaemonStore: (selector: (state: { settings: typeof settings }) => unknown) => selector({ settings }),
}));

beforeEach(() => {
  request.mockClear();
  toastError.mockClear();
  request.mockImplementation(() => Promise.resolve({}) as Promise<never>);
  settings = {};
});

// What each switch says while it is on; every one says the same while it is off.
const OFF_TEXT = "Once this is on";
const switches = [
  { Setting: ArchiveCleanupSetting, key: "auto_delete_unresumable_archives", onText: "is deleted automatically" },
  { Setting: AutoApproveResumeRequestsSetting, key: "auto_approve_resume_requests", onText: "without asking you first" },
  { Setting: AutoApproveAccountSwitchesSetting, key: "auto_approve_account_switches", onText: "without asking you first" },
  { Setting: AutoApproveHandoffsSetting, key: "auto_approve_handoffs", onText: "without asking you first" },
  { Setting: AskQuestionsAsChoicesSetting, key: "ask_questions_as_choices", onText: "Turn this off to let sessions ask in their own way" },
  { Setting: ReplyInInterfaceLanguageSetting, key: "reply_in_interface_language", onText: "A change reaches sessions started afterwards" },
  { Setting: AutoApproveSessionRequestsSetting, key: "auto_approve_session_requests", onText: "without asking you first" },
  { Setting: AutoSelectAccountSetting, key: "auto_select_account", onText: "whose weekly limit resets soonest" },
  { Setting: SidebarOrderSetting, key: "sidebar_order_by_status", onText: "move as their status changes" },
  { Setting: StartQuotaWindowsSetting, key: "auto_start_quota_windows", onText: "starts counting once it resets" },
];

function mount(Setting: () => React.ReactElement) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <LanguageProvider>
        <Setting />
      </LanguageProvider>,
    ),
  );
  return container.querySelector<HTMLInputElement>("input[type=checkbox]")!;
}

describe.each(switches)("$Setting.name", ({ Setting, key, onText }) => {
  it.each([
    { name: "off while it is on", on: true },
    { name: "on while it is off", on: false },
  ])("shows the daemon's value and turns the switch $name", async ({ on }) => {
    settings = { [key]: on };
    const toggle = mount(Setting);
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
    mount(Setting);
    expect(document.body.textContent).toContain(shown);
    if (hidden) expect(document.body.textContent).not.toContain(hidden);
  });

  it("reports a failed change to the user", async () => {
    request.mockImplementation(() => Promise.reject(new Error("daemon refused")) as Promise<never>);
    const toggle = mount(Setting);
    await act(async () => {
      toggle.click();
    });
    expect(toastError).toHaveBeenCalledOnce();
  });
});

it("reads a daemon that sends no value for asking questions as choices as on", () => {
  settings = { ask_questions_as_choices: undefined };
  expect(mount(AskQuestionsAsChoicesSetting).checked).toBe(true);
});

it("reads a daemon that sends no value for replying in the interface language as on", () => {
  settings = { reply_in_interface_language: undefined };
  expect(mount(ReplyInInterfaceLanguageSetting).checked).toBe(true);
});

it("reads a daemon that sends no value for the sidebar order as on", () => {
  settings = { sidebar_order_by_status: undefined };
  expect(mount(SidebarOrderSetting).checked).toBe(true);
});
