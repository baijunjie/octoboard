// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { ChangeBodiesError, type ChangeBodies } from "./content";
import { EXPANSION_STEP, Expansion, type ExpansionStatus, type HunkExpander, type SeparatorLabels } from "./expansion";

const labels: SeparatorLabels = {
  unmodified: (count) => `${count} hidden`,
  unknown: "more may follow",
  above: "above",
  below: "below",
  between: "between",
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
  expansion = new Expansion({ load, labels: () => labels, report: (next) => status.push(next), showAll, stop });
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
  [new ChangeBodiesError("unavailable"), { kind: "idle" }],
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
  expect(expanded).toEqual([]);
});

it("moves Tab along the controls gap by gap and leaves the diff after the last", () => {
  host.shadowRoot!.innerHTML += separator("gutter", 2) + separator("content", 2);
  expansion.decorate();
  const root = host.shadowRoot!;
  const stops = Array.from(root.querySelectorAll<HTMLElement>("[data-expand-button][tabindex='0']"));
  const order = [control("[data-expand-up]"), control("[data-expand-down]"), root.querySelector<HTMLElement>("[data-content] [data-whole-file]")!];
  expect(stops).toHaveLength(6);
  order[0].focus();
  expect(key(order[0], "Tab").defaultPrevented).toBe(true);
  expect(root.activeElement).toBe(order[1]);
  key(order[1], "Tab");
  expect(root.activeElement).toBe(order[2]);
  key(order[2], "Tab");
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
