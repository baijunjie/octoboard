// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { errorText } from "../errors/errorText";
import { setLanguage } from "../i18n/language";
import type { Event, RequestBody } from "../protocol";
import { DaemonProvider, type Daemon } from "../store";
import { testDaemon } from "../test/testDaemon";
import { AUTO_REFRESH_MS } from "./autoRefresh";
import { useDirectoryListings } from "./files/useDirectoryListings";
import { useBranchList } from "./git/useBranchList";
import { useChangeList, type ChangeList } from "./git/useChangeList";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Pending = { body: RequestBody; resolve: (event: Event) => void; reject: (err: Error) => void };

let pending: Pending[] = [];
let roots: Root[] = [];
let visibility: DocumentVisibilityState = "visible";

function daemon(): Daemon {
  return testDaemon({
    state: { connectionState: "open", snapshotEpoch: 1 },
    request: (body) =>
      new Promise((resolve, reject) => {
        pending.push({ body, resolve, reject });
      }),
  });
}

/** Mounts `use` as the browser would, and starts it: what it returns is how the browser asks for the
 * first read, where the hook waits to be told what is on screen. */
function mount(use: () => (() => void) | undefined) {
  let start: (() => void) | undefined;
  const Probe = () => {
    start = use();
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  roots.push(root);
  act(() =>
    root.render(
      <DaemonProvider value={daemon()}>
        <Probe />
      </DaemonProvider>,
    ),
  );
  if (start) act(start);
}

/** The three hooks that read a list on the browser's behalf every `AUTO_REFRESH_MS`, each with the
 * request it sends. */
const hooks: { name: string; type: RequestBody["type"]; use: () => (() => void) | undefined }[] = [
  {
    name: "useChangeList",
    type: "list_project_changes",
    use: () => {
      useChangeList("p", undefined, true);
      return undefined;
    },
  },
  {
    name: "useBranchList",
    type: "list_project_branches",
    use: () => {
      useBranchList("p", true);
      return undefined;
    },
  },
  {
    name: "useDirectoryListings",
    type: "list_project_dir",
    use: () => {
      const { want } = useDirectoryListings("p", true);
      return () => want([""]);
    },
  },
];

const count = (type: string) => pending.filter((request) => request.body.type === type).length;
/** Fails every request that is out, which ends the hook's wait on them. */
const settle = () => act(async () => pending.splice(0).forEach((request) => request.reject(new Error("no answer"))));
const advance = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));
const setVisibility = (state: DocumentVisibilityState) => {
  visibility = state;
  act(() => void document.dispatchEvent(new Event("visibilitychange")));
};

beforeEach(() => {
  vi.useFakeTimers();
  visibility = "visible";
  Object.defineProperty(document, "visibilityState", { configurable: true, get: () => visibility });
});

afterEach(async () => {
  act(() => setLanguage("en"));
  for (const root of roots.splice(0)) act(() => root.unmount());
  await settle();
  pending = [];
  roots = [];
  document.body.replaceChildren();
  vi.useRealTimers();
});

for (const { name, type, use } of hooks) {
  it(`${name} reads again every interval, and not while the window is hidden or its last read is out`, async () => {
    mount(use);
    await advance(0);
    expect(count(type)).toBe(1);

    // A read still out holds the next one back.
    await advance(AUTO_REFRESH_MS);
    expect(count(type)).toBe(1);

    await settle();
    await advance(AUTO_REFRESH_MS);
    expect(count(type)).toBe(1);
    await settle();

    // Hidden: the tick is skipped.
    setVisibility("hidden");
    await advance(AUTO_REFRESH_MS);
    expect(count(type)).toBe(0);
  });

  it(`${name} catches up on coming back only after half an interval since its last read`, async () => {
    mount(use);
    await advance(0);
    await settle();
    await advance(AUTO_REFRESH_MS);
    await settle();

    // Within half an interval, coming back asks for nothing.
    await advance(AUTO_REFRESH_MS / 2 - 1000);
    act(() => void window.dispatchEvent(new Event("focus")));
    setVisibility("visible");
    expect(count(type)).toBe(0);

    // After it, window focus does.
    await advance(2000);
    act(() => void window.dispatchEvent(new Event("focus")));
    expect(count(type)).toBe(1);
    await settle();

    // So does a pane that was hidden and shown again, off the tick.
    setVisibility("hidden");
    await advance(AUTO_REFRESH_MS / 2 + 1000);
    expect(count(type)).toBe(0);
    setVisibility("visible");
    expect(count(type)).toBe(1);
  });
}

/** Renders `useChangeList` for a worktree, and again for another when `render` is called with it. */
function mountChangeList() {
  let list: ChangeList | undefined;
  const Probe = ({ worktree }: { worktree: string }) => {
    list = useChangeList("p", worktree, true).list;
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  roots.push(root);
  const render = (worktree: string) =>
    act(() =>
      root.render(
        <DaemonProvider value={daemon()}>
          <Probe worktree={worktree} />
        </DaemonProvider>,
      ),
    );
  render("a");
  return { render, list: () => list };
}

it("words a periodic refresh that fails after a language change in the new language", async () => {
  const { list } = mountChangeList();
  await advance(0);
  await settle();
  act(() => setLanguage("zh-Hans"));
  await advance(AUTO_REFRESH_MS);
  expect(count("list_project_changes")).toBe(1);
  await act(async () =>
    pending.splice(0).forEach((request) => request.reject(new DaemonRequestError("too big", "limit_exceeded", { limit: "patch_bytes", max: "1" }))),
  );
  const current = list();
  expect(current?.state).toBe("error");
  expect(current?.state === "error" ? errorText(current.message) : "").toContain("此更改过大");
});

it("restarts the interval of a change list when its worktree changes, reading the new worktree at once", async () => {
  const { render } = mountChangeList();
  await advance(0);
  await settle();
  await advance(AUTO_REFRESH_MS / 2 - 1000);

  render("b");
  await advance(0);
  expect(count("list_project_changes")).toBe(1);
  await settle();

  // The first worktree's timer would have fired by now.
  await advance(AUTO_REFRESH_MS / 2 + 1000);
  expect(count("list_project_changes")).toBe(0);
  await advance(AUTO_REFRESH_MS / 2 - 1000);
  expect(count("list_project_changes")).toBe(1);
});
