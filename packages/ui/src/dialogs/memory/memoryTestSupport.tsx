// Shared by the memory dialog's test files.
import { act } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished, vi } from "vitest";

import { consoleOf } from "../../fixtures/builders";
import type { ClientRequestBody, Event, MemoryEntry } from "../../protocol";
import { createMemoryListeners, type MemoryChangeEvent } from "../../store";
import { TestHost } from "../../test/TestHost";
import { testDaemon } from "../../test/testDaemon";
import type { State } from "../../store";
import { indexLine } from "./memoryState";
export { deletedEvent, entryOf, PROJECT, PROJECT_SCOPE, savedEvent } from "../../fixtures/memoryBuilders";
import { PROJECT_SCOPE } from "../../fixtures/memoryBuilders";
import { MemoryDialog } from "./MemoryDialog";
import type { MemoryTarget } from "./memoryTarget";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** What a test sets up of the daemon the dialog runs over: the entries it holds, `respond` for a request it answers
 * itself (return nothing to let the stand-in answer), and the state of the store. */
interface Setup {
  entries?: MemoryEntry[];
  respond?: (body: ClientRequestBody) => Promise<Event> | undefined;
  state?: Partial<State>;
  /** The memory the dialog shows: console `c-1`'s by default. */
  target?: MemoryTarget;
}

/** Renders the dialog for `target` (console `c-1` by default) over a daemon that holds `entries` and answers the memory requests from them,
 * and returns the requests sent, the daemon's store, and a way to broadcast a change the way the daemon does. */
export function renderMemory({ entries = [], respond, state, target = { kind: "console", console: consoleOf("c-1", "Main") } }: Setup = {}) {
  const listeners = createMemoryListeners();
  const request = vi.fn((body: ClientRequestBody): Promise<Event> => {
    const own = respond?.(body);
    if (own) return own;
    if (body.type === "list_memories") return Promise.resolve({ type: "memory_list", id: null, scope: target.kind === "console" ? { kind: "console", console_id: target.console.id } : PROJECT_SCOPE, memories: entries.map(indexLine) });
    if (body.type === "read_memory") {
      const entry = entries.find((candidate) => candidate.id === body.memory)!;
      return Promise.resolve({ type: "memory_content", id: null, entry });
    }
    return Promise.resolve({ type: "ack", id: null });
  });
  const daemon = testDaemon({ state: { connectionState: "open", snapshotEpoch: 1, ...state }, request, onMemoryChange: listeners.subscribe });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<TestHost daemon={daemon}><MemoryDialog target={target} onClose={() => {}} /></TestHost>));
  return {
    request,
    store: daemon.store,
    /** Broadcasts `event` to the dialog. */
    broadcast: (event: MemoryChangeEvent) => act(() => listeners.deliver(event)),
    /** The requests the dialog sent that are not the listing and the reads it asks for itself. */
    sent: () => request.mock.calls.map((call) => call[0]).filter((body) => body.type !== "list_memories" && body.type !== "read_memory"),
    listings: () => request.mock.calls.filter((call) => call[0].type === "list_memories").length,
  };
}

/** Waits for the dialog's first listing and the render it causes. */
export const settled = () => act(async () => {});
