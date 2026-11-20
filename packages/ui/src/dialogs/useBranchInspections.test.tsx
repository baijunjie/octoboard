// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { sessionOf } from "../fixtures/builders";
import type { ProcessTree, Session } from "../protocol";
import type { RequestHandler } from "../requests";
import { DaemonProvider, type Daemon } from "../store";
import { testDaemon, withEmptyReplies } from "../test/testDaemon";
import type { SessionInspection } from "./branchInspection";
import { useArchiveScope } from "./useBranchInspections";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const owner = sessionOf("owner", "c", "p", "Owner", "idle");
const TREE: ProcessTree = {
  id: "4242:1", session_id: owner.id, command: "npm run dev", cwd: "/r", cwd_missing: false, kept: false, keepable: true,
  started_at: 1_000, process_count: 1, memory_bytes: 1_000_000, ports: [],
};

type Scope = ReturnType<typeof useArchiveScope>;

/** Mounts a host that reads the scope of an archive of `owner` over a store holding `sessions`; `scope()` is what it
 * read last. */
function mountScope(sessions: Session[], request: RequestHandler, initial?: SessionInspection): { daemon: Daemon; scope: () => Scope } {
  const daemon = testDaemon({
    state: { sessions: new Map(sessions.map((s) => [s.id, s])) },
    request: vi.fn(withEmptyReplies(request)) as never,
    showNotice: vi.fn(),
  });
  let read!: Scope;
  function Subject() {
    read = useArchiveScope(owner, initial);
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<DaemonProvider value={daemon}><Subject /></DaemonProvider>));
  return { daemon, scope: () => read };
}

const inspections = (daemon: Daemon) => (daemon.request as ReturnType<typeof vi.fn>).mock.calls.map(([body]) => body).filter((body) => body.type === "inspect_session_branches");
const answering = (processes: ProcessTree[]): RequestHandler => async (body) =>
  body.type === "inspect_session_branches" ? { type: "session_branches_inspected", id: null, items: [], processes } : { type: "ack", id: null };

it("reads the session, the unarchived sessions bound to it and what archiving them all finds, pending until it is answered", async () => {
  const worker = sessionOf("worker", "c", "p", "Worker", "working", { bound_to: owner.id });
  const nested = sessionOf("nested", "c", "p", "Nested", "waiting_user", { bound_to: worker.id });
  const archived = sessionOf("old", "c", "p", "Old", "archived", { bound_to: owner.id });
  const other = sessionOf("other", "c", "p", "Other", "working");
  const { daemon, scope } = mountScope([owner, worker, nested, archived, other], answering([TREE]));

  expect(scope().session).toEqual(owner);
  expect(scope().withIt.map((s) => s.id).sort()).toEqual(["nested", "worker"]);
  expect(scope().inspected.pending).toBe(true);
  expect([...inspections(daemon)[0].sessions].sort()).toEqual(["nested", "owner", "worker"]);
  expect(inspections(daemon)[0].processes).toBe(true);

  await act(async () => {});

  expect(scope().inspected.pending).toBe(false);
  expect(scope().inspected.processes).toEqual([TREE]);
});

it("follows the store: a session bound later joins the scope and is inspected, the title is the current one, and a dropped session stands as requested", async () => {
  const { daemon, scope } = mountScope([owner], answering([]));
  await act(async () => {});
  expect(scope().withIt).toEqual([]);

  const late = sessionOf("late", "c", "p", "Late", "idle", { bound_to: owner.id });
  act(() => daemon.store.setState({ sessions: new Map([[owner.id, { ...owner, title: "Renamed" }], [late.id, late]]) }));
  await act(async () => {});

  expect(scope().session.title).toBe("Renamed");
  expect(scope().withIt.map((s) => s.id)).toEqual(["late"]);
  expect([...inspections(daemon).at(-1).sessions].sort()).toEqual(["late", "owner"]);

  act(() => daemon.store.setState({ sessions: new Map() }));
  expect(scope().session).toEqual(owner);
  expect(scope().withIt).toEqual([]);
});

it("takes an inspection already made for exactly this scope without asking again, and asks for any other", async () => {
  const worker = sessionOf("worker", "c", "p", "Worker", "working", { bound_to: owner.id });
  const request = vi.fn(answering([]));
  const reused = mountScope([owner, worker], request, { sessions: [worker.id, owner.id], items: [], processes: [TREE] });
  expect(reused.scope().inspected.pending).toBe(false);
  expect(reused.scope().inspected.processes).toEqual([TREE]);
  expect(inspections(reused.daemon)).toHaveLength(0);

  const stale = mountScope([owner, worker], answering([]), { sessions: [owner.id], items: [], processes: [TREE] });
  expect(stale.scope().inspected.pending).toBe(true);
  expect(inspections(stale.daemon)).toHaveLength(1);

  const failed = mountScope([owner, worker], answering([]), { sessions: [owner.id, worker.id], items: [], processes: [TREE], failed: true });
  expect(failed.scope().inspected.pending).toBe(true);
  expect(inspections(failed.daemon)).toHaveLength(1);
});
