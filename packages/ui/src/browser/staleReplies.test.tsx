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
import { useBranchComparison } from "./useBranchComparison";
import { useBranchList } from "./useBranchList";
import { shortCommit, useChangeReader, type ChangeOrigin, type ChangeReader } from "./useChangeReader";
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

const OWN: ChangeOrigin = { kind: "worktree", worktree: undefined };
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
  act(() => hook.current().open(item("a.ts"), OWN));
  act(() => hook.current().open(item("b.ts"), OWN));
  const [readA, readB] = pending.splice(0);
  expect(readB.body).toMatchObject({ type: "read_project_change", slot: "viewer", change: { group: "unstaged", old: { path: "b.ts" } } });
  await act(async () => readA.resolve(change("a.ts", "-a\n")));
  expect(hook.current().subject).toMatchObject({ path: "b.ts", content: { state: "loading" } });
  await act(async () => readB.resolve(change("b.ts", "-b\n")));
  expect(hook.current().subject).toMatchObject({ path: "b.ts", content: { state: "change", change: { patch: "-b\n" } } });
  const unstagedKey = hook.current().subject?.key;
  act(() => hook.current().open(item("b.ts", "staged"), OWN));
  expect(hook.current().subject?.key).not.toBe(unstagedKey);
  expect(hook.current().subject?.content).toEqual({ state: "loading" });
});

it("words a comparison's source as plain text too, each branch isolated, for a tooltip", () => {
  const { daemon } = controlledDaemon();
  const hook = mount<ChangeReader>(daemon, () => useChangeReader("p"));
  act(() =>
    hook.current().open(item("a.ts"), {
      kind: "comparison",
      left: { branch: "main", commit: "1234567890abcdef" },
      right: { branch: "feature", commit: "fedcba0987654321" },
    }),
  );
  expect(hook.current().subject?.sourceText).toBe(`\u2068main\u2069 at ${shortCommit("1234567890abcdef")} to \u2068feature\u2069 at ${shortCommit("fedcba0987654321")}`);
});

it("tags a staged or unstaged change with its stage, and marks the rest by their status", () => {
  const { daemon } = controlledDaemon();
  const hook = mount<ChangeReader>(daemon, () => useChangeReader("p"));
  act(() => hook.current().open(item("a.ts", "staged"), OWN));
  expect(hook.current().subject).toMatchObject({ stage: "Staged", source: undefined, sourceText: undefined, status: "deleted" });
  act(() => hook.current().open(item("a.ts", "unstaged"), OWN));
  expect(hook.current().subject).toMatchObject({ stage: "Unstaged", status: "deleted" });
  act(() => hook.current().open(item("b.ts", "untracked"), OWN));
  expect(hook.current().subject).toMatchObject({ stage: undefined, source: undefined, sourceText: undefined, status: "untracked" });
});

it("gives a conflicted or compared change no stage tag, and its status as the list had it", () => {
  const { daemon } = controlledDaemon();
  const hook = mount<ChangeReader>(daemon, () => useChangeReader("p"));
  act(() => hook.current().open(changeItem({ group: "conflicted", path: "c.ts", conflict: "both_modified" }), OWN));
  expect(hook.current().subject).toMatchObject({ stage: undefined, status: "conflicted" });
  const compared = changeItem({ group: "committed", old: { state: "absent" }, new: { state: "present", path: "d.ts", kind: "file", source: { kind: "index", worktree: "w", blob: "b" } } });
  act(() =>
    hook.current().open(compared, {
      kind: "comparison",
      left: { branch: "main", commit: "1234567890abcdef" },
      right: { branch: "feature", commit: "fedcba0987654321" },
    }),
  );
  expect(hook.current().subject).toMatchObject({ stage: undefined, status: "added" });
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
  act(() => void hook.current().refresh());
  expect(hook.current().list).toEqual({ state: "loading" });
});

it("sends a request replacing one in its own slot at once, past the window's bound", async () => {
  const { daemon, pending } = controlledDaemon();
  const a = mount(daemon, () => useChangeList("a", undefined, true));
  mount(daemon, () => useChangeList("b", undefined, true));
  mount(daemon, () => useChangeList("c", undefined, true));
  expect(pending.map((p) => "project" in p.body && p.body.project)).toEqual(["a", "b"]);
  act(() => void a.current().refresh());
  expect(pending.map((p) => "project" in p.body && p.body.project)).toEqual(["a", "b", "a"]);
});

const endpoint = (branch: string, commit: string) => ({ branch, commit });
const comparison = (left: string, right: string, rightCommit: string, path: string): Event => ({
  type: "project_comparison",
  project: "p",
  left: endpoint(left, "c-left"),
  right: endpoint(right, rightCommit),
  changes: [{ group: "committed", old: { state: "absent" }, new: { state: "present", path, kind: "file", source: { kind: "commit", commit: rightCommit, branch: null, blob: "b" } } }],
  complete: true,
});
const compared = (state: ReturnType<typeof useBranchComparison>["comparison"]) =>
  state.state === "loaded" ? [state.right.commit, ...state.items.map((i) => i.path)] : state.state;

it("shows the comparison of the branches chosen last, whichever comparison answers last", async () => {
  const { daemon, pending } = controlledDaemon();
  const hook = mountWith(daemon, "a" as string, (right) => useBranchComparison("p", "main", right, true));
  hook.set("b");
  const [a, b] = pending.splice(0);
  expect(b.body).toMatchObject({ type: "compare_project_branches", left: "main", right: "b", slot: "comparison:p" });
  await act(async () => b.resolve(comparison("main", "b", "c-b", "b.txt")));
  await act(async () => a.resolve(comparison("main", "a", "c-a", "a.txt")));
  expect(compared(hook.current().comparison)).toEqual(["c-b", "b.txt"]);
});

it("keeps a comparison's commits through a reconnect, and asks again only for one lost with the connection", async () => {
  const { daemon, pending } = controlledDaemon();
  const hook = mountWith(daemon, "a" as string, (right) => useBranchComparison("p", "main", right, true));
  await act(async () => pending.splice(0)[0].resolve(comparison("main", "a", "c-a", "a.txt")));
  act(() => daemon.store.setState((state) => ({ snapshotEpoch: state.snapshotEpoch + 1 })));
  expect(pending).toEqual([]);
  expect(compared(hook.current().comparison)).toEqual(["c-a", "a.txt"]);

  hook.set("b");
  await act(async () => pending.splice(0)[0].reject(new Error("connection closed")));
  expect(hook.current().comparison).toEqual({ state: "loading" });
  act(() => daemon.store.setState((state) => ({ snapshotEpoch: state.snapshotEpoch + 1 })));
  expect(pending.map((p) => p.body)).toMatchObject([{ type: "compare_project_branches", right: "b" }]);
});

it("reads a compared change from the comparison's two commits, and never takes another pair's reply", async () => {
  const { daemon, pending } = controlledDaemon();
  const hook = mount<ChangeReader>(daemon, () => useChangeReader("p"));
  const entry = changeItem({ group: "committed", old: { state: "present", path: "a.ts", kind: "file", source: { kind: "commit", commit: "c1", branch: null, blob: "o" } }, new: { state: "absent" } });
  const origin = (rightCommit: string): ChangeOrigin => ({ kind: "comparison", left: endpoint("main", "c1"), right: endpoint("topic", rightCommit) });
  act(() => hook.current().open(entry, origin("c2")));
  act(() => hook.current().open(entry, origin("c3")));
  const [first, second] = pending.splice(0);
  expect(second.body).toMatchObject({ type: "read_project_comparison_change", slot: "viewer", left: { commit: "c1" }, right: { commit: "c3" } });
  const reply = (rightCommit: string, patch: string): Event => ({
    type: "project_comparison_change",
    project: "p",
    left: endpoint("main", "c1"),
    right: endpoint("topic", rightCommit),
    old: { state: "present", path: "a.ts", kind: "file", source: { kind: "commit", commit: "c1", branch: null, blob: "o" }, file: null },
    new: { state: "absent" },
    patch: { size: patch.length, kind: "text", media_type: null, text: patch, data: null },
  });
  // The first read finished before the second superseded it: its reply is of the other pair.
  await act(async () => first.resolve(reply("c2", "-from c2\n")));
  expect(hook.current().subject?.content).toEqual({ state: "loading" });
  await act(async () => second.resolve(reply("c3", "-from c3\n")));
  expect(hook.current().subject).toMatchObject({ content: { state: "change", change: { patch: "-from c3\n" } } });
  expect(hook.current().subject?.key).toContain("c3");
});

const indexSide = (blob: string) => ({ state: "present", path: "a.ts", kind: "file", source: { kind: "index", worktree: "w", blob } }) as const;
const bodiesOf = (text: (side: "old" | "new") => string): Event => ({
  type: "project_change_bodies",
  project: "p",
  worktree: null,
  group: "staged",
  old: { ...indexSide("o"), file: { size: 1, kind: "text", media_type: null, text: text("old"), data: null } },
  new: { ...indexSide("n"), file: { size: 1, kind: "text", media_type: null, text: text("new"), data: null } },
});

/** Opens a staged change of a file on both sides and answers its patch, returning what the viewer holds. */
async function openedChange() {
  const { daemon, pending } = controlledDaemon();
  const hook = mount<ChangeReader>(daemon, () => useChangeReader("p"));
  const item = changeItem({ group: "staged", old: indexSide("o"), new: indexSide("n") });
  act(() => hook.current().open(item, { kind: "worktree", worktree: undefined }));
  await act(async () =>
    pending.splice(0)[0].resolve({
      type: "project_change",
      project: "p",
      worktree: null,
      group: "staged",
      head: null,
      old: { ...indexSide("o"), file: null },
      new: { ...indexSide("n"), file: null },
      patch: { size: 1, kind: "text", media_type: null, text: "@@ -1 +1 @@\n-a\n+b\n", data: null },
    }),
  );
  const content = hook.current().subject?.content;
  if (content?.state !== "change" || !content.change.loadBodies) throw new Error("the change offers no bodies");
  return { pending, loadBodies: content.change.loadBodies };
}

it("reads a change's whole bodies only when asked, from the versions its patch reported", async () => {
  const { pending, loadBodies } = await openedChange();
  expect(pending).toEqual([]);
  const loaded = loadBodies();
  expect(pending[0].body).toMatchObject({
    type: "read_project_change_bodies",
    slot: "viewer-bodies",
    change: { group: "staged" },
    old: { state: "present", source: { blob: "o" } },
    new: { state: "present", source: { blob: "n" } },
  });
  await act(async () => pending[0].resolve(bodiesOf((side) => `${side}\n`)));
  expect(await loaded).toEqual({ old: "old\n", new: "new\n" });
});

it.each([
  [{ code: "limit_exceeded", params: { limit: "file_bytes", size: "5242880", max: "4194304" } }, "unavailable"],
  [{ code: "unsupported_file_type", params: { file_type: "symlink" } }, "unavailable"],
  [{ code: "source_changed", params: {} }, "changed"],
  [{ code: "request_superseded", params: {} }, "superseded"],
  [{ code: "git_failed", params: { detail: "no object" } }, "failed"],
])("tells why a change's bodies were not delivered: %j is %s", async ({ code, params }, reason) => {
  const { pending, loadBodies } = await openedChange();
  const outcome = expect(loadBodies()).rejects.toMatchObject({ reason });
  await act(async () => pending[0].reject(new DaemonRequestError("daemon said so", code, params)));
  await outcome;
});

it("counts branch lists and comparisons in the window's bound of two Git list requests", async () => {
  const { daemon, pending } = controlledDaemon();
  mount(daemon, () => useBranchList("a", true));
  mount(daemon, () => useBranchComparison("b", "main", "topic", true));
  mount(daemon, () => useChangeList("c", undefined, true));
  expect(pending.map((p) => p.body.type)).toEqual(["list_project_branches", "compare_project_branches"]);
  await act(async () => pending.splice(0, 1)[0].resolve({ type: "project_branches", project: "a", branches: [], complete: true }));
  expect(pending.map((p) => p.body.type)).toEqual(["compare_project_branches", "list_project_changes"]);
});
