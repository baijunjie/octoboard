// @vitest-environment jsdom
import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { PlatformProvider } from "../platform/react";
import type { Event, GitStatus, Project, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { useProjectBrowserState } from "./browserState";
import { ProjectBrowser } from "./ProjectBrowser";

// These checks exercise the browser's reads and navigation; the viewer's rendering has its own suite.
vi.mock("../viewer/FileViewer", () => ({ FileViewer: () => null }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Pending = { body: RequestBody; resolve: (event: Event) => void; reject: (err: Error) => void };
const pending: Pending[] = [];
const roots: Root[] = [];

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
});

afterEach(async () => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  await act(async () => pending.splice(0).forEach((request) => request.reject(new Error("test over"))));
  vi.useRealTimers();
  vi.unstubAllGlobals();
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

/** Answers the request of `type` still out with `event`. */
async function answer(type: RequestBody["type"], event: Event): Promise<void> {
  const index = pending.findIndex((request) => request.body.type === type);
  expect(index, `a ${type} request out among ${pending.map((p) => p.body.type).join(", ")}`).not.toBe(-1);
  const [request] = pending.splice(index, 1);
  await act(async () => request.resolve(event));
}

const dir = (project: string): Event => ({ type: "project_dir", project, worktree: null, path: "", root_id: "r", entries: [], complete: true });
const source = (project: string): Event => ({
  type: "project_source",
  source: {
    project,
    root: "/r",
    resolved_root: "/r",
    root_id: "r",
    git_error: null,
    git: {
      repository: "repo",
      common_dir: "/r/.git",
      worktree: "w",
      scope: "",
      worktrees: [{ id: "w", root: "/r", main: true, head: "c1", branch: "main", scope_present: true }],
    },
  },
});
const branches = (project: string): Event => ({ type: "project_branches", project, branches: [], complete: true });
const changes = (project: string): Event => ({ type: "project_changes", project, worktree: null, head: null, changes: [], complete: true });

/** Mounts the browser of project `id` in `mode` and returns its Refresh button's icon. */
async function mount(id: string, mode: "files" | "git"): Promise<() => Element> {
  const Open = () => {
    const browser = useProjectBrowserState(id);
    useEffect(() => browser.setMode(mode), []);
    return <ProjectBrowser project={{ id, name: id } as Project} layout={{ open: true, width: 300 }} active selectedSessionId={undefined} />;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  roots.push(root);
  await act(async () =>
    root.render(
      <DaemonProvider value={daemon}>
        <PlatformProvider value={{ kind: "browser" }}><Open /></PlatformProvider>
      </DaemonProvider>,
    ),
  );
  return () => document.querySelector(".lucide-refresh-cw")!;
}
const press = (icon: Element) => act(async () => void icon.closest("button")!.click());
const spinning = (icon: Element) => icon.classList.contains("motion-safe:animate-spin-slow");
const wait = (ms: number) => act(async () => void vi.advanceTimersByTime(ms));

it("spins the Files refresh icon while the listings it asked for are out, and for a moment at least", async () => {
  const icon = await mount("files-p", "files");
  await answer("list_project_dir", dir("files-p"));
  expect(spinning(icon())).toBe(false);

  await press(icon());
  expect(spinning(icon())).toBe(true);

  // Answered at once, it keeps spinning until the minimum has passed, then stops.
  await answer("list_project_dir", dir("files-p"));
  expect(spinning(icon())).toBe(true);
  await wait(1100);
  expect(spinning(icon())).toBe(false);

  // Slower than the minimum, it spins until the listing is answered.
  await press(icon());
  await wait(5000);
  expect(spinning(icon())).toBe(true);
  await answer("list_project_dir", dir("files-p"));
  expect(spinning(icon())).toBe(false);
});

it("does not spin the icon for the periodic refresh", async () => {
  const icon = await mount("files-bg", "files");
  await answer("list_project_dir", dir("files-bg"));
  await wait(10_000);
  expect(pending.some((request) => request.body.type === "list_project_dir")).toBe(true);
  expect(spinning(icon())).toBe(false);
  await answer("list_project_dir", dir("files-bg"));
  expect(spinning(icon())).toBe(false);
});

it("spins the Git refresh icon until the worktrees and the changes it asked for are answered", async () => {
  const icon = await mount("git-p", "git");
  await answer("get_project_source", source("git-p"));
  await answer("list_project_branches", branches("git-p"));
  await answer("list_project_changes", changes("git-p"));
  expect(spinning(icon())).toBe(false);

  await press(icon());
  expect(spinning(icon())).toBe(true);
  await wait(1100);
  await answer("get_project_source", source("git-p"));
  await answer("list_project_branches", branches("git-p"));
  expect(spinning(icon())).toBe(true);
  await answer("list_project_changes", changes("git-p"));
  expect(spinning(icon())).toBe(false);
});

it("stops the Files icon after the minimum when the listing it asked for fails", async () => {
  const icon = await mount("files-fail", "files");
  await answer("list_project_dir", dir("files-fail"));

  await press(icon());
  const index = pending.findIndex((request) => request.body.type === "list_project_dir");
  const [request] = pending.splice(index, 1);
  await act(async () => request.reject(new Error("no answer")));
  expect(spinning(icon())).toBe(true);
  await wait(1100);
  expect(spinning(icon())).toBe(false);
});

it("stops the Git icon after the minimum when the source request it asked for is rejected", async () => {
  const icon = await mount("git-fail", "git");
  await answer("get_project_source", source("git-fail"));
  await answer("list_project_branches", branches("git-fail"));
  await answer("list_project_changes", changes("git-fail"));

  await press(icon());
  const index = pending.findIndex((request) => request.body.type === "get_project_source");
  const [request] = pending.splice(index, 1);
  await act(async () => request.reject(new Error("rejected")));
  await answer("list_project_branches", branches("git-fail"));
  await answer("list_project_changes", changes("git-fail"));
  expect(spinning(icon())).toBe(true);
  await wait(1100);
  expect(spinning(icon())).toBe(false);
});

it("keeps spinning after two quick presses until the last refresh settles", async () => {
  const icon = await mount("files-twice", "files");
  await answer("list_project_dir", dir("files-twice"));

  await press(icon());
  await wait(500);
  await press(icon());
  await answer("list_project_dir", dir("files-twice"));
  await wait(1100);
  // The first press is done, the second one is still out.
  expect(spinning(icon())).toBe(true);
  await answer("list_project_dir", dir("files-twice"));
  expect(spinning(icon())).toBe(false);
});

it("keeps each Git view's filter field independently open or closed", async () => {
  await mount("git-filter", "git");
  await answer("get_project_source", source("git-filter"));
  await answer("list_project_changes", changes("git-filter"));
  const fields = () => document.querySelectorAll("input[aria-label='File name contains']");
  const toggle = (label: string) => document.querySelector<HTMLElement>(`button[aria-label='${label}']`)!;
  expect(fields()).toHaveLength(0);

  await act(async () => toggle("Filter by file name").click());
  expect(fields()).toHaveLength(1);
  expect(toggle("Hide the filter").getAttribute("aria-expanded")).toBe("true");

  await act(async () => document.querySelector<HTMLElement>('[role="tab"][data-key="compare"]')!.click());
  expect(fields()).toHaveLength(0);
  await act(async () => toggle("Filter by file name").click());
  expect(fields()).toHaveLength(1);
  await act(async () => document.querySelector<HTMLElement>('[role="tab"][data-key="worktree"]')!.click());
  expect(fields()).toHaveLength(1);
  await act(async () => toggle("Hide the filter").click());
  expect(fields()).toHaveLength(0);
});

// A live region inserted already holding its text is not announced (WebKit, VoiceOver), so each
// status region is on the page empty and says its text a moment later, then follows the loading.
// The spinner is a `role=status` of HeroUI's, hidden from assistive technology here.
const statuses = () => [...document.querySelectorAll("[role=status]:not([aria-hidden])")];

it("says the files are loading from a status region that outlives the loading", async () => {
  await mount("files-live", "files");
  const [region] = statuses();
  expect(statuses()).toHaveLength(1);
  expect(region?.textContent).toBe("");
  await wait(150);
  expect(region?.textContent).toBe("Loading files…");

  await answer("list_project_dir", dir("files-live"));
  expect(region?.isConnected).toBe(true);
  expect(region?.textContent).toBe("");
});

it("says the changes are loading once, from one status region, across the source and the list", async () => {
  await mount("git-live", "git");
  const [region] = statuses();
  expect(statuses()).toHaveLength(1);
  expect(region?.textContent).toBe("");
  await wait(150);
  expect(region?.textContent).toBe("Loading changes…");

  // The source arrives and the change list starts loading: the same words, in the same region.
  await answer("get_project_source", source("git-live"));
  expect(statuses()).toEqual([region]);
  expect(region?.textContent).toBe("Loading changes…");

  await answer("list_project_changes", changes("git-live"));
  expect(statuses()).toEqual([region]);
  expect(region?.textContent).toBe("");
});


it.each(["files", "git"] as const)("refreshes the visible %s pane when a switched branch is published", async (mode) => {
  const project = `switch-${mode}`;
  const publish = async (branch: string) => act(async () => {
    daemon.store.setState((state) => ({ gitStatuses: new Map(state.gitStatuses).set(project, {
      project, branch, detached: false, repository: true, ahead: 0, behind: 0, activity: "idle", error: null,
    } as GitStatus) }));
  });
  await publish("main");
  await mount(project, mode);
  if (mode === "files") await answer("list_project_dir", dir(project));
  else {
    await answer("get_project_source", source(project));
    await answer("list_project_changes", changes(project));
    await answer("list_project_branches", branches(project));
  }
  if (pending.some((request) => request.body.type === "list_project_dir")) await answer("list_project_dir", dir(project));
  expect(pending).toHaveLength(0);
  await publish("feature");
  const types = pending.map((request) => request.body.type);
  expect(types).toContain(mode === "files" ? "list_project_dir" : "get_project_source");
  expect(types).not.toContain("pull_project_branch");
  expect(types).not.toContain("refresh_git_status");
});

it("opens the Files filter, narrows rows, and clears it when hidden", async () => {
  await mount("files-filter", "files");
  await answer("list_project_dir", {
    ...dir("files-filter"),
    entries: ["alpha.md", "beta.ts"].map((name) => ({ name, kind: "file", size: 1, version: "v1", target: null })),
  } as Event);
  const button = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
  await act(async () => button("Show as flat list").click());
  await act(async () => button("Filter by file name").click());
  const input = document.querySelector<HTMLInputElement>('input[aria-label="File name contains"]')!;
  expect(document.activeElement).toBe(input);
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "ALPHA");
    input.dispatchEvent(new window.Event("input", { bubbles: true }));
  });
  await wait(250);
  expect(pending.find((request) => request.body.type === "search_project_files")?.body).toMatchObject({ query: "ALPHA", project: "files-filter" });
  await answer("search_project_files", { type: "project_file_matches", project: "files-filter", query: "ALPHA", paths: ["alpha.md", "nested/alpha.md"], complete: true });
  expect(document.querySelector('[data-key="path:nested/alpha.md"]')).not.toBeNull();
  expect(document.querySelector('[data-key="path:nested/alpha.md"]')?.textContent).toContain("nested/alpha.md");
  expect(document.querySelector('[data-key="path:beta.ts"]')).toBeNull();
  expect([...document.querySelectorAll('[role="row"][data-key]')].map((row) => row.getAttribute("data-key"))).toEqual(["path:nested/alpha.md", "path:alpha.md"]);
  await act(async () => button("Hide the filter").click());
  expect(document.querySelector('input[aria-label="File name contains"]')).toBeNull();
  await wait(250);
  await answer("search_project_files", { type: "project_file_matches", project: "files-filter", query: "", paths: ["alpha.md", "beta.ts"], complete: true });
  expect(document.querySelector('[data-key="path:beta.ts"]')).not.toBeNull();
  expect(document.activeElement).toBe(button("Filter by file name"));
});

it("debounces whole-project searches and ignores an older query's reply", async () => {
  await mount("search-races", "files");
  await answer("list_project_dir", dir("search-races"));
  await act(async () => document.querySelector<HTMLButtonElement>('button[aria-label="Filter by file name"]')!.click());
  const input = document.querySelector<HTMLInputElement>('input[aria-label="File name contains"]')!;
  const type = (value: string) => act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new window.Event("input", { bubbles: true }));
  });
  await type("old");
  await wait(100);
  expect(pending.some((request) => request.body.type === "search_project_files")).toBe(false);
  await wait(150);
  await type("new");
  await answer("search_project_files", { type: "project_file_matches", project: "search-races", query: "old", paths: ["old.ts"], complete: true });
  expect(document.querySelector('[data-key="path:old.ts"]')).toBeNull();
  await wait(250);
  await answer("search_project_files", { type: "project_file_matches", project: "search-races", query: "new", paths: [], complete: false });
  expect(document.body.textContent).toContain("No matching files");
  expect(document.body.textContent).toContain("The search is incomplete");
});

it("lists every ancestor of an unopened search match so its viewer can notice file changes", async () => {
  await mount("search-viewer", "files");
  await answer("list_project_dir", { ...dir("search-viewer"), entries: [{ name: "src", kind: "directory", size: null, version: null, target: null }] } as Event);
  await act(async () => document.querySelector<HTMLButtonElement>('button[aria-label="Filter by file name"]')!.click());
  const input = document.querySelector<HTMLInputElement>('input[aria-label="File name contains"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "target");
    input.dispatchEvent(new window.Event("input", { bubbles: true }));
  });
  await wait(250);
  await answer("search_project_files", { type: "project_file_matches", project: "search-viewer", query: "target", paths: ["src/nested/target.txt"], complete: true });
  await act(async () => document.querySelector<HTMLElement>('[data-key="path:src/nested/target.txt"]')!.click());
  expect(pending.some((request) => request.body.type === "read_project_file")).toBe(true);
  expect(pending.filter((request) => request.body.type === "list_project_dir").map((request) => (request.body as { path: string }).path)).toEqual(["src", "src/nested"]);
  await answer("list_project_dir", { ...dir("search-viewer"), path: "src", entries: [{ name: "nested", kind: "directory", size: null, version: null, target: null }] } as Event);
  await answer("list_project_dir", { ...dir("search-viewer"), path: "src/nested", entries: [{ name: "target.txt", kind: "file", size: 5, version: "v1", target: null }] } as Event);
  await answer("read_project_file", { type: "project_file", project: "search-viewer", worktree: null, path: "src/nested/target.txt", source: { kind: "live", root_id: "r", version: "v1" }, file: { size: 5, kind: "text", text: "hello", data: null, media_type: null } });
  await wait(10_000);
  expect(pending.filter((request) => request.body.type === "list_project_dir").map((request) => (request.body as { path: string }).path)).toEqual(["", "src", "src/nested"]);
  await answer("list_project_dir", { ...dir("search-viewer"), entries: [{ name: "src", kind: "directory", size: null, version: null, target: null }] } as Event);
  await answer("list_project_dir", { ...dir("search-viewer"), path: "src", entries: [{ name: "nested", kind: "directory", size: null, version: null, target: null }] } as Event);
  await answer("list_project_dir", { ...dir("search-viewer"), path: "src/nested", entries: [{ name: "target.txt", kind: "file", size: 5, version: "v2", target: null }] } as Event);
  expect(pending.some((request) => request.body.type === "read_project_file")).toBe(true);
});

it("remembers independent keywords, open states and layouts for all three tabs", async () => {
  await mount("independent-filter", "files");
  await answer("list_project_dir", dir("independent-filter"));
  const toggle = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
  const field = () => document.querySelector<HTMLInputElement>('input[aria-label="File name contains"]')!;
  const tab = (key: string) => act(async () => {
    const element = document.querySelector<HTMLElement>(`[role="tab"][data-key="${key}"]`)!;
    element.focus();
    element.click();
  });
  const type = (value: string) => act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(field(), value);
    field().dispatchEvent(new window.Event("input", { bubbles: true }));
  });
  await act(async () => toggle("Filter by file name").click());
  await type("server");
  await act(async () => toggle("Show as flat list").click());
  await tab("worktree");
  await answer("get_project_source", source("independent-filter"));
  await answer("list_project_changes", changes("independent-filter"));
  expect(field()).toBeNull();
  await act(async () => toggle("Filter by file name").click());
  await type("alpha");
  await act(async () => toggle("Group by folder").click());
  await tab("compare");
  expect(field()).toBeNull();
  await act(async () => toggle("Filter by file name").click());
  await type("beta");
  expect(toggle("Group by folder").getAttribute("aria-pressed")).toBe("false");
  await tab("files");
  expect(field().value).toBe("server");
  expect(document.activeElement).toBe(document.querySelector('[role="tab"][data-key="files"]'));
  expect(toggle("Group by folder").getAttribute("aria-pressed")).toBe("false");
  await tab("worktree");
  expect(field().value).toBe("alpha");
  expect(toggle("Show as flat list").getAttribute("aria-pressed")).toBe("true");
  await act(async () => toggle("Hide the filter").click());
  await tab("compare");
  expect(field().value).toBe("beta");
  await tab("files");
  expect(field().value).toBe("server");
});

it("keeps Files result folders collapsed through tab and flat/tree switches", async () => {
  await mount("files-folds", "files");
  await answer("list_project_dir", dir("files-folds"));
  const button = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
  await act(async () => button("Filter by file name").click());
  const input = document.querySelector<HTMLInputElement>('input[aria-label="File name contains"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "target");
    input.dispatchEvent(new window.Event("input", { bubbles: true }));
  });
  await wait(250);
  await answer("search_project_files", { type: "project_file_matches", project: "files-folds", query: "target", paths: ["src/nested/target.txt"], complete: true });
  await act(async () => document.querySelector<HTMLElement>('[data-key="path:src"]')!.click());
  expect(document.querySelector('[data-key="path:src/nested/target.txt"]')).toBeNull();
  await act(async () => document.querySelector<HTMLElement>('[role="tab"][data-key="worktree"]')!.click());
  await act(async () => document.querySelector<HTMLElement>('[role="tab"][data-key="files"]')!.click());
  expect(document.querySelector('[data-key="path:src/nested/target.txt"]')).toBeNull();
  await act(async () => button("Show as flat list").click());
  expect(document.querySelector('[data-key="path:src/nested/target.txt"]')).not.toBeNull();
  await act(async () => button("Group by folder").click());
  expect(document.querySelector('[data-key="path:src/nested/target.txt"]')).toBeNull();
});

it("keeps focus in the header when a mouse layout switch removes the focused Files directory", async () => {
  await mount("files-layout-focus", "files");
  await answer("list_project_dir", { ...dir("files-layout-focus"), entries: [{ name: "src", kind: "directory", size: null, version: null, target: null }] } as Event);
  const directory = document.querySelector<HTMLElement>('[data-key="path:src"]')!;
  await act(async () => directory.focus());
  expect(document.activeElement).toBe(directory);
  await act(async () => document.querySelector<HTMLButtonElement>('button[aria-label="Show as flat list"]')!.click());
  expect(document.activeElement).toBe(document.querySelector('button[aria-label="Refresh"]'));
});
