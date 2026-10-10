// @vitest-environment jsdom
import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { Event, Project, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { useProjectBrowserState } from "./browserState";
import { ProjectBrowser } from "./ProjectBrowser";

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
const changes = (project: string): Event => ({ type: "project_changes", project, worktree: null, head: null, changes: [], complete: true });

/** Mounts the browser of project `id` in `mode` and returns its Refresh button's icon. */
async function mount(id: string, mode: "files" | "git"): Promise<() => Element> {
  const Open = () => {
    const browser = useProjectBrowserState(id);
    useEffect(() => browser.setMode(mode), []);
    return <ProjectBrowser project={{ id, name: id } as Project} layout={{ open: true, width: 300 }} active />;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  roots.push(root);
  await act(async () =>
    root.render(
      <DaemonProvider value={daemon}>
        <Open />
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
  await answer("list_project_changes", changes("git-p"));
  expect(spinning(icon())).toBe(false);

  await press(icon());
  expect(spinning(icon())).toBe(true);
  await wait(1100);
  await answer("get_project_source", source("git-p"));
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
  await answer("list_project_changes", changes("git-fail"));

  await press(icon());
  const index = pending.findIndex((request) => request.body.type === "get_project_source");
  const [request] = pending.splice(index, 1);
  await act(async () => request.reject(new Error("rejected")));
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
