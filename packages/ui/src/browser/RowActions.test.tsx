// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { isolate } from "../i18n/isolate";
import type { PlatformAdapter } from "../platform";
import { RowActions, RowPathScope } from "./RowActions";
import { ChangeList } from "./ChangeList";
import { change, changeViewFixture, committedChange, mountRows } from "./browserTestSupport";
import { changeItems } from "./changes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const toasts = { notice: vi.fn(), error: vi.fn() };
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
    { clipboard, toasts },
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

it("copies the change's path from the menu and confirms with a toast, without opening the change", async () => {
  const writeText = vi.fn(async () => {});
  const { container, onOpen } = render("flat", { writeText });
  act(() => actionsOf(rowOf(container, "alpha.ts")).click());
  const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
  expect(item.textContent).toBe("Copy path");
  act(() => item.click());
  await flush();
  expect(writeText).toHaveBeenCalledWith("src/alpha.ts");
  expect(toasts.notice).toHaveBeenCalledWith("Path copied");
  expect(onOpen).not.toHaveBeenCalled();
});

it("copies a path as text to paste: a percent-encoded UTF-8 byte sequence decoded, an invalid byte as U+FFFD, never the wire form", async () => {
  const writeText = vi.fn(async () => {});
  const wirePaths = ["src/caf%C3%A9.ts", "src/bad%FF.ts"];
  const { container } = render("flat", { writeText }, changeItems(wirePaths.map((path) => change("unstaged", path))));
  for (const name of ["café.ts", "bad\uFFFD.ts"]) {
    act(() => actionsOf(rowOf(container, name)).click());
    act(() => document.querySelector<HTMLElement>("[role=menuitem]")!.click());
    await flush();
  }
  expect(writeText.mock.calls).toEqual([["src/café.ts"], ["src/bad\uFFFD.ts"]]);
});

it("says so when the clipboard refuses", async () => {
  const { container } = render("flat", { writeText: async () => Promise.reject(new Error("denied")) });
  act(() => actionsOf(rowOf(container, "beta.ts")).click());
  act(() => document.querySelector<HTMLElement>("[role=menuitem]")!.click());
  await flush();
  expect(toasts.notice).not.toHaveBeenCalled();
  expect(toasts.error).toHaveBeenCalledWith("Could not copy the path");
});

it("offers Copy path where the platform has no clipboard, copying through the selection", async () => {
  const copied: string[] = [];
  const execCommand = vi.fn(() => copied.push(document.getSelection()!.toString()) > 0);
  Object.assign(document, { execCommand });
  onTestFinished(() => { Reflect.deleteProperty(document, "execCommand"); });
  const { container } = render("flat", undefined);
  act(() => actionsOf(rowOf(container, "alpha.ts")).click());
  const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
  expect(item.textContent).toBe("Copy path");
  act(() => item.click());
  await flush();
  expect(execCommand).toHaveBeenCalledWith("copy");
  expect(copied).toEqual(["src/alpha.ts"]);
  expect(toasts.notice).toHaveBeenCalledWith("Path copied");
});


const openIn = () => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === "Open in")!;

it.each([
  { layout: "flat", worktree: undefined },
  { layout: "tree", worktree: undefined },
  { layout: "flat", worktree: "w" },
  { layout: "tree", worktree: "w" },
] as const)("$layout: committed row host actions use worktree $worktree, falling back to the project directory", async ({ layout, worktree }) => {
  const request = vi.fn(async () => ({ type: "ack" as const }));
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
    { request, toasts },
  );
  for (const [label, type] of [["File manager", "reveal_project_path"], ["Default application", "open_project_path"]] as const) {
    act(() => actionsOf(rowOf(container, "alpha.ts")).click());
    act(() => openIn().click());
    act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === label)!.click());
    await flush();
    expect(request).toHaveBeenLastCalledWith({ type, project: "p", ...(worktree !== undefined && { worktree }), path: "src/alpha.ts" });
  }
  expect(request).toHaveBeenCalledTimes(2);
  expect(onOpen).not.toHaveBeenCalled();
});

it.each(["open_project_path", "reveal_project_path"] as const)("sends %s with the live scope and wire path even without a clipboard", async (type) => {
  const request = vi.fn(async () => ({ type: "ack" as const }));
  const container = mountRows(
    <RowPathScope.Provider value={{ project: "p", worktree: "w" }}>
      <RowActions path="src/caf%C3%A9.ts" name="café.ts" />
    </RowPathScope.Provider>,
    { request, toasts },
  );
  act(() => actionsOf(container).click());
  expect([...document.querySelectorAll<HTMLElement>("[role=menuitem]")].map((entry) => entry.textContent)).toEqual(["Copy path", "Open in"]);
  act(() => openIn().click());
  const entries = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")];
  const entry = entries.find((item) => item.textContent === (type === "open_project_path" ? "Default application" : "File manager"))!;
  expect(entry).toBeDefined();
  act(() => entry.click());
  await flush();
  expect(request).toHaveBeenCalledExactlyOnceWith({ type, project: "p", worktree: "w", path: "src/caf%C3%A9.ts" });
});

it("reports a system-open failure through the existing error toast", async () => {
  const container = mountRows(
    <RowPathScope.Provider value={{ project: "p" }}><RowActions path="gone.ts" name="gone.ts" /></RowPathScope.Provider>,
    { request: async () => { throw new Error("File not found"); }, toasts },
  );
  act(() => actionsOf(container).click());
  act(() => openIn().click());
  act(() => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === "Default application")!.click());
  await flush();
  expect(toasts.error).toHaveBeenCalledWith("File not found");
});

it.each(["flat", "tree"] as const)("%s: opens a change row's menu on right-click without opening the change", (layout) => {
  const { container, onOpen } = render(layout, { writeText: async () => {} });
  const row = rowOf(container, "alpha.ts");
  act(() => row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 20, clientY: 30 })));
  expect(document.querySelector<HTMLElement>("[role=menuitem]")!.textContent).toBe("Copy path");
  expect(onOpen).not.toHaveBeenCalled();
});
