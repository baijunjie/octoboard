// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { useQuickCreateModifier } from "./useQuickCreateModifier";
import { IS_MAC } from "./useWindowShortcut";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Reader(): React.ReactElement {
  return <span>{useQuickCreateModifier() ? "held" : "free"}</span>;
}

function mount(): { text: () => string; unmount: () => void } {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Reader />));
  let mounted = true;
  const unmount = () => {
    if (!mounted) return;
    mounted = false;
    act(() => root.unmount());
    container.remove();
  };
  onTestFinished(() => {
    act(() => void window.dispatchEvent(new Event("blur")));
    unmount();
  });
  return { text: () => container.textContent ?? "", unmount };
}

const MODIFIER = IS_MAC ? { metaKey: true } : { ctrlKey: true };
const hold = () => act(() => void window.dispatchEvent(new KeyboardEvent("keydown", MODIFIER)));

it("keeps the hold when the reader is swapped for another while the key stays down", () => {
  const first = mount();
  hold();
  expect(first.text()).toBe("held");
  first.unmount();
  expect(mount().text()).toBe("held");
});

it("lets go when the modifier's key comes up", () => {
  const { text } = mount();
  hold();
  expect(text()).toBe("held");
  act(() => void window.dispatchEvent(new KeyboardEvent("keyup")));
  expect(text()).toBe("free");
});

it("lets go when the window loses focus", () => {
  const { text } = mount();
  hold();
  act(() => void window.dispatchEvent(new Event("blur")));
  expect(text()).toBe("free");
});

it("lets go when the page is hidden", () => {
  const { text } = mount();
  hold();
  const hidden = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
  onTestFinished(() => hidden.mockRestore());
  act(() => void document.dispatchEvent(new Event("visibilitychange")));
  expect(text()).toBe("free");
});
