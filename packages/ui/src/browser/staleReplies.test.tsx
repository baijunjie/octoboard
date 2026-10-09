// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it } from "vitest";

import type { BrowseEntry, Event, FileContent, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import type { DirListing } from "./tree";
import { useDirectoryListings, type DirectoryListings } from "./useDirectoryListings";
import { useFileReader, type FileReader } from "./useFileReader";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A daemon whose replies the test hands out itself, in whatever order it likes. */
type Pending = { body: RequestBody; resolve: (event: Event) => void; reject: (err: Error) => void };

/** Every request any test left unanswered and every root it mounted, settled and unmounted after
 * each test: the listings' window-wide limit is module state, and a listing still out would count
 * against the next test. */
const leftOver: Pending[] = [];
const roots: Root[] = [];

function controlledDaemon() {
  const pending: Pending[] = [];
  const daemon: Daemon = {
    store: createStateStore({ connectionState: "open", snapshotEpoch: 1 }),
    request: (body) =>
      new Promise((resolve, reject) => {
        const request = { body, resolve, reject };
        pending.push(request);
        leftOver.push(request);
      }),
    toastError: () => {},
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  return { daemon, pending };
}

function mount<T>(daemon: Daemon, use: () => T): { current: () => T } {
  let latest: T;
  const Probe = () => {
    latest = use();
    return null;
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <Probe />
      </DaemonProvider>,
    ),
  );
  return { current: () => latest };
}

afterEach(async () => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  await act(async () => leftOver.splice(0).forEach((request) => request.reject(new Error("test over"))));
  document.body.replaceChildren();
});

const listing = (path: string, rootId: string, names: string[]): Event => ({
  type: "project_dir",
  project: "p",
  worktree: null,
  path,
  root_id: rootId,
  entries: names.map((name): BrowseEntry => ({ name, kind: "file", size: 1, version: "v1", target: null })),
  complete: true,
});
const names = (held: DirListing | undefined) => (held?.state === "loaded" ? held.entries.map((e) => e.name) : held?.state);

it("takes a directory's latest listing only, and drops what was held when the project's directory is replaced", async () => {
  const { daemon, pending } = controlledDaemon();
  const hook = mount<DirectoryListings>(daemon, () => useDirectoryListings("p", false));
  act(() => hook.current().want(["", "src"]));
  const [rootFirst, src] = pending.splice(0);
  act(() => hook.current().refresh([""]));
  const [rootSecond] = pending.splice(0);
  await act(async () => rootSecond.resolve(listing("", "dir-1", ["new.ts"])));
  await act(async () => rootFirst.resolve(listing("", "dir-1", ["old.ts"])));
  expect(names(hook.current().listings.get(""))).toEqual(["new.ts"]);

  await act(async () => src.resolve(listing("src", "dir-1", ["a.ts"])));
  act(() => hook.current().refresh([""]));
  await act(async () => pending.splice(0)[0].resolve(listing("", "dir-2", ["other.ts"])));
  // The listing of `src` came from the directory that was replaced, so it is asked for again.
  expect(names(hook.current().listings.get("src"))).toBe("loading");
  expect(pending.map((p) => "path" in p.body && p.body.path)).toEqual(["src"]);
});

const file = (path: string, text: string, version = "1"): Event => {
  const content: FileContent = { size: text.length, kind: "text", media_type: null, text, data: null };
  return { type: "project_file", project: "p", worktree: null, path, source: { kind: "live", root_id: "r", version }, file: content };
};

it("shows the file moved to loading until its own reply, whatever the earlier reads answer", async () => {
  const { daemon, pending } = controlledDaemon();
  const hook = mount<FileReader>(daemon, () => useFileReader("p"));
  act(() => hook.current().open("a.ts"));
  act(() => hook.current().open("b.ts"));
  const [readA, readB] = pending.splice(0);
  await act(async () => readA.resolve(file("a.ts", "a")));
  expect(hook.current().subject).toMatchObject({ path: "b.ts", content: { state: "loading" } });
  await act(async () => readB.resolve(file("b.ts", "b")));
  expect(hook.current().subject).toMatchObject({ path: "b.ts", content: { state: "file", body: { text: "b" } } });
});

it("reads the file again after a reconnect, keeping what is shown through a lost read and an unchanged version", async () => {
  const { daemon, pending } = controlledDaemon();
  const hook = mount<FileReader>(daemon, () => useFileReader("p"));
  act(() => hook.current().open("a.ts"));
  await act(async () => pending.splice(0)[0].resolve(file("a.ts", "a")));
  const shown = hook.current().subject?.content;

  // The connection drops with a read out: the body stays.
  act(() => hook.current().reload());
  await act(async () => pending.splice(0)[0].reject(new Error("connection closed")));
  expect(hook.current().subject?.content).toBe(shown);

  // The reconnect's snapshot reads it again; the same version keeps the very same content.
  act(() => daemon.store.setState((state) => ({ snapshotEpoch: state.snapshotEpoch + 1 })));
  expect(pending.map((p) => p.body.type)).toEqual(["read_project_file"]);
  await act(async () => pending.splice(0)[0].resolve(file("a.ts", "a")));
  expect(hook.current().subject?.content).toBe(shown);
  expect(hook.current().shown).toEqual({ state: "file", version: "1" });
});
