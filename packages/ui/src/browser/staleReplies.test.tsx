// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import type { BrowseEntry, ChangeGroup, Event, FileContent, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { changeItem, type ChangeItem } from "./changes";
import type { DirListing } from "./tree";
import { useChangeList, type ChangeList } from "./useChangeList";
import { useChangeReader, type ChangeReader } from "./useChangeReader";
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

/** Mounts `use` over arguments the test changes later, as a parent re-rendering with new props does. */
function mountWith<A, T>(daemon: Daemon, initial: A, use: (args: A) => T): { current: () => T; set: (args: A) => void } {
  let latest: T;
  let args = initial;
  const Probe = () => {
    latest = use(args);
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  roots.push(root);
  const render = () =>
    act(() =>
      root.render(
        <DaemonProvider value={daemon}>
          <Probe />
        </DaemonProvider>,
      ),
    );
  render();
  return {
    current: () => latest,
    set: (next) => {
      args = next;
      render();
    },
  };
}

const changes = (worktree: string | null, path: string): Event => ({
  type: "project_changes",
  project: "p",
  worktree,
  head: "c1",
  changes: [{ group: "untracked", old: { state: "absent" }, new: { state: "present", path, kind: "file", source: { kind: "live", root_id: "r", version: "1" } } }],
  complete: true,
});
const listed = (list: ChangeList) => (list.state === "loaded" ? list.items.map((item) => item.path) : list.state);

it("shows the changes of the worktree chosen last, whichever worktree's list answers last", async () => {
  const { daemon, pending } = controlledDaemon();
  const hook = mountWith(daemon, undefined as string | undefined, (worktree) => useChangeList("p", worktree, true));
  hook.set("side");
  const [own, side] = pending.splice(0);
  expect(side.body).toMatchObject({ type: "list_project_changes", worktree: "side" });
  await act(async () => side.resolve(changes("side", "side.txt")));
  await act(async () => own.resolve(changes(null, "own.txt")));
  expect(listed(hook.current().list)).toEqual(["side.txt"]);
});

it("keeps at most two Git list requests out in the window, sending the next as one finishes", async () => {
  const { daemon, pending } = controlledDaemon();
  for (const project of ["a", "b", "c"]) mount(daemon, () => useChangeList(project, undefined, true));
  expect(pending.map((p) => "project" in p.body && p.body.project)).toEqual(["a", "b"]);
  await act(async () => pending.splice(0, 1)[0].resolve(changes(null, "a.txt")));
  expect(pending.map((p) => "project" in p.body && p.body.project)).toEqual(["b", "c"]);
});

const item = (path: string, group: ChangeGroup = "unstaged"): ChangeItem =>
  changeItem({ group, old: { state: "present", path, kind: "file", source: { kind: "index", worktree: "w", blob: "b" } }, new: { state: "absent" } });
const change = (path: string, patch: string): Event => ({
  type: "project_change",
  project: "p",
  worktree: null,
  group: "unstaged",
  head: null,
  old: { state: "present", path, kind: "file", source: { kind: "index", worktree: "w", blob: "b" }, file: null },
  new: { state: "absent" },
  patch: { size: patch.length, kind: "text", media_type: null, text: patch, data: null },
});

it("shows the change moved to loading until its own reply, and never a change of another group under its key", async () => {
  const { daemon, pending } = controlledDaemon();
  const hook = mount<ChangeReader>(daemon, () => useChangeReader("p"));
  act(() => hook.current().open(item("a.ts"), undefined));
  act(() => hook.current().open(item("b.ts"), undefined));
  const [readA, readB] = pending.splice(0);
  expect(readB.body).toMatchObject({ type: "read_project_change", slot: "viewer", change: { group: "unstaged", old: { path: "b.ts" } } });
  await act(async () => readA.resolve(change("a.ts", "-a\n")));
  expect(hook.current().subject).toMatchObject({ path: "b.ts", content: { state: "loading" } });
  await act(async () => readB.resolve(change("b.ts", "-b\n")));
  expect(hook.current().subject).toMatchObject({ path: "b.ts", content: { state: "change", change: { patch: "-b\n" } } });
  const unstagedKey = hook.current().subject?.key;
  act(() => hook.current().open(item("b.ts", "staged"), undefined));
  expect(hook.current().subject?.key).not.toBe(unstagedKey);
  expect(hook.current().subject?.content).toEqual({ state: "loading" });
});

it("keeps a failed change list on screen through a background refresh, and shows loading when asked again", async () => {
  const { daemon, pending } = controlledDaemon();
  const hook = mount(daemon, () => useChangeList("p", undefined, true));
  const failed = { code: "git_failed", params: { detail: "boom" }, message: "boom" };
  await act(async () => pending.splice(0)[0].reject(new DaemonRequestError(failed.message, failed.code, failed.params)));
  expect(hook.current().list).toMatchObject({ state: "error", refreshing: false });
  // A reconnect asks again in the background: the failure, and its Try again, stay.
  act(() => daemon.store.setState((state) => ({ snapshotEpoch: state.snapshotEpoch + 1 })));
  expect(hook.current().list).toMatchObject({ state: "error", refreshing: true });
  await act(async () => pending.splice(0)[0].reject(new DaemonRequestError(failed.message, failed.code, failed.params)));
  act(() => hook.current().refresh());
  expect(hook.current().list).toEqual({ state: "loading" });
});

it("sends a request replacing one in its own slot at once, past the window's bound", async () => {
  const { daemon, pending } = controlledDaemon();
  const a = mount(daemon, () => useChangeList("a", undefined, true));
  mount(daemon, () => useChangeList("b", undefined, true));
  mount(daemon, () => useChangeList("c", undefined, true));
  expect(pending.map((p) => "project" in p.body && p.body.project)).toEqual(["a", "b"]);
  act(() => a.current().refresh());
  expect(pending.map((p) => "project" in p.body && p.body.project)).toEqual(["a", "b", "a"]);
});
