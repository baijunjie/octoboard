// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { looseRequestSender } from "../requests";
import { CodedError } from "../errors/codedError";
import type { ProcessTree } from "../protocol";
import { isUnkeepable, keepProcesses, listProcesses, stopProcesses, unkeepableMessage, unkeepableRefusal, type UnkeepableTree } from "./processes";
import type { Event, ClientRequestBody } from "../protocol";

function daemon(reply: Event) {
  const request = vi.fn(async (_body: ClientRequestBody) => reply);
  return Object.assign(looseRequestSender(request), { calls: () => request.mock.calls.map(([body]) => body) });
}

it("lists a session's or a project's trees and names the stopped and kept ones", async () => {
  const tree = { id: "1:2", command: "vite", cwd: null, cwd_missing: false, kept: false, keepable: true, started_at: 0, process_count: 1, memory_bytes: 0, ports: [5173] };
  const listing = daemon({ type: "processes_listed", id: null, trees: [tree] });
  expect(await listProcesses(listing, { session_id: "s1" })).toEqual([tree]);
  await listProcesses(listing, { project_id: "p1" });
  expect(listing.calls()).toEqual([
    { type: "list_processes", session_id: "s1" },
    { type: "list_processes", project_id: "p1" },
  ]);
  const stopping = daemon({ type: "processes_stopped", id: null, ids: ["1:2"] });
  expect(await stopProcesses(stopping, ["1:2", "3:4"], ((key: string) => key) as never)).toEqual(["1:2"]);
  expect(stopping.calls()).toEqual([{ type: "stop_processes", ids: ["1:2", "3:4"] }]);
  expect(await keepProcesses(daemon({ type: "processes_kept", id: null, ids: ["1:2"] }), ["1:2"])).toEqual(["1:2"]);
});

it("words why a tree cannot be kept as the refusal of keeping it", () => {
  const tree: UnkeepableTree = { id: "1:2", command: "vite", cwd: null, cwd_missing: false, kept: false, keepable: false, started_at: 0, process_count: 1, memory_bytes: 0, ports: [] };
  expect(unkeepableMessage({ ...tree, unkeepable_reason: "agent_console" })).toBe("daemon.process_shares_agent_console");
  // A newer daemon's reason, or none, is said plainly.
  expect(unkeepableMessage(tree)).toBe("processes.unkeepable");
  expect(unkeepableMessage({ ...tree, unkeepable_reason: "later" as never })).toBe("processes.unkeepable");
});

it("takes only a keepable the daemon sends as false as a tree that cannot be kept", () => {
  const tree: ProcessTree = { id: "1:2", command: "vite", cwd: null, cwd_missing: false, kept: false, keepable: true, started_at: 0, process_count: 1, memory_bytes: 0, ports: [] };
  expect(isUnkeepable(tree)).toBe(false);
  expect(isUnkeepable({ ...tree, keepable: false })).toBe(true);
  // An older daemon sends no `keepable`, and its trees could always be kept.
  const { keepable: _, ...older } = tree;
  expect(isUnkeepable(older as ProcessTree)).toBe(false);
});

it("refuses a keep of a tree that cannot be kept as the daemon does, with its code where the reason is known", () => {
  const t = ((key: string) => `[${key}]`) as never;
  const tree: UnkeepableTree = { id: "1:2", command: "vite", cwd: null, cwd_missing: false, kept: false, keepable: false, started_at: 0, process_count: 1, memory_bytes: 0, ports: [] };
  const known = unkeepableRefusal({ ...tree, unkeepable_reason: "agent_console" }, t);
  expect(known).toBeInstanceOf(CodedError);
  expect(known).toMatchObject({ code: "process_shares_agent_console", message: "[daemon.process_shares_agent_console]" });
  expect(unkeepableRefusal(tree, t)).toMatchObject({ message: "[processes.unkeepableUnknownReason]", recovery: "none" });
});

it("refuses a stop that left a process running, worded by why it was left", async () => {
  const t = ((key: string) => `[${key}]`) as never;
  const left = (reason: string) => daemon({ type: "processes_stopped", id: null, ids: ["3:4"], not_stopped: [{ tree_id: "1:2", pid: 7, name: "node", reason: reason as never }] });
  await expect(stopProcesses(left("access_denied"), ["1:2", "3:4"], t)).rejects.toMatchObject({ code: "process_not_stopped", params: { reason: "access_denied" }, message: "[daemon.process_not_stopped.accessDenied]" });
  await expect(stopProcesses(left("other"), ["1:2"], t)).rejects.toMatchObject({ code: "process_not_stopped", message: "[daemon.process_not_stopped]" });
  // A newer daemon's reason is said as anything else is.
  await expect(stopProcesses(left("later"), ["1:2"], t)).rejects.toMatchObject({ message: "[daemon.process_not_stopped]" });
});
