// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import type { Event } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { useFileSearch, type FileSearch } from "./useFileSearch";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("searches only while the caller enables it, and does not search again on a timer", async () => {
  vi.useFakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let resolve!: (event: Event) => void;
  let result!: FileSearch;
  const request = vi.fn(() => new Promise<Event>((done) => { resolve = done; }));
  const daemon = { store: createStateStore({ connectionState: "open", snapshotEpoch: 1 }), request } as unknown as Daemon;
  function Search({ active }: { active: boolean }) {
    result = useFileSearch("all-files", "ts", active).result;
    return null;
  }
  try {
    await act(async () => root.render(<DaemonProvider value={daemon}><Search active={false} /></DaemonProvider>));
    await act(async () => { vi.advanceTimersByTime(250); });
    expect(request).not.toHaveBeenCalled();
    await act(async () => root.render(<DaemonProvider value={daemon}><Search active /></DaemonProvider>));
    await act(async () => { vi.advanceTimersByTime(250); });
    expect(request).toHaveBeenCalledWith({ type: "search_project_files", project: "all-files", query: "ts", slot: "file-search:all-files" });
    await act(async () => resolve({ type: "project_file_matches", project: "all-files", query: "ts", paths: ["nested/file.ts"], complete: true }));
    expect(result).toEqual({ state: "loaded", paths: ["nested/file.ts"], complete: true });
    await act(async () => { vi.advanceTimersByTime(60_000); });
    expect(request).toHaveBeenCalledTimes(1);
    await act(async () => root.render(<DaemonProvider value={daemon}><Search active={false} /></DaemonProvider>));
    await act(async () => { vi.advanceTimersByTime(60_000); });
    expect(request).toHaveBeenCalledTimes(1);
  } finally {
    act(() => root.unmount());
    container.remove();
    vi.useRealTimers();
  }
});
