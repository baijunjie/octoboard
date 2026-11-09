// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import { MermaidBlock } from "./MermaidBlock";
import { LAZY_IMPORT_TIMEOUT_MS } from "../../test/lazyImport";
import { markNames } from "../viewerTestSupport";

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
});

function mount(source: string, overBudget = false): void {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<MermaidBlock source={source} overBudget={overBudget} theme="light" />));
}

it("draws a diagram as an image with a text alternative, and does not follow a link inside it", async () => {
  render.mockResolvedValue({ svg: '<svg><a href="https://example.com/evil">go</a></svg>' });
  mount("accTitle: Flow\naccDescr: From A to B\ngraph TD\nA-->B");
  await vi.waitFor(() => expect(document.querySelector("[role=region]")).not.toBeNull(), { timeout: LAZY_IMPORT_TIMEOUT_MS });
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

it("shows an over-budget diagram as source without drawing it", async () => {
  render.mockResolvedValue({ svg: "<svg></svg>" });
  // The library loads on first use, so a block that is drawn reaches `render` only after an async import: the in-budget
  // block beside it is drawn, and by then an over-budget block that was going to be drawn would have been too.
  mount("graph TD\nOVER-->BUDGET", true);
  mount("graph TD\nA-->B");
  await vi.waitFor(() => expect(document.querySelector("[role=region]")).not.toBeNull(), { timeout: LAZY_IMPORT_TIMEOUT_MS });
  expect(render).toHaveBeenCalledTimes(1);
  expect(render).toHaveBeenCalledWith(expect.any(String), "graph TD\nA-->B");
  expect(markNames()).toEqual(["Shown as source because the document is large."]);
  expect(document.body.textContent).toContain("OVER-->BUDGET");
});

it.each([
  new Error("nope"),
  Object.assign(new Error("Diagram type is unknown"), { name: "UnknownDiagramError" }),
  Object.assign(new Error("Unexpected token"), { hash: { expected: ["NODE"], token: "EOF", line: 2 } }),
  new TypeError("Library initialization failed"),
])("shows the source and says so for a Mermaid failure, with no error alert dialog: %s", async (error) => {
  render.mockRejectedValue(error);
  mount("graph TD\nA-->");
  await vi.waitFor(() => expect(markNames()).toEqual(["This diagram could not be drawn, so its source is shown."]), { timeout: LAZY_IMPORT_TIMEOUT_MS });
  expect(document.querySelector("[role=img]")).toBeNull();
  expect(document.body.textContent).toContain("graph TD");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});
