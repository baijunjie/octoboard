// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { withModality } from "../components/dialog/dialogTestSupport";
import { Button } from "../primitives";
import { useLinkedFile } from "./useLinkedFile";

it.each([true, false])("restores the linked viewer's opener after removal, falling back to the terminal if needed (%s)", async (openerCanFocus) => {
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => {
    act(() => root.unmount());
    host.remove();
  });

  const button = (name: string) => host.querySelector<HTMLButtonElement>(`button[data-action="${name}"]`)!;
  const focusTerminal = vi.fn(() => button("terminal").focus());
  const returnFocus = vi.fn(() => {
    expect(host.querySelector("[role=dialog]")).toBeNull();
    if (openerCanFocus) button("open").focus();
    return openerCanFocus;
  });
  function Probe() {
    const { linkedFile, viewLinkedFile, setLinkedFile, closeLinkedFile } = useLinkedFile({ focusTerminal });
    return (
      <>
        <Button data-action="open" onPress={() => viewLinkedFile({ project: "p", path: "original.md" }, returnFocus)}>Open</Button>
        <Button data-action="terminal">Terminal</Button>
        {linkedFile && (
          <div role="dialog" aria-label="Linked file">
            <button data-action="next" onClick={() => setLinkedFile({ project: "p", path: "next.md" })}>Next</button>
            <button data-action="close" onKeyDown={(event) => { if (event.key === "Escape") closeLinkedFile(); }}>Close</button>
          </div>
        )}
      </>
    );
  }

  act(() => root.render(<Probe />));
  act(() => button("open").click());
  act(() => button("next").click());
  expect(returnFocus).not.toHaveBeenCalled();
  withModality("keyboard");
  await act(async () => {
    const close = button("close");
    close.focus();
    close.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
  const destination = button(openerCanFocus ? "open" : "terminal");
  expect(returnFocus).toHaveBeenCalledTimes(1);
  expect(document.activeElement).toBe(destination);
  expect(destination.hasAttribute("data-focus-visible")).toBe(true);
});
