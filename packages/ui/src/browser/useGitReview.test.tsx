// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it } from "vitest";

import type { ChangeEntry, Event, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { useProjectBrowserState } from "./browserState";
import { changeLayout } from "./changeLayout";
import { directoryKey } from "./changes";
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
    changes: paths.map((path) => committed(path, right)),
    complete: true,
  }) satisfies Event;
const read = (body: RequestBody): Event => {
  if (body.type !== "read_project_comparison_change") throw new Error(body.type);
  return { type: "project_comparison_change", project: "p", left: body.left, right: body.right, old: { state: "absent" }, new: { state: "absent" }, patch: null };
};

it("follows a comparison made again with the change still in it, and closes on one without it", async () => {
  let latest!: { browser: ReturnType<typeof useProjectBrowserState>; git: ReturnType<typeof useGitReview> };
  const Probe = () => {
    const browser = useProjectBrowserState("p");
    latest = { browser, git: useGitReview("p", browser, true) };
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
    latest = { browser, git: useGitReview("p", browser, true) };
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
    latest = { browser, git: useGitReview("p", browser, true) };
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
    changeLayout.set("tree");
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
    act(() => latest.browser.setCollapsedChangeDirs(new Set([directoryKey("unstaged", "a")])));
    expect(latest.git.viewer?.navigation?.onPrevious).toBeUndefined();
    expect(latest.git.viewer?.navigation?.onNext).toBeUndefined();

    // The flat list has every change on screen whatever was folded in the tree.
    act(() => changeLayout.set("flat"));
    expect(latest.git.viewer?.navigation?.onPrevious).toBeDefined();
  } finally {
    act(() => {
      changeLayout.set("flat");
      latest.browser.setCollapsedChangeDirs(new Set());
    });
  }
});

it("moves the viewer through the rows a file name filter leaves", async () => {
  let latest!: { browser: ReturnType<typeof useProjectBrowserState>; git: ReturnType<typeof useGitReview> };
  const Probe = () => {
    const browser = useProjectBrowserState("p");
    latest = { browser, git: useGitReview("p", browser, true) };
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
