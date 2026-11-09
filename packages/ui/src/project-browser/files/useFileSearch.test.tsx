// @vitest-environment jsdom
import { act, StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import type { Event, ClientRequestBody } from "../../protocol";
import { DaemonProvider } from "../../store";
import { testDaemon } from "../../test/testDaemon";
import { AUTO_REFRESH_MS } from "../autoRefresh";
import { SEARCH_DEBOUNCE_MS, useFileSearch, type FileSearch } from "./useFileSearch";
import { fakeTimers } from "../../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("searches only while the caller enables it, and does not search again on a timer", async () => {
  fakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let resolve!: (event: Event) => void;
  let result!: FileSearch;
  const request = vi.fn(() => new Promise<Event>((done) => { resolve = done; }));
  const daemon = testDaemon({ state: { connectionState: "open", snapshotEpoch: 1 }, request });
  function Search({ active, includeIgnored = false }: { active: boolean; includeIgnored?: boolean }) {
    result = useFileSearch("all-files", "ts", active, includeIgnored).result;
    return null;
  }
  try {
    await act(async () => root.render(<DaemonProvider value={daemon}><Search active={false} /></DaemonProvider>));
    await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
    expect(request).not.toHaveBeenCalled();
    await act(async () => root.render(<DaemonProvider value={daemon}><Search active /></DaemonProvider>));
    await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
    expect(request).toHaveBeenCalledWith({ type: "search_project_files", project: "all-files", query: "ts", slot: "file-search:all-files" });
    await act(async () => resolve({ type: "project_file_matches", id: null, project: "all-files", query: "ts", paths: ["nested/file.ts"], complete: true }));
    expect(result).toEqual({ state: "loaded", paths: ["nested/file.ts"], ignored: new Set(), complete: true });
    await act(async () => { vi.advanceTimersByTime(AUTO_REFRESH_MS); });
    expect(request).toHaveBeenCalledTimes(1);
    await act(async () => root.render(<DaemonProvider value={daemon}><Search active={false} /></DaemonProvider>));
    await act(async () => { vi.advanceTimersByTime(AUTO_REFRESH_MS); });
    expect(request).toHaveBeenCalledTimes(1);
  } finally {
    act(() => root.unmount());
    container.remove();
  }
});

it("repeats the search at once when ignored files are included, and tells which matches are ignored", async () => {
  fakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let result!: FileSearch;
  const request = vi.fn(async (body: ClientRequestBody & { include_ignored?: boolean }): Promise<Event> => ({
    type: "project_file_matches",
    id: null,
    project: "all-files",
    query: "ts",
    paths: body.include_ignored ? ["dist/a.ts", "src/b.ts"] : ["src/b.ts"],
    ...(body.include_ignored && { ignored: ["dist/a.ts"] }),
    complete: true,
  }));
  const daemon = testDaemon({ state: { connectionState: "open", snapshotEpoch: 1 }, request });
  function Search({ includeIgnored }: { includeIgnored: boolean }) {
    result = useFileSearch("all-files", "ts", true, includeIgnored).result;
    return null;
  }
  try {
    await act(async () => root.render(<DaemonProvider value={daemon}><Search includeIgnored={false} /></DaemonProvider>));
    await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
    expect(request).toHaveBeenCalledTimes(1);
    expect(request.mock.calls[0][0]).not.toHaveProperty("include_ignored");
    expect(result).toMatchObject({ state: "loaded", paths: ["src/b.ts"] });
    await act(async () => root.render(<DaemonProvider value={daemon}><Search includeIgnored /></DaemonProvider>));
    // Not a keystroke: no debounce is waited out.
    await act(async () => { vi.advanceTimersByTime(0); });
    expect(request).toHaveBeenCalledTimes(2);
    expect(request.mock.calls[1][0]).toMatchObject({ include_ignored: true });
    expect(result).toEqual({ state: "loaded", paths: ["dist/a.ts", "src/b.ts"], ignored: new Set(["dist/a.ts"]), complete: true });
  } finally {
    act(() => root.unmount());
    container.remove();
  }
});

it("repeats the search at once on a toggle under StrictMode, whose effects run twice", async () => {
  fakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const request = vi.fn(async (): Promise<Event> => ({ type: "project_file_matches", id: null, project: "all-files", query: "ts", paths: ["src/b.ts"], complete: true }));
  const daemon = testDaemon({ state: { connectionState: "open", snapshotEpoch: 1 }, request });
  function Search({ includeIgnored }: { includeIgnored: boolean }) {
    useFileSearch("all-files", "ts", true, includeIgnored);
    return null;
  }
  const render = (includeIgnored: boolean) => act(async () => root.render(<StrictMode><DaemonProvider value={daemon}><Search includeIgnored={includeIgnored} /></DaemonProvider></StrictMode>));
  try {
    await render(false);
    await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
    const sent = request.mock.calls.length;
    await render(true);
    await act(async () => { vi.advanceTimersByTime(0); });
    expect(request).toHaveBeenCalledTimes(sent + 1);
    expect(request.mock.calls[sent]).toMatchObject([{ include_ignored: true }]);
  } finally {
    act(() => root.unmount());
    container.remove();
  }
});
