// @vitest-environment jsdom
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { ActionMenu } from "../components/menus/ActionMenu";
import { consoleOf, projectOf } from "../fixtures/builders";
import { translatorFor } from "../i18n/catalog";
import { partSelector } from "../primitives";
import { mountHost } from "../test/mountHost";
import { consoleMenu, projectMenu } from "./menus";
import type { SidebarHandlers } from "./types";

const t = translatorFor("en");
const console = consoleOf("console", "Main");
const project = projectOf("project", console.id, "Website");

function mountMenus() {
  const handlers = { onOpenDialog: vi.fn(), onSetPinned: vi.fn(), onFocus: vi.fn(), onSelectSession: vi.fn(), onOpenArchive: vi.fn() } as unknown as SidebarHandlers;
  const host = mountHost(<>
    <ActionMenu label="Project actions" items={() => projectMenu(t, handlers, project, [], { gitRepository: true })} />
    <ActionMenu label="Console actions" items={() => consoleMenu(t, console, handlers, [])} />
  </>);
  const open = async (label: string) => {
    await act(async () => host.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!.click());
    return document.querySelector<HTMLElement>("[role=menu]")!;
  };
  const key = (target: Element, key: string) => act(() => target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true })));
  return { handlers, open, key };
}

const item = (menu: HTMLElement, label: string) => [...menu.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.querySelector(partSelector.itemLabel)?.textContent === label)!;

it.each([
  ["Pin", "P"], ["Focus mode", "F"], ["Rename", "R"], ["Project settings", "E"], ["Remove project", "D"],
])("takes the project's %s action with %s only in its open menu", async (label, letter) => {
  const { handlers, open, key } = mountMenus();
  key(document.body, letter.toLowerCase());
  for (const handler of [handlers.onOpenDialog, handlers.onSetPinned, handlers.onFocus]) expect(handler).not.toHaveBeenCalled();
  const menu = await open("Project actions");
  expect(item(menu, label).querySelector("kbd")?.textContent).toBe(letter);
  expect(item(menu, label).getAttribute("aria-keyshortcuts")).toBe(letter);
  for (const label of ["View archive", "Copy", "Open in", "Open panel", "Git"]) {
    expect(item(menu, label).querySelector("kbd")).toBeNull();
    expect(item(menu, label).getAttribute("aria-keyshortcuts")).toBeNull();
  }
  key(menu, letter.toLowerCase());
  if (letter === "P") expect(handlers.onSetPinned).toHaveBeenCalledExactlyOnceWith({ project }, true);
  else if (letter === "F") expect(handlers.onFocus).toHaveBeenCalledExactlyOnceWith({ project });
  else expect(handlers.onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind: letter === "R" ? "rename-project" : letter === "E" ? "edit-project" : "delete-project", project });
  expect(document.querySelector("[role=menu]")).toBeNull();
});

describe("console menu letters", () => {
  it.each([["Add project", "A", "new-project"], ["Console settings", "E", "edit-console"], ["Delete console", "D", "delete-console"]])("takes %s with %s", async (label, letter, kind) => {
    const { handlers, open, key } = mountMenus();
    const menu = await open("Console actions");
    expect(item(menu, label).querySelector("kbd")?.textContent).toBe(letter);
    expect(item(menu, "View archive").querySelector("kbd")).toBeNull();
    expect(item(menu, "View archive").getAttribute("aria-keyshortcuts")).toBeNull();
    key(menu, letter.toLowerCase());
    expect(handlers.onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind, console });
    expect(document.querySelector("[role=menu]")).toBeNull();
  });

  it("leaves closed menus alone and reuses E for the menu that is open", async () => {
    const { handlers, open, key } = mountMenus();
    key(await open("Project actions"), "e");
    expect(handlers.onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind: "edit-project", project });
    key(document.body, "e");
    expect(handlers.onOpenDialog).toHaveBeenCalledTimes(1);
    key(await open("Console actions"), "e");
    expect(handlers.onOpenDialog).toHaveBeenLastCalledWith({ kind: "edit-console", console });
    expect(handlers.onOpenDialog).toHaveBeenCalledTimes(2);
  });
});
