// @vitest-environment jsdom
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { ChangeBodiesError, type ChangeBodies } from "./content";
import { EXPANSION_STEP, Expansion, type ExpansionStatus, type HunkExpander, type SeparatorLabels } from "./expansion";

const labels: SeparatorLabels = {
  unmodified: (count) => `${count} hidden`,
  unknown: "more may follow",
  above: "above",
  below: "below",
  between: "between",
  hiddenAll: (count) => `all ${count}`,
  all: "whole file",
  loading: "loading",
  failedHere: "failed",
};

// What the library draws for a diff of one long gap (1), as far as the expansion reads it: the gap's
// separator once in the gutter, where its buttons and label are shown, and once in the code column.
const separator = (column: "gutter" | "content", gap = 1) => `
  <div data-${column}>
    <div data-separator="line-info-basic" data-expand-index="${gap}">
      <div data-separator-wrapper>
        <div role="button" data-expand-button data-expand-up></div>
        <div role="button" data-expand-button data-expand-down></div>
        <div data-separator-content><span data-unmodified-lines>53 unmodified lines</span></div>
      </div>
    </div>
  </div>`;

let host: HTMLElement;
let status: ExpansionStatus[];
let expanded: [number, string, number][];
let load: ReturnType<typeof vi.fn<() => Promise<ChangeBodies>>>;
let showAll: ReturnType<typeof vi.fn<() => void>>;
let stop: ReturnType<typeof vi.fn<() => void>>;
let trailing: { gap: number; lines: number } | undefined;
let expansion: Expansion;

const expander: HunkExpander = { expandHunk: (gap, direction, lines) => expanded.push([gap, direction, lines]) };
const control = (selector: string) => host.shadowRoot!.querySelector<HTMLElement>(`[data-gutter] ${selector}`)!;
const key = (target: HTMLElement, name: string, shiftKey = false) => {
  const event = new KeyboardEvent("keydown", { key: name, shiftKey, bubbles: true, composed: true, cancelable: true });
  target.dispatchEvent(event);
  return event;
};
const press = async (element: HTMLElement) => {
  element.dispatchEvent(new MouseEvent("click", { bubbles: true, composed: true }));
  await new Promise((resolve) => setTimeout(resolve));
};

beforeEach(() => {
  host = document.body.appendChild(document.createElement("div"));
  host.attachShadow({ mode: "open" }).innerHTML = separator("gutter") + separator("content");
  status = [];
  expanded = [];
  load = vi.fn(async () => ({ old: "a\n", new: "b\n" }));
  showAll = vi.fn();
  stop = vi.fn();
  trailing = undefined;
  expansion = new Expansion({ load, labels: () => labels, trailing: () => trailing, report: (next) => status.push(next), showAll, stop });
  expansion.attach(host, expander);
});

afterEach(() => {
  expansion.detach();
  host.remove();
});

it("words the separators, makes the first copy of each control a tab stop, and offers the whole file once", () => {
  const root = host.shadowRoot!;
  expect(Array.from(root.querySelectorAll("[data-unmodified-lines]")).map((label) => label.textContent)).toEqual(["53 hidden", "53 hidden"]);
  expect(Array.from(root.querySelectorAll("[data-expand-button]")).map((button) => [button.getAttribute("aria-label"), button.getAttribute("tabindex")])).toEqual([
    ["above", "0"],
    ["below", "0"],
    [null, "-1"],
    [null, "-1"],
    ["whole file", "0"],
  ]);
});

it("makes the label a button, a tab stop and named in the first copy only, hidden in the rest", () => {
  const labelsOf = Array.from(host.shadowRoot!.querySelectorAll<HTMLElement>("[data-separator-content]"));
  expect(labelsOf.map((label) => [label.getAttribute("role"), label.getAttribute("tabindex"), label.getAttribute("aria-label"), label.getAttribute("title"), label.getAttribute("aria-hidden")])).toEqual([
    ["button", "0", "all 53", "53 hidden", null],
    [null, "-1", null, null, "true"],
  ]);
});

it("leaves the labels of a separator the library drew with no expansion as plain text", () => {
  // Without `loadBodies`, or after `stop()`, the library draws no `data-expand-index` and no arrows.
  host.shadowRoot!.innerHTML = `
    <div data-gutter>
      <div data-separator="line-info-basic">
        <div data-separator-wrapper>
          <div data-separator-content><span data-unmodified-lines>53 unmodified lines</span></div>
        </div>
      </div>
    </div>`;
  expansion.decorate();
  const label = control("[data-separator-content]");
  expect([label.getAttribute("role"), label.getAttribute("tabindex"), key(label, "Enter").defaultPrevented]).toEqual([null, null, false]);
  expect(expanded).toEqual([]);
});

it("reads the bodies on the first expansion only, then reveals 20 lines each time and the rest on the third", async () => {
  await press(control("[data-expand-up]"));
  expect(status).toEqual([{ kind: "loading" }, { kind: "idle" }]);
  await press(control("[data-expand-down]"));
  await press(control("[data-expand-up]"));
  expect(load).toHaveBeenCalledTimes(1);
  expect(expanded).toEqual([
    [1, "up", EXPANSION_STEP],
    [1, "down", EXPANSION_STEP],
    [1, "both", Number.POSITIVE_INFINITY],
  ]);
  expect((await expansion.files({ name: "a.ts" })).newFile).toEqual({ name: "a.ts", contents: "b\n" });
});

it("reveals the whole gap when the separator's own text is used", async () => {
  await press(control("[data-unmodified-lines]"));
  expect(expanded).toEqual([[1, "both", Number.POSITIVE_INFINITY]]);
});

it.each(["Enter", " "])("reveals the whole gap on %j pressed on the separator's label", async (name) => {
  const label = control("[data-separator-content]");
  label.focus();
  expect(key(label, name).defaultPrevented).toBe(true);
  await new Promise((resolve) => setTimeout(resolve));
  expect(expanded).toEqual([[1, "both", Number.POSITIVE_INFINITY]]);
});

it("shows the loading state at the separator while the bodies are read, and expands nothing while it waits", async () => {
  let arrive: (bodies: ChangeBodies) => void = () => {};
  load.mockReturnValueOnce(new Promise((resolve) => (arrive = resolve)));
  await press(control("[data-expand-up]"));
  await press(control("[data-expand-down]"));
  expect(host.shadowRoot!.querySelector("[data-unmodified-lines]")!.textContent).toBe("loading");
  expect(load).toHaveBeenCalledTimes(1);
  arrive({ old: "a", new: "b" });
  await new Promise((resolve) => setTimeout(resolve));
  expect(host.shadowRoot!.querySelector("[data-unmodified-lines]")!.textContent).toBe("53 hidden");
  expect(expanded).toEqual([[1, "up", EXPANSION_STEP]]);
});

it("opens the whole file from any separator", async () => {
  await press(host.shadowRoot!.querySelector<HTMLElement>("[data-whole-file]")!);
  expect(showAll).toHaveBeenCalledTimes(1);
});

it("says a failed read at the separator, and reads again on the next expansion", async () => {
  load.mockRejectedValueOnce(new ChangeBodiesError("failed", "no luck"));
  await press(control("[data-expand-up]"));
  expect(status.at(-1)).toEqual({ kind: "failed", message: "no luck" });
  expect(host.shadowRoot!.querySelector("[data-unmodified-lines]")!.textContent).toBe("failed");
  await press(control("[data-expand-up]"));
  expect(load).toHaveBeenCalledTimes(2);
  expect(expanded).toEqual([[1, "up", EXPANSION_STEP]]);
});

it("drops a failure said at the separator when the change is read again", async () => {
  load.mockRejectedValueOnce(new ChangeBodiesError("failed", "no luck"));
  await press(control("[data-expand-up]"));
  const label = () => host.shadowRoot!.querySelector("[data-unmodified-lines]")!.textContent;
  expect(label()).toBe("failed");
  expansion.forgetFailure();
  expect(label()).toBe("53 hidden");
});

it.each([
  [new ChangeBodiesError("unavailable"), { kind: "unavailable" }],
  [new ChangeBodiesError("changed"), { kind: "changed" }],
])("stops offering expansion when the bodies %j cannot be had", async (error, last) => {
  load.mockRejectedValueOnce(error);
  await press(control("[data-expand-up]"));
  expect(stop).toHaveBeenCalledTimes(1);
  expect(status.at(-1)).toEqual(last);
  expect(expanded).toEqual([]);
});

it("keeps the lines collapsed when the bodies are past the render budget", async () => {
  load.mockResolvedValueOnce({ old: "x\n".repeat(20_000), new: "" });
  await press(control("[data-expand-up]"));
  expect(stop).toHaveBeenCalledTimes(1);
  expect(status.at(-1)).toEqual({ kind: "overBudget" });
  expect(expanded).toEqual([]);
});

// The bar after the last hunk (gap 2 here): the file's length settles what it hides.
// The library draws it without a number until the bodies are handed over (as it cannot count the run
// before), and with the number still hidden after; `drawn` is what it wrote.
const UNCOUNTED = "More unchanged context may be available";
it.each<[string, string, { gap: number; lines: number } | undefined, [string, string] | null]>([
  ["the real count when lines follow", UNCOUNTED, { gap: 2, lines: 17 }, ["17 hidden", "all 17"]],
  ["its old wording when the length is unknown", UNCOUNTED, undefined, ["more may follow", "between"]],
  ["nothing when no line follows", UNCOUNTED, { gap: 2, lines: 0 }, null],
  ["the library's own count once part of the run is revealed", "30 unmodified lines", { gap: 2, lines: 50 }, ["30 hidden", "all 30"]],
  ["the library's own count over a daemon count of none", "30 unmodified lines", { gap: 2, lines: 0 }, ["30 hidden", "all 30"]],
])("words the bar after the last hunk with %s", (_, drawn, run, expected) => {
  host.shadowRoot!.innerHTML += (separator("gutter", 2) + separator("content", 2)).replaceAll("53 unmodified lines", drawn);
  trailing = run;
  expansion.decorate();
  const bars = Array.from(host.shadowRoot!.querySelectorAll<HTMLElement>('[data-separator][data-expand-index="2"]'));
  if (expected === null) {
    expect(bars).toEqual([]);
    expect(host.shadowRoot!.querySelectorAll("[data-expand-button][tabindex='0']")).toHaveLength(3);
    return;
  }
  expect(bars.map((bar) => bar.querySelector("[data-unmodified-lines]")!.textContent)).toEqual(Array(2).fill(expected[0]));
  // The control's tooltip is the visible sentence and its name the action.
  const label = bars[0].querySelector<HTMLElement>("[data-separator-content]")!;
  expect([label.getAttribute("title"), label.getAttribute("aria-label")]).toEqual([expected[0], expected[1]]);
});

it("moves Tab along the controls gap by gap and leaves the diff after the last", () => {
  host.shadowRoot!.innerHTML += separator("gutter", 2) + separator("content", 2);
  expansion.decorate();
  const root = host.shadowRoot!;
  const stops = Array.from(root.querySelectorAll<HTMLElement>("[data-expand-button][tabindex='0'], [data-separator-content][tabindex='0']"));
  const order = [
    control("[data-expand-up]"),
    control("[data-expand-down]"),
    control("[data-separator-content]"),
    root.querySelector<HTMLElement>("[data-content] [data-whole-file]")!,
  ];
  expect(stops).toHaveLength(8);
  order[0].focus();
  expect(key(order[0], "Tab").defaultPrevented).toBe(true);
  expect(root.activeElement).toBe(order[1]);
  key(order[1], "Tab");
  expect(root.activeElement).toBe(order[2]);
  key(order[2], "Tab");
  expect(root.activeElement).toBe(order[3]);
  key(order[3], "Tab");
  expect(root.activeElement?.closest("[data-separator]")).toBe(root.querySelectorAll("[data-gutter] [data-separator]")[1]);
  const last = root.querySelectorAll<HTMLElement>("[data-content] [data-whole-file]")[1];
  last.focus();
  expect(key(last, "Tab").defaultPrevented).toBe(false);
  expect(root.activeElement).toBe(last);
});

it("moves Shift+Tab from the first control to the code region", () => {
  const region = document.body.appendChild(document.createElement("div"));
  region.setAttribute("role", "region");
  region.tabIndex = 0;
  region.append(host);
  const first = control("[data-expand-up]");
  first.focus();
  expect(key(first, "Tab", true).defaultPrevented).toBe(true);
  expect(document.activeElement).toBe(region);
  region.remove();
});

it("gives focus back after a redraw to the same control, or the nearest one", async () => {
  const root = host.shadowRoot!;
  key(control("[data-expand-down]"), "Enter");
  await new Promise((resolve) => setTimeout(resolve));
  root.innerHTML = separator("gutter") + separator("content");
  expansion.attach(host, expander);
  expect(root.activeElement).toBe(control("[data-expand-down]"));
  root.innerHTML = (separator("gutter") + separator("content")).replaceAll("data-expand-down", "data-expand-gone");
  expansion.attach(host, expander);
  expect(root.activeElement).toBe(control("[data-expand-up]"));
});

it("gives focus back after the label revealed its whole gap to the next gap's control, else to the code", async () => {
  const root = host.shadowRoot!;
  const label = control("[data-separator-content]");
  label.focus();
  key(label, "Enter");
  await new Promise((resolve) => setTimeout(resolve));
  root.innerHTML = separator("gutter", 2) + separator("content", 2);
  expansion.attach(host, expander);
  expect(root.activeElement).toBe(control("[data-expand-up]"));
  expect(root.activeElement?.closest("[data-separator]")?.getAttribute("data-expand-index")).toBe("2");
  const region = document.body.appendChild(document.createElement("div"));
  region.setAttribute("role", "region");
  region.tabIndex = 0;
  region.append(host);
  key(control("[data-separator-content]"), "Enter");
  await new Promise((resolve) => setTimeout(resolve));
  root.innerHTML = "";
  expansion.attach(host, expander);
  expect(document.activeElement).toBe(region);
  region.remove();
});

// VoiceOver presses the control it is on with a click, not a key, and the redraw takes that control
// away with the focus on it.
it("gives focus back after a redraw when the control clicked had focus, as a screen reader's press does", async () => {
  const root = host.shadowRoot!;
  const label = control("[data-separator-content]");
  label.focus();
  // On the text inside the label, where a click on it lands.
  await press(label.querySelector<HTMLElement>("[data-unmodified-lines]")!);
  root.innerHTML = separator("gutter", 2) + separator("content", 2);
  expansion.attach(host, expander);
  expect(root.activeElement).toBe(control("[data-expand-up]"));
  expect(root.activeElement?.closest("[data-separator]")?.getAttribute("data-expand-index")).toBe("2");
});

it("gives focus to the code with a focus ring when a focused control's press leaves no control", async () => {
  const region = document.body.appendChild(document.createElement("div"));
  region.setAttribute("role", "region");
  region.tabIndex = 0;
  region.append(host);
  const label = control("[data-separator-content]");
  label.focus();
  const modality = getInteractionModality();
  // The getter can return the initial null state, which the setter accepts at runtime.
  onTestFinished(() => Reflect.apply(setInteractionModality, undefined, [modality]));
  setInteractionModality("pointer");
  await press(label);
  host.shadowRoot!.innerHTML = "";
  expansion.attach(host, expander);
  expect(document.activeElement).toBe(region);
  expect(getInteractionModality()).toBe("keyboard");
  region.remove();
});

it("leaves focus where it was after a click on a control that did not have it", async () => {
  const region = document.body.appendChild(document.createElement("div"));
  region.setAttribute("role", "region");
  region.tabIndex = 0;
  region.append(host);
  region.focus();
  await press(control("[data-separator-content]"));
  host.shadowRoot!.innerHTML = separator("gutter", 2) + separator("content", 2);
  expansion.attach(host, expander);
  expect(document.activeElement).toBe(region);
  expect(host.shadowRoot!.activeElement).toBeNull();
  region.remove();
});
