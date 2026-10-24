// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import type { Account, Settings } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ConsoleAccountsField } from "./ConsoleAccountsField";
import { SwitchAccountDialog } from "./SwitchAccountDialog";

vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const work: Account = { id: "claude-work", agent: "claude", name: "Work", config_dir: "/a/work" };

function render(settings: Partial<Settings>, ui: React.ReactElement) {
  const store = createStateStore({});
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
const switchDialog = (account: string | null) => (
  <SwitchAccountDialog session={switching} account={account} accountName="Work" onClose={() => {}} onSwitchAccounts={() => {}} />
);

it("holds Confirm of a switch whose target is turned off, saying so, and frees it once it is back on", () => {
  const { store } = render({ accounts: [{ ...work, disabled: true }] }, switchDialog("claude-work"));
  expect(document.body.textContent).toContain("Account is disabled");
  expect(confirm().disabled).toBe(true);
  act(() => store.setState({ settings: { ...store.getState().settings, accounts: [work] } }));
  expect(document.body.textContent).not.toContain("Account is disabled");
  expect(confirm().disabled).toBe(false);
});

it("holds Confirm of a switch to a default account that is turned off", () => {
  render({ accounts: [work], disabled_default_accounts: ["claude"] }, switchDialog(null));
  expect(confirm().disabled).toBe(true);
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
