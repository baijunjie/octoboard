// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { PanelFrameProvider, PanelHeader } from "./PanelHeader";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("keeps the accessible heading, tab and action nodes when the number of columns changes", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (compact: boolean) => act(() => root.render(
    <PanelFrameProvider value={{ compact, canMaximize: true, maximized: false, onToggleMaximized: () => {}, onClose: () => {} }}>
      <PanelHeader title="Terminal" actions={<button>New terminal</button>}>
        <div role="tablist" aria-label="Shell terminals"><button role="tab" aria-selected="true">shell</button></div>
      </PanelHeader>
    </PanelFrameProvider>,
  ));
  render(false);
  const heading = container.querySelector("h2")!;
  const list = container.querySelector("[role=tablist]");
  const tab = container.querySelector<HTMLElement>("[role=tab]")!;
  const action = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "New terminal");
  act(() => tab.focus());
  for (const compact of [true, false]) {
    render(compact);
    expect(container.querySelector("h2")).toBe(heading);
    expect(heading.textContent).toBe("Terminal");
    expect(heading.hasAttribute("aria-hidden")).toBe(false);
    expect(heading.hidden).toBe(false);
    expect(container.querySelector("[role=tablist]")).toBe(list);
    expect(container.querySelector("[role=tab]")).toBe(tab);
    expect(Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "New terminal")).toBe(action);
    expect(tab.getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(tab);
  }
});
