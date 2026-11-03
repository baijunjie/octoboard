// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { looseRequestSender } from "../requests";
import { keepProcesses, listProcesses, stopProcesses } from "./processes";
import type { Event, ClientRequestBody } from "../protocol";

function daemon(reply: Event) {
  const request = vi.fn(async (_body: ClientRequestBody) => reply);
  return Object.assign(looseRequestSender(request), { calls: () => request.mock.calls.map(([body]) => body) });
}

it("lists a session's or a project's trees and names the stopped and kept ones", async () => {
  const tree = { id: "1:2", command: "vite", cwd: null, cwd_missing: false, kept: false, started_at: 0, process_count: 1, memory_bytes: 0, ports: [5173] };
  const listing = daemon({ type: "processes_listed", id: null, trees: [tree] });
  expect(await listProcesses(listing, { session_id: "s1" })).toEqual([tree]);
  await listProcesses(listing, { project_id: "p1" });
  expect(listing.calls()).toEqual([
    { type: "list_processes", session_id: "s1" },
    { type: "list_processes", project_id: "p1" },
  ]);
  const stopping = daemon({ type: "processes_stopped", id: null, ids: ["1:2"] });
  expect(await stopProcesses(stopping, ["1:2", "3:4"])).toEqual(["1:2"]);
  expect(stopping.calls()).toEqual([{ type: "stop_processes", ids: ["1:2", "3:4"] }]);
  expect(await keepProcesses(daemon({ type: "processes_kept", id: null, ids: ["1:2"] }), ["1:2"])).toEqual(["1:2"]);
});
