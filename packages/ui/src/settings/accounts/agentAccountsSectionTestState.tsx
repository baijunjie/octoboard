// The daemon state the `AgentAccountsSection` tests start from, and the stand-ins their `vi.mock` factories return.
// The tests are split across several files by area; each file mocks the same five modules and loads this one from its
// factories, so it must import nothing those files mock.
import { useEffect } from "react";
import { vi } from "vitest";

import { DEFAULT_SETTINGS, type Daemon } from "../../store";
import { accountOf } from "../../fixtures/builders";
import { testDaemon } from "../../test/testDaemon";
import type { Account, AccountLogin, AccountQuota, AccountSharing, Agent, AgentAvailability, AgentCliStatus, Console, Event, Project, ClientRequestBody, Session, Settings } from "../../protocol";

export const request = vi.fn((_body: ClientRequestBody): Promise<object> => Promise.resolve({}));
/** The answer to an account's MCP inventory reading, kept apart from `request` so that the tests stubbing every other
 * request need not answer it: by default a reading that never answers, which leaves the Connectors and MCP dialog reading. */
export const readMcpInventory = vi.fn((_body: Extract<ClientRequestBody, { type: "read_account_mcp_inventory" }>): Promise<object> => new Promise(() => {}));
export const showNotice = vi.fn();
export const settings: Settings = { ...DEFAULT_SETTINGS, added_agents: ["claude", "codex", "grok"], default_clone_dir: "/home/dev/Projects" };
export const accountQuotas: AccountQuota[] = [];
export const accountLogins: AccountLogin[] = [];
export const accountSharing = new Map<string, AccountSharing>();
export const availability = new Map<Agent, AgentAvailability>();
export const cliStatuses = new Map<Agent, AgentCliStatus>();
export const consoles = new Map<string, Console>();
export const projects = new Map<string, Project>();
export const sessions = new Map<string, Session>();

/** The daemon a test mounts the section over: its store starts at a copy of the state above, so a test lays out what
 * the section opens on by changing that state before it mounts, and changes the store itself afterwards. */
export const startDaemon = (): Daemon =>
  testDaemon({
    state: structuredClone({ settings, agentAvailability: availability, agentCliStatuses: cliStatuses, accountQuotas, accountLogins, accountSharing, consoles, projects, sessions }),
    request: (body) => (body.type === "read_account_mcp_inventory" ? readMcpInventory(body) : request(body)) as Promise<Event>,
    showNotice,
  });

// What a section does when focus is lost, with the Settings dialog's section entry stood in for by a button.
export const sectionEntry = { current: null as HTMLElement | null };

export const account = (id: string, agent: Account["agent"]): Account => accountOf(id, agent, id, `/home/dev/.${id}`);

/** `../useSectionRefocus` taking focus that fell to `<body>` to `sectionEntry`. */
export const sectionRefocusStandIn = (original: typeof import("../useSectionRefocus")) => ({
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

/** `../../components/icons/AgentIcon` drawing no icon. */
export const agentIconStandIn = (original: typeof import("../../components/icons/AgentIcon")) => ({ ...original, AgentIcon: () => null });

/** `FadeOverflow` as its bare children. */
export const FadeOverflowStandIn = ({ children }: { children: React.ReactNode }) => children;

/** `MarkedPath` as the bare path. */
export const MarkedPathStandIn = ({ path }: { path: string }) => path;
