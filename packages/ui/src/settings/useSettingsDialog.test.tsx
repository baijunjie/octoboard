// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { PlatformProvider } from "../platform/react";
import { useSettingsDialog } from "./useSettingsDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(otherModalOpen: boolean, focusTerminal: () => void) {
  let hook!: ReturnType<typeof useSettingsDialog>;
  function Probe() {
    hook = useSettingsDialog({ ready: true, otherModalOpen, focusTerminal });
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<PlatformProvider value={{ kind: "browser" }}><Probe /></PlatformProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return () => hook;
}

it("hands focus to the terminal when Settings closes with nothing open under it", () => {
  const focusTerminal = vi.fn();
  const hook = mount(false, focusTerminal);
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  expect(focusTerminal).toHaveBeenCalledOnce();
});

const frame = () => act(async () => { await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined))); });

it("hands focus back to a dialog open under Settings, a frame after the modal's own restore, when that left it on the body", async () => {
  const dialog = document.body.appendChild(document.createElement("div"));
  dialog.setAttribute("role", "dialog");
  dialog.tabIndex = -1;
  onTestFinished(() => dialog.remove());
  const focusTerminal = vi.fn();
  const hook = mount(true, focusTerminal);
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  expect(document.activeElement).toBe(document.body);
  await frame();
  expect(focusTerminal).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(dialog);
});

it("leaves alone a control the modal's restore has put focus on inside the dialog under Settings", async () => {
  const dialog = document.body.appendChild(document.createElement("div"));
  dialog.setAttribute("role", "dialog");
  dialog.tabIndex = -1;
  const opener = dialog.appendChild(document.createElement("button"));
  onTestFinished(() => dialog.remove());
  const hook = mount(true, () => {});
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  opener.focus();
  await frame();
  expect(document.activeElement).toBe(opener);
});

it("numbers each request, so an open dialog can follow a later one", () => {
  const hook = mount(false, () => {});
  act(() => hook().openSettingsAt("accounts", { addAgent: true }));
  const first = hook().settingsNavigation;
  act(() => hook().openSettingsAt("accounts"));
  expect(hook().settingsNavigation.nonce).toBe(first.nonce + 1);
  expect(first).toMatchObject({ section: "accounts", addAgent: true });
  expect(hook().settingsNavigation).toMatchObject({ section: "accounts", addAgent: false });
});

function mountWithMenu(options: { otherModalOpen?: boolean; ready?: boolean }) {
  let about: () => void = () => {};
  const menu = { onSettingsRequested: () => () => {}, onAboutRequested: (handler: () => void) => { about = handler; return () => {}; }, setLabels: async () => {} };
  let hook!: ReturnType<typeof useSettingsDialog>;
  function Probe() {
    hook = useSettingsDialog({ ready: options.ready ?? true, otherModalOpen: options.otherModalOpen ?? false, focusTerminal: () => {} });
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<PlatformProvider value={{ kind: "tauri", appMenu: menu }}><Probe /></PlatformProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { hook: () => hook, about: () => act(() => about()) };
}

it("opens Settings on About from the native menu's About item, and moves an open Settings there", () => {
  const { hook, about } = mountWithMenu({});
  about();
  expect(hook()).toMatchObject({ settingsOpen: true, settingsNavigation: { section: "about" } });
  act(() => hook().openSettingsAt("git"));
  about();
  expect(hook().settingsNavigation.section).toBe("about");
});

it("ignores the menu's About item before the UI is ready and while another modal is open", () => {
  for (const options of [{ ready: false }, { otherModalOpen: true }]) {
    const { hook, about } = mountWithMenu(options);
    about();
    expect(hook().settingsOpen).toBe(false);
  }
});
