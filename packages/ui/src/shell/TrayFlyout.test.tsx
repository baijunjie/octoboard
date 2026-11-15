// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { setLanguage } from "../i18n/language";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter, TrayFlyoutModel } from "../platform";
import { MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { TrayFlyout } from "./TrayFlyout";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const RED: [number, number, number] = [200, 30, 20];
const GREY: [number, number, number] = [120, 120, 120];

const MODEL: TrayFlyoutModel = {
  tabs: [
    { key: "local", label: "This PC", remote: false, raised: false },
    { key: "build", label: "Build box", remote: true, raised: true },
  ],
  chosen: 0,
  panels: [
    [
      { kind: "heading", label: "Waiting (1)", icon: { png: "AAAA", template: true } },
      { kind: "item", id: "session:a", label: "Fix the tests" },
      { kind: "note", label: "3 more…" },
      {
        kind: "quota",
        submenu: {
          title: "Claude usage",
          quota_remaining: 80,
          quota_weekly_remaining: 40,
          quota_label: "Claude, 5h 80% remaining",
          items: [
            { kind: "header", title: "me@example.com", suffix: "Max", detail: "updated 2m ago", status: { label: "Limit reached", foreground_colour: [1, 2, 3], background_colour: [4, 5, 6, 255] } },
            { kind: "window", name: "5h", detail: "resets in 1h", label: "5h 80% remaining", fraction: 0.8, fill_colour: RED },
            { kind: "window", name: "Weekly", detail: "resets in 2d", label: "Weekly 0% remaining", fraction: 0, used_up: true, fill_colour: RED, used_up_colour: GREY },
            { kind: "meter", session: { name: "5h", detail: "1h", fraction: 0.5 }, weekly: { name: "Weekly", detail: "2d", fraction: 0.25 }, label: "5h and Weekly", fill_colour: RED },
            { kind: "separator" },
            { kind: "note", text: "Out of date" },
          ],
        },
      },
    ],
    [],
  ],
  shared: [{ kind: "item", id: "updates", label: "Check for updates" }],
  actions: [
    [{ kind: "item", id: "open", label: "Open board" }],
    [{ kind: "item", id: "open:build", label: "Open board" }, { kind: "item", id: "disconnect:build", label: "Disconnect" }],
  ],
  footer: [{ kind: "item", id: "quit", label: "Quit completely" }],
};
const SINGLE: TrayFlyoutModel = { tabs: [MODEL.tabs[0]], chosen: 0, panels: [MODEL.panels[0]], shared: MODEL.shared, actions: [MODEL.actions[0]], footer: MODEL.footer };

const choose = vi.fn(async (_id: string) => {});
const undoModel = vi.fn();
const undoOpened = vi.fn();
const close = vi.fn(async () => {});
const setSize = vi.fn(async (_size: { width: number; height: number }) => {});
let model: TrayFlyoutModel | null;
let pushModel: (model: TrayFlyoutModel) => void;
let open: (opened: { chosen: number }) => void;
// Every observer's callback: the scroll fade watches elements of its own beside the content.
const callbacks: (() => void)[] = [];
const observed = () => { for (const callback of callbacks) callback(); };

beforeEach(() => {
  model = MODEL;
  choose.mockClear();
  close.mockClear();
  undoModel.mockClear();
  undoOpened.mockClear();
  setSize.mockClear();
  callbacks.length = 0;
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: () => void) {
      callbacks.push(callback);
    }
    observe() {}
    disconnect() {}
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 311.2, height: 250.5 } as DOMRect);
});

const PLATFORM: PlatformAdapter = {
  kind: "tauri",
  host: { ...MAC_HOST, os: "windows" },
  trayFlyout: {
    model: async () => model,
    onModel: (handler) => {
      pushModel = handler;
      return undoModel;
    },
    onOpened: (handler) => {
      open = handler;
      return undoOpened;
    },
    setSize: (size) => setSize(size),
    choose: (id) => choose(id),
    close: () => close(),
  },
};

async function mount() {
  let container!: HTMLElement;
  await act(async () => {
    container = mountHost(<LanguageProvider><TrayFlyout /></LanguageProvider>, { platform: PLATFORM });
  });
  return container;
}

const tabs = (container: ParentNode) => Array.from(container.querySelectorAll<HTMLElement>("[role=tab]"));
const rows = (container: ParentNode) => Array.from(container.querySelectorAll<HTMLElement>("[role=menuitem]"));
const rowNamed = (container: ParentNode, name: string) => rows(container).find((row) => (row.getAttribute("aria-label") ?? row.textContent ?? "").startsWith(name))!;
const press = (element: Element) =>
  act(async () => {
    const init = { bubbles: true, cancelable: true, button: 0, pointerId: 1, isPrimary: true, pointerType: "mouse" };
    element.dispatchEvent(new PointerEvent("pointerdown", init));
    element.dispatchEvent(new PointerEvent("pointerup", init));
    element.dispatchEvent(new MouseEvent("click", init));
  });
const key = (element: Element, name: string) =>
  act(async () => {
    element.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key: name, bubbles: true, cancelable: true }));
  });

it("draws a tab per daemon, marks a raised one and names it so, then the chosen tab's lines and the shared ones", async () => {
  const container = await mount();
  expect(container.querySelector("[role=tablist]")!.getAttribute("aria-label")).toBe("Computers");
  const [local, build] = tabs(container);
  expect(local.textContent).toBe("This PC");
  expect(local.getAttribute("aria-selected")).toBe("true");
  expect(local.getAttribute("aria-label")).toBeNull();
  expect(build.getAttribute("aria-label")).toBe("⁨Build box⁩, a session is waiting");
  expect(build.querySelector("[aria-hidden=true]")).not.toBeNull();
  expect(local.querySelector("[aria-hidden=true]")).toBeNull();

  expect(container.textContent).toContain("Waiting (1)");
  expect(container.textContent).toContain("3 more…");
  expect(rows(container).map((row) => row.getAttribute("aria-label") ?? row.textContent)).toEqual([
    "Fix the tests", "3 more…", "Claude, 5h 80% remaining", "Check for updates", "Open board", "Quit completely",
  ]);
  // A divider before Check for updates and another before Quit completely, none between Check for updates and Open
  // board.
  const order = Array.from(container.querySelectorAll<HTMLElement>("[role=separator], [role=menuitem]")).map((element) => (element.getAttribute("role") === "separator" ? "-" : (element.getAttribute("aria-label") ?? element.textContent)));
  expect(order.slice(-6)).toEqual(["Claude, 5h 80% remaining", "-", "Check for updates", "Open board", "-", "Quit completely"]);
  expect(container.querySelector(".tray-flyout-picture")).not.toBeNull();
  expect(rowNamed(container, "3 more…").getAttribute("aria-disabled")).toBe("true");
});

it("shows another tab's lines on choosing it and keeps the shared lines", async () => {
  const container = await mount();
  await press(tabs(container)[1]);
  expect(tabs(container)[1].getAttribute("aria-selected")).toBe("true");
  expect(container.textContent).not.toContain("Fix the tests");
  // The tab lists nothing, so the menu starts at Check for updates, with no divider above it.
  expect(rows(container).map((row) => row.textContent)).toEqual(["Check for updates", "Open board", "Disconnect", "Quit completely"]);
  expect(container.querySelectorAll("[role=separator]").length).toBe(1);
});

it("selects the tab it is shown on, collapses what was open and focuses the tab", async () => {
  const container = await mount();
  await press(rowNamed(container, "Claude"));
  expect(container.textContent).toContain("me@example.com");
  await act(async () => open({ chosen: 1 }));
  expect(tabs(container)[1].getAttribute("aria-selected")).toBe("true");
  expect(document.activeElement).toBe(tabs(container)[1]);
  await act(async () => open({ chosen: 0 }));
  expect(container.textContent).not.toContain("me@example.com");
  expect(document.activeElement).toBe(tabs(container)[0]);
});

it("has no tab strip for one tab, and focuses its first line when shown", async () => {
  model = SINGLE;
  const container = await mount();
  expect(container.querySelector("[role=tablist]")).toBeNull();
  await act(async () => open({ chosen: 0 }));
  expect(document.activeElement).toBe(rowNamed(container, "Fix the tests"));
});

it("starts with Check for updates, under no divider, when the daemon lists nothing", async () => {
  model = { ...SINGLE, panels: [[]] };
  const container = await mount();
  expect(rows(container).map((row) => row.textContent)).toEqual(["Check for updates", "Open board", "Quit completely"]);
  expect(container.querySelector("[role=menu]")!.firstElementChild!.getAttribute("role")).not.toBe("separator");
  expect(container.querySelectorAll("[role=separator]").length).toBe(1);
});

it("takes a newer model while hidden", async () => {
  const container = await mount();
  await act(async () => pushModel({ ...SINGLE, panels: [[{ kind: "item", id: "x", label: "Fresh" }]] }));
  expect(container.textContent).toContain("Fresh");
  expect(container.querySelector("[role=tablist]")).toBeNull();
});

it("sends the id of the line chosen", async () => {
  const container = await mount();
  await press(rowNamed(container, "Fix the tests"));
  expect(choose).toHaveBeenCalledWith("session:a");
  await press(rowNamed(container, "Quit completely"));
  expect(choose).toHaveBeenLastCalledWith("quit");
  await press(rowNamed(container, "3 more…"));
  expect(choose).toHaveBeenCalledTimes(2);
});

it("opens a quota line in place to its rows, with their bars' colours, and Left then Esc close it before the flyout", async () => {
  const container = await mount();
  const opener = rowNamed(container, "Claude");
  expect(opener.getAttribute("aria-expanded")).toBe("false");
  const separators = container.querySelectorAll("[role=separator]").length;
  await press(opener);
  // The submenu's separator sets one account off from the next, as on macOS.
  expect(container.querySelectorAll("[role=separator]").length).toBe(separators + 1);
  expect(choose).not.toHaveBeenCalled();
  expect(opener.getAttribute("aria-expanded")).toBe("true");
  const text = container.textContent!;
  for (const part of ["me@example.com", "Max", "Limit reached", "updated 2m ago", "5h", "resets in 1h", "Weekly", "Out of date"]) expect(text).toContain(part);
  expect(rowNamed(container, "5h 80% remaining").getAttribute("aria-disabled")).toBe("true");
  expect(rowNamed(container, "5h and Weekly")).toBeDefined();
  const fills = Array.from(container.querySelectorAll<HTMLElement>(".tray-flyout-bar")).map((bar) => bar.style.getPropertyValue("--tray-flyout-fill"));
  expect(fills).toEqual(["rgb(200 30 20)", "rgb(120 120 120)", "rgb(200 30 20)", "rgb(200 30 20)"]);
  // The account's state is a warning badge of the app's own, not drawn in the menu's colours.
  const status = Array.from(container.querySelectorAll<HTMLElement>("span")).find((element) => element.textContent === "Limit reached")!;
  expect(status.style.color).toBe("");
  expect(status.style.backgroundColor).toBe("");

  await key(opener, "Escape");
  expect(container.textContent).not.toContain("me@example.com");
  expect(close).not.toHaveBeenCalled();
  await key(opener, "ArrowRight");
  expect(container.textContent).toContain("me@example.com");
  await key(opener, "ArrowLeft");
  expect(container.textContent).not.toContain("me@example.com");
  await key(opener, "Escape");
  expect(close).toHaveBeenCalledTimes(1);
});

it("closes an opened quota row with a divider only while it is open and another line follows without one", async () => {
  const container = await mount();
  const withCodex: TrayFlyoutModel = {
    ...MODEL,
    panels: [[...MODEL.panels[0], { kind: "quota", submenu: { title: "Codex usage", items: [] } }], MODEL.panels[1]],
  };
  await act(async () => pushModel(withCodex));
  const separators = () => container.querySelectorAll("[role=separator]").length;
  const closed = separators();
  const claude = rowNamed(container, "Claude");
  await press(claude);
  // The block's own separator between accounts, and the one closing it before Codex usage.
  expect(separators()).toBe(closed + 2);
  expect(rowNamed(container, "Codex").previousElementSibling!.getAttribute("role")).toBe("separator");
  await press(claude);
  expect(separators()).toBe(closed);
  // Last in its panel, the divider the page draws before the shared lines is the only one.
  await act(async () => pushModel(MODEL));
  await press(rowNamed(container, "Claude"));
  expect(separators()).toBe(closed + 1);
});

it("reports the content's size, rounded up, as it changes", async () => {
  await mount();
  expect(setSize).toHaveBeenLastCalledWith({ width: 312, height: 251 });
  setSize.mockClear();
  vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockReturnValue({ width: 320, height: 400 } as DOMRect);
  act(() => observed());
  expect(setSize).toHaveBeenCalledWith({ width: 320, height: 400 });
  act(() => observed());
  expect(setSize).toHaveBeenCalledTimes(1);
});

it("takes Esc before the menu does, which stops it on a row, and closes the flyout when nothing is open", async () => {
  const container = await mount();
  const row = rowNamed(container, "Fix the tests");
  const menu = container.querySelector("[role=menu]")!;
  menu.addEventListener("keydown", (event) => (event as KeyboardEvent).key === "Escape" && event.stopPropagation());
  act(() => row.focus());
  await key(row, "Escape");
  expect(close).toHaveBeenCalledTimes(1);
});

it("keeps the focus on the opener it collapses, from the opener and from one of its rows", async () => {
  const container = await mount();
  const opener = rowNamed(container, "Claude");
  act(() => opener.focus());
  await key(opener, "ArrowRight");
  expect(document.activeElement).toBe(opener);
  await key(opener, "Escape");
  expect(rowNamed(container, "Claude")).toBe(opener);
  expect(document.activeElement).toBe(opener);

  await key(opener, "ArrowRight");
  const detail = rowNamed(container, "5h 80% remaining");
  act(() => detail.focus());
  await key(detail, "Escape");
  expect(container.textContent).not.toContain("me@example.com");
  expect(document.activeElement).toBe(rowNamed(container, "Claude"));
});

it("starts on the tab the model was built with, showing that daemon's lines", async () => {
  model = { ...MODEL, chosen: 1 };
  const container = await mount();
  expect(tabs(container)[1].getAttribute("aria-selected")).toBe("true");
  expect(rowNamed(container, "Disconnect")).toBeDefined();
  expect(container.textContent).not.toContain("Fix the tests");
});

it("moves between the tabs with the arrow keys, showing each daemon's lines", async () => {
  const container = await mount();
  act(() => tabs(container)[0].focus());
  await key(tabs(container)[0], "ArrowRight");
  expect(tabs(container)[1].getAttribute("aria-selected")).toBe("true");
  expect(rowNamed(container, "Disconnect")).toBeDefined();
  await key(tabs(container)[1], "ArrowLeft");
  expect(tabs(container)[0].getAttribute("aria-selected")).toBe("true");
  expect(container.textContent).toContain("Fix the tests");
});

it("closes what is open in place when another tab is chosen, and when a newer model arrives", async () => {
  const container = await mount();
  await press(rowNamed(container, "Claude"));
  expect(container.textContent).toContain("me@example.com");
  await press(tabs(container)[1]);
  await press(tabs(container)[0]);
  expect(container.textContent).not.toContain("me@example.com");

  await press(rowNamed(container, "Claude"));
  expect(container.textContent).toContain("me@example.com");
  await act(async () => pushModel(MODEL));
  expect(container.textContent).not.toContain("me@example.com");
});

it("draws nothing until the shell has a model, and then what it sends", async () => {
  model = null;
  const container = await mount();
  expect(container.querySelector("[role=menu]")).toBeNull();
  expect(container.querySelector("[role=tablist]")).toBeNull();
  await act(async () => pushModel(MODEL));
  expect(container.querySelector("[role=menu]")).not.toBeNull();
  expect(tabs(container)).toHaveLength(2);
});

it("stops listening to the shell once it is gone", async () => {
  const container = await mount();
  expect(container.querySelector("[role=menu]")).not.toBeNull();
  expect(undoModel).not.toHaveBeenCalled();
  await act(async () => (container as ReturnType<typeof mountHost>).render(null));
  expect(undoModel).toHaveBeenCalledOnce();
  expect(undoOpened).toHaveBeenCalledOnce();
});

it("survives the shell failing a choice or the dismissal, which it can do nothing about", async () => {
  choose.mockRejectedValueOnce(new Error("gone"));
  close.mockRejectedValueOnce(new Error("gone"));
  const container = await mount();
  await press(rowNamed(container, "Fix the tests"));
  const row = rowNamed(container, "Fix the tests");
  act(() => row.focus());
  await key(row, "Escape");
  expect(choose).toHaveBeenCalledOnce();
  expect(close).toHaveBeenCalledOnce();
});

it("names a quota header by its parts joined as a list, and a bar's row by the label it was given", async () => {
  const container = await mount();
  await press(rowNamed(container, "Claude"));
  expect(rowNamed(container, "me@example.com").getAttribute("aria-label")).toBe("me@example.com, Max, Limit reached, updated 2m ago");
  expect(rowNamed(container, "Weekly 0% remaining").getAttribute("aria-label")).toBe("Weekly 0% remaining");
});

it("draws a picture that is not a template as it is, hidden from assistive technology, and a template one in the text colour", async () => {
  model = {
    ...SINGLE,
    panels: [[
      { kind: "item", id: "a", label: "Plain", icon: { png: "BBBB", template: false } },
      { kind: "item", id: "b", label: "Tinted", icon: { png: "CCCC", template: true } },
    ]],
  };
  const container = await mount();
  const img = rowNamed(container, "Plain").querySelector("img")!;
  expect(img.getAttribute("src")).toBe("data:image/png;base64,BBBB");
  expect(img.getAttribute("aria-hidden")).toBe("true");
  expect(rowNamed(container, "Plain").querySelector(".tray-flyout-picture")).toBeNull();
  expect(rowNamed(container, "Tinted").querySelector(".tray-flyout-picture")!.getAttribute("aria-hidden")).toBe("true");
  expect(rowNamed(container, "Tinted").querySelector("img")).toBeNull();
});

it("shows the opener's compact bars only for a quota it has a remainder for, the weekly one beneath the 5-hour one", async () => {
  const quotaOf = (extra: object) => ({ kind: "quota" as const, submenu: { title: "Codex usage", items: [], ...extra } });
  model = { ...SINGLE, panels: [[quotaOf({ quota_remaining: 60, quota_weekly_remaining: 30 })]] };
  const both = await mount();
  expect(both.querySelectorAll(".tray-flyout-mini-fill")).toHaveLength(2);
  expect((both.querySelectorAll<HTMLElement>(".tray-flyout-mini-fill")[0]).style.width).toBe("60%");
  expect((both.querySelectorAll<HTMLElement>(".tray-flyout-mini-fill")[1]).style.width).toBe("30%");

  model = { ...SINGLE, panels: [[quotaOf({ quota_remaining: 60 })]] };
  expect((await mount()).querySelectorAll(".tray-flyout-mini-fill")).toHaveLength(1);

  model = { ...SINGLE, panels: [[quotaOf({})]] };
  expect((await mount()).querySelectorAll(".tray-flyout-mini-fill")).toHaveLength(0);
});

it("puts the items and notes after a heading in its section, and a separator between sections", async () => {
  model = {
    ...SINGLE,
    panels: [[
      { kind: "heading", label: "Waiting (1)" },
      { kind: "item", id: "a", label: "First" },
      { kind: "separator" },
      { kind: "heading", label: "Working (1)" },
      { kind: "item", id: "b", label: "Second" },
    ]],
    shared: [],
  };
  const container = await mount();
  const sections = [...container.querySelectorAll("[role=group]")];
  expect(sections.map((section) => section.textContent)).toEqual(["Waiting (1)First", "Working (1)Second"]);
  expect(container.querySelectorAll("[role=separator]").length).toBeGreaterThanOrEqual(1);
});

it("opens a row with the key toward the end of the text and closes it with the other, whichever way the language reads", async () => {
  setLanguage("ar");
  onTestFinished(() => setLanguage("en"));
  const container = await mount();
  const opener = rowNamed(container, "Claude");
  await key(opener, "ArrowRight");
  expect(container.textContent).not.toContain("me@example.com");
  await key(opener, "ArrowLeft");
  expect(container.textContent).toContain("me@example.com");
  await key(opener, "ArrowRight");
  expect(container.textContent).not.toContain("me@example.com");
});

it("reports no size while the content has no width, as when the window is hidden", async () => {
  vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockReturnValue({ width: 0, height: 0 } as DOMRect);
  await mount();
  expect(setSize).not.toHaveBeenCalled();
});
