// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { MermaidBlock } from "./MermaidBlock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const render = vi.fn();
const initialize = vi.fn();

vi.mock("mermaid", () => ({
  default: {
    initialize: (...args: unknown[]) => initialize(...args),
    render: (...args: unknown[]) => render(...args),
  },
}));

afterEach(() => {
  render.mockReset();
  initialize.mockReset();
  while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
});

async function mount(source: string, overBudget = false): Promise<void> {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<MermaidBlock source={source} overBudget={overBudget} theme="light" />));
  for (let i = 0; i < 20; i += 1) await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
}

it("draws a diagram as an image with a text alternative, and does not follow a link inside it", async () => {
  render.mockResolvedValue({ svg: '<svg><a href="https://example.com/evil">go</a></svg>' });
  await mount("accTitle: Flow\naccDescr: From A to B\ngraph TD\nA-->B");
  const region = document.querySelector<HTMLElement>("[role=region]")!;
  expect(region.getAttribute("aria-label")).toBe("Flow");
  expect(region.getAttribute("dir")).toBe("ltr");
  expect(region.querySelector("[role=img]")?.getAttribute("aria-label")).toBe("Flow");
  expect(region.textContent).toContain("From A to B");
  expect(initialize).toHaveBeenCalledWith(expect.objectContaining({ securityLevel: "strict", htmlLabels: false }));
  const link = region.querySelector("a")!;
  const click = new MouseEvent("click", { bubbles: true, cancelable: true });
  act(() => void link.dispatchEvent(click));
  expect(click.defaultPrevented).toBe(true);
});

it("shows the source when a diagram cannot be drawn", async () => {
  const error = new Error("nope");
  render.mockRejectedValue(error);
  await mount("graph TD\nA-->B");
  expect(document.querySelector("[role=img]")).toBeNull();
  expect(document.body.textContent).toContain("This diagram could not be drawn, so its source is shown.");
  expect(document.body.textContent).toContain("graph TD");
  // A failure of what the document shows is on the screen only: no error alert dialog.
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("shows an over-budget diagram as source without drawing it", async () => {
  await mount("graph TD\nA-->B", true);
  expect(render).not.toHaveBeenCalled();
  expect(document.body.textContent).toContain("Shown as source because the document is large.");
  expect(document.body.textContent).toContain("graph TD");
});

it.each([
  Object.assign(new Error("Diagram type is unknown"), { name: "UnknownDiagramError" }),
  Object.assign(new Error("Unexpected token"), { hash: { expected: ["NODE"], token: "EOF", line: 2 } }),
  new TypeError("Library initialization failed"),
])("shows the source and says so for a Mermaid failure, with no error alert dialog: %s", async (error) => {
  render.mockRejectedValue(error);
  await mount("graph TD\nA-->");
  expect(document.body.textContent).toContain("This diagram could not be drawn, so its source is shown.");
  expect(document.body.textContent).toContain("graph TD");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});
