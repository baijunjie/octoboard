// @vitest-environment jsdom
import { beforeEach, expect, it, vi } from "vitest";

import { CodedError } from "../errors/codedError";
import type { Event } from "../protocol";
import { createDaemon } from ".";
import type { WindowNotification } from "./notifications";

const client = vi.hoisted(() => ({
  onEvent: undefined as ((event: Event) => void) | undefined,
  onEvents: undefined as ((events: Event[]) => void) | undefined,
  resolve: undefined as (() => void) | undefined,
  reject: undefined as ((error: unknown) => void) | undefined,
}));
vi.mock("../daemon-client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../daemon-client")>()),
  DaemonClient: class {
    onEvent(listener: (event: Event) => void) { client.onEvent = listener; }
    onEvents(listener: (events: Event[]) => void) { client.onEvents = listener; }
    onConnectionChange() {}
    request() { return new Promise<Event>((resolve, reject) => { client.resolve = () => resolve({ type: "ok", id: null } as unknown as Event); client.reject = reject; }); }
  },
}));

const notice = (session: string, code = "trust_answer_failed"): Event => ({ type: "session_notice", session, code, params: {}, message: "raw" });
const snapshot = {
  type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [], trusted_directories: [], settings: {}, git_statuses: [],
  agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [], home_dir: null, host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
} as unknown as Event;
const prompt = (session: string): Event => ({ type: "trust_prompt", session, agent: "claude", project: "p", path: "/work/app", trust_dir: null });

let daemon: ReturnType<typeof createDaemon>;
let notifications: WindowNotification[];
const receive = (event: Event) => {
  client.onEvents!([event]);
  client.onEvent!(event);
};

beforeEach(() => {
  daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  notifications = [];
  daemon.onNotification((notification) => notifications.push(notification));
});

it("swallows the notice of this window's own failed go-ahead but still removes the session's trust prompt", async () => {
  receive(prompt("s"));
  receive(prompt("t"));
  const outcome = daemon.request({ type: "confirm_trust", session: "s", remember: false }).catch((err: unknown) => err);
  receive(notice("s"));
  client.reject!(new CodedError("raw", "trust_answer_failed", {}));
  expect(await outcome).toBeInstanceOf(CodedError);
  expect(notifications).toEqual([]);
  expect(daemon.store.getState().trustPrompts.map((p) => p.session)).toEqual(["t"]);
});

it("shows the notice held while a go-ahead was in flight once the go-ahead succeeds", async () => {
  const outcome = daemon.request({ type: "confirm_trust", session: "s", remember: false });
  receive(notice("s", "trust_not_carried_over"));
  expect(notifications).toEqual([]);
  client.resolve!();
  await outcome;
  expect(notifications).toHaveLength(1);
});

it("leaves no token when a snapshot arrives between the go-ahead and its rejection", async () => {
  const outcome = daemon.request({ type: "confirm_trust", session: "s", remember: false }).catch((err: unknown) => err);
  receive(snapshot);
  client.reject!(new CodedError("raw", "trust_answer_failed", {}));
  await outcome;
  receive(notice("s"));
  expect(notifications).toHaveLength(1);
});
