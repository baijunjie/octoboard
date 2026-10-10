// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import { Dialog } from "../dialogs/Dialog";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ConnectionBanner } from "./ConnectionBanner";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const daemon: Daemon = {
  store: createStateStore({}),
  request: vi.fn(),
  toastError: vi.fn(),
  toastNotice: vi.fn(),
  onToast: () => () => {},
  dismissTrustPrompt: () => {},
  reconnect: () => {},
  terminalUrl: () => "",
};

// The banner measures itself with one as it appears, and jsdom has none of its own.
class FakeResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

afterEach(() => vi.unstubAllGlobals());

/** Hidden from the rest of the window by an open dialog: React Aria sets `inert` where the browser
 * supports it and `aria-hidden` where it does not, and jsdom is the latter today — so both are
 * read, and the test says the same thing whichever path react-aria takes. */
const hidden = (element: Element | null | undefined) => element?.closest('[aria-hidden="true"], [inert]') ?? null;

/** The window's own order: the banner before the dialogs, mounted whatever the connection does,
 * and beside them something ordinary that the dialog is expected to hide. */
function Host({ state }: { state: "open" | "closed" }) {
  return (
    <DaemonProvider value={daemon}>
      <div data-testid="elsewhere">the rest of the window</div>
      <ConnectionBanner state={state} onRetry={() => {}} focusTerminal={() => {}} />
      <Dialog title="Rename" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}>
        <span>body</span>
      </Dialog>
    </DaemonProvider>
  );
}

/** Opens a dialog with the connection still up, then spends the attempts — the order the defect
 * was filed in — and hands what the check needs to `check`. */
function whenTheConnectionDropsUnderAnOpenDialog(check: (found: { banner: HTMLElement; elsewhere: HTMLElement }) => void): void {
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<Host state="open" />));
    act(() => root.render(<Host state="closed" />));
    const banner = container.querySelector<HTMLElement>('[data-region="banner"]');
    const elsewhere = container.querySelector<HTMLElement>('[data-testid="elsewhere"]');
    if (!banner) throw new Error("the banner did not render in its disconnected state");
    if (!elsewhere) throw new Error("the element the dialog is meant to hide did not render");
    check({ banner, elsewhere });
  } finally {
    act(() => root.unmount());
    container.remove();
  }
}

// What an open modal does to everything outside it is what would otherwise take the only way back
// out of a lost connection with it. The press landing on the button rather than on the modal's
// backdrop is a matter of stacking, which jsdom cannot judge at all (no hit testing, no
// stylesheet), and is checked in a browser instead.
it("keeps the banner's Retry out of what an open dialog hides from the rest of the window", () => {
  whenTheConnectionDropsUnderAnOpenDialog(({ banner, elsewhere }) => {
    expect(hidden(elsewhere)).not.toBe(null);
    expect(hidden(banner.querySelector("button"))).toBe(null);
  });
});
