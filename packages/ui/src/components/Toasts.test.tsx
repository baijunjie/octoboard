// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { toast } from "@heroui/react";
import { afterEach, expect, it, vi } from "vitest";

import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { Toasts } from "./Toasts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const daemon: Daemon = {
  store: createStateStore({}),
  request: vi.fn(),
  toastError: vi.fn(),
  onToast: () => () => {},
  dismissTrustPrompt: () => {},
  reconnect: () => {},
} as unknown as Daemon;

class FakeResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

/** A `matchMedia` that answers the toast breakpoint's `min-width` query for a window `width` wide. */
function stubWidth(width: number): void {
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: width >= Number(/min-width: (\d+)px/.exec(query)?.[1]),
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}

afterEach(() => vi.unstubAllGlobals());

// The toast region only mounts once a toast is queued, so the placement is read off a shown one.
it("puts the toasts at the top in a window narrower than 640px and at the bottom end otherwise", () => {
  for (const [width, expected] of [
    [390, "toast-region--top"],
    [639, "toast-region--top"],
    [640, "toast-region--bottom-end"],
    [1280, "toast-region--bottom-end"],
  ] as const) {
    stubWidth(width);
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    try {
      act(() => {
        root.render(
          <DaemonProvider value={daemon}>
            <Toasts focusTerminal={() => {}} />
          </DaemonProvider>,
        );
      });
      act(() => {
        toast("hello");
      });
      expect(document.querySelector(".toast-region")?.className).toContain(expected);
    } finally {
      toast.clear();
      act(() => root.unmount());
      container.remove();
    }
  }
});
