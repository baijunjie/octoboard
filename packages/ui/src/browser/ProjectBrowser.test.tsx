// @vitest-environment jsdom
import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { isolate } from "../i18n/isolate";
import { DaemonRequestError } from "../daemon-client";
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
    return <ProjectBrowser project={{ id, name: id } as Project} active selectedSessionId={undefined} />;
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

it("keeps each selected tab linked to its mounted panel across repeated switches", async () => {
  await mount("panel-identity", "files");
  await answer("list_project_dir", dir("panel-identity"));
  const tab = (key: string) => document.querySelector<HTMLElement>(`[role="tab"][data-key="${key}"]`)!;
  const linkedPanel = (key: string) => {
    const selected = tab(key);
    expect(selected.getAttribute("aria-selected")).toBe("true");
    expect(document.querySelectorAll('[role="tab"][aria-selected="true"]')).toHaveLength(1);
    const panel = document.getElementById(selected.getAttribute("aria-controls")!);
    expect(panel?.isConnected).toBe(true);
    expect(panel?.getAttribute("role")).toBe("tabpanel");
    expect(panel?.getAttribute("aria-labelledby")).toBe(selected.id);
    expect(panel?.closest('[hidden], [inert], [aria-hidden="true"]')).toBeNull();
    expect(document.querySelectorAll('[role="tabpanel"]')).toHaveLength(1);
    return panel;
  };
  linkedPanel("files");
  await act(async () => tab("files").focus());
  for (const [key, arrow] of [["changes", "ArrowRight"], ["files", "ArrowLeft"], ["changes", "ArrowRight"]]) {
    const previous = document.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')!;
    await act(async () => previous.dispatchEvent(new KeyboardEvent("keydown", { key: arrow, bubbles: true })));
    expect(document.activeElement).toBe(tab(key));
    linkedPanel(key);
  }
});

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

it("keeps the Changes tab's filter field open or closed apart from the Files tab's", async () => {
  await mount("git-filter", "git");
  await answer("get_project_source", source("git-filter"));
  await answer("list_project_changes", changes("git-filter"));
  const fields = () => document.querySelectorAll("input[aria-label='File name contains']");
  const toggle = (label: string) => document.querySelector<HTMLElement>(`button[aria-label='${label}']`)!;
  expect(fields()).toHaveLength(0);

  await act(async () => toggle("Filter by file name").click());
  expect(fields()).toHaveLength(1);
  expect(toggle("Hide the filter").getAttribute("aria-expanded")).toBe("true");

  await act(async () => document.querySelector<HTMLElement>('[role="tab"][data-key="files"]')!.click());
  expect(fields()).toHaveLength(0);
  await act(async () => document.querySelector<HTMLElement>('[role="tab"][data-key="changes"]')!.click());
  expect(fields()).toHaveLength(1);
  await act(async () => toggle("Hide the filter").click());
  expect(fields()).toHaveLength(0);
});

it("offers grouping by commit on the Changes tab alone, its name saying what pressing does", async () => {
  await mount("git-by-commit", "git");
  await answer("get_project_source", source("git-by-commit"));
  const toggle = (label: string) => document.querySelector<HTMLElement>(`button[aria-label='${label}']`);
  expect(toggle("Group by commit")?.getAttribute("aria-pressed")).toBe("false");
  await act(async () => toggle("Group by commit")!.click());
  expect(toggle("Show commits together")?.getAttribute("aria-pressed")).toBe("true");
  await act(async () => document.querySelector<HTMLElement>('[role="tab"][data-key="files"]')!.click());
  expect(toggle("Show commits together")).toBeNull();
  expect(toggle("Group by commit")).toBeNull();
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
  // Results are in their folders, as the tree is without a filter.
  expect(document.querySelector('[data-key="path:beta.ts"]')).toBeNull();
  expect([...document.querySelectorAll('[role="row"][data-key]')].map((row) => row.getAttribute("data-key"))).toEqual(["path:nested", "path:nested/alpha.md", "path:alpha.md"]);
  await act(async () => button("Hide the filter").click());
  expect(document.querySelector('input[aria-label="File name contains"]')).toBeNull();
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

it("remembers independent keywords and open states for both tabs, and the Changes tab's layout", async () => {
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
  // The Files tree is always by folder: there is no layout to choose.
  expect(toggle("Show as flat list")).toBeNull();
  expect(toggle("Group by folder")).toBeNull();
  await tab("changes");
  await answer("get_project_source", source("independent-filter"));
  await answer("list_project_changes", changes("independent-filter"));
  expect(field()).toBeNull();
  await act(async () => toggle("Filter by file name").click());
  await type("alpha");
  await act(async () => toggle("Group by folder").click());
  await tab("files");
  expect(field().value).toBe("server");
  expect(document.activeElement).toBe(document.querySelector('[role="tab"][data-key="files"]'));
  expect(toggle("Group by folder")).toBeNull();
  await tab("changes");
  expect(field().value).toBe("alpha");
  expect(toggle("Show as flat list").getAttribute("aria-pressed")).toBe("true");
  await act(async () => toggle("Hide the filter").click());
  await tab("files");
  expect(field().value).toBe("server");
});

it("keeps Files result folders collapsed through tab switches", async () => {
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
  await act(async () => document.querySelector<HTMLElement>('[role="tab"][data-key="changes"]')!.click());
  await act(async () => document.querySelector<HTMLElement>('[role="tab"][data-key="files"]')!.click());
  expect(document.querySelector('[data-key="path:src/nested/target.txt"]')).toBeNull();
});

it("puts the Files / Changes switch right after the project name, and only icon buttons at the end of the header", async () => {
  await mount("header-order", "files");
  await answer("list_project_dir", dir("header-order"));
  const header = document.querySelector<HTMLElement>("h2")!.closest(".groove-b")!;
  const follows = (a: Element, b: Element) => Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
  const tabs = header.querySelector('[role="tablist"]')!;
  const refresh = header.querySelector('button[aria-label="Refresh"]')!;
  expect(follows(header.querySelector("h2")!, tabs)).toBe(true);
  expect(follows(tabs, refresh)).toBe(true);
  // The group pushed to the end holds the icon buttons alone.
  const end = [...header.children].find((child) => child.contains(refresh))!;
  expect(end.contains(tabs)).toBe(false);
  expect(end.querySelector('[role="tab"]')).toBeNull();
});

it("announces a files failure that appears after the panel painted, but not one already there when the tab is switched back to", async () => {
  await mount("files-fail-announce", "files");
  const index = pending.findIndex((request) => request.body.type === "list_project_dir");
  const [request] = pending.splice(index, 1);
  await act(async () => request.reject(new DaemonRequestError("no access", "unexpected_error", {})));
  expect(document.querySelector('[data-slot="empty-state"]')).not.toBeNull();
  expect(document.querySelector("[role=alert]")?.textContent).toContain("Something unexpected went wrong.");
  expect(document.querySelector("[role=alert]")?.textContent).not.toContain("no access");

  const tab = (key: string) => document.querySelector<HTMLElement>(`[role="tab"][data-key="${key}"]`)!;
  await act(async () => tab("changes").click());
  await act(async () => tab("files").click());
  expect(document.querySelector('[data-slot="empty-state"]')?.textContent).toContain("Something unexpected went wrong.");
  expect(document.querySelector("[role=alert]")).toBeNull();
});

const labelled = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
const typeKeyword = (value: string) => act(async () => {
  const input = document.querySelector<HTMLInputElement>('input[aria-label="File name contains"]')!;
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
  input.dispatchEvent(new window.Event("input", { bubbles: true }));
});
const rowKeys = () => [...document.querySelectorAll('[role="row"][data-key]')].map((row) => row.getAttribute("data-key"));
const folder = (name: string) => ({ name, kind: "directory", size: null, version: null, target: null });

it("folds every expanded folder from the header, and is disabled while none is open", async () => {
  await mount("collapse-all", "files");
  await answer("list_project_dir", { ...dir("collapse-all"), entries: [folder("src"), folder("docs")] } as Event);
  expect(labelled("Collapse all folders").disabled).toBe(true);
  await act(async () => document.querySelector<HTMLElement>('[data-key="path:src"]')!.click());
  await answer("list_project_dir", { ...dir("collapse-all"), path: "src", entries: [folder("nested")] } as Event);
  await act(async () => document.querySelector<HTMLElement>('[data-key="path:src/nested"]')!.click());
  await answer("list_project_dir", { ...dir("collapse-all"), path: "src/nested", entries: [{ name: "a.ts", kind: "file", size: 1, version: "v", target: null }] } as Event);
  expect(rowKeys()).toEqual(["path:docs", "path:src", "path:src/nested", "path:src/nested/a.ts"]);
  expect(labelled("Collapse all folders").disabled).toBe(false);
  // Focus on a row that goes with the folding moves to the top-level folder it was under.
  await act(async () => document.querySelector<HTMLElement>('[data-key="path:src/nested/a.ts"]')!.focus());
  await act(async () => labelled("Collapse all folders").click());
  await act(async () => void vi.advanceTimersByTime(100));
  expect(rowKeys()).toEqual(["path:docs", "path:src"]);
  expect(document.activeElement).toBe(document.querySelector('[data-key="path:src"]'));
  expect(labelled("Collapse all folders").disabled).toBe(true);
});

it("keeps the Collapse all button disabled when only folders hidden under a folded one stay expanded", async () => {
  await mount("collapse-hidden", "files");
  await answer("list_project_dir", { ...dir("collapse-hidden"), entries: [folder("src")] } as Event);
  await act(async () => document.querySelector<HTMLElement>('[data-key="path:src"]')!.click());
  await answer("list_project_dir", { ...dir("collapse-hidden"), path: "src", entries: [folder("nested")] } as Event);
  await act(async () => document.querySelector<HTMLElement>('[data-key="path:src/nested"]')!.click());
  await answer("list_project_dir", { ...dir("collapse-hidden"), path: "src/nested", entries: [{ name: "a.ts", kind: "file", size: 1, version: "v", target: null }] } as Event);
  expect(labelled("Collapse all folders").disabled).toBe(false);
  // Folding the parent leaves `src/nested` expanded in the state, but nothing on screen is open.
  await act(async () => document.querySelector<HTMLElement>('[data-key="path:src"]')!.click());
  expect(rowKeys()).toEqual(["path:src"]);
  expect(labelled("Collapse all folders").disabled).toBe(true);
});

it("hands focus to Refresh when Collapse all disables itself under a keyboard press", async () => {
  await mount("collapse-focus", "files");
  await answer("list_project_dir", { ...dir("collapse-focus"), entries: [folder("src")] } as Event);
  await act(async () => document.querySelector<HTMLElement>('[data-key="path:src"]')!.click());
  await answer("list_project_dir", { ...dir("collapse-focus"), path: "src", entries: [{ name: "a.ts", kind: "file", size: 1, version: "v", target: null }] } as Event);
  const button = labelled("Collapse all folders");
  await act(async () => button.focus());
  // WebKit blurs a control as it is disabled, before the render's effect: the `focusout` arrives
  // with the button already disabled and focus on `<body>`. jsdom never blurs a disabled element,
  // so both are reported by hand, the `focusout` as the attribute lands.
  const active = vi.spyOn(document, "activeElement", "get").mockReturnValue(document.body);
  const blur = new MutationObserver(() => button.dispatchEvent(new FocusEvent("focusout", { bubbles: true, relatedTarget: null })));
  blur.observe(button, { attributeFilter: ["disabled"] });
  try {
    await act(async () => button.click());
  } finally {
    blur.disconnect();
    active.mockRestore();
  }
  expect(button.disabled).toBe(true);
  expect(document.activeElement).toBe(labelled("Refresh"));
});

it("folds every folder of the filter's results, and keeps the button out of the Changes tab", async () => {
  await mount("collapse-results", "files");
  await answer("list_project_dir", dir("collapse-results"));
  await act(async () => labelled("Filter by file name").click());
  await typeKeyword("target");
  await wait(250);
  await answer("search_project_files", { type: "project_file_matches", project: "collapse-results", query: "target", paths: ["src/nested/target.txt", "docs/target.md"], complete: true });
  expect(rowKeys()).toEqual(["path:docs", "path:docs/target.md", "path:src", "path:src/nested", "path:src/nested/target.txt"]);
  await act(async () => labelled("Collapse all folders").click());
  expect(rowKeys()).toEqual(["path:docs", "path:src"]);
  expect(labelled("Collapse all folders").disabled).toBe(true);
  await act(async () => document.querySelector<HTMLElement>('[data-key="path:src"]')!.click());
  expect(labelled("Collapse all folders").disabled).toBe(false);
  await act(async () => document.querySelector<HTMLElement>('[role="tab"][data-key="changes"]')!.click());
  expect(labelled("Collapse all folders")).toBeNull();
});

it("keeps Collapse all disabled over the filter's results when only a folder under a folded one is expanded", async () => {
  await mount("collapse-results-hidden", "files");
  await answer("list_project_dir", dir("collapse-results-hidden"));
  await act(async () => labelled("Filter by file name").click());
  await typeKeyword("target");
  await wait(250);
  await answer("search_project_files", { type: "project_file_matches", project: "collapse-results-hidden", query: "target", paths: ["src/nested/target.txt"], complete: true });
  expect(labelled("Collapse all folders").disabled).toBe(false);
  await act(async () => document.querySelector<HTMLElement>('[data-key="path:src"]')!.click());
  expect(rowKeys()).toEqual(["path:src"]);
  expect(labelled("Collapse all folders").disabled).toBe(true);
});

it("leaves ignored files out of the search until the toggle beside the field includes them", async () => {
  await mount("include-ignored", "files");
  await answer("list_project_dir", dir("include-ignored"));
  expect(labelled("Include ignored files")).toBeNull();
  await act(async () => labelled("Filter by file name").click());
  const toggle = () => labelled("Include ignored files") ?? labelled("Exclude ignored files");
  expect(toggle().getAttribute("aria-label")).toBe("Include ignored files");
  expect(toggle().getAttribute("aria-pressed")).toBe("false");
  await typeKeyword("app");
  await wait(250);
  expect(pending.find((request) => request.body.type === "search_project_files")?.body).not.toHaveProperty("include_ignored");
  await answer("search_project_files", { type: "project_file_matches", project: "include-ignored", query: "app", paths: ["src/app.ts"], complete: true });
  // The toggle repeats the search at once, with no debounce to wait out.
  await act(async () => toggle().click());
  await wait(0);
  expect(toggle().getAttribute("aria-label")).toBe("Exclude ignored files");
  expect(toggle().getAttribute("aria-pressed")).toBe("true");
  expect(pending.find((request) => request.body.type === "search_project_files")?.body).toMatchObject({ query: "app", include_ignored: true });
  await answer("search_project_files", { type: "project_file_matches", project: "include-ignored", query: "app", paths: ["dist/app.js", "src/app.ts"], ignored: ["dist/app.js"], complete: true });
  const row = (path: string) => document.querySelector<HTMLElement>(`[data-key="path:${path}"]`)!;
  expect(row("dist/app.js").getAttribute("aria-label")).toBe(`${isolate("app.js")}, ignored`);
  expect(row("dist").getAttribute("aria-label")).toBe(`${isolate("dist")}, ignored`);
  expect(row("src/app.ts").getAttribute("aria-label")).toBe("app.ts");
  // The veil that dims a row is keyed off `data-ignored`, on the folder whose every result is ignored too and not on a row with any other.
  expect(row("dist/app.js").hasAttribute("data-ignored")).toBe(true);
  expect(row("dist").hasAttribute("data-ignored")).toBe(true);
  expect(row("src/app.ts").hasAttribute("data-ignored")).toBe(false);
  expect(row("src").hasAttribute("data-ignored")).toBe(false);
  // Closing the field drops the choice with the keyword.
  await act(async () => labelled("Hide the filter").click());
  await act(async () => labelled("Filter by file name").click());
  expect(toggle().getAttribute("aria-pressed")).toBe("false");
});
