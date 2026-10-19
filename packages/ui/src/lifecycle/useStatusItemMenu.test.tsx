// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { consoleOf, sessionOf } from "../gallery/fixtures/builders";
import type { AccountQuota } from "../protocol";
import type { PlatformAdapter, StatusItemCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";
import type { StatusItemMenuDraft } from "./statusItemMenu";
import { useStatusItemMenu } from "./useStatusItemMenu";

vi.mock("./statusItemIcons", () => ({
  withIcons: async (draft: StatusItemMenuDraft) => ({ ...draft, icons: {} }),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Host({ selectedSessionId, currentConsoleId }: { selectedSessionId?: string; currentConsoleId?: string }) {
  useStatusItemMenu({ ready: true, selectedSessionId, currentConsoleId, onSessionChosen: () => {} });
  return null;
}

function mount(statusItem: StatusItemCapability, request: Daemon["request"], initial: Partial<State> = {}) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const daemon = { store: createStateStore(initial), request } as unknown as Daemon;
  const select = (selectedSessionId?: string, currentConsoleId?: string) => act(() =>
    root.render(
      <PlatformProvider value={{ statusItem } as PlatformAdapter}>
        <DaemonProvider value={daemon}>
          <Host selectedSessionId={selectedSessionId} currentConsoleId={currentConsoleId} />
        </DaemonProvider>
      </PlatformProvider>,
    ),
  );
  select();
  return { store: daemon.store, select };
}

it("asks the daemon to refresh the quotas that are due each time the menu begins to open", () => {
  let opened: () => void = () => {};
  const undo = vi.fn();
  const statusItem: StatusItemCapability = {
    setMenu: () => Promise.resolve(),
    onSessionChosen: () => () => {},
    onOpened(handler) {
      opened = handler;
      return undo;
    },
  };
  const request = vi.fn(() => Promise.reject(new Error("offline")));
  mount(statusItem, request as unknown as Daemon["request"]);
  expect(request).not.toHaveBeenCalled();
  opened();
  opened();
  expect(request.mock.calls).toEqual([[{ type: "refresh_account_quotas" }], [{ type: "refresh_account_quotas" }]]);
});

it("updates the native icon on session selection, quota changes and cleared selection", async () => {
  const setMenu = vi.fn<StatusItemCapability["setMenu"]>(() => Promise.resolve());
  const statusItem: StatusItemCapability = { setMenu, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  const codex = sessionOf("codex", "console", undefined, "Codex", "idle", { agent: "codex" });
  const claude = sessionOf("claude", "console", undefined, "Claude", "idle", { account_id: "work", config_dir: "/work" });
  const reading = (agent: "claude" | "codex", account: string | null, used: number): AccountQuota => ({
    agent, account, fetched_at: 1, plan: null, unavailable: null,
    windows: [{ kind: agent === "codex" ? "session" : "weekly", label: null, used_percent: used, resets_at: null, window_minutes: null }],
  });
  const { store, select } = mount(statusItem, vi.fn(), {
    sessions: new Map([[codex.id, codex], [claude.id, claude]]),
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "", accounts: [{ id: "work", agent: "claude", name: "Work", config_dir: "/work" }] },
    agentAvailability: new Map([["codex", { agent: "codex", availability: "available" }], ["claude", { agent: "claude", availability: "available" }]]),
    accountQuotas: [reading("codex", null, 100), reading("claude", "work", 35)],
  });
  const settle = () => act(async () => {});
  const last = () => setMenu.mock.lastCall![0];
  await settle();
  expect(last().quota_remaining).toBeUndefined();
  select(codex.id);
  await settle();
  expect(last().quota_remaining).toBe(0);
  expect(last().quota_label).toContain("5-hour 0% remaining");
  select(claude.id);
  await settle();
  expect(last().quota_remaining).toBe(65);
  expect(last().quota_label).toContain("Work");
  expect(last().quota_label).toContain("Weekly 65% remaining");
  act(() => store.setState({ accountQuotas: [reading("claude", "work", 50)] }));
  await settle();
  expect(last().quota_remaining).toBe(50);
  act(() => store.setState({ accountQuotas: [] }));
  await settle();
  expect(last().quota_remaining).toBeUndefined();
  expect(last().quota_label).toBeUndefined();
  act(() => store.setState({ accountQuotas: [reading("claude", "work", 50)] }));
  await settle();
  expect(last().quota_remaining).toBe(50);
  select();
  await settle();
  expect(last().quota_remaining).toBeUndefined();
});

it("shows each submenu's current-console default account and follows console switches and edits", async () => {
  const setMenu = vi.fn<StatusItemCapability["setMenu"]>(() => Promise.resolve());
  const statusItem: StatusItemCapability = { setMenu, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  const work = consoleOf("work", "Work", { claude_account_id: "work-account" });
  const personal = consoleOf("personal", "Personal");
  const reading = (agent: "claude" | "codex", account: string | null, used: number): AccountQuota => ({
    agent, account, fetched_at: 1, plan: null, unavailable: null,
    windows: [{ kind: "session", label: null, used_percent: used, resets_at: null, window_minutes: 300 }],
  });
  const quotas = [reading("claude", "work-account", 35), reading("claude", null, 0), reading("codex", null, 100)];
  const { store, select } = mount(statusItem, vi.fn(), {
    consoles: new Map([[work.id, work], [personal.id, personal]]),
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "", accounts: [{ id: "work-account", agent: "claude", name: "Team", config_dir: "/team" }] },
    agentAvailability: new Map([["codex", { agent: "codex", availability: "available" }], ["claude", { agent: "claude", availability: "available" }]]),
    accountQuotas: quotas,
  });
  const settle = () => act(async () => {});
  const rows = () => setMenu.mock.lastCall![0].submenus;
  await settle();
  expect(rows().map((row) => row.quota_remaining)).toEqual([undefined, undefined]);
  select(undefined, work.id);
  await settle();
  expect(rows().map((row) => row.quota_remaining)).toEqual([65, 0]);
  expect(rows()[0]!.quota_label).toContain("Team");
  select(undefined, personal.id);
  await settle();
  expect(rows().map((row) => row.quota_remaining)).toEqual([100, 0]);
  expect(rows()[0]!.quota_label).toContain("Default");
  act(() => store.setState({ consoles: new Map([[work.id, work], [personal.id, { ...personal, claude_account_id: "work-account" }]]) }));
  await settle();
  expect(rows()[0]!.quota_remaining).toBe(65);
  act(() => store.setState({ accountQuotas: [{ ...quotas[0]!, unavailable: { code: "quota_timed_out", params: {}, message: "timeout" } }, ...quotas.slice(1)] }));
  await settle();
  expect(rows()[0]!.quota_remaining).toBeUndefined();
  expect(rows()[0]!.quota_label).toBeUndefined();
  select();
  await settle();
  expect(rows().map((row) => row.quota_remaining)).toEqual([undefined, undefined]);
});
