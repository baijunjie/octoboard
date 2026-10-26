// The daemon state the `AgentAccountsSection` tests run against, and the stand-ins their `vi.mock` factories
// return. The tests are split across several files by area; each file mocks the same six modules and loads this
// one from its factories, so it must import nothing those files mock.
import { useEffect, useSyncExternalStore } from "react";
import { vi } from "vitest";

import type { Account, AccountLogin, AccountQuota, AccountSharing, Agent, AgentAvailability, Console, Project, Session, Settings } from "../protocol";
import { DEFAULT_GIT_WORKFLOW } from "../gitWorkflow";

export const request = vi.fn(() => Promise.resolve({}));
export const toastError = vi.fn();
export const settings: Settings = {
  auto_pull_repositories: false,
  auto_delete_unresumable_archives: true,
  added_agents: ["claude", "codex", "grok"], default_clone_dir: "/home/dev/Projects", git_workflow: DEFAULT_GIT_WORKFLOW,
  accounts: [],
};
export const accountQuotas: AccountQuota[] = [];
export const accountLogins: AccountLogin[] = [];
export const accountSharing = new Map<string, AccountSharing>();
export const availability = new Map<Agent, AgentAvailability>();
export const consoles = new Map<string, Console>();
export const projects = new Map<string, Project>();
export const sessions = new Map<string, Session>();
export const dismissAccountLogin = vi.fn((_store: unknown, login: AccountLogin) => {
  const at = accountLogins.indexOf(login);
  if (at >= 0) accountLogins.splice(at, 1);
});
export const trackAccountLogin = vi.fn();
// The store is not reactive here; a test that changes it calls `bus.notify`, which re-renders whatever reads it.
export const bus = (() => {
  let version = 0;
  const listeners = new Set<() => void>();
  return {
    subscribe: (listener: () => void) => (listeners.add(listener), () => listeners.delete(listener)),
    version: () => version,
    notify: () => {
      version += 1;
      listeners.forEach((listener) => listener());
    },
  };
})();
const state = () => ({ settings, agentAvailability: availability, accountQuotas, accountLogins, accountSharing, consoles, projects, sessions });
// What a section does when focus is lost, with the Settings dialog's section entry stood in for by a button.
export const sectionEntry = { current: null as HTMLElement | null };

export const account = (id: string, agent: Account["agent"]): Account => ({ id, agent, name: id, config_dir: `/home/dev/.${id}` });

/** `../store` reading `state()`, with `request` and `toastError` as the daemon's. */
export const storeStandIn = (original: typeof import("../store")) => ({
  ...original,
  useDaemon: () => ({ request, toastError, store: { getState: () => state() } }),
  useDaemonStore: (selector: (s: unknown) => unknown) => {
    useSyncExternalStore(bus.subscribe, bus.version);
    return selector(state());
  },
  useDaemonStoreEqual: (selector: (s: unknown) => unknown) => selector(state()),
  dismissAccountLogin: (...args: Parameters<typeof dismissAccountLogin>) => dismissAccountLogin(...args),
  trackAccountLogin: (...args: unknown[]) => trackAccountLogin(...args),
});

/** `./useSectionRefocus` taking focus that fell to `<body>` to `sectionEntry`. */
export const sectionRefocusStandIn = (original: typeof import("./useSectionRefocus")) => ({
  ...original,
  useSectionRefocus: (deps: unknown[]) => useEffect(() => {
    if (document.activeElement === document.body) sectionEntry.current?.focus();
  }, deps),
});

/** `OptionSelect` as a native `<select>`. */
export const OptionSelectStandIn = ({ label, value, onChange, options, isDisabled }: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string; isDisabled?: boolean }[];
  isDisabled?: boolean;
}) => (
  <select aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} disabled={isDisabled}>
    {options.map((option) => <option key={option.value} value={option.value} disabled={option.isDisabled}>{option.label}</option>)}
  </select>
);

/** `../components/AgentIcon` drawing no icon. */
export const agentIconStandIn = (original: typeof import("../components/AgentIcon")) => ({ ...original, AgentIcon: () => null });

/** `FadeOverflow` as its bare children. */
export const FadeOverflowStandIn = ({ children }: { children: React.ReactNode }) => children;

/** `MarkedPath` as the bare path. */
export const MarkedPathStandIn = ({ path }: { path: string }) => path;
