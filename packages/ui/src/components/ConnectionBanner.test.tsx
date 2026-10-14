// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import type { ConnectionState } from "../daemon-client";
import { Dialog } from "../dialogs/Dialog";
import { useRegionCycle } from "../layout/useRegionCycle";
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

function announcementBanner(state: ConnectionState) {
  vi.useFakeTimers();
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    try {
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });
  const render = (next: ConnectionState) => act(() => root.render(
    <ConnectionBanner state={next} onRetry={() => {}} focusTerminal={() => {}} />,
  ));
  render(state);
  const advance = (ms: number) => act(() => vi.advanceTimersByTime(ms));
  return { container, render, advance };
}

it.each(["reconnecting", "closed"] as const)("defers an initial %s status in an already-mounted region", (state) => {
  const { container, advance } = announcementBanner(state);
  const region = container.querySelector('[role="status"]');
  expect(region).not.toBe(null);
  expect(region!.textContent).toBe("");
  advance(100);
  expect(container.querySelector('[role="status"]')).toBe(region);
  expect(region!.textContent).toBe(state === "closed"
    ? "Disconnected from the daemon." : "Disconnected from the daemon. Reconnecting…");
  const strip = container.querySelector('[data-region="banner"]')!;
  expect(strip.hasAttribute("role")).toBe(false);
  expect(strip.querySelector("span")!.getAttribute("aria-hidden")).toBe("true");
  if (state === "closed") expect(hidden(strip.querySelector("button"))).toBe(null);
});

it("announces each outage in the same region and updates it when automatic retries are spent", () => {
  const { container, render, advance } = announcementBanner("open");
  const region = container.querySelector('[role="status"]');
  expect(region).not.toBe(null);
  advance(1000);
  expect(region!.textContent).toBe("");
  render("reconnecting");
  expect(region!.textContent).toBe("");
  advance(100);
  expect(region!.textContent).toBe("Disconnected from the daemon. Reconnecting…");
  render("closed");
  expect(region!.textContent).toBe("Disconnected from the daemon.");
  render("open");
  expect(region!.textContent).toBe("");
  render("closed");
  advance(100);
  expect(container.querySelectorAll('[role="status"]')).toHaveLength(1);
  expect(container.querySelector('[role="status"]')).toBe(region);
  expect(region!.textContent).toBe("Disconnected from the daemon.");
});

it("keeps a quickly recovered drop silent after a long healthy connection", () => {
  const { container, render, advance } = announcementBanner("open");
  advance(1000);
  const region = container.querySelector('[role="status"]');
  render("reconnecting");
  expect(region).not.toBe(null);
  expect(region!.textContent).toBe("");
  advance(50);
  render("open");
  advance(1000);
  expect(region!.textContent).toBe("");
});

function KeyboardHost({ state, menu = false }: { state: "open" | "closed" | "reconnecting"; menu?: boolean }) {
  useRegionCycle({
    shown: {
      topbar: true, rail: false, welcome: false, sidebar: false, archive: false,
      terminal: false, aside: false, banner: state !== "open",
    },
    focusTerminal: () => {},
  });
  return (
    <>
      <div data-region="topbar"><button>Outside</button></div>
      <ConnectionBanner state={state} onRetry={() => {}} focusTerminal={() => {}} />
      <section role="dialog" tabIndex={-1}><button>Dialog control</button></section>
      {menu && <div role="menu" />}
    </>
  );
}

function keyboardBanner(state: "open" | "closed" | "reconnecting", menu = false) {
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (next: typeof state) => act(() => root.render(<KeyboardHost state={next} menu={menu} />));
  render(state);
  const control = container.querySelector<HTMLElement>('[role="dialog"] button')!;
  act(() => control.focus());
  const key = (options: KeyboardEventInit = {}) => act(() => control.ownerDocument.activeElement!.dispatchEvent(
    new KeyboardEvent("keydown", { key: "F6", bubbles: true, cancelable: true, ...options }),
  ));
  return { container, control, render, key };
}

it("cycles only between the open dialog and Retry in either direction", () => {
  const { container, control, key } = keyboardBanner("closed");
  const retry = container.querySelector('[data-region="banner"] button');
  key();
  expect(document.activeElement).toBe(retry);
  key();
  expect(document.activeElement).toBe(control);
  key({ shiftKey: true });
  expect(document.activeElement).toBe(retry);
  key({ shiftKey: true });
  expect(document.activeElement).toBe(control);
});

it.each([
  { state: "open", menu: false, options: {} },
  { state: "reconnecting", menu: false, options: {} },
  { state: "closed", menu: true, options: {} },
  { state: "closed", menu: false, options: { ctrlKey: true } },
] as const)("keeps modal focus for $state with menu=$menu and modifiers=$options", ({ state, menu, options }) => {
  const { control, key } = keyboardBanner(state, menu);
  key(options);
  expect(document.activeElement).toBe(control);
});

it.each(["reconnecting", "open"] as const)("returns Retry focus to the dialog when the connection becomes %s", (next) => {
  const { control, render, key } = keyboardBanner("closed");
  key();
  render(next);
  expect(document.activeElement).toBe(control);
});
