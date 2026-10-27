// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { sessionOf } from "../gallery/fixtures/builders";
import type { Account, AccountQuota, Settings } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ConsoleAccountsField } from "./ConsoleAccountsField";
import { SwitchAccountDialog } from "./SwitchAccountDialog";

vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const work: Account = { id: "claude-work", agent: "claude", name: "Work", config_dir: "/a/work" };

const notLoggedIn: AccountQuota = { agent: "claude", account: "claude-work", fetched_at: 0, plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" } };

function render(settings: Partial<Settings>, ui: React.ReactElement, accountQuotas: AccountQuota[] = []) {
  const store = createStateStore({ accountQuotas });
  store.setState({ settings: { ...store.getState().settings, ...settings } });
  const daemon = { store, request: vi.fn(), toastError: vi.fn(), toastNotice: vi.fn(), onToast: () => () => {}, dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "" } as unknown as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<DaemonProvider value={daemon}>{ui}</DaemonProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { store, container };
}

const confirm = () => Array.from(document.body.querySelectorAll("button")).find((button) => button.textContent === "Switch")!;
const switching = sessionOf("s-1", "c-1", "p-1", "Fix search", "idle");
const switchDialog = (account: string | null, onSwitchAccounts: () => void = () => {}) => (
  <SwitchAccountDialog session={account === null ? { ...switching, account_id: "claude-work" } : switching} account={account} accountName="Work" onClose={() => {}} onSwitchAccounts={onSwitchAccounts} />
);

it("says in the error alert dialog that a switch's target was turned off, switching nothing, and switches once it is back on", async () => {
  const onSwitchAccounts = vi.fn();
  const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
  onTestFinished(() => report.mockRestore());
  const { store } = render({ accounts: [{ ...work, disabled: true }] }, switchDialog("claude-work", onSwitchAccounts));
  await act(async () => confirm().click());
  expect(report).toHaveBeenCalledTimes(1);
  expect(report.mock.calls[0]![0].message).toContain("Work");
  // The alert's own remedy and Manage accounts button carry the advice, so its sentence does not repeat it.
  expect(report.mock.calls[0]![0]).toMatchObject({ recovery: "accountDisabled" });
  expect(report.mock.calls[0]![0].message).not.toContain("Choose another account");
  expect(onSwitchAccounts).not.toHaveBeenCalled();
  expect(confirm()).toBeDefined();
  act(() => store.setState({ settings: { ...store.getState().settings, accounts: [work] } }));
  await act(async () => confirm().click());
  expect(report).toHaveBeenCalledTimes(1);
  expect(onSwitchAccounts).toHaveBeenCalledTimes(1);
});

it("says in the error alert dialog that a switch's target is not logged in, with the Manage accounts remedy, switching nothing, and switches once it is", async () => {
  const onSwitchAccounts = vi.fn();
  const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
  onTestFinished(() => report.mockRestore());
  const { store } = render({ accounts: [work] }, switchDialog("claude-work", onSwitchAccounts), [notLoggedIn]);
  await act(async () => confirm().click());
  expect(report).toHaveBeenCalledTimes(1);
  expect(report.mock.calls[0]![0]).toMatchObject({ recovery: "accountNotSignedIn" });
  expect(report.mock.calls[0]![0].message).toContain("Work");
  expect(report.mock.calls[0]![0].message).toContain("is not logged in, so a session cannot use it.");
  expect(report.mock.calls[0]![0].message).not.toContain("Choose another account");
  expect(onSwitchAccounts).not.toHaveBeenCalled();
  act(() => store.setState({ accountQuotas: [] }));
  await act(async () => confirm().click());
  expect(report).toHaveBeenCalledTimes(1);
  expect(onSwitchAccounts).toHaveBeenCalledTimes(1);
});

it("refuses a switch to a default account that is turned off in the error alert dialog", async () => {
  const onSwitchAccounts = vi.fn();
  const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
  onTestFinished(() => report.mockRestore());
  render({ accounts: [work], disabled_default_accounts: ["claude"] }, switchDialog(null, onSwitchAccounts));
  await act(async () => confirm().click());
  expect(report).toHaveBeenCalledTimes(1);
  expect(onSwitchAccounts).not.toHaveBeenCalled();
});

function openConsoleOptions(settings: Partial<Settings>, pinned: string | null) {
  const { container } = render(
    { added_agents: ["claude"], ...settings },
    <ConsoleAccountsField label="Accounts" description="d" values={{ claude_account_id: pinned, codex_account_id: null, grok_account_id: null }} onChange={() => {}} onManageAccounts={() => {}} />,
  );
  const trigger = container.querySelector<HTMLElement>('[data-slot="select-trigger"]')!;
  act(() => {
    trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.click();
  });
  return Array.from(document.querySelectorAll<HTMLElement>('[role="option"]'));
}

it("hides a turned-off account from the console's account picker", () => {
  const options = openConsoleOptions({ accounts: [{ ...work, disabled: true }] }, null);
  expect(options.some((option) => option.textContent?.includes("Work"))).toBe(false);
  expect(options).toHaveLength(1);
});

it("keeps the turned-off account a console still pins as its marked current value", () => {
  const options = openConsoleOptions({ accounts: [{ ...work, disabled: true }, { id: "claude-spare", agent: "claude", name: "Spare", config_dir: "/a/spare", disabled: true }] }, "claude-work");
  const kept = options.find((option) => option.textContent?.includes("Work"))!;
  expect(kept.textContent).toContain("Disabled");
  expect(options.some((option) => option.textContent?.includes("Spare"))).toBe(false);
});
