// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import type { Account, Settings } from "../protocol";
import { AgentAccountsSection } from "./AgentAccountsSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}));
const toastError = vi.fn();
const settings: Settings = {
  auto_pull_repositories: false,
  default_clone_dir: "/home/dev/Projects",
  accounts: [],
};
vi.mock("../store", () => ({
  useDaemon: () => ({ request, toastError }),
  useDaemonStore: (selector: (s: unknown) => unknown) => selector({ settings, agentAvailability: new Map() }),
}));
vi.mock("../components/useListMotion", () => ({ useListMotion: () => undefined }));
vi.mock("../components/AgentIcon", () => ({ AgentIcon: () => null }));
vi.mock("../components/FadeOverflow", () => ({ FadeOverflow: ({ children }: { children: React.ReactNode }) => children }));
vi.mock("../components/MarkedPath", () => ({ MarkedPath: ({ path }: { path: string }) => path }));
vi.mock("./useSectionRefocus", () => ({ useSectionRefocus: () => {} }));
vi.mock("../dialogs/OptionSelect", () => ({
  OptionSelect: ({ label, value, onChange, options, isDisabled }: {
    label: string;
    value: string;
    onChange: (value: string) => void;
    options: { value: string; label: string; isDisabled?: boolean }[];
    isDisabled?: boolean;
  }) => (
    <select aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} disabled={isDisabled}>
      {options.map((option) => <option key={option.value} value={option.value} disabled={option.isDisabled}>{option.label}</option>)}
    </select>
  ),
}));

const account = (id: string, agent: Account["agent"]): Account => ({ id, agent, name: id, config_dir: `/home/dev/.${id}` });
beforeEach(() => {
  request.mockReset().mockResolvedValue({});
  toastError.mockClear();
  settings.accounts = [account("claude-work", "claude"), account("codex-work", "codex"), account("grok-work", "grok")];
  settings.codex_default_approval_reviewer = null;
});

function mount() {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<AgentAccountsSection />));
  const selects = [...container.querySelectorAll("select")];
  const change = (select: HTMLSelectElement, value: string) => act(() => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  return { selects, change };
}

it("offers a mode for the Codex default and each Codex account, following configuration by default", () => {
  const { selects } = mount();
  expect(selects).toHaveLength(2);
  expect(selects.map((select) => select.value)).toEqual(["inherit", "inherit"]);
  expect(selects[1]!.getAttribute("aria-label")).toContain("codex-work");
});

it("shows stored choices separately for default and named accounts", () => {
  settings.codex_default_approval_reviewer = "user";
  settings.accounts[1]!.approval_reviewer = "auto_review";
  expect(mount().selects.map((select) => select.value)).toEqual(["user", "auto_review"]);
});

it.each([
  [0, "auto_review", null, "auto_review"],
  [1, "user", "codex-work", "user"],
  [1, "inherit", "codex-work", null],
] as const)("saves selector %i as %s without affecting other accounts", async (index, value, account, reviewer) => {
  settings.accounts[1]!.approval_reviewer = "auto_review";
  const { selects, change } = mount();
  change(selects[index]!, value);
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_account_approval_reviewer", account, reviewer });
  await act(async () => {});
});

it("keeps the stored value and reports a failed save", async () => {
  request.mockRejectedValueOnce(new Error("save refused"));
  const { selects, change } = mount();
  change(selects[0]!, "auto_review");
  await act(async () => {});
  expect(selects[0]!.value).toBe("inherit");
  expect(toastError).toHaveBeenCalledOnce();
  expect(selects[0]!.disabled).toBe(false);
});


it("keeps the trigger available while saving and prevents another option selection until answered", async () => {
  let finish!: (value: object) => void;
  request.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  const { selects, change } = mount();
  change(selects[0]!, "auto_review");
  expect(selects[0]!.disabled).toBe(false);
  expect([...selects[0]!.options].every((option) => option.disabled)).toBe(true);
  await act(async () => finish({}));
  expect([...selects[0]!.options].every((option) => !option.disabled)).toBe(true);
});
