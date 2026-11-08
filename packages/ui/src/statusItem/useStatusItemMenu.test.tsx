// @vitest-environment jsdom
import { act, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { accountOf, consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import type { AccountQuota, Event, ClientRequestBody } from "../protocol";
import type { PlatformAdapter, StatusItemCapability } from "../platform";
import { DEFAULT_SESSION_ORDER } from "../sessions/order";
import { sessionOrderPreference, type State } from "../store";
import { BROWSER_HOST } from "../test/hosts";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import type { StatusItemMenuDraft } from "./statusItemMenu";
import { useStatusItemMenu } from "./useStatusItemMenu";
import { DEFAULT_GIT_WORKFLOW } from "../git/gitWorkflow";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { UpdatesProvider, useUpdateStore } from "../updates/UpdatesProvider";
import { useUpdateMenuLabel } from "../updates/useUpdateMenuLabel";
import type { UpdateStore } from "../updates/updateStore";

const withIcons = vi.hoisted(() => vi.fn(async (draft: StatusItemMenuDraft, _appearance: "light" | "dark") => ({ ...draft, icons: {} })));
vi.mock("./statusItemIcons", () => ({ withIcons }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Host({ selectedSessionId, currentConsoleId, updatesLabel }: { selectedSessionId?: string; currentConsoleId?: string; updatesLabel?: string }) {
  useStatusItemMenu({ ready: true, selectedSessionId, currentConsoleId, updatesLabel, onSessionChosen: () => {} });
  return null;
}

function mount(statusItem: StatusItemCapability, request: (body: ClientRequestBody) => Promise<Event>, initial: Partial<State> = {}) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const daemon = testDaemon({ state: initial, request });
  const select = (selectedSessionId?: string, currentConsoleId?: string) => act(() =>
    root.render(
      <TestHost daemon={daemon} platform={{ statusItem } as PlatformAdapter}>
        <Host selectedSessionId={selectedSessionId} currentConsoleId={currentConsoleId} />
      </TestHost>,
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
    setVisible: async () => {},
    onSessionChosen: () => () => {},
    onOpened(handler) {
      opened = handler;
      return undo;
    },
  };
  const request = vi.fn(() => Promise.reject(new Error("offline")));
  mount(statusItem, request);
  expect(request).not.toHaveBeenCalled();
  opened();
  opened();
  expect(request.mock.calls).toEqual([[{ type: "refresh_account_quotas" }], [{ type: "refresh_account_quotas" }]]);
});

it("updates the native icon on session selection, quota changes and cleared selection", async () => {
  const setMenu = vi.fn<StatusItemCapability["setMenu"]>(() => Promise.resolve());
  const statusItem: StatusItemCapability = { setMenu, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  const codex = sessionOf("codex", "console", undefined, "Codex", "idle", { agent: "codex" });
  const claude = sessionOf("claude", "console", undefined, "Claude", "idle", { account_id: "work", config_dir: "/work" });
  const reading = (agent: "claude" | "codex", account: string | null, used: number): AccountQuota => ({
    agent, account, fetched_at: 1, plan: null, unavailable: null,
    windows: [{ kind: agent === "codex" ? "session" : "weekly", label: null, used_percent: used, resets_at: null, window_minutes: null }], email: null,
  });
  const { store, select } = mount(statusItem, vi.fn(), {
    sessions: new Map([[codex.id, codex], [claude.id, claude]]),
    settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ["claude", "codex"], default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [accountOf("work", "claude", "Work", "/work")] },
    agentAvailability: new Map([["codex", { agent: "codex", availability: "available", default_account_dir: null }], ["claude", { agent: "claude", availability: "available", default_account_dir: null }]]),
    accountQuotas: [reading("codex", null, 100), reading("claude", "work", 35)],
  });
  const settle = () => act(async () => {});
  const last = () => setMenu.mock.lastCall![0];
  await settle();
  expect(last().quota_remaining).toBeUndefined();
  expect(last().keep_awake_label).toBe("Octoboard is keeping the computer awake");
  select(codex.id);
  await settle();
  expect(last().quota_remaining).toBe(0);
  expect(last().quota_label).toContain("5h 0% remaining");
  expect(last().keep_awake_label).toBe(`${last().quota_label}, Octoboard is keeping the computer awake`);
  expect(last().quota_weekly_remaining).toBeUndefined();
  act(() => store.setState({
    accountQuotas: [{ ...reading("codex", null, 20), windows: [
      { kind: "weekly", label: null, used_percent: 70, resets_at: null, window_minutes: null },
      { kind: "session", label: null, used_percent: 20, resets_at: null, window_minutes: null },
    ] }, reading("claude", "work", 35)],
  }));
  await settle();
  expect(last().quota_remaining).toBe(80);
  expect(last().quota_weekly_remaining).toBe(30);
  expect(last().quota_label).toContain("5h 80% remaining, Weekly 30% remaining");
  act(() => store.setState({ accountQuotas: [reading("codex", null, 100), reading("claude", "work", 35)] }));
  await settle();
  select(claude.id);
  await settle();
  expect(last().quota_remaining).toBe(65);
  expect(last().quota_weekly_remaining).toBeUndefined();
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
  const statusItem: StatusItemCapability = { setMenu, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  const work = consoleOf("work", "Work", { accounts: { claude: "work-account" } });
  const personal = consoleOf("personal", "Personal");
  const reading = (agent: "claude" | "codex", account: string | null, used: number): AccountQuota => ({
    agent, account, fetched_at: 1, plan: null, unavailable: null,
    windows: [{ kind: "session", label: null, used_percent: used, resets_at: null, window_minutes: 300 }], email: null,
  });
  const quotas = [reading("claude", "work-account", 35), reading("claude", null, 0), reading("codex", null, 100)];
  quotas[0]!.windows.push({ kind: "weekly", label: null, used_percent: 70, resets_at: null, window_minutes: 10080 });
  quotas[2]!.windows.push({ kind: "weekly", label: null, used_percent: 100, resets_at: null, window_minutes: 10080 });
  const { store, select } = mount(statusItem, vi.fn(), {
    consoles: new Map([[work.id, work], [personal.id, personal]]),
    settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ["claude", "codex"], default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [accountOf("work-account", "claude", "Team", "/team")] },
    agentAvailability: new Map([["codex", { agent: "codex", availability: "available", default_account_dir: null }], ["claude", { agent: "claude", availability: "available", default_account_dir: null }]]),
    accountQuotas: quotas,
  });
  const settle = () => act(async () => {});
  const rows = () => setMenu.mock.lastCall![0].submenus;
  await settle();
  expect(rows().map((row) => row.quota_remaining)).toEqual([undefined, undefined]);
  expect(rows().map((row) => row.quota_weekly_remaining)).toEqual([undefined, undefined]);
  select(undefined, work.id);
  await settle();
  expect(rows().map((row) => row.quota_remaining)).toEqual([65, 0]);
  expect(rows().map((row) => row.quota_weekly_remaining)).toEqual([30, 0]);
  expect(rows()[0]!.quota_label).toContain("Weekly 30% remaining");
  expect(rows()[0]!.quota_label).toContain("Team");
  select(undefined, personal.id);
  await settle();
  expect(rows().map((row) => row.quota_remaining)).toEqual([100, 0]);
  expect(rows().map((row) => row.quota_weekly_remaining)).toEqual([undefined, 0]);
  expect(rows()[0]!.quota_label).toContain("Default");
  act(() => store.setState({ consoles: new Map([[work.id, work], [personal.id, { ...personal, accounts: { ...personal.accounts, claude: "work-account" } }]]) }));
  await settle();
  expect(rows()[0]!.quota_remaining).toBe(65);
  act(() => store.setState({ accountQuotas: [{ ...quotas[0]!, unavailable: { code: "quota_timed_out", params: {}, message: "timeout" } }, ...quotas.slice(1)] }));
  await settle();
  expect(rows()[0]!.quota_remaining).toBeUndefined();
  expect(rows()[0]!.quota_weekly_remaining).toBeUndefined();
  expect(rows()[0]!.quota_label).toBeUndefined();
  select();
  await settle();
  expect(rows().map((row) => row.quota_remaining)).toEqual([undefined, undefined]);
  expect(rows().map((row) => row.quota_weekly_remaining)).toEqual([undefined, undefined]);
});

it("pushes the menu again when a held window's reset time passes, drawing it full", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const start = Date.UTC(2026, 9, 19, 12);
  vi.setSystemTime(start);
  const setMenu = vi.fn<StatusItemCapability["setMenu"]>(() => Promise.resolve());
  const statusItem: StatusItemCapability = { setMenu, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  const session = sessionOf("s", "console", undefined, "S", "idle", { agent: "codex" });
  const reading: AccountQuota = {
    agent: "codex", account: null, fetched_at: start - 30_000, plan: null, unavailable: null,
    windows: [{ kind: "session", label: null, used_percent: 100, resets_at: start + 10_000, window_minutes: 300 }], email: null,
  };
  mount(statusItem, vi.fn(), {
    sessions: new Map([[session.id, session]]),
    agentAvailability: new Map([["codex", { agent: "codex", availability: "available", default_account_dir: null }]]),
    accountQuotas: [reading],
  });
  await act(async () => {});
  expect(setMenu).toHaveBeenCalledTimes(1);
  await act(async () => void vi.advanceTimersByTime(9_000));
  expect(setMenu).toHaveBeenCalledTimes(1);
  await act(async () => void vi.advanceTimersByTime(1_000));
  expect(setMenu).toHaveBeenCalledTimes(2);
  expect(JSON.stringify(setMenu.mock.lastCall![0])).toContain("100% remaining");
});

it("pushes the menu again at each whole minute of a reading's age, so the age in its header stays current", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const start = Date.UTC(2026, 9, 19, 12);
  vi.setSystemTime(start);
  const setMenu = vi.fn<StatusItemCapability["setMenu"]>(() => Promise.resolve());
  const statusItem: StatusItemCapability = { setMenu, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  const reading: AccountQuota = {
    agent: "codex", account: null, fetched_at: start - 30_000, plan: null, unavailable: null,
    windows: [{ kind: "session", label: null, used_percent: 10, resets_at: null, window_minutes: 300 }], email: null,
  };
  mount(statusItem, vi.fn(), {
    agentAvailability: new Map([["codex", { agent: "codex", availability: "available", default_account_dir: null }]]),
    accountQuotas: [reading],
  });
  await act(async () => {});
  expect(setMenu).toHaveBeenCalledTimes(1);
  await act(async () => void vi.advanceTimersByTime(30_000));
  expect(setMenu).toHaveBeenCalledTimes(2);
  expect(JSON.stringify(setMenu.mock.lastCall![0])).toContain("updated 1 minute ago");
  await act(async () => void vi.advanceTimersByTime(60_000));
  expect(setMenu).toHaveBeenCalledTimes(3);
  expect(JSON.stringify(setMenu.mock.lastCall![0])).toContain("updated 2 minutes ago");
});

it("draws the pictures in the system's appearance, not the page's theme, and pushes the menu again when it changes", async () => {
  let dark = false;
  const listeners = new Set<() => void>();
  vi.stubGlobal("matchMedia", (query: string) => ({
    get matches() { return query.includes("dark") && dark; },
    addEventListener: (_type: string, listener: () => void) => void listeners.add(listener),
    removeEventListener: (_type: string, listener: () => void) => void listeners.delete(listener),
  }));
  onTestFinished(() => void vi.unstubAllGlobals());
  document.documentElement.dataset.theme = "light";
  onTestFinished(() => void delete document.documentElement.dataset.theme);
  withIcons.mockClear();
  const setMenu = vi.fn<StatusItemCapability["setMenu"]>(() => Promise.resolve());
  const statusItem: StatusItemCapability = { setMenu, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  mount(statusItem, vi.fn());
  await act(async () => {});
  expect(withIcons.mock.lastCall![1]).toBe("light");
  expect(setMenu).toHaveBeenCalledTimes(1);
  dark = true;
  await act(async () => listeners.forEach((listener) => listener()));
  // Nothing in the menu's own content changed, but its pictures are drawn in other colours.
  expect(withIcons.mock.lastCall![1]).toBe("dark");
  expect(setMenu).toHaveBeenCalledTimes(2);
  await act(async () => listeners.forEach((listener) => listener()));
  expect(setMenu).toHaveBeenCalledTimes(2);
});


it("keeps the tray update entry in sync even after the update notice is dismissed", async () => {
  const setMenu = vi.fn<StatusItemCapability["setMenu"]>(() => Promise.resolve());
  const statusItem: StatusItemCapability = { setMenu, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  let updates: UpdateStore | undefined;
  let available = true;
  const check = vi.fn(async () => available ? { version: "99.0.0" } : undefined);
  function UpdatedHost() {
    const store = useUpdateStore();
    useEffect(() => { updates = store; }, [store]);
    const updatesLabel = useUpdateMenuLabel();
    return <Host updatesLabel={updatesLabel} />;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(
    <TestHost daemon={testDaemon()} platform={{
      kind: "tauri", host: BROWSER_HOST, statusItem,
      updater: { currentVersion: async () => "1.0.0", check, install: async () => {} },
    }}>
      <UpdatesProvider firstCheckDelayMs={null}><UpdatedHost /></UpdatesProvider>
    </TestHost>,
  ));
  expect(setMenu.mock.lastCall![0].updates).toBe("Check for updates");
  await act(async () => updates!.check("manual"));
  expect(setMenu.mock.lastCall![0].updates).toContain("99.0.0");
  const sent = setMenu.mock.calls.length;
  await act(async () => updates!.dismissNotice());
  expect(setMenu.mock.calls).toHaveLength(sent);
  expect(setMenu.mock.lastCall![0].updates).toContain("99.0.0");
  available = false;
  await act(async () => updates!.check("manual"));
  expect(setMenu.mock.lastCall![0].updates).toBe("Check for updates");
  expect(check).toHaveBeenCalledTimes(2);
  available = true;
  await act(async () => updates!.check("manual"));
  await act(async () => updates!.install());
  expect(setMenu.mock.lastCall![0].updates).toBe("Updates");
});

it("pushes the menu again when the session order changes, listing the projects' sessions in the new order", async () => {
  onTestFinished(() => sessionOrderPreference.set(DEFAULT_SESSION_ORDER));
  const setMenu = vi.fn<StatusItemCapability["setMenu"]>(() => Promise.resolve());
  const statusItem: StatusItemCapability = { setMenu, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  const main = consoleOf("main", "Main");
  const alpha = projectOf("alpha", main.id, "Alpha");
  const beta = projectOf("beta", main.id, "Beta");
  const first = sessionOf("first", main.id, alpha.id, "First", "waiting_user", { started_at: 1 });
  const second = sessionOf("second", main.id, beta.id, "Second", "waiting_user", { started_at: 2 });
  mount(statusItem, vi.fn(), {
    consoles: new Map([[main.id, main]]),
    projects: new Map([[alpha.id, alpha], [beta.id, beta]]),
    sessions: new Map([[first.id, first], [second.id, second]]),
  });
  await act(async () => {});
  const listed = () => setMenu.mock.lastCall![0].sections[0]!.items.map((item) => item.session);
  expect(setMenu).toHaveBeenCalledTimes(1);
  expect(listed()).toEqual(["first", "second"]);
  await act(async () => sessionOrderPreference.set({ ...DEFAULT_SESSION_ORDER, activeProjects: "used" }));
  expect(setMenu).toHaveBeenCalledTimes(2);
  expect(listed()).toEqual(["second", "first"]);
});
