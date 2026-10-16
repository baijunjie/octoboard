// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import type { ChangeEntry, Event, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { noteSessionSwitch, useProjectBrowserState } from "./browserState";
import { directoryKey, sectionKey } from "./changes";
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

const committed = (path: string, commit: string): ChangeEntry => ({
  group: "committed",
  old: { state: "absent" },
  new: { state: "present", path, kind: "file", source: { kind: "commit", commit, branch: null, blob: "b" } },
});
const comparison = (right: string, paths: string[]) => () =>
  ({
    type: "project_comparison",
    project: "p",
    left: { branch: "main", commit: "c1" },
    right: { branch: "topic", commit: right },
    base: "c1",
    changes: paths.map((path) => committed(path, right)),
    complete: true,
  }) satisfies Event;
const read = (body: RequestBody): Event => {
  if (body.type !== "read_project_comparison_change") throw new Error(body.type);
  return { type: "project_comparison_change", project: "p", left: body.left, right: body.right, base: body.base, old: { state: "absent" }, new: { state: "absent" }, patch: null };
};

it("follows a comparison made again with the change still in it, and closes on one without it", async () => {
  let latest!: { browser: ReturnType<typeof useProjectBrowserState>; git: ReturnType<typeof useGitReview> };
  const Probe = () => {
    const browser = useProjectBrowserState("p");
    latest = { browser, git: useGitReview("p", browser, true, undefined, { worktree: useFileFilterState(), compare: useFileFilterState() }) };
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  roots.push(root);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <Probe />
      </DaemonProvider>,
    ),
  );
  act(() => {
    latest.browser.setGitView("compare");
    latest.browser.setBranches("main", "topic");
  });
  const worktree = { id: "w", root: "/r", main: true, head: "c1", branch: "main", scope_present: true };
  // The source and the branches, asked for first and again on every refresh; the comparison waits
  // for them within the window's bound of two.
  const sourceAndBranches = async () => {
    await answer("get_project_source", () => ({
      type: "project_source",
      source: { project: "p", root: "/r", resolved_root: "/r", root_id: "r", git_error: null, git: { repository: "repo", common_dir: "/r/.git", worktree: "w", scope: "", worktrees: [worktree] } },
    }));
    await answer("list_project_branches", () => ({ type: "project_branches", project: "p", branches: [], complete: true }));
  };
  await sourceAndBranches();
  await answer("compare_project_branches", comparison("c2", ["a.txt", "b.txt"]));
  const loaded = latest.git.view.compare.comparison;
  if (loaded.state !== "loaded") throw new Error(loaded.state);
  act(() => latest.git.view.compare.onOpen(loaded.items[0]));
  expect((await answer("read_project_comparison_change", read)) as object).toMatchObject({ right: { commit: "c2" } });

  // Made again at another commit, with the change still in it: read again from the new pair.
  act(() => void latest.git.refresh());
  await sourceAndBranches();
  await answer("compare_project_branches", comparison("c3", ["a.txt"]));
  expect((await answer("read_project_comparison_change", read)) as object).toMatchObject({ right: { commit: "c3" } });
  expect(latest.git.viewer?.subject.key).toContain("c3");

  // Made again without it: no such change between those commits, so the viewer closes.
  act(() => void latest.git.refresh());
  await sourceAndBranches();
  await answer("compare_project_branches", comparison("c4", ["b.txt"]));
  expect(latest.git.viewer).toBeUndefined();
  expect(latest.browser.selectedComparedChange).toBeUndefined();
  expect(pending.some((request) => request.body.type === "read_project_comparison_change")).toBe(false);
});

it("follows the list's status for an open worktree change whose key did not change", async () => {
  let latest!: { browser: ReturnType<typeof useProjectBrowserState>; git: ReturnType<typeof useGitReview> };
  const Probe = () => {
    const browser = useProjectBrowserState("p");
    latest = { browser, git: useGitReview("p", browser, true, undefined, { worktree: useFileFilterState(), compare: useFileFilterState() }) };
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  roots.push(root);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <Probe />
      </DaemonProvider>,
    ),
  );
  act(() => latest.browser.setGitView("worktree"));
  const worktree = { id: "w", root: "/r", main: true, head: "c1", branch: "main", scope_present: true };
  await answer("get_project_source", () => ({
    type: "project_source",
    source: { project: "p", root: "/r", resolved_root: "/r", root_id: "r", git_error: null, git: { repository: "repo", common_dir: "/r/.git", worktree: "w", scope: "", worktrees: [worktree] } },
  }));
  // A key names a present side by its path alone, so a file turning into a link keeps its key.
  const source = { kind: "index", worktree: "w", blob: "b" } as const;
  const listed = (kind: "file" | "symlink", version: string) => () =>
    ({
      type: "project_changes",
      project: "p",
      worktree: null,
      head: "c1",
      changes: [
        {
          group: "unstaged",
          old: { state: "present", path: "a.txt", kind: "file", source },
          new: { state: "present", path: "a.txt", kind, source: { kind: "live", root_id: "r", version } },
        },
      ],
      complete: true,
    }) satisfies Event;
  await answer("list_project_branches", () => ({ type: "project_branches", project: "p", branches: [], complete: true }));
  await answer("list_project_changes", listed("file", "1"));
  const items = latest.git.view.worktree.list;
  if (items.state !== "loaded") throw new Error(items.state);
  act(() => latest.git.view.worktree.onOpen(items.items[0]));
  const side = { state: "absent" } as const;
  const readBack = (): Event => ({ type: "project_change", project: "p", worktree: null, group: "unstaged", head: null, old: side, new: side, patch: null });
  await answer("read_project_change", readBack);
  expect(latest.git.viewer?.subject.status).toBe("modified");

  act(() => void latest.git.refresh());
  await answer("get_project_source", () => ({
    type: "project_source",
    source: { project: "p", root: "/r", resolved_root: "/r", root_id: "r", git_error: null, git: { repository: "repo", common_dir: "/r/.git", worktree: "w", scope: "", worktrees: [worktree] } },
  }));
  await answer("list_project_changes", listed("symlink", "2"));
  await answer("read_project_change", readBack);
  expect(latest.git.viewer?.subject.status).toBe("typeChanged");
});

it("moves the viewer through the change tree's rows on screen, skipping a collapsed directory", async () => {
  let latest!: { browser: ReturnType<typeof useProjectBrowserState>; git: ReturnType<typeof useGitReview> };
  const Probe = () => {
    const browser = useProjectBrowserState("p");
    latest = { browser, git: useGitReview("p", browser, true, undefined, { worktree: useFileFilterState(), compare: useFileFilterState() }) };
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  roots.push(root);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <Probe />
      </DaemonProvider>,
    ),
  );
  act(() => {
    latest.browser.setGitView("worktree");
    latest.browser.setLayout("worktree", "tree");
  });
  try {
    const worktree = { id: "w", root: "/r", main: true, head: "c1", branch: "main", scope_present: true };
    await answer("get_project_source", () => ({
      type: "project_source",
      source: { project: "p", root: "/r", resolved_root: "/r", root_id: "r", git_error: null, git: { repository: "repo", common_dir: "/r/.git", worktree: "w", scope: "", worktrees: [worktree] } },
    }));
    const source = { kind: "index", worktree: "w", blob: "b" } as const;
    const unstaged = (path: string): ChangeEntry => ({
      group: "unstaged",
      old: { state: "present", path, kind: "file", source },
      new: { state: "present", path, kind: "file", source: { kind: "live", root_id: "r", version: "1" } },
    });
    await answer("list_project_changes", () => ({
      type: "project_changes",
      project: "p",
      worktree: null,
      head: "c1",
      changes: ["a/x.ts", "a/y.ts", "z.ts"].map(unstaged),
      complete: true,
    }));
    const list = latest.git.view.worktree.list;
    if (list.state !== "loaded") throw new Error(list.state);
    const side = { state: "absent" } as const;
    const readBack = (): Event => ({ type: "project_change", project: "p", worktree: null, group: "unstaged", head: null, old: side, new: side, patch: null });
    const z = list.items[2];

    act(() => latest.git.view.worktree.onOpen(z));
    await answer("read_project_change", readBack);
    expect(latest.git.viewer?.navigation?.onPrevious).toBeDefined();

    // Folding `a` takes its changes off the screen, so there is nothing before `z.ts` to move to.
    act(() => latest.browser.setCollapsedChangeRows(new Set([directoryKey("unstaged", "a")])));
    expect(latest.git.viewer?.navigation?.onPrevious).toBeUndefined();
    expect(latest.git.viewer?.navigation?.onNext).toBeUndefined();

    // The flat list has every change on screen whatever was folded in the tree.
    act(() => latest.browser.setLayout("worktree", "flat"));
    expect(latest.git.viewer?.navigation?.onPrevious).toBeDefined();

    // A folded section takes its changes off the screen in either layout, this list holding only
    // unstaged ones.
    act(() => latest.browser.setCollapsedChangeRows(new Set([sectionKey("unstaged")])));
    expect(latest.git.viewer?.navigation?.onPrevious).toBeUndefined();
    expect(latest.git.viewer?.navigation?.onNext).toBeUndefined();
  } finally {
    act(() => {
      latest.browser.setLayout("worktree", "flat");
      latest.browser.setCollapsedChangeRows(new Set());
    });
  }
});

it("moves the viewer through the rows a file name filter leaves", async () => {
  let latest!: { browser: ReturnType<typeof useProjectBrowserState>; git: ReturnType<typeof useGitReview> };
  const Probe = () => {
    const browser = useProjectBrowserState("p");
    latest = { browser, git: useGitReview("p", browser, true, undefined, { worktree: useFileFilterState(), compare: useFileFilterState() }) };
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  roots.push(root);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <Probe />
      </DaemonProvider>,
    ),
  );
  act(() => latest.browser.setGitView("worktree"));
  const worktree = { id: "w", root: "/r", main: true, head: "c1", branch: "main", scope_present: true };
  await answer("get_project_source", () => ({
    type: "project_source",
    source: { project: "p", root: "/r", resolved_root: "/r", root_id: "r", git_error: null, git: { repository: "repo", common_dir: "/r/.git", worktree: "w", scope: "", worktrees: [worktree] } },
  }));
  const source = { kind: "index", worktree: "w", blob: "b" } as const;
  const unstaged = (path: string): ChangeEntry => ({
    group: "unstaged",
    old: { state: "present", path, kind: "file", source },
    new: { state: "present", path, kind: "file", source: { kind: "live", root_id: "r", version: "1" } },
  });
  await answer("list_project_changes", () => ({ type: "project_changes", project: "p", worktree: null, head: "c1", changes: ["a/one.ts", "b/two.md", "c/three.ts"].map(unstaged), complete: true }));
  const list = latest.git.view.worktree.list;
  if (list.state !== "loaded") throw new Error(list.state);
  const side = { state: "absent" } as const;
  const readBack = (): Event => ({ type: "project_change", project: "p", worktree: null, group: "unstaged", head: null, old: side, new: side, patch: null });

  act(() => latest.git.view.worktree.onOpen(list.items[0]));
  await answer("read_project_change", readBack);
  expect(latest.git.viewer?.navigation?.onNext).toBeDefined();

  // `two.md` is filtered out, so Next from `one.ts` reaches `three.ts`; the directories are not
  // matched.
  act(() => latest.git.view.changeView.onFilterChange(".ts"));
  act(() => latest.git.viewer?.navigation?.onNext?.());
  await answer("read_project_change", readBack);
  expect(latest.browser.selectedChange).toBe(list.items[2].key);

  // Only the open change itself is left: nothing on either side.
  act(() => latest.git.view.changeView.onFilterChange("three"));
  expect(latest.git.viewer?.navigation?.onPrevious).toBeUndefined();
  expect(latest.git.viewer?.navigation?.onNext).toBeUndefined();

  // Cleared, the whole list is walked again.
  act(() => latest.git.view.changeView.onFilterChange(""));
  expect(latest.git.viewer?.navigation?.onPrevious).toBeDefined();
});

it("drops the filter text with the field, so no filter out of sight narrows a list", () => {
  let latest!: ReturnType<typeof useGitReview>;
  const Probe = () => {
    latest = useGitReview("p", useProjectBrowserState("p"), true, undefined, { worktree: useFileFilterState(), compare: useFileFilterState() });
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  roots.push(root);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <Probe />
      </DaemonProvider>,
    ),
  );
  expect(latest.view.changeView.filterOpen).toBe(false);
  act(() => latest.view.changeView.onFilterOpenChange(true));
  act(() => latest.view.changeView.onFilterChange("alpha"));
  act(() => latest.view.changeView.onFilterOpenChange(false));
  expect(latest.view.changeView.filter).toBe("");
  act(() => latest.view.changeView.onFilterOpenChange(true));
  expect(latest.view.changeView.filter).toBe("");
});

type Tree = { id: string; branch: string | null; main?: boolean };
type Made = { name: string; session?: string };

/** The Git mode of project `id` with a selected session, as the app drives it: every change of the
 * session is noted (`noteSessionSwitch`), a pane mounted again keeps the project's state. */
function followHarness(id: string) {
  let latest!: { browser: ReturnType<typeof useProjectBrowserState>; git: ReturnType<typeof useGitReview> };
  let root!: Root;
  let selected: string | undefined;
  const Probe = ({ session, active }: { session: string | undefined; active: boolean }) => {
    const browser = useProjectBrowserState(id);
    latest = { browser, git: useGitReview(id, browser, active, session, { worktree: useFileFilterState(), compare: useFileFilterState() }) };
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
  const mount = (session: string | undefined, active = true) => {
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
  // Answers whatever the Git mode has asked for; the window's bound of two holds some requests
  // back until others are answered, hence the second pass.
  const settle = async (worktrees: Tree[], branches: Made[]) => {
    const replies: Partial<Record<RequestBody["type"], () => Event>> = {
      get_project_source: () => ({
        type: "project_source",
        source: {
          project: id,
          root: "/r",
          resolved_root: "/r",
          root_id: "r",
          git_error: null,
          git: {
            repository: "repo",
            common_dir: "/r/.git",
            worktree: "m",
            scope: "",
            worktrees: worktrees.map((w) => ({ id: w.id, root: `/${w.id}`, main: w.main ?? false, head: "c1", branch: w.branch, scope_present: true })),
          },
        },
      }),
      list_project_changes: () => ({ type: "project_changes", project: id, worktree: null, head: "c1", changes: [], complete: true }),
      list_project_branches: () => ({
        type: "project_branches",
        project: id,
        branches: branches.map((b) => ({
          name: b.name,
          commit: "c",
          origin: b.session ? { source: "ref_hook" as const, session: b.session, title: "t", created_at: 1 } : undefined,
        })),
        complete: true,
      }),
    };
    for (let pass = 0; pass < 2; pass++) {
      for (const [type, reply] of Object.entries(replies)) {
        if (pending.some((request) => request.body.type === type)) await answer(type as RequestBody["type"], reply);
      }
    }
  };
  const selectors = () => ({ worktree: latest.browser.worktree, left: latest.browser.left, right: latest.browser.right });
  return { mount, select, show, settle, selectors, get latest() { return latest; }, unmount: () => act(() => root.unmount()) };
}
const MAIN: Tree = { id: "m", branch: "main", main: true };
const TOPIC: Tree = { id: "w", branch: "topic" };
const NONE = { worktree: undefined, left: undefined, right: undefined };

it("sets the selectors from the selected session's branch once it is known, unless the user chose first", async () => {
  const h = followHarness("f1");
  h.mount(undefined);
  h.show("s1");
  await h.settle([MAIN, TOPIC], [{ name: "main" }]);
  expect(h.selectors()).toEqual(NONE);

  // The attribution arrives in a later list: set.
  act(() => void h.latest.git.refresh());
  await h.settle([MAIN, TOPIC], [{ name: "main" }, { name: "topic", session: "s1" }]);
  expect(h.selectors()).toEqual({ worktree: "w", left: "main", right: "topic" });

  // Another session with no branch restores the defaults; a choice made since settles it.
  h.show("s2");
  await h.settle([MAIN, TOPIC], [{ name: "main" }, { name: "topic", session: "s1" }]);
  expect(h.selectors()).toEqual(NONE);
  act(() => h.latest.git.view.compare.onBranchesChange("main", undefined));
  act(() => void h.latest.git.refresh());
  await h.settle([MAIN, TOPIC], [{ name: "topic", session: "s2" }]);
  expect(h.selectors()).toMatchObject({ left: "main", right: undefined });
});

it("sets the selectors again on switching between two sessions that each created a branch", async () => {
  const h = followHarness("f2");
  const branches = [{ name: "main" }, { name: "topic", session: "s1" }, { name: "other", session: "s2" }];
  h.mount(undefined);
  h.show("s1");
  await h.settle([MAIN, TOPIC], branches);
  expect(h.selectors()).toEqual({ worktree: "w", left: "main", right: "topic" });
  h.show("s2");
  await h.settle([MAIN, TOPIC], branches);
  expect(h.selectors()).toEqual({ worktree: undefined, left: "main", right: "other" });
});

it("asks again for a worktree no listed one is, and sets it from the list that answers", async () => {
  const h = followHarness("f3");
  h.mount(undefined);
  h.show("s1");
  const branches = [{ name: "main" }, { name: "topic", session: "s1" }];
  await h.settle([MAIN], branches);
  // The list read so far has no worktree for the branch: the comparison is set, the worktree waits
  // for the list asked for again.
  expect(h.selectors()).toEqual({ worktree: undefined, left: "main", right: "topic" });
  expect(pending.some((request) => request.body.type === "get_project_source")).toBe(true);
  await h.settle([MAIN, TOPIC], branches);
  expect(h.selectors()).toEqual({ worktree: "w", left: "main", right: "topic" });
});

it("weighs only what was read after the selection, asking again for what a pane held", async () => {
  const h = followHarness("f4");
  const branches = [{ name: "main" }, { name: "topic", session: "s1" }, { name: "other", session: "s2" }];
  h.mount(undefined);
  h.show("s1");
  await h.settle([MAIN, TOPIC], branches);
  expect(h.selectors().right).toBe("topic");
  // Selected while the Git mode is off screen: the lists held name a branch for s2, but are from
  // before the selection and are not weighed.
  h.show("s2", false);
  expect(h.selectors().right).toBe("topic");
  expect(pending).toHaveLength(0);
  h.show("s2", true);
  expect(h.selectors().right).toBe("topic");
  expect(pending.map((request) => request.body.type)).toEqual(expect.arrayContaining(["get_project_source", "list_project_branches"]));
  await h.settle([MAIN, TOPIC], branches);
  expect(h.selectors().right).toBe("other");
});

it("sets the selectors again on a new selection of the session, but not on a pane mounted under the same one", async () => {
  const h = followHarness("f5");
  const branches = [{ name: "main" }, { name: "topic", session: "s1" }];
  h.mount(undefined);
  h.show("s1");
  await h.settle([MAIN, TOPIC], branches);
  act(() => h.latest.git.view.compare.onBranchesChange("topic", "main"));

  // The pane goes and comes back with the selection unchanged: the choice stands.
  h.unmount();
  h.mount("s1");
  await h.settle([MAIN, TOPIC], branches);
  expect(h.selectors()).toMatchObject({ left: "topic", right: "main" });

  // Another selection in between, the pane away: the session is a new switch.
  h.unmount();
  h.select("other");
  h.select("s1");
  h.mount("s1");
  await h.settle([MAIN, TOPIC], branches);
  expect(h.selectors()).toEqual({ worktree: "w", left: "main", right: "topic" });
});

it("leaves the selectors as they are for a session that is not the project's", async () => {
  const h = followHarness("f6");
  h.mount(undefined);
  h.show("s1");
  await h.settle([MAIN, TOPIC], [{ name: "main" }, { name: "topic", session: "s1" }]);
  const before = h.selectors();
  h.show(undefined);
  expect(pending).toHaveLength(0);
  expect(h.selectors()).toEqual(before);
});

it("sets the worktree of a branch that shows up later in a worktree made since, on a clock that stands still", async () => {
  const h = followHarness("f7");
  h.mount(undefined);
  h.show("s1");
  await h.settle([MAIN], [{ name: "main" }]);
  expect(h.selectors()).toEqual(NONE);

  // WebKit rounds `performance.now()` to the millisecond: what one handler does reads one time.
  const clock = vi.spyOn(performance, "now").mockReturnValue(performance.now() + 1000);
  try {
    // The session makes a worktree on a new branch; the branch list's own refresh, on coming back
    // to the window, is the first to name it, beside a worktree list read before it was made.
    const later = vi.spyOn(Date, "now").mockReturnValue(Date.now() + 60_000);
    act(() => void window.dispatchEvent(new Event("focus")));
    later.mockRestore();
    await answer("list_project_branches", () => ({
      type: "project_branches",
      project: "f7",
      branches: [
        { name: "main", commit: "c" },
        { name: "topic", commit: "c", origin: { source: "ref_hook", session: "s1", title: "t", created_at: 1 } },
      ],
      complete: true,
    }));
    expect(h.selectors()).toEqual({ worktree: undefined, left: "main", right: "topic" });
    await h.settle([MAIN, TOPIC], [{ name: "main" }, { name: "topic", session: "s1" }]);
    expect(h.selectors()).toEqual({ worktree: "w", left: "main", right: "topic" });
  } finally {
    clock.mockRestore();
  }
});
