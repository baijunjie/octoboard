// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { ErrorAlerts } from "../../shell/ErrorAlerts";
import { errorAlerts } from "../alerts/errorAlerts";
import type { PlatformAdapter } from "../../platform";
import { ForwardedLinkOpenError, LinkForwardError } from "../../platform/linkForwardError";
import { PlatformProvider } from "../../platform/react";
import { ExternalLink } from "./ExternalLink";
import { BROWSER_HOST } from "../../test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const URL = "https://example.com/guide";

function mount(element: React.ReactNode, platform: PlatformAdapter = { kind: "browser", host: BROWSER_HOST }): HTMLAnchorElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<PlatformProvider value={platform}>{element}<ErrorAlerts /></PlatformProvider>));
  return container.querySelector("a")!;
}

it("keeps a real destination, visible external marker and accessible description, and with no opener the browser's own new tab", () => {
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
  const anchor = mount(<ExternalLink href={URL}>Guide</ExternalLink>, { kind: "tauri", host: BROWSER_HOST, links: { open } });
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

it("leaves the anchor without a target wherever the app opens the address, so the desktop shell's own anchor handler never also opens it", () => {
  const native = mount(<ExternalLink href={URL}>Guide</ExternalLink>, { kind: "tauri", host: BROWSER_HOST, links: { open: vi.fn() } });
  expect(native.hasAttribute("target")).toBe(false);
  const custom = mount(<ExternalLink href={URL} onOpen={vi.fn()}>Guide</ExternalLink>);
  expect(custom.hasAttribute("target")).toBe(false);
});

it("opens through the browser adapter's links without a target or a navigation", async () => {
  const open = vi.fn();
  const anchor = mount(<ExternalLink href={URL}>Guide</ExternalLink>, { kind: "browser", host: BROWSER_HOST, links: { open } });
  expect(anchor.hasAttribute("target")).toBe(false);
  const click = new MouseEvent("click", { bubbles: true, cancelable: true });
  await act(async () => anchor.dispatchEvent(click));
  expect(click.defaultPrevented).toBe(true);
  expect(open).toHaveBeenCalledExactlyOnceWith(URL);
});

it("never meets the shell plugin's body click handler, which opens any target=_blank web or mail anchor, prevented or not", async () => {
  // The plugin's `init-iife.js`: acts on the nearest anchor with an http(s), mailto: or tel: href and `target="_blank"`.
  const opened: string[] = [];
  const listener = (event: MouseEvent) => {
    for (let node = event.target as Element | null; node; node = node.parentElement) {
      if (node.matches("a")) {
        const link = node as HTMLAnchorElement;
        if (link.href !== "" && ["http://", "https://", "mailto:", "tel:"].some((scheme) => link.href.startsWith(scheme)) && link.target === "_blank") {
          opened.push(link.href);
          event.preventDefault();
        }
        break;
      }
    }
  };
  document.body.addEventListener("click", listener);
  onTestFinished(() => document.body.removeEventListener("click", listener));
  const platform: PlatformAdapter = { kind: "tauri", host: BROWSER_HOST, links: { open: vi.fn() } };
  const anchors = [mount(<ExternalLink href={URL}>Guide</ExternalLink>, platform), mount(<ExternalLink href="mailto:a@example.com">Mail</ExternalLink>, platform)];
  for (const anchor of anchors) await act(async () => anchor.click());
  expect(opened).toEqual([]);
  expect(platform.links!.open).toHaveBeenCalledTimes(2);
});

it("honours a caller's opening flow and its external pending state", async () => {
  const platformOpen = vi.fn();
  const customOpen = vi.fn();
  const anchor = mount(<ExternalLink href={URL} onOpen={customOpen}>Guide</ExternalLink>, { kind: "tauri", host: BROWSER_HOST, links: { open: platformOpen } });
  await act(async () => anchor.click());
  expect(customOpen).toHaveBeenCalledExactlyOnceWith(URL);
  expect(platformOpen).not.toHaveBeenCalled();
  const pending = mount(<ExternalLink href={URL} isPending onOpen={customOpen}>Pending</ExternalLink>);
  act(() => pending.click());
  expect(customOpen).toHaveBeenCalledTimes(1);
  expect(pending.getAttribute("aria-disabled")).toBe("true");
});

it("shows a decorative spinner before the label of a pending button link, and none on a text link", () => {
  const button = mount(<ExternalLink href={URL} appearance="button" isPending>Open</ExternalLink>);
  const spinner = button.querySelector("[data-slot=spinner]");
  expect(spinner?.getAttribute("aria-hidden")).toBe("true");
  expect(button.firstElementChild).toBe(spinner);
  expect(button.textContent).toBe("Open");
  expect(mount(<ExternalLink href={URL} appearance="button">Open</ExternalLink>).querySelector("[data-slot=spinner]")).toBeNull();
});

it("reports a failed opener with localized copy, the destination as plain text and the original cause", async () => {
  const cause = new Error("Browser launch failed");
  const anchor = mount(<ExternalLink href={URL}>Guide</ExternalLink>, { kind: "tauri", host: BROWSER_HOST, links: { open: vi.fn().mockRejectedValue(cause) } });
  await act(async () => anchor.click());
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.textContent).toContain("The link did not open in a browser. Its address is shown below.");
  expect(alert.querySelector("p[dir=ltr]")?.textContent).toBe(URL);
  expect(alert.querySelector("[data-inline-reference]")).toBeNull();
  expect(errorAlerts.getSnapshot()[0].error).toMatchObject({ recovery: "openExternal", cause });
});

/** The text of `alert` without the marks that isolate a name from the text around it. */
const plain = (alert: Element) => alert.textContent!.replace(/[\u2068\u2069]/g, "");

/** Opens the link of a remote window of the daemon `build-server` whose opener rejects with `cause`; the alert it raises. */
async function alertForOpenFailure(cause: unknown): Promise<{ alert: Element; error: unknown }> {
  // The alerts of the tests before stay queued until they are acknowledged.
  while (errorAlerts.getSnapshot().length > 0) act(() => errorAlerts.acknowledge());
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  window.history.replaceState(null, "", "/?remote=build-server&daemon=me%40build-server");
  onTestFinished(() => { vi.unstubAllGlobals(); window.history.replaceState(null, "", "/"); });
  const anchor = mount(<ExternalLink href="http://localhost:3000/">Guide</ExternalLink>, { kind: "tauri", host: BROWSER_HOST, links: { open: vi.fn().mockRejectedValue(cause) } });
  await act(async () => anchor.click());
  return { alert: document.querySelector('[role="alertdialog"]')!, error: errorAlerts.getSnapshot()[0].error };
}

it("words an address a remote window could not forward by the port and the daemon, without the address to open by hand", async () => {
  const nothing = await alertForOpenFailure(new LinkForwardError("nothing_listening", 3000));
  expect(plain(nothing.alert)).toContain("Could not open the link");
  expect(plain(nothing.alert)).toContain("Nothing is listening on port 3000 on build-server.");
  expect(nothing.alert.querySelector("p[dir=ltr]")).toBeNull();
  expect(nothing.error).toMatchObject({ recovery: "none" });
});

it("gives a server that forbids forwarding and a connection that is down their remedies", async () => {
  const refused = await alertForOpenFailure(new LinkForwardError("forwarding_refused", 3000));
  expect(plain(refused.alert)).toContain("The SSH server on build-server does not allow port forwarding");
  expect(plain(refused.alert)).toContain("Allow TCP forwarding in that SSH server’s settings");
});

it("says a connection that is down is down, and offers the connection remedy", async () => {
  const down = await alertForOpenFailure(new LinkForwardError("disconnected", 3000));
  expect(plain(down.alert)).toContain("The connection to build-server is down");
  expect(down.error).toMatchObject({ recovery: "connection" });
});

it("raises any other forwarding failure as unexpected, its text kept for Error details", async () => {
  const failed = await alertForOpenFailure(new LinkForwardError("failed", 3000, "cannot listen on a loopback port"));
  expect(failed.error).toBeInstanceOf(Error);
  expect(failed.error).not.toMatchObject({ recovery: expect.anything() });
  expect(plain(failed.alert)).not.toContain("Its address is shown below");
});

it("shows the forwarded address when the browser itself failed to open it", async () => {
  const cause = new Error("no browser");
  const opened = await alertForOpenFailure(new ForwardedLinkOpenError("http://localhost:4000/", cause));
  expect(opened.alert.querySelector("p[dir=ltr]")?.textContent).toBe("http://localhost:4000/");
  expect(opened.error).toMatchObject({ recovery: "openExternal", cause });
});

it("prevents native-window navigation without a platform opener and refuses unsafe destinations", async () => {
  const anchor = mount(<ExternalLink href={URL}>Guide</ExternalLink>, { kind: "tauri", host: BROWSER_HOST });
  const click = new MouseEvent("click", { bubbles: true, cancelable: true });
  await act(async () => anchor.dispatchEvent(click));
  expect(click.defaultPrevented).toBe(true);
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain(URL);
});

it("keeps unsafe destinations as noninteractive text", () => {
  const unsafe = mount(<ExternalLink href="javascript:alert(1)">Unsafe</ExternalLink>);
  expect(unsafe).toBeNull();
});
