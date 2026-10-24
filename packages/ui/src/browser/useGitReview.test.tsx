// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import type { ChangeEntry, CommitChanges, Event, RequestBody, UpstreamInfo, WorktreeInfo } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { noteSessionSwitch, useProjectBrowserState } from "./browserState";
import { directoryKey, sectionKey, type ChangeItem } from "./changes";
import { useFileFilterState } from "./FileFilterField";
import { useGitReview } from "./useGitReview";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Pending = { body: RequestBody; resolve: (event: Event) => void; reject: (err: Error) => void };
const pending: Pending[] = [];
const roots: Root[] = [];

afterEach(async () => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  await act(async () => pending.splice(0).forEach((request) => request.reject(new Error("test over"))));
});

const daemon: Daemon = {
  store: createStateStore({ connectionState: "open", snapshotEpoch: 1 }),
  request: (body) => new Promise((resolve, reject) => pending.push({ body, resolve, reject })),
  toastError: () => {},
  toastNotice: () => {},
  onToast: () => () => {},
  dismissTrustPrompt: () => {},
  reconnect: () => {},
  terminalUrl: () => "",
};

/** Answers the request of `type` still out with what `reply` makes of it. */
async function answer(type: RequestBody["type"], reply: (body: RequestBody) => Event): Promise<RequestBody> {
  const index = pending.findIndex((request) => request.body.type === type);
  expect(index, `a ${type} request out among ${pending.map((p) => p.body.type).join(", ")}`).not.toBe(-1);
  const [request] = pending.splice(index, 1);
  await act(async () => request.resolve(reply(request.body)));
  return request.body;
}

const asked = (type: RequestBody["type"]) => pending.filter((request) => request.body.type === type).map((request) => request.body);

type Tree = { id: string; branch: string | null; main?: boolean };
/** `responsible` is the session working on the branch; none when the daemon names no one. */
type Made = { name: string; commit?: string; responsible?: string; upstream?: UpstreamInfo };
type Replies = Partial<Record<RequestBody["type"], (body: RequestBody) => Event>>;

const worktreeInfo = (w: Tree): WorktreeInfo => ({ id: w.id, root: `/${w.id}`, main: w.main ?? false, head: "c1", branch: w.branch, scope_present: true });
const source = (id: string, worktrees: Tree[], own = "m"): Replies => ({
  get_project_source: () => ({
    type: "project_source",
    source: {
      project: id,
      root: "/r",
      resolved_root: "/r",
      root_id: "r",
      git_error: null,
      git: { repository: "repo", common_dir: "/r/.git", worktree: own, scope: "", worktrees: worktrees.map(worktreeInfo) },
    },
  }),
});
const branchList = (id: string, branches: Made[]): Replies => ({
  list_project_branches: () => ({
    type: "project_branches",
    project: id,
    branches: branches.map((b) => ({
      name: b.name,
      commit: b.commit ?? "c",
      upstream: b.upstream,
      origin: b.responsible ? { responsible_session: b.responsible, responsible_title: "t", responsible_since: 1 } : undefined,
    })),
    complete: true,
  }),
});
const live = (path: string, version = "1"): ChangeEntry => ({
  group: "unstaged",
  old: { state: "present", path, kind: "file", source: { kind: "index", worktree: "w", blob: "b" } },
  new: { state: "present", path, kind: "file", source: { kind: "live", root_id: "r", version } },
});
const changeList = (id: string, changes: ChangeEntry[] = []): Replies => ({
  list_project_changes: (body) => ({
    type: "project_changes",
    project: id,
    worktree: body.type === "list_project_changes" ? (body.worktree ?? null) : null,
    head: "c1",
    changes,
    complete: true,
  }),
});
const committed = (path: string, commit: string): ChangeEntry => ({
  group: "committed",
  old: { state: "absent" },
  new: { state: "present", path, kind: "file", source: { kind: "commit", commit, branch: null, blob: "b" } },
});
/** A comparison of whatever was asked, `base` the fork and `tip` the right branch's commit. */
const comparison = (id: string, tip: string, paths: string[], base = "c1"): Replies => ({
  compare_project_branches: (body) => {
    if (body.type !== "compare_project_branches") throw new Error(body.type);
    return {
      type: "project_comparison",
      project: id,
      left: body.left_empty ? { branch: "", commit: base, empty: true } : { branch: body.left, commit: base, ...(body.left_remote ? { remote: true } : {}) },
      right: { branch: body.right, commit: tip },
      base,
      changes: paths.map((path) => committed(path, tip)),
      complete: true,
    };
  },
});
const commitList = (id: string, commits: CommitChanges[]): Replies => ({
  list_project_commits: (body) => {
    if (body.type !== "list_project_commits") throw new Error(body.type);
    return { type: "project_commits", project: id, base: body.base, tip: body.tip, commits, complete: true, commits_complete: true };
  },
});
const absent = { state: "absent" } as const;
const readBack: Replies = {
  read_project_change: (body) => {
    if (body.type !== "read_project_change") throw new Error(body.type);
    return { type: "project_change", project: body.project, worktree: body.worktree ?? null, group: body.change.group, head: null, old: absent, new: absent, patch: null };
  },
};

/** The Git mode of project `id`, as the app drives it: every change of the selected session is
 * noted (`noteSessionSwitch`), and a pane mounted again keeps the project's state. */
function harness(id: string) {
  let latest!: { browser: ReturnType<typeof useProjectBrowserState>; git: ReturnType<typeof useGitReview> };
  let root!: Root;
  let selected: string | undefined;
  // The branch every render showed, in order.
  const shown: (string | undefined)[] = [];
  const Probe = ({ session, active }: { session: string | undefined; active: boolean }) => {
    const browser = useProjectBrowserState(id);
    latest = { browser, git: useGitReview(id, browser, active, session, useFileFilterState()) };
    shown.push(latest.git.view.branch);
    return null;
  };
  const render = (session: string | undefined, active: boolean) =>
    act(() =>
      root.render(
        <DaemonProvider value={daemon}>
          <Probe session={session} active={active} />
        </DaemonProvider>,
      ),
    );
  const mount = (session: string | undefined = undefined, active = true) => {
    root = createRoot(document.body.appendChild(document.createElement("div")));
    roots.push(root);
    render(session, active);
  };
  const select = (session: string | undefined) => {
    if (session !== selected) noteSessionSwitch();
    selected = session;
  };
  const show = (session: string | undefined, active = true) => {
    select(session);
    render(session, active);
  };
  // Answers whatever the Git mode has asked for among `replies`; the window's bound of two holds
  // some requests back until others are answered, hence the passes.
  const settle = async (...parts: Replies[]) => {
    const replies: Replies = Object.assign({}, ...parts);
    for (let pass = 0; pass < 4; pass++) {
      for (const [type, reply] of Object.entries(replies)) {
        while (pending.some((request) => request.body.type === type)) await answer(type as RequestBody["type"], reply);
      }
    }
  };
  const items = () => latest.git.view.items;
  const item = (predicate: (item: ChangeItem) => boolean) => {
    const found = items().find(predicate);
    if (!found) throw new Error(`no such item among ${items().map((i) => i.key).join(", ")}`);
    return found;
  };
  const selection = () => ({ branch: latest.git.view.branch, worktree: latest.git.worktree });
  return { mount, select, show, settle, items, item, selection, shown, get latest() { return latest; }, unmount: () => act(() => root.unmount()) };
}
const MAIN: Tree = { id: "m", branch: "main", main: true };
const TOPIC: Tree = { id: "w", branch: "topic" };

it("lists the default branch's uncommitted changes from its worktree, then what it committed since the main working copy's branch", async () => {
  const h = harness("p1");
  // The project's directory is in the worktree that has `topic` checked out.
  h.mount();
  await h.settle(source("p1", [MAIN, TOPIC], "w"), branchList("p1", [{ name: "loose" }, { name: "main" }, { name: "topic" }]), changeList("p1", [live("a.txt")]), comparison("p1", "c3", ["b.txt"]));
  expect(h.selection()).toEqual({ branch: "topic", worktree: undefined });
  expect(h.items().map((i) => `${i.section} ${i.path}`)).toEqual(["unstaged a.txt", "committed b.txt"]);
  expect(h.latest.git.view.committed).toMatchObject({ state: "compare", base: "main" });

  // Chosen: the branch the main working copy has checked out is compared with no other branch of
  // the repository, and its uncommitted changes come from that worktree.
  act(() => h.latest.git.view.onBranchChange("main"));
  await h.settle(changeList("p1", [live("m.txt")]));
  expect(h.selection()).toEqual({ branch: "main", worktree: "m" });
  expect(asked("compare_project_branches")).toMatchObject([{ left_empty: true, right: "main" }]);
  expect(h.latest.git.view.committed).toMatchObject({ state: "compare", baseKind: "empty" });
  expect(h.items().map((i) => i.path)).toEqual(["m.txt"]);

  // A branch checked out nowhere has no uncommitted part: only what it committed.
  act(() => h.latest.git.view.onBranchChange("loose"));
  await h.settle(comparison("p1", "c5", ["l.txt"]));
  expect(asked("list_project_changes")).toHaveLength(0);
  expect(h.latest.git.view.uncommitted.state).toBe("notCheckedOut");
  expect(h.items().map((i) => i.section)).toEqual(["committed"]);
});

it("compares the main working copy's own branch with its upstream, and again once the upstream has moved", async () => {
  const h = harness("p5");
  h.mount();
  act(() => h.latest.browser.setBranch("main"));
  const listing = (upstream: string) => [branchList("p5", [{ name: "main", commit: "c3", upstream: { name: "origin/main", commit: upstream } }, { name: "topic" }]), changeList("p5")];
  await h.settle(source("p5", [MAIN, TOPIC]), ...listing("c1"), comparison("p5", "c3", ["u.txt"]));
  expect(h.latest.git.view.committed).toMatchObject({ state: "compare", base: "origin/main" });
  expect(h.items().map((i) => `${i.section} ${i.path}`)).toEqual(["committed u.txt"]);

  // A fetch moved the remote-tracking branch: the comparison is made again against its new tip.
  const later = vi.spyOn(Date, "now").mockReturnValue(Date.now() + 60_000);
  act(() => void window.dispatchEvent(new Event("focus")));
  later.mockRestore();
  await h.settle(...listing("c2"));
  expect(asked("compare_project_branches")).toMatchObject([{ left: "origin/main", left_remote: true, right: "main" }]);
});

it("compares the main working copy's own branch with nothing until a remote branch appears, then with that", async () => {
  const h = harness("p5b");
  h.mount();
  act(() => h.latest.browser.setBranch("main"));
  await h.settle(source("p5b", [MAIN, TOPIC]), branchList("p5b", [{ name: "main", commit: "c3" }, { name: "topic" }]), changeList("p5b"), comparison("p5b", "c3", ["a.txt", "b.txt"]));
  expect(h.latest.git.view.committed).toMatchObject({ state: "compare", base: "", baseKind: "empty" });
  expect(h.items().map((i) => `${i.section} ${i.path}`)).toEqual(["committed a.txt", "committed b.txt"]);

  // The branch has been pushed, and a fetch brought its remote branch: it is compared with that.
  const later = vi.spyOn(Date, "now").mockReturnValue(Date.now() + 60_000);
  act(() => void window.dispatchEvent(new Event("focus")));
  later.mockRestore();
  await h.settle(branchList("p5b", [{ name: "main", commit: "c3", upstream: { name: "origin/main", commit: "c2" } }, { name: "topic" }]), changeList("p5b"));
  expect(h.latest.git.view.committed).toMatchObject({ state: "compare", base: "origin/main", baseKind: "remote" });
  expect(asked("compare_project_branches")).toMatchObject([{ left: "origin/main", left_remote: true, right: "main" }]);
});

it("chooses no branch while the project's worktree is detached, and compares nothing while the main working copy is", async () => {
  const h = harness("p2");
  h.mount();
  await h.settle(source("p2", [{ id: "m", branch: null, main: true }, TOPIC]), branchList("p2", [{ name: "topic" }]));
  expect(h.latest.git.view.branch).toBeUndefined();
  expect(asked("list_project_changes")).toHaveLength(0);
  act(() => h.latest.git.view.onBranchChange("topic"));
  await h.settle(changeList("p2", [live("a.txt")]));
  expect(asked("compare_project_branches")).toHaveLength(0);
  expect(h.latest.git.view.committed.state).toBe("noBase");
  expect(h.items().map((i) => i.path)).toEqual(["a.txt"]);
});

it("compares again once a branch list shows either branch at another commit", async () => {
  const h = harness("p3");
  h.mount();
  act(() => h.latest.browser.setBranch("topic"));
  await h.settle(source("p3", [MAIN, TOPIC]), branchList("p3", [{ name: "main", commit: "c1" }, { name: "topic", commit: "c3" }]), changeList("p3", [live("a.txt")]), comparison("p3", "c3", ["b.txt"]));
  expect(asked("compare_project_branches")).toHaveLength(0);

  // A list at the commits compared says nothing new; one with the branch moved makes it again.
  act(() => void h.latest.git.refresh());
  await h.settle(source("p3", [MAIN, TOPIC]), branchList("p3", [{ name: "main", commit: "c1" }, { name: "topic", commit: "c3" }]), changeList("p3", [live("a.txt")]), comparison("p3", "c3", ["b.txt"]));
  const later = vi.spyOn(Date, "now").mockReturnValue(Date.now() + 60_000);
  act(() => void window.dispatchEvent(new Event("focus")));
  later.mockRestore();
  await h.settle(branchList("p3", [{ name: "main", commit: "c1" }, { name: "topic", commit: "c4" }]), changeList("p3", []));
  expect(asked("compare_project_branches")).toHaveLength(1);
  // The comparison on screen stays until the new one is in, which has what was uncommitted.
  expect(h.items().map((i) => i.path)).toEqual(["b.txt"]);
  await h.settle(comparison("p3", "c4", ["a.txt", "b.txt"]));
  expect(h.items().map((i) => `${i.section} ${i.path}`)).toEqual(["committed a.txt", "committed b.txt"]);
});

it("lists the committed changes by commit, newest first, and reads a commit's change against its parent", async () => {
  const h = harness("p4");
  h.mount();
  act(() => {
    h.latest.browser.setBranch("topic");
    h.latest.browser.setByCommit(true);
  });
  const commits: CommitChanges[] = [
    { commit: "c3", parent: "c2", subject: "second", changes: [committed("a.txt", "c3")] },
    { commit: "c2", parent: "c1", subject: "first", changes: [committed("a.txt", "c2"), committed("b.txt", "c2")] },
  ];
  try {
    await h.settle(source("p4", [MAIN, TOPIC]), branchList("p4", [{ name: "main" }, { name: "topic" }]), changeList("p4"), comparison("p4", "c3", ["a.txt", "b.txt"]));
    expect(asked("list_project_commits")).toMatchObject([{ base: "c1", tip: "c3" }]);
    await h.settle(commitList("p4", commits));
    expect(h.items().map((i) => `${i.commit?.commit} ${i.path}`)).toEqual(["c3 a.txt", "c2 a.txt", "c2 b.txt"]);
    // The same file changed by two commits is two rows, and two stops of the viewer.
    const [newer, older] = h.items();
    expect(newer.key).not.toBe(older.key);

    act(() => h.latest.git.view.onOpen(older));
    const read = asked("read_project_commit_change")[0];
    expect(read).toMatchObject({ commit: "c2", parent: "c1", change: { new: { state: "present", path: "a.txt" } } });
    await h.settle({
      read_project_commit_change: () => ({ type: "project_commit_change", project: "p4", commit: "c2", parent: "c1", old: absent, new: absent, patch: null }),
    });
    expect(h.latest.git.viewer?.subject.sourceText).toContain("c2");
    // A committed change has no file on disk to show or open.
    expect(h.latest.git.viewer?.subject.host).toBeUndefined();
    expect(h.latest.git.viewer?.navigation?.onPrevious).toBeDefined();

    // Commits listed again without it: the viewer closes. Grouped back together, the cumulative
    // section is shown, and nothing is asked again.
    act(() => void h.latest.git.refresh());
    await h.settle(source("p4", [MAIN, TOPIC]), branchList("p4", [{ name: "main" }, { name: "topic" }]), changeList("p4"), comparison("p4", "c3", ["a.txt"]), commitList("p4", [commits[0]]));
    expect(h.latest.git.viewer).toBeUndefined();
    act(() => h.latest.browser.setByCommit(false));
    expect(pending).toHaveLength(0);
    expect(h.items().map((i) => `${i.section} ${i.path}`)).toEqual(["committed a.txt"]);
  } finally {
    act(() => h.latest.browser.setByCommit(false));
  }
});

it("does not ask for a comparison again while one is in flight, whatever the branch lists read meanwhile", async () => {
  const h = harness("p3b");
  h.mount();
  act(() => h.latest.browser.setBranch("topic"));
  const listing = (topic: string) => [branchList("p3b", [{ name: "main", commit: "c1" }, { name: "topic", commit: topic }]), changeList("p3b")];
  await h.settle(source("p3b", [MAIN, TOPIC]), ...listing("c3"), comparison("p3b", "c3", ["b.txt"]));
  const focusLater = (ahead: number) => {
    const later = vi.spyOn(Date, "now").mockReturnValue(Date.now() + ahead);
    act(() => void window.dispatchEvent(new Event("focus")));
    later.mockRestore();
  };
  // The branch moved: the comparison is asked for again, and takes its time.
  focusLater(100_000);
  await h.settle(...listing("c4"));
  expect(asked("compare_project_branches")).toHaveLength(1);
  const inFlight = asked("compare_project_branches")[0];

  // Lists read while it is out still show the branch at the commit it was asked for: it is not
  // asked for again, and the request out is not replaced.
  focusLater(200_000);
  await h.settle(...listing("c4"));
  focusLater(300_000);
  await h.settle(...listing("c4"));
  expect(asked("compare_project_branches")).toEqual([inFlight]);

  // The branch moving on again is news of its own.
  focusLater(400_000);
  await h.settle(...listing("c5"));
  expect(asked("compare_project_branches")).toHaveLength(2);
  await h.settle(comparison("p3b", "c5", ["a.txt"]));
  expect(h.items().map((i) => i.path)).toEqual(["a.txt"]);
});

it("asks for a comparison that failed in the background again only once a later list has the branches elsewhere", async () => {
  const h = harness("p3c");
  h.mount();
  act(() => h.latest.browser.setBranch("topic"));
  const listing = (topic: string) => [branchList("p3c", [{ name: "main", commit: "c1" }, { name: "topic", commit: topic }]), changeList("p3c")];
  await h.settle(source("p3c", [MAIN, TOPIC]), ...listing("c3"), comparison("p3c", "c3", ["b.txt"]));
  const focusLater = (ahead: number) => {
    const later = vi.spyOn(Date, "now").mockReturnValue(Date.now() + ahead);
    act(() => void window.dispatchEvent(new Event("focus")));
    later.mockRestore();
  };
  const failOne = async () => {
    const index = pending.findIndex((request) => request.body.type === "compare_project_branches");
    expect(index).not.toBe(-1);
    const [request] = pending.splice(index, 1);
    await act(async () => request.reject(new DaemonRequestError("broken", "git_failed", {})));
  };

  // The branch moved and the comparison made again fails.
  focusLater(100_000);
  await h.settle(...listing("c4"));
  expect(asked("compare_project_branches")).toHaveLength(1);
  await failOne();
  expect(h.latest.git.view.committed).toMatchObject({ state: "compare", comparison: { state: "error" } });

  // Lists with the branch where it was asked for do not ask again.
  focusLater(200_000);
  await h.settle(...listing("c4"));
  expect(asked("compare_project_branches")).toHaveLength(0);

  // The branch at another commit does.
  focusLater(300_000);
  await h.settle(...listing("c5"));
  expect(asked("compare_project_branches")).toHaveLength(1);
  await h.settle(comparison("p3c", "c5", ["a.txt"]));
  expect(h.items().map((i) => i.path)).toEqual(["a.txt"]);
});

it("shows no commit section of the previous branch while the new branch's commits are listed", async () => {
  const h = harness("p4b");
  h.mount();
  act(() => {
    h.latest.browser.setBranch("topic");
    h.latest.browser.setByCommit(true);
  });
  const commitsOf = (tip: string, path: string): CommitChanges[] => [{ commit: tip, parent: "c1", subject: `on ${tip}`, changes: [committed(path, tip)] }];
  const base = [source("p4b", [MAIN, TOPIC]), branchList("p4b", [{ name: "main" }, { name: "topic" }, { name: "other" }]), changeList("p4b")];
  try {
    await h.settle(...base, comparison("p4b", "c3", ["a.txt"]), commitList("p4b", commitsOf("c3", "a.txt")));
    expect(h.items().map((i) => i.commit?.commit)).toEqual(["c3"]);

    // Another branch: the commits are asked for, and while they are not in, nothing of the
    // previous branch's stands, and no footer of its list does either.
    act(() => h.latest.git.view.onBranchChange("other"));
    await h.settle(comparison("p4b", "c6", ["z.txt"]));
    expect(asked("list_project_commits")).toMatchObject([{ tip: "c6" }]);
    expect(h.items()).toEqual([]);
    expect(h.latest.git.view.committed).toMatchObject({ state: "compare", commits: { state: "loading" } });
    await h.settle(commitList("p4b", commitsOf("c6", "z.txt")));
    expect(h.items().map((i) => i.commit?.commit)).toEqual(["c6"]);

    // The same branch at another commit keeps its sections until the new list is in.
    act(() => void h.latest.git.refresh());
    await h.settle(source("p4b", [MAIN, TOPIC]), branchList("p4b", [{ name: "main" }, { name: "topic" }, { name: "other" }]), changeList("p4b"), comparison("p4b", "c7", ["z.txt"]));
    expect(h.items().map((i) => i.commit?.commit)).toEqual(["c6"]);
    await h.settle(commitList("p4b", commitsOf("c7", "z.txt")));
    expect(h.items().map((i) => i.commit?.commit)).toEqual(["c7"]);
  } finally {
    act(() => h.latest.browser.setByCommit(false));
  }
});

it("shows no commit section of the previous base branch while the commits against the new one are listed", async () => {
  const h = harness("p4c");
  h.mount();
  act(() => {
    h.latest.browser.setBranch("topic");
    h.latest.browser.setByCommit(true);
  });
  const commitsOf = (path: string): CommitChanges[] => [{ commit: "c3", parent: "c1", subject: path, changes: [committed(path, "c3")] }];
  const listing = (base: Tree) => [source("p4c", [base, TOPIC]), branchList("p4c", [{ name: "main" }, { name: "dev" }, { name: "topic" }]), changeList("p4c")];
  try {
    await h.settle(...listing(MAIN), comparison("p4c", "c3", ["a.txt"]), commitList("p4c", commitsOf("a.txt")));
    expect(h.items().map((i) => i.path)).toEqual(["a.txt"]);

    // The main working copy moved to another branch: the same branch is compared with it from
    // another fork, and the sections of the commits against the old base are not shown meanwhile.
    act(() => void h.latest.git.refresh());
    await h.settle(...listing({ id: "m", branch: "dev", main: true }), comparison("p4c", "c3", ["a.txt"], "c0"));
    expect(asked("list_project_commits")).toMatchObject([{ base: "c0", tip: "c3" }]);
    expect(h.items()).toEqual([]);
    expect(h.latest.git.view.committed).toMatchObject({ state: "compare", base: "dev", commits: { state: "loading" } });
    await h.settle(commitList("p4c", commitsOf("b.txt")));
    expect(h.items().map((i) => i.path)).toEqual(["b.txt"]);
  } finally {
    act(() => h.latest.browser.setByCommit(false));
  }
});

it("follows a comparison made again with the change still in it, and closes on one without it", async () => {
  const h = harness("p5");
  h.mount();
  act(() => h.latest.browser.setBranch("topic"));
  const replies = (tip: string, paths: string[]) => [source("p5", [MAIN]), branchList("p5", [{ name: "main", commit: "c1" }, { name: "topic", commit: tip }]), comparison("p5", tip, paths)];
  await h.settle(...replies("c2", ["a.txt", "b.txt"]));
  act(() => h.latest.git.view.onOpen(h.items()[0]));
  const read: Replies = {
    read_project_comparison_change: (body) => {
      if (body.type !== "read_project_comparison_change") throw new Error(body.type);
      return { type: "project_comparison_change", project: "p5", left: body.left, right: body.right, base: body.base, old: absent, new: absent, patch: null };
    },
  };
  expect(asked("read_project_comparison_change")).toMatchObject([{ right: { commit: "c2" } }]);
  await h.settle(read);

  // Made again at another commit, with the change still in it: read again from the new pair.
  act(() => void h.latest.git.refresh());
  await h.settle(...replies("c3", ["a.txt"]));
  expect(asked("read_project_comparison_change")).toMatchObject([{ right: { commit: "c3" } }]);
  await h.settle(read);
  expect(h.latest.git.viewer?.subject.key).toContain("c3");

  // Made again without it: no such change between those commits, so the viewer closes.
  act(() => void h.latest.git.refresh());
  await h.settle(...replies("c4", ["b.txt"]));
  expect(h.latest.git.viewer).toBeUndefined();
  expect(h.latest.browser.selectedChange).toBeUndefined();
  expect(asked("read_project_comparison_change")).toHaveLength(0);
});

it("follows the list's status for an open uncommitted change whose key did not change", async () => {
  const h = harness("p6");
  h.mount();
  // A key names a present side by its path alone, so a file turning into a link keeps its key.
  const listed = (kind: "file" | "symlink", version: string): ChangeEntry => {
    const entry = live("a.txt", version);
    return entry.group === "unstaged" && entry.new.state === "present" ? { ...entry, new: { ...entry.new, kind } } : entry;
  };
  await h.settle(source("p6", [MAIN]), branchList("p6", []), changeList("p6", [listed("file", "1")]));
  act(() => h.latest.git.view.onOpen(h.items()[0]));
  await h.settle(readBack);
  expect(h.latest.git.viewer?.subject.status).toBe("modified");
  // A change in a worktree is a live file the header can show in the file manager or open.
  expect(h.latest.git.viewer?.subject.host).toEqual({ project: "p6", path: "a.txt" });

  act(() => void h.latest.git.refresh());
  await h.settle(source("p6", [MAIN]), branchList("p6", []), changeList("p6", [listed("symlink", "2")]));
  await h.settle(readBack);
  expect(h.latest.git.viewer?.subject.status).toBe("typeChanged");
});

it("moves the viewer through the rows on screen, skipping a collapsed directory, a folded section and what the filter hides", async () => {
  const h = harness("p7");
  h.mount();
  act(() => h.latest.browser.setLayout("tree"));
  try {
    await h.settle(source("p7", [MAIN]), branchList("p7", []), changeList("p7", ["a/x.ts", "a/y.ts", "z.ts"].map((path) => live(path))));
    act(() => h.latest.git.view.onOpen(h.item((i) => i.path === "z.ts")));
    await h.settle(readBack);
    expect(h.latest.git.viewer?.navigation?.onPrevious).toBeDefined();

    // Folding `a` takes its changes off the screen, so there is nothing before `z.ts` to move to.
    act(() => h.latest.browser.setCollapsedChangeRows(new Set([directoryKey("unstaged", "a")])));
    expect(h.latest.git.viewer?.navigation?.onPrevious).toBeUndefined();
    // The flat list has every change on screen whatever was folded in the tree.
    act(() => h.latest.browser.setLayout("flat"));
    expect(h.latest.git.viewer?.navigation?.onPrevious).toBeDefined();
    // A folded section takes its changes off the screen in either layout.
    act(() => h.latest.browser.setCollapsedChangeRows(new Set([sectionKey("unstaged")])));
    expect(h.latest.git.viewer?.navigation?.onPrevious).toBeUndefined();
    act(() => h.latest.browser.setCollapsedChangeRows(new Set()));

    // Only the open change matches: nothing on either side. The directories are not matched.
    act(() => h.latest.git.view.changeView.onFilterChange("z"));
    expect(h.latest.git.viewer?.navigation?.onPrevious).toBeUndefined();
    act(() => h.latest.git.view.changeView.onFilterChange("a"));
    expect(h.latest.git.viewer?.navigation?.onPrevious).toBeUndefined();
    act(() => h.latest.git.view.changeView.onFilterChange(""));
    expect(h.latest.git.viewer?.navigation?.onPrevious).toBeDefined();
  } finally {
    act(() => {
      h.latest.browser.setLayout("flat");
      h.latest.browser.setCollapsedChangeRows(new Set());
    });
  }
});

it("drops the filter text with the field, so no filter out of sight narrows a list", () => {
  const h = harness("p8");
  h.mount();
  const view = () => h.latest.git.view.changeView;
  expect(view().filterOpen).toBe(false);
  act(() => view().onFilterOpenChange(true));
  act(() => view().onFilterChange("alpha"));
  act(() => view().onFilterOpenChange(false));
  expect(view().filter).toBe("");
  act(() => view().onFilterOpenChange(true));
  expect(view().filter).toBe("");
});

/** Everything a followed session's Git mode asks for, answered for `worktrees` and `branches`. */
const around = (id: string, worktrees: Tree[], branches: Made[]) => [source(id, worktrees), branchList(id, branches), changeList(id), comparison(id, "c2", [])];
const DEFAULT = { branch: "main", worktree: undefined };

it("sets the branch from the selected session's once it is known, unless the user chose first", async () => {
  const h = harness("f1");
  h.mount(undefined);
  h.show("s1");
  await h.settle(...around("f1", [MAIN, TOPIC], [{ name: "main" }]));
  expect(h.selection()).toEqual(DEFAULT);

  // The attribution arrives in a later list: set, and its worktree followed.
  act(() => void h.latest.git.refresh());
  await h.settle(...around("f1", [MAIN, TOPIC], [{ name: "main" }, { name: "topic", responsible: "s1" }]));
  expect(h.selection()).toEqual({ branch: "topic", worktree: "w" });

  // Another session with no branch goes back to the default; a choice made since settles it.
  h.show("s2");
  await h.settle(...around("f1", [MAIN, TOPIC], [{ name: "loose" }, { name: "main" }, { name: "topic", responsible: "s1" }]));
  expect(h.selection()).toEqual(DEFAULT);
  act(() => h.latest.git.view.onBranchChange("loose"));
  act(() => void h.latest.git.refresh());
  await h.settle(...around("f1", [MAIN, TOPIC], [{ name: "loose" }, { name: "topic", responsible: "s2" }]));
  expect(h.selection().branch).toBe("loose");
});

it("sets the branch again on switching between two sessions that are each responsible for one", async () => {
  const h = harness("f2");
  const branches = [{ name: "main" }, { name: "topic", responsible: "s1" }, { name: "other", responsible: "s2" }];
  h.mount(undefined);
  h.show("s1");
  await h.settle(...around("f2", [MAIN, TOPIC], branches));
  expect(h.selection()).toEqual({ branch: "topic", worktree: "w" });
  h.show("s2");
  await h.settle(...around("f2", [MAIN, TOPIC], branches));
  expect(h.selection()).toEqual({ branch: "other", worktree: undefined });
});

it("asks again for a worktree no listed one is, and reads the branch's changes from the one that answers", async () => {
  const h = harness("f3");
  h.mount(undefined);
  h.show("s1");
  const branches = [{ name: "main" }, { name: "topic", responsible: "s1" }];
  // The worktrees are read once from before the selection, which is asked for again, and once
  // since, and neither has the branch checked out: they are asked for once more, and the worktree
  // made in the meantime is in that answer.
  let sources = 0;
  const growing: Replies = { get_project_source: (body) => source("f3", sources++ < 2 ? [MAIN] : [MAIN, TOPIC]).get_project_source!(body) };
  await h.settle(growing, branchList("f3", branches), changeList("f3"), comparison("f3", "c2", []));
  expect(sources).toBe(3);
  expect(h.selection()).toEqual({ branch: "topic", worktree: "w" });
});

it("weighs only what was read after the selection, asking again for what a pane held", async () => {
  const h = harness("f4");
  const branches = [{ name: "main" }, { name: "topic", responsible: "s1" }, { name: "other", responsible: "s2" }];
  h.mount(undefined);
  h.show("s1");
  await h.settle(...around("f4", [MAIN, TOPIC], branches));
  expect(h.selection().branch).toBe("topic");
  // Selected while the Git mode is off screen: the lists held name a branch for s2, but are from
  // before the selection and are not weighed.
  h.show("s2", false);
  expect(h.selection().branch).toBe("topic");
  expect(pending).toHaveLength(0);
  h.show("s2", true);
  expect(h.selection().branch).toBe("topic");
  expect(pending.map((request) => request.body.type)).toEqual(expect.arrayContaining(["get_project_source", "list_project_branches"]));
  await h.settle(...around("f4", [MAIN, TOPIC], branches));
  expect(h.selection().branch).toBe("other");
});

it("sets the branch again on a new selection of the session, but not on a pane mounted under the same one", async () => {
  const h = harness("f5");
  const branches = [{ name: "main" }, { name: "topic", responsible: "s1" }];
  h.mount(undefined);
  h.show("s1");
  await h.settle(...around("f5", [MAIN, TOPIC], branches));
  act(() => h.latest.git.view.onBranchChange("main"));

  // The pane goes and comes back with the selection unchanged: the choice stands.
  h.unmount();
  h.mount("s1");
  await h.settle(...around("f5", [MAIN, TOPIC], branches));
  expect(h.selection().branch).toBe("main");

  // Another selection in between, the pane away: the session is a new switch.
  h.unmount();
  h.select("other");
  h.select("s1");
  h.mount("s1");
  await h.settle(...around("f5", [MAIN, TOPIC], branches));
  expect(h.selection()).toEqual({ branch: "topic", worktree: "w" });
});

it("leaves the branch as it is for a session that is not the project's", async () => {
  const h = harness("f6");
  h.mount(undefined);
  h.show("s1");
  await h.settle(...around("f6", [MAIN, TOPIC], [{ name: "main" }, { name: "topic", responsible: "s1" }]));
  const before = h.selection();
  h.show(undefined);
  expect(pending).toHaveLength(0);
  expect(h.selection()).toEqual(before);
});

it("follows a branch that shows up later in a worktree made since, on a clock that stands still", async () => {
  const h = harness("f7");
  h.mount(undefined);
  h.show("s1");
  await h.settle(...around("f7", [MAIN], [{ name: "main" }]));
  expect(h.selection()).toEqual(DEFAULT);

  // WebKit rounds `performance.now()` to the millisecond: what one handler does reads one time.
  const clock = vi.spyOn(performance, "now").mockReturnValue(performance.now() + 1000);
  try {
    // The session makes a worktree on a new branch; the branch list's own refresh, on coming back
    // to the window, is the first to name it, beside a worktree list read before it was made.
    const later = vi.spyOn(Date, "now").mockReturnValue(Date.now() + 60_000);
    act(() => void window.dispatchEvent(new Event("focus")));
    later.mockRestore();
    await h.settle(branchList("f7", [{ name: "main" }, { name: "topic", responsible: "s1" }]));
    expect(h.selection()).toEqual({ branch: "topic", worktree: undefined });
    await h.settle(...around("f7", [MAIN, TOPIC], [{ name: "main" }, { name: "topic", responsible: "s1" }]));
    expect(h.selection()).toEqual({ branch: "topic", worktree: "w" });
  } finally {
    clock.mockRestore();
  }
});

it("goes back to the default branch once a complete branch list no longer has the chosen one, whoever chose it", async () => {
  const h = harness("g1");
  const branches = [{ name: "main" }, { name: "topic", responsible: "s1" }];
  h.mount(undefined);
  await h.settle(...around("g1", [MAIN, TOPIC], branches));
  act(() => h.latest.git.view.onBranchChange("topic"));
  await h.settle(changeList("g1"), comparison("g1", "c2", []));
  expect(h.selection()).toEqual({ branch: "topic", worktree: "w" });
  act(() => void h.latest.git.refresh());
  await h.settle(source("g1", [MAIN]));
  h.shown.length = 0;
  await h.settle(...around("g1", [MAIN], [{ name: "main" }]));
  expect(h.selection()).toEqual(DEFAULT);
  expect(h.latest.browser.branch).toBeUndefined();
  // Not even one render shows the gone branch as the one chosen.
  expect(h.shown).not.toContain("topic");

  // Set by following the session, and merged and deleted since.
  h.show("s1");
  await h.settle(...around("g1", [MAIN, TOPIC], branches));
  expect(h.selection()).toEqual({ branch: "topic", worktree: "w" });
  act(() => void h.latest.git.refresh());
  await h.settle(...around("g1", [MAIN], [{ name: "main" }]));
  expect(h.selection()).toEqual(DEFAULT);
});

it("reads the worktrees again for a worktree found gone, the list still being read meanwhile, and keeps a branch that is still there", async () => {
  const h = harness("g2");
  const branches = [{ name: "main" }, { name: "topic" }];
  h.mount(undefined);
  await h.settle(...around("g2", [MAIN, TOPIC], branches));
  act(() => h.latest.git.view.onBranchChange("topic"));
  const index = pending.findIndex((request) => request.body.type === "list_project_changes");
  const [request] = pending.splice(index, 1);
  await act(async () => request.reject(new DaemonRequestError("gone", "worktree_unavailable", { worktree: "w" })));
  expect(h.latest.git.view.uncommitted).toMatchObject({ state: "listed", list: { state: "loading" } });
  expect(asked("get_project_source")).toHaveLength(1);
  // The branch may have gone with its worktree: the branches are read again with the worktrees.
  let branchReads = 0;
  const counted: Replies = { list_project_branches: (body) => (branchReads++, branchList("g2", branches).list_project_branches!(body)) };
  await h.settle(...around("g2", [MAIN], branches), counted);
  expect(branchReads).toBe(1);
  expect(h.selection()).toEqual({ branch: "topic", worktree: undefined });
  expect(h.latest.git.view.uncommitted).toEqual({ state: "notCheckedOut", branch: "topic" });
});

it("closes an open uncommitted change, saying nothing, once its worktree is found gone", async () => {
  const h = harness("g3");
  const branches = [{ name: "main" }, { name: "topic" }];
  h.mount(undefined);
  await h.settle(...around("g3", [MAIN, TOPIC], branches));
  act(() => h.latest.git.view.onBranchChange("topic"));
  await h.settle(changeList("g3", [live("a.txt")]), comparison("g3", "c2", []));
  act(() => h.latest.git.view.onOpen(h.item((i) => i.path === "a.txt")));
  await h.settle(readBack);
  expect(h.latest.git.viewer).toBeDefined();

  // The worktree is removed while the branch stays: the change is read again, from where it was.
  act(() => void h.latest.git.refresh());
  await h.settle(source("g3", [MAIN]));
  const index = pending.findIndex((request) => request.body.type === "read_project_change");
  expect(pending[index]?.body).toMatchObject({ worktree: "w" });
  const [read] = pending.splice(index, 1);
  await act(async () => read.reject(new DaemonRequestError("gone", "worktree_unavailable", { worktree: "w" })));
  expect(h.latest.git.viewer).toBeUndefined();
  expect(h.latest.browser.selectedChange).toBeUndefined();
});
