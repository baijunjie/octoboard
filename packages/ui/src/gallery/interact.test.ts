// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { createUi, stuckExit, type Matcher } from "./interact";
import { format } from "../i18n/catalog";
import { IS_MAC } from "../useWindowShortcut";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.useRealTimers();
  document.body.replaceChildren();
});

function exiting(text: string): HTMLElement {
  const element = document.createElement("div");
  element.dataset.exiting = "true";
  element.textContent = text;
  document.body.append(element);
  return element;
}

it.each(["input", "textarea"])("focus and type replace a labelled %s and emit its input event", async (tag) => {
  document.body.innerHTML = `<label for="description">Description</label><${tag} id="description"></${tag}>`;
  const field = document.getElementById("description") as HTMLInputElement | HTMLTextAreaElement;
  const seen: string[] = [];
  field.addEventListener("input", () => seen.push(field.value));
  const ui = createUi(document, "en");
  await ui.focus("Description");
  const done = ui.type("Project notes\nwith another line");
  await vi.advanceTimersByTimeAsync(200);
  await done;
  expect(document.activeElement).toBe(field);
  expect(seen).toEqual([tag === "textarea" ? "Project notes\nwith another line" : "Project noteswith another line"]);
});

it("focus finds a field by the accessible name of a label whose visible text is shorter", async () => {
  document.body.innerHTML =
    '<label for="dir"><span aria-hidden="true">Directory</span><span class="sr-only">Directory for Notes</span></label><input id="dir">';
  const ui = createUi(document, "en");
  await ui.focus("Directory for Notes");
  expect(document.activeElement).toBe(document.getElementById("dir"));
});

it("stuckExit names what is still closing once the wait is spent", async () => {
  exiting("General Appearance");
  const result = stuckExit(document);
  await vi.advanceTimersByTimeAsync(6000);
  expect(await result).toBe("<div> beginning “General Appearance”");
});

it("stuckExit reports nothing for an exit that ends within the wait", async () => {
  const element = exiting("General");
  const result = stuckExit(document);
  await vi.advanceTimersByTimeAsync(200);
  element.remove();
  await vi.advanceTimersByTimeAsync(100);
  expect(await result).toBeUndefined();
});

it("tabToRegion sends Tab before it focuses the region its dialog's title names", async () => {
  document.body.innerHTML = "<h2 id=\"title\">Import configuration</h2><div role=\"region\" tabindex=\"0\" aria-labelledby=\"title\"></div>";
  const region = document.querySelector<HTMLElement>("[role=region]")!;
  const seen: string[] = [];
  document.addEventListener("keydown", (event) => seen.push(`${event.key}:${document.activeElement === region}`), { once: true });
  const done = createUi(document, "en").tabToRegion("Import configuration");
  await vi.advanceTimersByTimeAsync(500);
  await done;
  expect(seen).toEqual(["Tab:false"]);
  expect(document.activeElement).toBe(region);
});

it("tabToRegion fails where no region has the name, as in a window tall enough for the dialog", async () => {
  document.body.innerHTML = "<h2 id=\"title\">Import configuration</h2><div tabindex=\"-1\" aria-labelledby=\"title\"></div>";
  const done = expect(createUi(document, "en").tabToRegion("Import configuration")).rejects.toThrow(/No scroll region/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
});

it("scrollPanel scrolls the scroller inside the tab panel its tab names", async () => {
  document.body.innerHTML = "<div role=\"tab\" id=\"tab\">Agent accounts</div><div role=\"tabpanel\" aria-labelledby=\"tab\"><div id=\"scroller\" style=\"overflow-y:auto\"></div></div>";
  const scroller = document.getElementById("scroller")!;
  Object.defineProperty(scroller, "scrollHeight", { value: 900 });
  Object.defineProperty(scroller, "clientHeight", { value: 400 });
  const scrolled: ScrollToOptions[] = [];
  scroller.scrollTo = ((options: ScrollToOptions) => { scrolled.push(options); }) as typeof scroller.scrollTo;
  const done = createUi(document, "en").scrollPanel("Agent accounts", 420);
  await vi.advanceTimersByTimeAsync(500);
  await done;
  expect(scrolled).toEqual([{ top: 420, behavior: "instant" }]);
});

it("scrollPanel fails where no panel of that name has a vertical scroller", async () => {
  document.body.innerHTML = "<div role=\"tab\" id=\"tab\">Agent accounts</div><div role=\"tabpanel\" aria-labelledby=\"tab\"><div></div></div>";
  const done = expect(createUi(document, "en").scrollPanel("Agent accounts", 420)).rejects.toThrow(/No tab panel named/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
});

it("press takes the control inside a band of the same name, not the band", async () => {
  document.body.innerHTML = "<div role=\"row\" id=\"band\"><button id=\"fold\">Staged15</button></div>";
  const pressed: string[] = [];
  document.addEventListener("click", (event) => pressed.push((event.target as Element).id));
  const done = createUi(document, "en").press("Staged15");
  await vi.advanceTimersByTimeAsync(500);
  await done;
  expect(pressed).toEqual(["fold"]);
});

it("press names a menu item by its label without its keyboard hint", async () => {
  document.body.innerHTML = '<div role="menuitem"><span><label data-slot="label"><span>Archive</span></label></span><kbd slot="keyboard">⇧⌘A</kbd></div>';
  const item = document.querySelector<HTMLElement>('[role="menuitem"]')!;
  const pressed = vi.fn();
  item.addEventListener("click", pressed);
  const done = createUi(document, "en").press("Archive");
  await vi.advanceTimersByTimeAsync(500);
  await done;
  expect(pressed).toHaveBeenCalledOnce();
});

it("pressEach presses every control of that name once, in document order", async () => {
  document.body.innerHTML = "<button id=\"a\">Details</button><button id=\"b\">Details</button><button id=\"c\">Other</button>";
  const pressed: string[] = [];
  document.addEventListener("click", (event) => pressed.push((event.target as Element).id));
  const done = createUi(document, "en").pressEach("Details");
  await vi.advanceTimersByTimeAsync(1000);
  await done;
  expect(pressed).toEqual(["a", "b"]);
});

it("changeRow names a change row by its file and status in the window's language, whatever follows", () => {
  const english = createUi(document, "en").changeRow("new-name.ts", "renamed");
  const chinese = createUi(document, "zh-Hans").changeRow("new-name.ts", "renamed");
  const translatedFrom = format("zh-Hans", "git.row.detail", {
    name: "new-name.ts", status: format("zh-Hans", "git.status.renamed"),
    detail: format("zh-Hans", "git.row.from", { path: "src/old-name.ts" }),
  });
  const translatedSelected = format("zh-Hans", "git.row.selected", { name: "new-name.ts", status: format("zh-Hans", "git.status.renamed") });
  const translatedFromSelected = format("zh-Hans", "git.row.detail.selected", {
    name: "new-name.ts", status: format("zh-Hans", "git.status.renamed"),
    detail: format("zh-Hans", "git.row.from", { path: "src/old-name.ts" }),
  });
  const matches = (matcher: Matcher, name: string) => (typeof matcher === "string" ? matcher === name : matcher(name));
  expect(matches(english, "new-name.ts, renamed, from src/old-name.ts")).toBe(true);
  expect(matches(english, "new-name.ts, renamed")).toBe(true);
  expect(matches(english, "new-name.ts, renamed, selected")).toBe(true);
  expect(matches(english, "new-name.ts, renamed, from src/old-name.ts, selected")).toBe(true);
  expect(matches(english, "new-name.ts, modified")).toBe(false);
  expect(matches(english, translatedFrom)).toBe(false);
  expect(matches(chinese, translatedFrom)).toBe(true);
  expect(matches(chinese, translatedSelected)).toBe(true);
  expect(matches(chinese, translatedFromSelected)).toBe(true);
  expect(matches(chinese, "new-name.ts, renamed, from src/old-name.ts")).toBe(false);
});

it("a matcher from the helpers says what it looks for when a step fails", () => {
  const ui = createUi(document, "en");
  expect(String(ui.changeRow("new-name.ts", "renamed"))).toBe("changeRow(new-name.ts, renamed)");
  expect(String(ui.session("Hub"))).toBe("session(Hub)");
});

it("floatPane moves the pointer from the toggle onto the pane that floats in, so the pane stays up", async () => {
  document.body.innerHTML = "<button aria-label=\"Show files panel\"></button><div data-pane=\"aside\"><p>card</p></div>";
  const pane = document.querySelector<HTMLElement>("[data-pane]")!;
  // jsdom loads no styles, so the pane is as shown as the app's classes would make it.
  pane.style.position = "fixed";
  pane.style.visibility = "visible";
  pane.style.opacity = "1";
  const seen: string[] = [];
  document.addEventListener("pointerover", (event) => seen.push(`over:${(event.target as HTMLElement).tagName}`));
  document.addEventListener("pointerout", (event) => seen.push(`out:${(event.target as HTMLElement).tagName}`));
  const done = createUi(document, "en").floatPane("Show files panel");
  await vi.advanceTimersByTimeAsync(2000);
  await done;
  expect(seen).toEqual(["over:BUTTON", "out:BUTTON", "over:DIV"]);
});

it("floatPane fails where the toggle floats in no pane, as one whose pane is docked", async () => {
  document.body.innerHTML = "<button aria-label=\"Hide files panel\"></button><div data-pane=\"aside\"></div>";
  const done = expect(createUi(document, "en").floatPane("Hide files panel")).rejects.toThrow(/floated in no pane/);
  await vi.advanceTimersByTimeAsync(10_000);
  await done;
});

it("pressInPanel presses the control of the panel it names and leaves the same name in another untouched", async () => {
  document.body.innerHTML = "<section id=\"panel-files\"><button>Close panel</button></section><section id=\"panel-terminal\"><button>Close panel</button></section>";
  const pressed: string[] = [];
  for (const section of document.querySelectorAll("section")) section.querySelector("button")!.addEventListener("click", () => pressed.push(section.id));
  const done = createUi(document, "en").pressInPanel("terminal", "Close panel");
  await vi.advanceTimersByTimeAsync(500);
  await done;
  expect(pressed).toEqual(["panel-terminal"]);
});

it("pressInDiff presses the named control inside the diff renderer's shadow root", async () => {
  document.body.innerHTML = "<button aria-label=\"Show unmodified lines above\"></button><div id=\"diff\"></div>";
  const root = document.getElementById("diff")!.attachShadow({ mode: "open" });
  root.innerHTML = "<div data-expand-button data-expand-up aria-label=\"Show unmodified lines above\"></div>";
  const pressed: string[] = [];
  document.querySelector("button")!.addEventListener("click", () => pressed.push("outside"));
  root.querySelector("div")!.addEventListener("click", () => pressed.push("diff"));
  const done = createUi(document, "en").pressInDiff("Show unmodified lines above");
  await vi.advanceTimersByTimeAsync(500);
  await done;
  expect(pressed).toEqual(["diff"]);
});

it("pressWithModifier presses with the platform's shortcut modifier down and press without it", async () => {
  document.body.innerHTML = "<button>New session</button>";
  const seen: { meta: boolean; ctrl: boolean }[] = [];
  document.querySelector("button")!.addEventListener("click", (event) => seen.push({ meta: event.metaKey, ctrl: event.ctrlKey }));
  const ui = createUi(document, "en");
  const modified = ui.pressWithModifier("New session");
  await vi.advanceTimersByTimeAsync(200);
  await modified;
  const plain = ui.press("New session");
  await vi.advanceTimersByTimeAsync(200);
  await plain;
  expect(seen).toEqual([IS_MAC ? { meta: true, ctrl: false } : { meta: false, ctrl: true }, { meta: false, ctrl: false }]);
});

it("press finds a switch by the text beside it that labels it, past a label holding only the control", async () => {
  document.body.innerHTML = "<div><label><input type=\"checkbox\" role=\"switch\" aria-labelledby=\"words\"></label><span id=\"words\">Show agent replies</span></div>";
  const control = document.querySelector("input") as HTMLInputElement;
  const ui = createUi(document, "en");
  const pressed = ui.press("Show agent replies");
  await vi.advanceTimersByTimeAsync(300);
  await pressed;
  expect(control.checked).toBe(true);
});

it("chord presses a portable binding with this platform's modifiers on the focused control", async () => {
  document.body.innerHTML = "<button id=\"target\"></button>";
  document.getElementById("target")!.focus();
  const seen: string[] = [];
  document.addEventListener("keydown", (event) => seen.push([event.code, event.key, event.metaKey, event.ctrlKey, event.shiftKey].join(":")));
  const done = createUi(document, "en").chord({ code: "KeyF", primary: true, shift: true });
  await vi.advanceTimersByTimeAsync(400);
  await done;
  expect(seen).toEqual([`KeyF:F:${IS_MAC}:${!IS_MAC}:true`]);
});

// The sidebar's drawer, a panel's toggle and the breakpoints they hang on, below: the stylesheet declares the
// breakpoints and the window's width decides which side of them a media query falls.
const SHOW_SESSIONS = format("en", "titleBar.sidebar.show");
const HIDE_SESSIONS = format("en", "titleBar.sidebar.hide");

function windowOfWidth(width: number): void {
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  document.documentElement.style.setProperty("--breakpoint-phone", "782px");
  vi.stubGlobal("matchMedia", (query: string) => ({ matches: width >= Number(/min-width: ([\d.]+)px/.exec(query)![1]) }));
}
afterEach(() => vi.unstubAllGlobals());

/** A sidebar toggle and a drawer, closed (inert) until the toggle is pressed, holding a session row named Hub. */
function drawerWithToggle(): { toggle: HTMLButtonElement; hub: HTMLButtonElement; drawer: HTMLElement } {
  document.body.innerHTML = `<button id="toggle" aria-label="${SHOW_SESSIONS}"></button><nav data-region="sidebar" inert><button id="hub">Hub</button></nav>`;
  const toggle = document.getElementById("toggle") as HTMLButtonElement;
  const drawer = document.querySelector("nav") as HTMLElement;
  toggle.addEventListener("click", () => {
    const open = toggle.getAttribute("aria-label") === HIDE_SESSIONS;
    toggle.setAttribute("aria-label", open ? SHOW_SESSIONS : HIDE_SESSIONS);
    drawer.toggleAttribute("inert", open);
  });
  return { toggle, hub: document.getElementById("hub") as HTMLButtonElement, drawer };
}

it("press opens the sidebar's drawer first for a control that is only inside it, below the docked breakpoint", async () => {
  windowOfWidth(420);
  const { hub, drawer } = drawerWithToggle();
  const pressed = vi.fn();
  hub.addEventListener("click", pressed);
  const done = createUi(document, "en").press("Hub");
  await vi.advanceTimersByTimeAsync(2000);
  await done;
  expect(drawer.hasAttribute("inert")).toBe(false);
  expect(pressed).toHaveBeenCalledOnce();
});

it("press leaves the drawer alone in a docked window and fails for a control inert in it", async () => {
  windowOfWidth(1440);
  const { toggle } = drawerWithToggle();
  const done = expect(createUi(document, "en").press("Hub")).rejects.toThrow(/Nothing to press matching Hub/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
  expect(toggle.getAttribute("aria-label")).toBe(SHOW_SESSIONS);
});

it("press does not open the drawer for a control that only takes a moment to appear outside it", async () => {
  windowOfWidth(420);
  const { toggle } = drawerWithToggle();
  const pressed: string[] = [];
  document.addEventListener("click", (event) => pressed.push((event.target as Element).id));
  setTimeout(() => document.body.insertAdjacentHTML("beforeend", '<button id="late">Hub</button>'), 100);
  const done = createUi(document, "en").press("Hub");
  await vi.advanceTimersByTimeAsync(1000);
  await done;
  expect(toggle.getAttribute("aria-label")).toBe(SHOW_SESSIONS);
  expect(pressed).toEqual(["late"]);
});

it.each([
  ["Show", "titleBar.files.show", true],
  ["Hide", "titleBar.files.hide", false],
] as const)("showPanel presses a %s toggle only when it is the one that brings the panel in", async (_word, key, pressesIt) => {
  document.body.innerHTML = `<button aria-label="${format("en", key)}"></button>`;
  const pressed = vi.fn();
  document.addEventListener("click", pressed);
  const done = createUi(document, "en").showPanel("files");
  await vi.advanceTimersByTimeAsync(500);
  await done;
  expect(pressed).toHaveBeenCalledTimes(pressesIt ? 1 : 0);
});

it("showPanel waits for a toggle that is drawn late and fails where there is none", async () => {
  document.body.innerHTML = "";
  setTimeout(() => document.body.insertAdjacentHTML("beforeend", `<button aria-label="${format("en", "titleBar.report.show")}"></button>`), 1000);
  const pressed = vi.fn();
  document.addEventListener("click", pressed);
  const late = createUi(document, "en").showPanel("report");
  await vi.advanceTimersByTimeAsync(1500);
  await late;
  expect(pressed).toHaveBeenCalledOnce();
  document.body.innerHTML = "";
  const none = expect(createUi(document, "en").showPanel("terminal")).rejects.toThrow(/Nothing is named Show terminal panel or Hide terminal panel/);
  await vi.advanceTimersByTimeAsync(6000);
  await none;
});

it.each([
  [420, true],
  [1440, false],
])("closeDrawer closes the drawer when it is open in a window of %i px and does nothing docked", async (width, closes) => {
  windowOfWidth(width);
  const { toggle } = drawerWithToggle();
  if (width < 1168) toggle.click();
  else toggle.setAttribute("aria-label", HIDE_SESSIONS);
  const done = createUi(document, "en").closeDrawer();
  await vi.advanceTimersByTimeAsync(500);
  await done;
  expect(toggle.getAttribute("aria-label")).toBe(closes ? SHOW_SESSIONS : HIDE_SESSIONS);
});

it("hoverTerminalUrl puts the pointer on the first row of a link that wraps in a narrow terminal", async () => {
  document.body.innerHTML = '<div data-region="terminal"><div class="xterm"><div class="xterm-screen"></div></div></div>';
  const xterm = document.querySelector(".xterm") as HTMLElement & { octoboardTerminalText?: unknown };
  const rows = ["xxxxabcdef", "gh        ", "          "];
  xterm.octoboardTerminalText = { cols: () => 10, rows: () => 3, row: (y: number) => rows[y] };
  const screen = document.querySelector(".xterm-screen") as HTMLElement;
  screen.getBoundingClientRect = () => ({ x: 0, y: 0, width: 100, height: 30, top: 0, left: 0, right: 100, bottom: 30, toJSON: () => ({}) });
  const moves: [number, number][] = [];
  screen.addEventListener("mousemove", (event) => {
    moves.push([(event as MouseEvent).clientX, (event as MouseEvent).clientY]);
    xterm.classList.add("xterm-cursor-pointer");
  });
  const done = createUi(document, "en").hoverTerminalUrl("abcdefgh");
  await vi.advanceTimersByTimeAsync(500);
  await done;
  // Cell 4 of row 0 starts it and six cells of the row hold it: the middle is cell 7, in the row's own height.
  expect(moves[0]).toEqual([70, 5]);
});
