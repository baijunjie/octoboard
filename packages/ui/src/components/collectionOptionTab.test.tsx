// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { useCollectionOptionTab } from "./collectionOptionTab";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Collection() {
  const ref = useCollectionOptionTab(null);
  return (
    <div ref={ref} tabIndex={-1} data-testid="collection">
      <button>first</button>
      <button>last shown</button>
      <button tabIndex={-1}>out of the tab order</button>
      <div inert><button>inert</button></div>
      <button hidden>hidden</button>
      <div style={{ display: "none" }}><button>not displayed</button></div>
    </div>
  );
}

function mount(): { collection: HTMLElement; button: (name: string) => HTMLElement } {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Collection />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return {
    collection: container.querySelector<HTMLElement>("[data-testid=collection]")!,
    button: (name) => [...container.querySelectorAll<HTMLElement>("button")].find((button) => button.textContent === name)!,
  };
}

const press = (element: HTMLElement, init: KeyboardEventInit) =>
  act(() => {
    element.focus();
    element.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true, ...init }));
  });

it("moves focus to the collection's last tabbable element on Option+Tab, skipping what Tab cannot reach", () => {
  const { button } = mount();
  press(button("first"), { altKey: true });
  expect(document.activeElement).toBe(button("last shown"));
});

it("moves focus to the collection itself on Option+Shift+Tab", () => {
  const { collection, button } = mount();
  press(button("last shown"), { altKey: true, shiftKey: true });
  expect(document.activeElement).toBe(collection);
});

it("leaves Tab without Option, and Option with Control or Command, alone", () => {
  const { button } = mount();
  for (const init of [{}, { shiftKey: true }, { altKey: true, ctrlKey: true }, { altKey: true, metaKey: true }]) {
    press(button("first"), init);
    expect(document.activeElement).toBe(button("first"));
  }
});
