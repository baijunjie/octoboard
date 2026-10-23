// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { ErrorAlerts } from "../dialogs/ErrorAlerts";
import { errorAlerts } from "../errorAlerts";
import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import { ExternalLink } from "./ExternalLink";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const URL = "https://example.com/guide";

afterEach(() => act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); }));

function mount(element: React.ReactNode, platform: PlatformAdapter = { kind: "browser" }): HTMLAnchorElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<PlatformProvider value={platform}>{element}<ErrorAlerts /></PlatformProvider>));
  return container.querySelector("a")!;
}

it("keeps a real destination, visible external marker and accessible description", () => {
  const anchor = mount(<ExternalLink href={URL}>Guide</ExternalLink>);
  expect(anchor.getAttribute("href")).toBe(URL);
  expect(anchor.target).toBe("_blank");
  expect(anchor.rel).toBe("noopener noreferrer");
  expect(anchor.getAttribute("aria-description")).toBe("Opens in your browser");
  expect(anchor.querySelector('svg[aria-hidden="true"]')).not.toBeNull();
  expect(anchor.textContent).toBe("Guide");
});

it("routes native opening through the platform, with one pending activation and no navigation", async () => {
  let finish!: () => void;
  const open = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
  const anchor = mount(<ExternalLink href={URL}>Guide</ExternalLink>, { kind: "tauri", links: { open } });
  const click = new MouseEvent("click", { bubbles: true, cancelable: true });
  act(() => { anchor.focus(); anchor.dispatchEvent(click); });
  expect(click.defaultPrevented).toBe(true);
  expect(open).toHaveBeenCalledExactlyOnceWith(URL);
  expect(anchor.getAttribute("aria-busy")).toBe("true");
  act(() => anchor.click());
  expect(open).toHaveBeenCalledTimes(1);
  expect(document.activeElement).toBe(anchor);
  await act(async () => finish());
  expect(anchor.hasAttribute("aria-disabled")).toBe(false);
});

it("honours a caller's opening flow and its external pending state", async () => {
  const platformOpen = vi.fn();
  const customOpen = vi.fn();
  const anchor = mount(<ExternalLink href={URL} onOpen={customOpen}>Guide</ExternalLink>, { kind: "tauri", links: { open: platformOpen } });
  await act(async () => anchor.click());
  expect(customOpen).toHaveBeenCalledExactlyOnceWith(URL);
  expect(platformOpen).not.toHaveBeenCalled();
  const pending = mount(<ExternalLink href={URL} isPending onOpen={customOpen}>Pending</ExternalLink>);
  act(() => pending.click());
  expect(customOpen).toHaveBeenCalledTimes(1);
  expect(pending.getAttribute("aria-disabled")).toBe("true");
});

it("reports a failed opener with localized copy, the destination as plain text and the original cause", async () => {
  const cause = new Error("Browser launch failed");
  const anchor = mount(<ExternalLink href={URL}>Guide</ExternalLink>, { kind: "tauri", links: { open: vi.fn().mockRejectedValue(cause) } });
  await act(async () => anchor.click());
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.textContent).toContain("Could not open the external link.");
  expect(alert.querySelector("p[dir=ltr]")?.textContent).toBe(URL);
  expect(alert.querySelector("[data-inline-chip]")).toBeNull();
  expect(errorAlerts.getSnapshot()[0].error).toMatchObject({ recovery: "openExternal", cause });
});

it("prevents native-window navigation without a platform opener and refuses unsafe destinations", async () => {
  const anchor = mount(<ExternalLink href={URL}>Guide</ExternalLink>, { kind: "tauri" });
  const click = new MouseEvent("click", { bubbles: true, cancelable: true });
  await act(async () => anchor.dispatchEvent(click));
  expect(click.defaultPrevented).toBe(true);
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain(URL);
});

it("keeps unsafe destinations as noninteractive text", () => {
  const unsafe = mount(<ExternalLink href="javascript:alert(1)">Unsafe</ExternalLink>);
  expect(unsafe).toBeNull();
});
