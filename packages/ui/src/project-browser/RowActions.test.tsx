// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { isolate } from "../i18n/isolate";
import type { PlatformAdapter } from "../platform";
import { OpenShellTerminal, RowActions, RowPathScope } from "./RowActions";
import { ChangeList } from "./git/ChangeList";
import { change, changeViewFixture, committedChange, mountRows } from "./browserTestSupport";
import { changeItems } from "./git/changes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const notifications = { notice: vi.fn(), error: vi.fn() };
beforeEach(() => vi.clearAllMocks());

const items = changeItems([change("unstaged", "src/alpha.ts"), change("unstaged", "beta.ts")]);

function render(layout: "flat" | "tree", clipboard: PlatformAdapter["clipboard"], rows = items, onOpen = vi.fn()) {
  const container = mountRows(
    <ChangeList
      items={rows}
      selected={undefined}
      label="Changes"
      onOpen={onOpen}
      listRef={() => {}}
      changeView={changeViewFixture({ layout })}
    />,
    { clipboard, notifications },
  );
  return { container, onOpen };
}
const rowOf = (container: HTMLElement, name: string) =>
  [...container.querySelectorAll<HTMLElement>("[role=row]")].find((row) => row.getAttribute("aria-label")?.startsWith(isolate(name)))!;
const actionsOf = (row: HTMLElement) => row.querySelector<HTMLButtonElement>("button[aria-haspopup]")!;
const press = (element: HTMLElement, key: string) =>
  act(() => {
    element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
  });
const flush = () => act(async () => {});

// The rows differ between the two layouts (changes alone, and under directories), so this runs on both.
it.each(["flat", "tree"] as const)("%s: gives each change row an actions button, named for its change, that the row's arrow keys reach", (layout) => {
  const { container } = render(layout, { writeText: async () => {} });
  const row = rowOf(container, "alpha.ts");
  expect(actionsOf(row).getAttribute("aria-label")).toBe(`Actions for file ${isolate("alpha.ts")}`);
  act(() => row.focus());
  press(row, "ArrowRight");
  expect(document.activeElement).toBe(actionsOf(row));
  press(actionsOf(row), "ArrowLeft");
  expect(document.activeElement).toBe(row);
});

it("still opens the change on a flat row's Enter", () => {
  const { container, onOpen } = render("flat", { writeText: async () => {} });
  const row = rowOf(container, "alpha.ts");
  act(() => row.focus());
  press(row, "Enter");
  expect(onOpen).toHaveBeenCalledTimes(1);
});

it("copies the change's path from the menu and confirms with a notification, without opening the change", async () => {
  const writeText = vi.fn(async () => {});
  const { container, onOpen } = render("flat", { writeText });
  act(() => actionsOf(rowOf(container, "alpha.ts")).click());
  const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
  expect(item.textContent).toBe("Copy relative path");
  await flush();
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === "Copy relative path")!.click());
  await flush();
  expect(writeText).toHaveBeenCalledWith("src/alpha.ts");
  expect(notifications.notice).toHaveBeenCalledWith("Path copied");
  expect(onOpen).not.toHaveBeenCalled();
});

it("copies a path as text to paste: a percent-encoded UTF-8 byte sequence decoded, an invalid byte as U+FFFD, never the wire form", async () => {
  const writeText = vi.fn(async () => {});
  const wirePaths = ["src/caf%C3%A9.ts", "src/bad%FF.ts"];
  const { container } = render("flat", { writeText }, changeItems(wirePaths.map((path) => change("unstaged", path))));
  for (const name of ["café.ts", "bad\uFFFD.ts"]) {
    act(() => actionsOf(rowOf(container, name)).click());
    await flush();
    act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === "Copy relative path")!.click());
    await flush();
  }
  expect(writeText.mock.calls).toEqual([["src/café.ts"], ["src/bad\uFFFD.ts"]]);
});

it("says so when the clipboard refuses", async () => {
  const { container } = render("flat", { writeText: async () => Promise.reject(new Error("denied")) });
  act(() => actionsOf(rowOf(container, "beta.ts")).click());
  await flush();
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === "Copy relative path")!.click());
  await flush();
  expect(notifications.notice).not.toHaveBeenCalled();
  expect(notifications.error).toHaveBeenCalledWith("Could not copy the path");
});

it("offers Copy path where the platform has no clipboard, copying through the selection", async () => {
  const copied: string[] = [];
  const execCommand = vi.fn(() => copied.push(document.getSelection()!.toString()) > 0);
  Object.assign(document, { execCommand });
  onTestFinished(() => { Reflect.deleteProperty(document, "execCommand"); });
  const { container } = render("flat", undefined);
  act(() => actionsOf(rowOf(container, "alpha.ts")).click());
  const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
  expect(item.textContent).toBe("Copy relative path");
  await flush();
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === "Copy relative path")!.click());
  await flush();
  expect(execCommand).toHaveBeenCalledWith("copy");
  expect(copied).toEqual(["src/alpha.ts"]);
  expect(notifications.notice).toHaveBeenCalledWith("Path copied");
});


const openIn = () => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === "Open in")!;

it.each([
  { layout: "flat", worktree: undefined },
  { layout: "tree", worktree: undefined },
  { layout: "flat", worktree: "w" },
  { layout: "tree", worktree: "w" },
] as const)("$layout: committed row host actions use worktree $worktree, falling back to the project directory", async ({ layout, worktree }) => {
  const request = vi.fn(async (_body: { type: string }) => ({ type: "ack" as const, id: null }));
  const onOpen = vi.fn();
  const container = mountRows(
    <RowPathScope.Provider value={{ project: "p", worktree }}>
      <ChangeList
        items={changeItems([committedChange("src/alpha.ts")])}
        selected={undefined}
        label="Changes"
        onOpen={onOpen}
        listRef={() => {}}
        changeView={changeViewFixture({ layout })}
      />
    </RowPathScope.Provider>,
    { request, notifications },
  );
  for (const [label, type] of [["File manager", "reveal_project_path"], ["Default application", "open_project_path"]] as const) {
    act(() => actionsOf(rowOf(container, "alpha.ts")).click());
    act(() => openIn().click());
    act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === label)!.click());
    await flush();
    expect(request.mock.calls.filter(([body]) => body.type === type)).toHaveLength(1);
    expect(request).toHaveBeenCalledWith({ type, project: "p", ...(worktree !== undefined && { worktree }), path: "src/alpha.ts" });
  }
  expect(request.mock.calls.filter(([body]) => body.type !== "get_project_source")).toHaveLength(2);
  expect(onOpen).not.toHaveBeenCalled();
});

it.each(["open_project_path", "reveal_project_path"] as const)("sends %s with the live scope and wire path even without a clipboard", async (type) => {
  const request = vi.fn(async (_body: { type: string }) => ({ type: "ack" as const, id: null }));
  const container = mountRows(
    <RowPathScope.Provider value={{ project: "p", worktree: "w" }}>
      <RowActions path="src/caf%C3%A9.ts" name="café.ts" />
    </RowPathScope.Provider>,
    { request, notifications },
  );
  act(() => actionsOf(container).click());
  expect([...document.querySelectorAll<HTMLElement>("[role=menuitem]")].map((entry) => entry.textContent)).toEqual(["Copy relative path", "Copy absolute path", "Open in"]);
  act(() => openIn().click());
  const entries = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")];
  const entry = entries.find((item) => item.textContent === (type === "open_project_path" ? "Default application" : "File manager"))!;
  expect(entry).toBeDefined();
  act(() => entry.click());
  await flush();
  expect(request).toHaveBeenCalledWith({ type, project: "p", worktree: "w", path: "src/caf%C3%A9.ts" });
});

const openInItems = () => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].map((entry) => entry.textContent);

it.each([undefined, "w"])("offers a folder File manager and Terminal in worktree %s, and no Default application", async (worktree) => {
  const request = vi.fn(async (_body: { type: string }) => ({ type: "ack" as const, id: null }));
  const openTerminal = vi.fn();
  const container = mountRows(
    <OpenShellTerminal.Provider value={openTerminal}>
      <RowPathScope.Provider value={{ project: "p", worktree }}><RowActions path="src/ui" name="ui" directory /></RowPathScope.Provider>
    </OpenShellTerminal.Provider>,
    { request, notifications },
  );
  act(() => actionsOf(container).click());
  act(() => openIn().click());
  expect(openInItems()).toEqual(["Copy relative path", "Copy absolute path", "Open in", "File manager", "Terminal"]);
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === "Terminal")!.click());
  await flush();
  expect(openTerminal).toHaveBeenCalledExactlyOnceWith({ project: "p", ...(worktree !== undefined && { worktree }), path: "src/ui" });
  expect(request.mock.calls.every(([body]) => body.type === "get_project_source")).toBe(true);
});

it("sends reveal_project_path for a folder's File manager", async () => {
  const request = vi.fn(async (_body: { type: string }) => ({ type: "ack" as const, id: null }));
  const container = mountRows(
    <RowPathScope.Provider value={{ project: "p" }}><RowActions path="src/ui" name="ui" directory /></RowPathScope.Provider>,
    { request, notifications },
  );
  act(() => actionsOf(container).click());
  act(() => openIn().click());
  expect(openInItems()).toEqual(["Copy relative path", "Copy absolute path", "Open in", "File manager"]);
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === "File manager")!.click());
  await flush();
  expect(request).toHaveBeenCalledWith({ type: "reveal_project_path", project: "p", path: "src/ui" });
});

it("leaves Terminal out of a file's Open in", () => {
  const container = mountRows(
    <OpenShellTerminal.Provider value={vi.fn()}>
      <RowPathScope.Provider value={{ project: "p" }}><RowActions path="a.ts" name="a.ts" /></RowPathScope.Provider>
    </OpenShellTerminal.Provider>,
    { notifications },
  );
  act(() => actionsOf(container).click());
  act(() => openIn().click());
  expect(openInItems()).toEqual(["Copy relative path", "Copy absolute path", "Open in", "File manager", "Default application"]);
});

it("gives the tree layout's folder row the actions button, reserved and reached by the arrow keys, with a menu for the folder's path", async () => {
  const openTerminal = vi.fn();
  const writeText = vi.fn(async () => {});
  const container = mountRows(
    <OpenShellTerminal.Provider value={openTerminal}>
      <RowPathScope.Provider value={{ project: "p", worktree: "w" }}>
        <ChangeList items={items} selected={undefined} label="Changes" onOpen={vi.fn()} listRef={() => {}} changeView={changeViewFixture({ layout: "tree" })} />
      </RowPathScope.Provider>
    </OpenShellTerminal.Provider>,
    { clipboard: { writeText }, notifications },
  );
  const folder = [...container.querySelectorAll<HTMLElement>("[role=row]")].find((row) => row.textContent?.startsWith("src"))!;
  expect(actionsOf(folder).getAttribute("aria-label")).toBe(`Actions for folder ${isolate("src")}`);
  act(() => folder.focus());
  press(folder, "ArrowRight");
  expect(document.activeElement).toBe(actionsOf(folder));
  act(() => actionsOf(folder).click());
  await flush();
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === "Copy relative path")!.click());
  await flush();
  expect(writeText).toHaveBeenCalledWith("src");
  act(() => actionsOf(folder).click());
  act(() => openIn().click());
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === "Terminal")!.click());
  expect(openTerminal).toHaveBeenCalledExactlyOnceWith({ project: "p", worktree: "w", path: "src" });
});

it("reports a system-open failure through the existing error notification", async () => {
  const container = mountRows(
    <RowPathScope.Provider value={{ project: "p" }}><RowActions path="gone.ts" name="gone.ts" /></RowPathScope.Provider>,
    { request: async () => { throw new Error("File not found"); }, notifications },
  );
  act(() => actionsOf(container).click());
  act(() => openIn().click());
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === "Default application")!.click());
  await flush();
  expect(notifications.error).toHaveBeenCalledWith("File not found");
});

it.each(["flat", "tree"] as const)("%s: opens a change row's menu on right-click without opening the change", (layout) => {
  const { container, onOpen } = render(layout, { writeText: async () => {} });
  const row = rowOf(container, "alpha.ts");
  act(() => row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 20, clientY: 30 })));
  expect(document.querySelector<HTMLElement>("[role=menuitem]")!.textContent).toBe("Copy relative path");
  expect(onOpen).not.toHaveBeenCalled();
});


it.each([
  [undefined, "/resolved/project", "/resolved/project/src/café.ts"],
  [undefined, "/", "/src/café.ts"],
  ["worktree-id", "/resolved/project", "/worktrees/topic/packages/app/src/café.ts"],
] as const)("copies the absolute path from the live directory for worktree %s", async (worktree, root, expected) => {
  const writeText = vi.fn(async () => {});
  const request = vi.fn(async () => ({
    type: "project_source" as const, id: null,
    source: {
      project: "p", root: "/stored/project", resolved_root: root, root_id: "root",
      git: { repository: "repo", common_dir: "/repo/.git", worktree: "main", scope: "packages/app", worktrees: [
        { id: "worktree-id", root: "/worktrees/topic", main: false, head: "head", branch: "topic", scope_present: true },
      ] }, git_error: null,
    },
  }));
  const container = mountRows(
    <RowPathScope.Provider value={{ project: "p", worktree }}><RowActions path="src/caf%C3%A9.ts" name="café.ts" /></RowPathScope.Provider>,
    { request, clipboard: { writeText }, notifications },
  );
  act(() => actionsOf(container).click());
  await flush();
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === "Copy absolute path")!.click());
  await flush();
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "get_project_source", project: "p" });
  expect(writeText).toHaveBeenCalledExactlyOnceWith(expected);
});


it.each([
  [undefined, "C:\\work\\app", "C:\\work\\app\\src\\café.ts"],
  [undefined, "C:\\", "C:\\src\\café.ts"],
  ["worktree-id", "C:\\work\\app", "D:\\worktrees\\topic\\packages\\app\\src\\café.ts"],
] as const)("copies the absolute path in the Windows host's spelling for worktree %s", async (worktree, root, expected) => {
  const writeText = vi.fn(async () => {});
  const request = vi.fn(async () => ({
    type: "project_source" as const, id: null,
    source: {
      project: "p", root: "C:\\stored\\app", resolved_root: root, root_id: "root",
      git: { repository: "repo", common_dir: "C:\\work\\app\\.git", worktree: "main", scope: "packages/app", worktrees: [
        { id: "worktree-id", root: "D:\\worktrees\\topic", main: false, head: "head", branch: "topic", scope_present: true },
      ] }, git_error: null,
    },
  }));
  const container = mountRows(
    <RowPathScope.Provider value={{ project: "p", worktree }}><RowActions path="src/caf%C3%A9.ts" name="café.ts" /></RowPathScope.Provider>,
    { request, clipboard: { writeText }, notifications, state: { host: { os: "windows", path_style: "windows", file_links: true, trash: "recycle_bin", settings: "settings" } } },
  );
  act(() => actionsOf(container).click());
  await flush();
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === "Copy absolute path")!.click());
  await flush();
  expect(writeText).toHaveBeenCalledExactlyOnceWith(expected);
});

it("keeps relative copying available when the absolute path cannot be resolved", async () => {
  const writeText = vi.fn(async () => {});
  const container = mountRows(
    <RowPathScope.Provider value={{ project: "p", worktree: "gone" }}><RowActions path="src/a.ts" name="a.ts" /></RowPathScope.Provider>,
    { request: async () => { throw new Error("The project is unavailable"); }, clipboard: { writeText }, notifications },
  );
  act(() => actionsOf(container).click());
  await flush();
  const absolute = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent?.startsWith("Copy absolute path"))!;
  expect(absolute.getAttribute("aria-disabled")).toBe("true");
  expect(absolute.textContent).toBe("Copy absolute pathCould not copy the path");
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === "Copy relative path")!.click());
  await flush();
  expect(writeText).toHaveBeenCalledExactlyOnceWith("src/a.ts");
  expect(notifications.error).not.toHaveBeenCalled();
});


it("opens immediately with a disabled absolute path and usable relative path while resolving", async () => {
  const writeText = vi.fn(async () => {});
  const request = vi.fn(() => new Promise<never>(() => {}));
  const container = mountRows(
    <RowPathScope.Provider value={{ project: "p" }}><RowActions path="src/a.ts" name="a.ts" /></RowPathScope.Provider>,
    { request, clipboard: { writeText }, notifications },
  );
  expect(request).not.toHaveBeenCalled();
  act(() => actionsOf(container).click());
  const menu = document.querySelector('[role="menu"]')!;
  expect([...menu.children].map((element) => element.getAttribute("role") === "separator" ? "-" : element.textContent)).toEqual([
    "Copy relative path", "Copy absolute path", "-", "Open in",
  ]);
  const [relative, absolute] = [...menu.querySelectorAll<HTMLElement>('[role="menuitem"]')];
  expect(absolute.getAttribute("aria-disabled")).toBe("true");
  expect(absolute.textContent).toBe("Copy absolute path");
  act(() => relative.click());
  await flush();
  expect(writeText).toHaveBeenCalledExactlyOnceWith("src/a.ts");
});
