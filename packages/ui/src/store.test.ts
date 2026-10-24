// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { sessionOf } from "./gallery/fixtures/builders";
import type { Event, Project, ShellTerminal } from "./protocol";
import { accountQuotaOf } from "./quota/accountQuotaOf";
import { createDaemon, dismissAccountLogin, trackAccountLogin } from "./store";

const client = vi.hoisted(() => ({
  onEvent: undefined as ((event: Event) => void) | undefined,
  onEvents: undefined as ((events: Event[]) => void) | undefined,
}));
vi.mock("./daemon-client", () => ({
  DaemonClient: class {
    onEvent(listener: (event: Event) => void) { client.onEvent = listener; }
    onEvents(listener: (events: Event[]) => void) { client.onEvents = listener; }
    onConnectionChange() {}
  },
  DaemonRequestError: class extends Error {},
}));

it.each(["reply-first", "broadcast-first"] as const)("keeps the authoritative session title when opening a session: %s", (order) => {
  const daemon = createDaemon("http://localhost:1234");
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
  const daemon = createDaemon("http://localhost:1234");
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

it("applies the events of one batch as a single update", () => {
  const daemon = createDaemon("http://localhost:1234");
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
  const daemon = createDaemon("http://localhost:1234");
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
  const daemon = createDaemon("http://localhost:1234");
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
  const daemon = createDaemon("http://localhost:1234");
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
    agent_availability: [], account_quotas: [], account_logins: [{ ...login, account: "b" }], account_sharing: [], home_dir: null, shell_terminals: [],
  } as unknown as Event;
  client.onEvents!([snapshot]);
  expect(daemon.store.getState().loginsStartedHere).toEqual([]);
});

it("replaces the account quotas with each update and keeps the entry of an account whose reading did not change", () => {
  const daemon = createDaemon("http://localhost:1234");
  const window = { kind: "session" as const, label: null, used_percent: 10, resets_at: null, window_minutes: 300 };
  const first = { agent: "claude" as const, account: null, fetched_at: 1, plan: "max", windows: [window], unavailable: null };
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
  const daemon = createDaemon("http://localhost:1234");
  const running = { account: "a", progress: { share: true, phase: "changing" as const, done: 1, total: 4, item: "skills" } };
  const snapshot = {
    type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [running, { account: "free" }], home_dir: null, shell_terminals: [],
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
  const failure = { share: true, items: [{ item: "skills", left: "backed_up" as const, detail: "denied" }], detail: "summary" };
  client.onEvents!([{ type: "account_sharing", account: "a", failure }]);
  expect(daemon.store.getState().accountSharing.get("a")).toEqual({ account: "a", failure });
  client.onEvents!([{ type: "account_sharing", account: "a" }]);
  expect(daemon.store.getState().accountSharing.has("a")).toBe(false);
  expect(daemon.store.getState().accountSharing.has("b")).toBe(true);
});

it("keeps the shell terminals a snapshot lists and the broadcasts change, and drops those of a removed project", () => {
  const daemon = createDaemon("http://localhost:1234");
  const shell = (id: string, project: string): ShellTerminal => ({ id, project_id: project, worktree: null, cwd: `/${project}`, title: "zsh", renamed: false, state: "running", busy: false, created_at: 1 });
  const project = (id: string) => ({ id, console_id: "c", name: id }) as Project;
  const snapshot = {
    type: "snapshot", hosts: [], consoles: [], projects: [project("p"), project("q")], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
    agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], home_dir: null, shell_terminals: [shell("a", "p"), shell("b", "q")],
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

it("keeps what a session request says it does, and drops it when it closes", () => {
  const daemon = createDaemon("http://localhost:1234");
  const base = { type: "session_request" as const, session: "s", role: "project" as const, console: "c", project: "p", agent: "claude" as const, branches: [], requested_at: 1 };
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
  const daemon = createDaemon("http://localhost:1234");
  const base = { type: "session_request" as const, session: "s", role: "project" as const, console: "c", project: "p", agent: "codex" as const, branches: [], requested_at: 1, owner: null, reopened_owners: [], conversation: null, target: "t", account: "a" };
  client.onEvents!([
    { ...base, request_id: "r-1", action: "switch" },
    { ...base, request_id: "r-2", action: "handoff", account: null, model: "m", effort: null, requested_at: 2 },
  ]);
  const [switching, handing] = daemon.store.getState().sessionRequests;
  expect(switching).toMatchObject({ action: "switch", target: "t", account: "a" });
  expect(handing).toMatchObject({ action: "handoff", target: "t", account: null, model: "m", effort: null });
});

it("does not offer a session request that lacks what its action needs or has an action it does not know", () => {
  const daemon = createDaemon("http://localhost:1234");
  const base = { type: "session_request" as const, session: "s", role: "project" as const, console: "c", project: "p", agent: "claude" as const, branches: [], requested_at: 1, owner: null, reopened_owners: [], target: null, account: null, conversation: null };
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
