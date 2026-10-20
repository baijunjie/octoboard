// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { useWindowAttended } from "./useWindowAttended";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Probe(): string {
  return String(useWindowAttended());
}

it("follows the document's visibility and the window's focus, as each changes", () => {
  let visibility: DocumentVisibilityState = "visible";
  let focused = true;
  const visible = vi.spyOn(document, "visibilityState", "get").mockImplementation(() => visibility);
  const hasFocus = vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    visible.mockRestore();
    hasFocus.mockRestore();
  });
  act(() => root.render(<Probe />));
  expect(container.textContent).toBe("true");
  act(() => {
    visibility = "hidden";
    document.dispatchEvent(new Event("visibilitychange"));
  });
  expect(container.textContent).toBe("false");
  act(() => {
    visibility = "visible";
    document.dispatchEvent(new Event("visibilitychange"));
  });
  expect(container.textContent).toBe("true");
  act(() => {
    focused = false;
    window.dispatchEvent(new Event("blur"));
  });
  expect(container.textContent).toBe("false");
  act(() => {
    focused = true;
    window.dispatchEvent(new Event("focus"));
  });
  expect(container.textContent).toBe("true");
});
