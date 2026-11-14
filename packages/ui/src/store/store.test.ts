// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import { accountOf, markdownContent, pageOf, sessionOf } from "../fixtures/builders";
import { DEFAULT_SETTINGS } from "./defaultSettings";
import type { ConnectionState } from "../daemon-client";
import type { Event, Project, ProviderInfo, ShellTerminal } from "../protocol";
import { TEST_PROVIDERS } from "../test/testProviders";
import { accountQuotaOf } from "../accounts/accountQuotaOf";
import { createDaemon, dismissAccountLogin, setAccountSwitchesQueued, trackAccountLogin, trackAgentCliUpdate, trackPagesListing } from ".";

const client = vi.hoisted(() => ({
  onEvent: undefined as ((event: Event) => void) | undefined,
  onEvents: undefined as ((events: Event[]) => void) | undefined,
  onConnectionChange: undefined as ((state: ConnectionState) => void) | undefined,
  keyCheck: undefined as (() => Promise<boolean>) | undefined,
}));
vi.mock("../daemon-client", () => ({
  DaemonClient: class {
    constructor(_url: string, keyCheck?: () => Promise<boolean>) { client.keyCheck = keyCheck; }
    onEvent(listener: (event: Event) => void) { client.onEvent = listener; }
    onEvents(listener: (events: Event[]) => void) { client.onEvents = listener; }
    onConnectionChange(listener: (state: ConnectionState) => void) { client.onConnectionChange = listener; }
  },
}));

it.each(["reply-first", "broadcast-first"] as const)("keeps the authoritative session title when opening a session: %s", (order) => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const original = sessionOf("s", "c", "p", "Created name", "idle");
  const current = { ...original, terminal_title: "Current task", status: "working" as const };
  const reply: Event = { type: "session_opened", id: "request", session: original };
  const broadcast: Event = { type: "session_upserted", session: current };
  for (const event of order === "reply-first" ? [reply, broadcast] : [broadcast, reply]) {
    client.onEvents!([event]);
    if (event === reply && order === "reply-first") expect(daemon.store.getState().sessions.get(original.id)).toBe(original);
  }
  expect(daemon.store.getState().sessions.get(original.id)).toBe(current);
});

it("keeps the stored session, and does not notify, when an upsert repeats what it already holds", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const session = sessionOf("s", "c", "p", "Same", "working");
  client.onEvents!([{ type: "session_upserted", session }]);
  const before = daemon.store.getState();
  const notified = vi.fn();
  daemon.store.subscribe(notified);
  client.onEvents!([{ type: "session_upserted", session: { ...session } }]);
  expect(daemon.store.getState()).toBe(before);
  expect(daemon.store.getState().sessions.get("s")).toBe(session);
  expect(notified).not.toHaveBeenCalled();
});

it("holds the daemon the hello describes, none before it", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  expect(daemon.store.getState().daemon).toBeNull();
  client.onEvents!([{ type: "hello", generation: 1, daemon_version: "1.4.0" }]);
  expect(daemon.store.getState().daemon).toEqual({ version: "1.4.0", generation: 1 });
});

it("forgets the daemon its hello described once the connection drops", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  client.onEvents!([{ type: "hello", generation: 1, daemon_version: "1.4.0" }]);
  client.onConnectionChange!("reconnecting");
  expect(daemon.store.getState().daemon).toBeNull();
});

it("holds the state of a key the daemon refused", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  client.onConnectionChange!("keyRefused");
  expect(daemon.store.getState().connectionState).toBe("keyRefused");
});

it("gives the client a key check only for a page the daemon itself served", () => {
  createDaemon({ origin: "ws://127.0.0.1:1234", key: "k" });
  expect(client.keyCheck).toBeUndefined();
  createDaemon({ origin: "ws://localhost:1234", key: "k", servedByDaemon: true });
  expect(client.keyCheck).toBeTypeOf("function");
});

it("applies the events of one batch as a single update", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const notified = vi.fn();
  daemon.store.subscribe(notified);
  client.onEvents!([
    { type: "session_upserted", session: sessionOf("a", "c", "p", "A", "working") },
    { type: "session_upserted", session: sessionOf("b", "c", "p", "B", "idle") },
    { type: "session_deleted", session: "a" },
  ]);
  expect(notified).toHaveBeenCalledTimes(1);
  expect(Array.from(daemon.store.getState().sessions.keys())).toEqual(["b"]);
});

it("keeps one login per account from its updates and replaces them all with a snapshot's", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const login = { agent: "grok" as const, account: null, status: "waiting" as const, authorization_url: "https://x", device_code: "ABCD-1234", opened_by_agent: true, failure: null };
  client.onEvents!([{ type: "account_login_updated", login: { ...login, status: "starting", device_code: null } }]);
  client.onEvents!([{ type: "account_login_updated", login: { ...login, account: "a" } }]);
  client.onEvents!([{ type: "account_login_updated", login }]);
  const held = daemon.store.getState().accountLogins;
  expect(held.map((entry) => [entry.account, entry.status])).toEqual([["a", "waiting"], [null, "waiting"]]);
  client.onEvents!([{ type: "account_login_updated", login: { ...login, status: "succeeded" } }]);
  expect(daemon.store.getState().accountLogins.find((entry) => entry.account === null)?.status).toBe("succeeded");
});

it("drops an ended login the user dismissed, and only that one, so it does not come back when the row mounts again", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const login = { agent: "claude" as const, account: "a", status: "failed" as const, authorization_url: null, device_code: null, opened_by_agent: false, failure: null };
  client.onEvents!([{ type: "account_login_updated", login }]);
  const held = daemon.store.getState().accountLogins[0]!;
  // The account's next login is a new object, which a dismissal of the earlier one must not take with it.
  client.onEvents!([{ type: "account_login_updated", login: { ...login, status: "starting" } }]);
  dismissAccountLogin(daemon.store, held);
  expect(daemon.store.getState().accountLogins.map((entry) => entry.status)).toEqual(["starting"]);
  dismissAccountLogin(daemon.store, daemon.store.getState().accountLogins[0]!);
  expect(daemon.store.getState().accountLogins).toEqual([]);
});

it("remembers the logins this window started until they end, and forgets those a snapshot no longer lists", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const login = { agent: "grok" as const, account: null, status: "waiting" as const, authorization_url: "https://x", device_code: null, opened_by_agent: false, failure: null };
  trackAccountLogin(daemon.store, "grok", null, true);
  trackAccountLogin(daemon.store, "grok", "a", true);
  trackAccountLogin(daemon.store, "grok", "a", true);
  expect(daemon.store.getState().loginsStartedHere).toEqual(["grok:", "grok:a"]);
  client.onEvents!([{ type: "account_login_updated", login }]);
  expect(daemon.store.getState().loginsStartedHere).toEqual(["grok:", "grok:a"]);
  client.onEvents!([{ type: "account_login_updated", login: { ...login, status: "succeeded" } }]);
  expect(daemon.store.getState().loginsStartedHere).toEqual(["grok:a"]);
  trackAccountLogin(daemon.store, "grok", "a", false);
  expect(daemon.store.getState().loginsStartedHere).toEqual([]);
  trackAccountLogin(daemon.store, "grok", null, true);
  const snapshot = {
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [{ ...login, account: "b" }], account_sharing: [], agent_cli_statuses: [], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
  } as unknown as Event;
  client.onEvents!([snapshot]);
  expect(daemon.store.getState().loginsStartedHere).toEqual([]);
});

it("stores every session as the daemon sent it while this window has it queued for an account switch", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  setAccountSwitchesQueued(daemon.store, ["s"], true);
  const sent = sessionOf("s", "c", "p", "Queued", "idle");
  const snapshot = {
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [sent], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
  } as unknown as Event;
  client.onEvents!([snapshot]);
  expect(daemon.store.getState().sessions.get("s")).toBe(sent);
  const switching = { ...sent, switching_account: true as const };
  client.onEvents!([{ type: "session_upserted", session: switching }]);
  expect(daemon.store.getState().sessions.get("s")).toBe(switching);
  const done = { ...sent, account_id: "a-1" };
  client.onEvents!([{ type: "session_upserted", session: done }]);
  expect(daemon.store.getState().sessions.get("s")).toBe(done);
  expect(daemon.store.getState().queuedAccountSwitches).toEqual(new Set(["s"]));
});

it("replaces the account quotas with each update and keeps the entry of an account whose reading did not change", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const window = { kind: "session" as const, label: null, used_percent: 10, resets_at: null, window_minutes: 300 };
  const first = { agent: "claude" as const, account: null, fetched_at: 1, plan: "max", windows: [window], unavailable: null, email: null };
  const second = { ...first, agent: "codex" as const, plan: "plus" };
  client.onEvents!([{ type: "account_quotas_updated", account_quotas: [first, second] }]);
  const held = daemon.store.getState().accountQuotas;
  expect(accountQuotaOf(held, "codex", null)).toBe(held[1]);
  client.onEvents!([{ type: "account_quotas_updated", account_quotas: [first, { ...second, fetched_at: 2 }] }]);
  const after = daemon.store.getState().accountQuotas;
  expect(after[0]).toBe(held[0]);
  expect(after[1]!.fetched_at).toBe(2);
  client.onEvents!([{ type: "account_quotas_updated", account_quotas: [first] }]);
  expect(daemon.store.getState().accountQuotas).toHaveLength(1);
  expect(accountQuotaOf(daemon.store.getState().accountQuotas, "codex", null)).toBeUndefined();
});

it("keeps each account's sharing state from the snapshot and the account_sharing events, and drops a free account", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const running = { account: "a", progress: { share: true, phase: "changing" as const, done: 1, total: 4, item: "skills" } };
  const snapshot = {
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [running, { account: "free" }], agent_cli_statuses: [], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
  } as unknown as Event;
  client.onEvents!([snapshot]);
  const held = daemon.store.getState().accountSharing.get("a");
  expect([...daemon.store.getState().accountSharing.keys()]).toEqual(["a"]);
  client.onEvents!([snapshot]);
  expect(daemon.store.getState().accountSharing.get("a")).toBe(held);
  client.onEvents!([{ type: "account_sharing", account: "a", progress: { share: true, phase: "changing" as const, done: 2, total: 4, item: "plugins" } }]);
  expect(daemon.store.getState().accountSharing.get("a")).toEqual({ account: "a", progress: { share: true, phase: "changing" as const, done: 2, total: 4, item: "plugins" } });
  client.onEvents!([{ type: "account_sharing", account: "b", in_use: { sessions: 2, signing_in: false } }]);
  expect(daemon.store.getState().accountSharing.get("b")).toEqual({ account: "b", in_use: { sessions: 2, signing_in: false } });
  const failure = { share: true, cause: "other" as const, items: [{ item: "skills", left: "backed_up" as const, cause: "other" as const, detail: "denied" }], detail: "summary" };
  client.onEvents!([{ type: "account_sharing", account: "a", failure }]);
  expect(daemon.store.getState().accountSharing.get("a")).toEqual({ account: "a", failure });
  client.onEvents!([{ type: "account_sharing", account: "a" }]);
  expect(daemon.store.getState().accountSharing.has("a")).toBe(false);
  expect(daemon.store.getState().accountSharing.has("b")).toBe(true);
});

it("keeps each agent's CLI status from the snapshot and the agent_cli_status_updated events", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const status = { agent: "claude" as const, installed_version: "2.1.319", install_method: "npm" as const, latest_version: "2.1.320", latest_error: null, latest_detail: null, checked_at: 1, update: null };
  const snapshot = {
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [status], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
  } as unknown as Event;
  client.onEvents!([snapshot]);
  const held = daemon.store.getState().agentCliStatuses.get("claude");
  expect(held).toEqual(status);
  client.onEvents!([snapshot]);
  expect(daemon.store.getState().agentCliStatuses.get("claude")).toBe(held);
  client.onEvents!([{ type: "agent_cli_status_updated", status: { ...status } }]);
  expect(daemon.store.getState().agentCliStatuses.get("claude")).toBe(held);
  const running = { ...status, update: { state: "running" as const, started_at: 2, finished_at: null, error: null, detail: null } };
  client.onEvents!([{ type: "agent_cli_status_updated", status: running }, { type: "agent_cli_status_updated", status: { ...status, agent: "codex" } }]);
  expect(daemon.store.getState().agentCliStatuses.get("claude")).toEqual(running);
  expect([...daemon.store.getState().agentCliStatuses.keys()]).toEqual(["claude", "codex"]);
});

it("keeps the daemon's own update from the snapshot and from daemon_update_status_updated, null where the daemon has none", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  expect(daemon.store.getState().daemonUpdate).toBeNull();
  const idle = { phase: "idle" as const, available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };
  const snapshotOf = (daemon_update: object | null) => ({
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [], daemon_update, home_dir: null, host: { os: "linux", path_style: "unix", file_links: false, trash: "none", settings: "none", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
  }) as unknown as Event;
  client.onEvents!([snapshotOf(idle)]);
  const held = daemon.store.getState().daemonUpdate;
  expect(held).toEqual(idle);
  client.onEvents!([snapshotOf({ ...idle })]);
  expect(daemon.store.getState().daemonUpdate).toBe(held);
  const downloading = { ...idle, phase: "downloading" as const, available: { version: "0.2.0", holder_floor: null, channel: "stable" as const }, progress: { downloaded: 1, total: 10 } };
  client.onEvents!([{ type: "daemon_update_status_updated", status: downloading }]);
  expect(daemon.store.getState().daemonUpdate).toEqual(downloading);
  client.onEvents!([{ type: "daemon_update_status_updated", status: { ...downloading } }]);
  expect(daemon.store.getState().daemonUpdate).toEqual(downloading);
  client.onEvents!([snapshotOf(null)]);
  expect(daemon.store.getState().daemonUpdate).toBeNull();
});

it("remembers the CLI updates this window asked for with the update each status held, and keeps after a snapshot one it shows running", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const status = (agent: "claude" | "codex" | "grok", update: object | null) => ({ agent, installed_version: "1.0.0", install_method: "npm", latest_version: "1.0.1", latest_error: null, latest_detail: null, checked_at: 1, update });
  trackAgentCliUpdate(daemon.store, "claude", null);
  trackAgentCliUpdate(daemon.store, "codex", 7);
  expect([...daemon.store.getState().cliUpdatesStartedHere]).toEqual([["claude", null], ["codex", 7]]);
  const held = daemon.store.getState().cliUpdatesStartedHere;
  trackAgentCliUpdate(daemon.store, "claude", null);
  expect(daemon.store.getState().cliUpdatesStartedHere).toBe(held);
  const running = { state: "running", started_at: 8, finished_at: null, error: null, detail: null };
  client.onEvents!([{
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [status("claude", null), status("codex", running)], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
  } as unknown as Event]);
  expect([...daemon.store.getState().cliUpdatesStartedHere.keys()]).toEqual(["codex"]);
  trackAgentCliUpdate(daemon.store, "codex", undefined);
  expect(daemon.store.getState().cliUpdatesStartedHere.size).toBe(0);
});

describe("the CLI updates this window asked for, across a snapshot", () => {
  const status = (agent: "claude" | "codex" | "grok", update: object | null) => ({ agent, installed_version: "1.0.0", install_method: "npm", latest_version: "1.0.1", latest_error: null, latest_detail: null, checked_at: 1, update });
  const ended = (started_at: number) => ({ state: "failed", started_at, finished_at: started_at + 1, error: "agent_cli_update_failed", detail: null });
  const snapshotOf = (...statuses: object[]) => ({
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: statuses, home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
  }) as unknown as Event;

  it("keeps one whose status holds an update that ended as another one than the request found, for the alert", () => {
    const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
    trackAgentCliUpdate(daemon.store, "claude", 5);
    trackAgentCliUpdate(daemon.store, "codex", null);
    client.onEvents!([snapshotOf(status("claude", ended(9)), status("codex", ended(3)))]);
    expect([...daemon.store.getState().cliUpdatesStartedHere]).toEqual([["claude", 5], ["codex", null]]);
  });

  it("drops one whose agent has no status, whose status holds no update, or still the earlier one that ended", () => {
    const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
    trackAgentCliUpdate(daemon.store, "claude", 5);
    trackAgentCliUpdate(daemon.store, "codex", 6);
    trackAgentCliUpdate(daemon.store, "grok", 7);
    client.onEvents!([snapshotOf(status("claude", ended(5)), status("codex", null))]);
    expect(daemon.store.getState().cliUpdatesStartedHere.size).toBe(0);
  });

  it("keeps the map it holds when the snapshot drops none", () => {
    const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
    trackAgentCliUpdate(daemon.store, "claude", 5);
    const held = daemon.store.getState().cliUpdatesStartedHere;
    client.onEvents!([snapshotOf(status("claude", ended(9)))]);
    expect(daemon.store.getState().cliUpdatesStartedHere).toBe(held);
  });
});

it("keeps the shell terminals a snapshot lists and the broadcasts change, and drops those of a removed project", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const shell = (id: string, project: string): ShellTerminal => ({ id, project_id: project, worktree: null, cwd: `/${project}`, title: "zsh", renamed: false, state: "running", busy: false, created_at: 1 });
  const project = (id: string) => ({ id, console_id: "c", name: id }) as Project;
  const snapshot = {
    type: "snapshot", hosts: [], consoles: [], projects: [project("p"), project("q")], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [shell("a", "p"), shell("b", "q")],
  } as unknown as Event;
  client.onEvents!([snapshot]);
  const held = daemon.store.getState().shellTerminals.get("a");
  expect([...daemon.store.getState().shellTerminals.keys()]).toEqual(["a", "b"]);
  // A snapshot naming it again, unchanged, keeps the record a component holds.
  client.onEvents!([snapshot]);
  expect(daemon.store.getState().shellTerminals.get("a")).toBe(held);
  client.onEvents!([{ type: "shell_terminal_upserted", shell_terminal: { ...shell("a", "p"), busy: true } }]);
  expect(daemon.store.getState().shellTerminals.get("a")?.busy).toBe(true);
  client.onEvents!([{ type: "shell_terminal_deleted", shell_terminal: "a" }]);
  expect(daemon.store.getState().shellTerminals.has("a")).toBe(false);
  client.onEvents!([{ type: "project_deleted", project: "q" }]);
  expect(daemon.store.getState().shellTerminals.size).toBe(0);
});

// Whether a project lies under a trusted directory is the daemon's call, made from where it really
// is on disk, so the queue holds every prompt it sends and drops one only when that session stops
// waiting at the confirmation, which the daemon shows as `waiting_user`.
it("queues every trust prompt the daemon sends, even for a path under a trusted directory", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  client.onEvents!([
    { type: "trusted_directories_updated", trusted_directories: ["/work"] },
    { type: "trust_prompt", session: "s", agent: "claude", project: "p", path: "/work/link/app", trust_dir: null },
  ]);
  expect(daemon.store.getState().trustPrompts.map((p) => p.session)).toEqual(["s"]);
});

it("drops a trust prompt when its own session stops waiting, and only then", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const prompt = (session: string): Event => ({ type: "trust_prompt", session, agent: "claude", project: "p", path: "/work/app", trust_dir: null });
  const queued = () => daemon.store.getState().trustPrompts.map((p) => p.session);
  client.onEvents!([{ type: "session_upserted", session: sessionOf("a", "c", "p", "A", "working") }, prompt("a"), prompt("b")]);
  // An upsert for another session leaves the queue be, and so does one that still shows the wait.
  client.onEvents!([{ type: "session_upserted", session: sessionOf("c", "c", "p", "C", "idle") }]);
  expect(queued()).toEqual(["a", "b"]);
  client.onEvents!([{ type: "session_upserted", session: sessionOf("a", "c", "p", "A", "waiting_user") }]);
  expect(queued()).toEqual(["a", "b"]);
  client.onEvents!([{ type: "session_upserted", session: sessionOf("a", "c", "p", "A", "working") }]);
  expect(queued()).toEqual(["b"]);
  client.onEvents!([{ type: "session_upserted", session: sessionOf("b", "c", "p", "B", "interrupted") }]);
  expect(queued()).toEqual([]);
});

it("holds the prompt a snapshot's repeat brings back, until the session stops waiting", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const snapshot = {
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [sessionOf("s", "c", "p", "S", "waiting_user")], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
  } as unknown as Event;
  const prompt: Event = { type: "trust_prompt", session: "s", agent: "claude", project: "p", path: "/work/app", trust_dir: null };
  client.onEvents!([prompt, snapshot, prompt]);
  expect(daemon.store.getState().trustPrompts.map((p) => p.session)).toEqual(["s"]);
  client.onEvents!([{ type: "session_upserted", session: sessionOf("s", "c", "p", "S", "idle") }]);
  expect(daemon.store.getState().trustPrompts).toEqual([]);
});

// A press that failed leaves the confirmation to the person in the terminal, so its prompt goes; a
// notice of another kind, or about another session, leaves the queue be.
it("drops a trust prompt when the daemon could not press its session's confirmation", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const prompt = (session: string): Event => ({ type: "trust_prompt", session, agent: "claude", project: "p", path: "/work/app", trust_dir: null });
  const notice = (session: string, code: string): Event => ({ type: "session_notice", session, code, params: {}, message: "" });
  const queued = () => daemon.store.getState().trustPrompts.map((p) => p.session);
  client.onEvents!([prompt("a"), prompt("b")]);
  client.onEvents!([notice("a", "trust_not_carried_over"), notice("c", "trust_answer_failed")]);
  expect(queued()).toEqual(["a", "b"]);
  client.onEvents!([notice("a", "trust_answer_failed")]);
  expect(queued()).toEqual(["b"]);
});

it("keeps what a session request says it does, and drops it when it closes", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const base = { compatibility: {}, type: "session_request" as const, session: "s", kind: "project" as const, console: "c", project: "p", agent: "claude" as const, branches: [], requested_at: 1 };
  const resume: Event = {
    ...base, request_id: "r-1", action: "reopen", target: "t", owner: "o", reopened_owners: ["o"], account: null, conversation: null,
  };
  const start: Event = { ...base, request_id: "r-2", action: "start", target: null, owner: null, reopened_owners: [], account: null, conversation: null, requested_at: 2 };
  client.onEvents!([start, resume, resume]);
  const [first, second] = daemon.store.getState().sessionRequests;
  if (first.action !== "reopen") throw new Error("expected a reopen");
  expect([first.requestId, first.target, first.owner, first.reopenedOwners]).toEqual(["r-1", "t", "o", ["o"]]);
  expect([second.requestId, second.action]).toEqual(["r-2", "start"]);
  client.onEvents!([{ type: "session_request_closed", request_id: "r-1", reason: "approved" }]);
  expect(daemon.store.getState().sessionRequests.map((r) => r.requestId)).toEqual(["r-2"]);
});

it("keeps what a switch and a handoff request carry", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const base = { compatibility: {}, type: "session_request" as const, session: "s", kind: "project" as const, console: "c", project: "p", agent: "codex" as const, branches: [], requested_at: 1, owner: null, reopened_owners: [], conversation: null, target: "t", account: "a" };
  client.onEvents!([
    { ...base, request_id: "r-1", action: "switch" },
    { ...base, request_id: "r-2", action: "handoff", account: null, model: "m", effort: null, requested_at: 2 },
  ]);
  const [switching, handing] = daemon.store.getState().sessionRequests;
  expect(switching).toMatchObject({ action: "switch", target: "t", account: "a" });
  expect(handing).toMatchObject({ action: "handoff", target: "t", account: null, model: "m", effort: null });
});

it("does not offer a session request that lacks what its action needs or has an action it does not know", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const base = { compatibility: {}, type: "session_request" as const, session: "s", kind: "project" as const, console: "c", project: "p", agent: "claude" as const, branches: [], requested_at: 1, owner: null, reopened_owners: [], target: null, account: null, conversation: null };
  client.onEvents!([
    { ...base, request_id: "r-1", action: "reopen" },
    { ...base, request_id: "r-2", action: "resume" },
    { ...base, request_id: "r-3", action: "resume_record" },
    { ...base, request_id: "r-5", action: "switch" },
    { ...base, request_id: "r-6", action: "handoff" },
    { ...base, request_id: "r-4", action: "from_the_future" as never },
  ]);
  expect(daemon.store.getState().sessionRequests).toEqual([]);
});

it("drops the pages a pages_deleted event names from that session's list only, keeping an emptied list as a known one", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  client.onEvents!([
    { type: "page_created", page: pageOf("a-1", "session-a", markdownContent("1"), 3) },
    { type: "page_created", page: pageOf("a-2", "session-a", markdownContent("2"), 2) },
    { type: "page_created", page: pageOf("b-1", "session-b", markdownContent("1"), 1) },
  ]);
  const ids = (session: string) => daemon.store.getState().pages.get(session)?.map((page) => page.id);

  client.onEvents!([{ type: "pages_deleted", session_id: "session-a", pages: ["a-1", "b-1"] }]);
  expect(ids("session-a")).toEqual(["a-2"]);
  expect(ids("session-b")).toEqual(["b-1"]);

  client.onEvents!([{ type: "pages_deleted", session_id: "session-a", pages: ["a-2"] }]);
  expect(ids("session-a")).toEqual([]);

  // A session whose pages were never listed has nothing to drop, and no list is made for it.
  client.onEvents!([{ type: "pages_deleted", session_id: "session-c", pages: ["c-1"] }]);
  expect(daemon.store.getState().pages.has("session-c")).toBe(false);
});

it("sets and clears a page's submission from page_submission in every window's store, for the page it names only", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  client.onEvents!([
    { type: "page_created", page: pageOf("a-1", "session-a", markdownContent("1"), 2) },
    { type: "page_created", page: pageOf("a-2", "session-a", markdownContent("2"), 1) },
  ]);
  const submissions = () => daemon.store.getState().pages.get("session-a")?.map((page) => page.submission);

  client.onEvents!([{ type: "page_submission", session_id: "session-a", page: "a-2", submission: { note: "Ada", tags: ["x"] }, submission_revision: 1 }]);
  expect(submissions()).toEqual([null, { note: "Ada", tags: ["x"] }]);

  // The mark taken off after a delivery that certainly failed.
  client.onEvents!([{ type: "page_submission", session_id: "session-a", page: "a-2", submission: null, submission_revision: 2 }]);
  expect(submissions()).toEqual([null, null]);

  // A page this window does not hold, or a session it has not listed, has nothing to mark yet.
  const before = daemon.store.getState().pages;
  client.onEvents!([
    { type: "page_submission", session_id: "session-a", page: "a-9", submission: { note: "x" }, submission_revision: 1 },
    { type: "page_submission", session_id: "session-z", page: "z-1", submission: { note: "x" }, submission_revision: 1 },
  ]);
  expect(daemon.store.getState().pages).toBe(before);
});

describe("a page's submission heard more than once", () => {
  const page = (submission: Record<string, string> | null, revision: number) => ({ ...pageOf("a-1", "session-a", markdownContent("1"), 1, submission), submission_revision: revision });
  const list = (submission: Record<string, string> | null, revision: number): Event => ({ type: "page_list", id: "r", session_id: "session-a", pages: [page(submission, revision)] });
  const heard = (submission: Record<string, string> | null, revision: number): Event => ({ type: "page_submission", session_id: "session-a", page: "a-1", submission, submission_revision: revision });
  const held = (daemon: ReturnType<typeof createDaemon>) => {
    const page = daemon.store.getState().pages.get("session-a")?.[0];
    return page && { submission: page.submission, revision: page.submission_revision };
  };

  it("keeps the later of two page_submission events, whichever arrives first", () => {
    const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
    client.onEvents!([list(null, 0)]);
    // The second mark arrives before the clearing of the first.
    client.onEvents!([heard({ who: "Grace" }, 3), heard(null, 2), heard({ who: "Ada" }, 1)]);
    expect(held(daemon)).toEqual({ submission: { who: "Grace" }, revision: 3 });
  });

  it.each([
    { name: "a mark", now: { submission: { who: "Ada" }, revision: 1 }, stale: { submission: null, revision: 0 } },
    { name: "a clearing", now: { submission: null, revision: 2 }, stale: { submission: { who: "Ada" }, revision: 1 } },
  ])("does not let a list computed before $name undo it, in either order", ({ now, stale }) => {
    for (const order of ["event-first", "list-first"] as const) {
      const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
      trackPagesListing(daemon.store, "session-a", true);
      const events = [list(stale.submission, stale.revision), heard(now.submission, now.revision)];
      // Event first: the page is not held yet, so the event is remembered until the list arrives.
      client.onEvents!(order === "event-first" ? events.reverse() : events);
      expect(held(daemon), order).toEqual(now);
      // Once held, a stale list again does not undo it.
      client.onEvents!([list(stale.submission, stale.revision)]);
      expect(held(daemon), order).toEqual(now);
      expect(daemon.store.getState().earlySubmissions.size, order).toBe(0);
    }
  });

  it("applies a later list's submission over an earlier event", () => {
    const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
    trackPagesListing(daemon.store, "session-a", true);
    client.onEvents!([heard({ who: "Ada" }, 1), list(null, 2)]);
    expect(held(daemon)).toEqual({ submission: null, revision: 2 });
  });

  it("remembers an event only for a session whose list is pending or whose pages are held", () => {
    const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
    const early = () => daemon.store.getState().earlySubmissions.size;
    // A session this window never lists: nothing accumulates.
    client.onEvents!([heard({ who: "Ada" }, 1)]);
    expect(early()).toBe(0);
    // Its list is out: remembered; and no longer once the request failed.
    trackPagesListing(daemon.store, "session-a", true);
    client.onEvents!([heard({ who: "Ada" }, 1)]);
    expect(early()).toBe(1);
    trackPagesListing(daemon.store, "session-a", false);
    client.onEvents!([{ type: "pages_deleted", session_id: "session-a", pages: ["a-1"] }]);
    expect(early()).toBe(0);
    client.onEvents!([heard({ who: "Ada" }, 2)]);
    expect(early()).toBe(0);
    // Its pages are held, but not this one: remembered for a page about to be created.
    client.onEvents!([list(null, 0)]);
    client.onEvents!([{ type: "page_submission", session_id: "session-a", page: "a-2", submission: { who: "Ada" }, submission_revision: 1 }]);
    expect(early()).toBe(1);
  });

  it("applies an event heard before the page's page_created to the page it creates", () => {
    const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
    trackPagesListing(daemon.store, "session-a", true);
    client.onEvents!([heard({ who: "Ada" }, 1), { type: "page_created", page: page(null, 0) }]);
    expect(held(daemon)).toEqual({ submission: { who: "Ada" }, revision: 1 });
    expect(daemon.store.getState().earlySubmissions.size).toBe(0);
  });

  it("forgets an event remembered for a page not held on a snapshot and when the page is deleted", () => {
    const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
    trackPagesListing(daemon.store, "session-a", true);
    client.onEvents!([heard({ who: "Ada" }, 1)]);
    expect(daemon.store.getState().earlySubmissions.get("session-a")?.has("a-1")).toBe(true);
    client.onEvents!([{ type: "pages_deleted", session_id: "session-a", pages: ["a-1"] }]);
    expect(daemon.store.getState().earlySubmissions.size).toBe(0);

    trackPagesListing(daemon.store, "session-a", true);
    client.onEvents!([heard({ who: "Ada" }, 1)]);
    client.onEvents!([{
      type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
      agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
    } as unknown as Event]);
    expect(daemon.store.getState().earlySubmissions.size).toBe(0);
    client.onEvents!([list(null, 0)]);
    expect(held(daemon)).toEqual({ submission: null, revision: 0 });
  });
});

it("does not bring back a deleted page when a list computed before the deletion is applied after it", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const stale: Event = {
    type: "page_list", id: "r", session_id: "session-a",
    pages: [pageOf("a-1", "session-a", markdownContent("1"), 2), pageOf("a-2", "session-a", markdownContent("2"), 1)],
  };
  const ids = () => daemon.store.getState().pages.get("session-a")?.map((page) => page.id);

  // Twice: first with no list held yet, then with one held.
  client.onEvents!([{ type: "pages_deleted", session_id: "session-a", pages: ["a-1"] }, stale]);
  expect(ids()).toEqual(["a-2"]);
  client.onEvents!([{ type: "pages_deleted", session_id: "session-a", pages: ["a-2"] }, stale]);
  expect(ids()).toEqual([]);

  // A snapshot forgets the deletions along with the pages.
  client.onEvents!([{
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
  } as unknown as Event, stale]);
  expect(ids()).toEqual(["a-1", "a-2"]);
});

// The daemon announces only the console's or the project's deletion, not each session it takes along.
it.each([
  { type: "console_deleted", console: "c" },
  { type: "project_deleted", project: "p" },
] as Event[])("drops what is held for the sessions a $type takes with it, and only theirs", (deletion) => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const request = { compatibility: {}, type: "session_request" as const, kind: "project" as const, console: "c", project: "p", agent: "claude" as const, branches: [], requested_at: 1, action: "start" as const, target: null, owner: null, reopened_owners: [], account: null, conversation: null };
  trackPagesListing(daemon.store, "a", true);
  trackPagesListing(daemon.store, "b", true);
  client.onEvents!(["a", "b"].flatMap((id): Event[] => [
    { type: "session_upserted", session: sessionOf(id, id === "a" ? "c" : "d", id === "a" ? "p" : "q", id, "waiting_user") },
    { type: "page_created", page: pageOf(`${id}-1`, id, markdownContent("1"), 2) },
    { type: "page_created", page: pageOf(`${id}-2`, id, markdownContent("2"), 1) },
    { type: "pages_deleted", session_id: id, pages: [`${id}-2`] },
    { type: "page_submission", session_id: id, page: `${id}-9`, submission: null, submission_revision: 2 },
    { type: "trust_prompt", session: id, agent: "claude", project: "p", path: "/work/app", trust_dir: null },
    { ...request, session: id, request_id: `r-${id}` },
  ]));
  client.onEvents!([deletion]);
  const state = daemon.store.getState();
  expect([...state.sessions.keys()]).toEqual(["b"]);
  expect([...state.pages.keys()]).toEqual(["b"]);
  expect([...state.deletedPages.keys()]).toEqual(["b"]);
  expect([...state.earlySubmissions.keys()]).toEqual(["b"]);
  expect(state.trustPrompts.map((p) => p.session)).toEqual(["b"]);
  expect(state.sessionRequests.map((r) => r.session)).toEqual(["b"]);
});

it("keeps each setting an update leaves as it was, so what reads only that setting does not render again", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const settings = { ...DEFAULT_SETTINGS, accounts: [accountOf("a1", "claude", "Work", "/home/u/.claude-work")], default_account_names: { claude: "Mine" } };
  client.onEvents!([{ type: "settings_updated", settings }]);
  const before = daemon.store.getState();
  client.onEvents!([{ type: "settings_updated", settings: structuredClone({ ...settings, interface_language: "zh-Hans" }) }]);
  const after = daemon.store.getState().settings;
  expect(after.interface_language).toBe("zh-Hans");
  expect(after.accounts).toBe(before.settings.accounts);
  expect(after.default_account_names).toBe(before.settings.default_account_names);
  // An update that changes nothing leaves the state as it was.
  client.onEvents!([{ type: "settings_updated", settings: structuredClone(after) }]);
  expect(daemon.store.getState().settings).toBe(after);
});

it("holds the providers a snapshot serves, empty before one arrives, and keeps the same array when a later snapshot repeats them", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  expect(daemon.store.getState().providers).toEqual([]);
  const snapshot = (providers: ProviderInfo[]) => ({
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [], providers,
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
  } as unknown as Event);
  client.onEvents!([snapshot(TEST_PROVIDERS)]);
  const held = daemon.store.getState().providers;
  expect(held).toEqual(TEST_PROVIDERS);
  client.onEvents!([snapshot(structuredClone(TEST_PROVIDERS))]);
  expect(daemon.store.getState().providers).toBe(held);
});

it("holds whether the daemon keeps the computer awake from the snapshot and follows keep_awake_held_updated", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  expect(daemon.store.getState().keepAwakeHeld).toBe(false);
  const snapshot = {
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: true }, keep_awake_held: true, shell_terminals: [],
  } as unknown as Event;
  client.onEvents!([snapshot]);
  expect(daemon.store.getState().keepAwakeHeld).toBe(true);
  client.onEvents!([{ type: "keep_awake_held_updated", keep_awake_held: false }]);
  expect(daemon.store.getState().keepAwakeHeld).toBe(false);
  client.onEvents!([{ type: "keep_awake_held_updated", keep_awake_held: true }]);
  expect(daemon.store.getState().keepAwakeHeld).toBe(true);
});
