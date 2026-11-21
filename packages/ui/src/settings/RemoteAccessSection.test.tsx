// @vitest-environment jsdom
import { act, type ComponentProps } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { remoteAccessOf } from "../fixtures/builders";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter } from "../platform";
import type { Event, RemoteAccess } from "../protocol";
import { BROWSER_HOST, MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { click } from "../test/dom";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { hasBrowserPort } from "./accessState";
import { remoteAccessOffered } from "./remoteAccessOffered";
import { RemoteAccessSection } from "./RemoteAccessSection";
import { SettingsSearchProvider } from "./settingsSearchFilter";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(async (_body: unknown): Promise<Event> => ({ type: "ack", id: null }));
const loginStatus = vi.fn(async (): Promise<boolean | undefined> => false);
const openLoginSettings = vi.fn(async () => {});
const openLink = vi.fn(async (_url: string) => {});
beforeEach(() => {
  acknowledgeAllAlerts();
  request.mockClear();
  loginStatus.mockReset().mockResolvedValue(false);
  openLoginSettings.mockClear();
  openLink.mockClear();
});

const DESKTOP: PlatformAdapter = {
  kind: "tauri",
  host: MAC_HOST,
  remoteLogin: { status: () => loginStatus(), openSystemSettings: () => openLoginSettings() },
  links: { open: openLink },
};
const BROWSER: PlatformAdapter = { kind: "browser", host: BROWSER_HOST };
const WINDOWS: PlatformAdapter = { ...DESKTOP, host: WINDOWS_HOST };

const SERVING = remoteAccessOf({
  allowed: true,
  state: "serving",
  address: "http://127.0.0.1:3369/?key=abc",
  forward_command: "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:3369:/Users/me/.octoboard/browser.sock me@<server>",
});

const DETAILS = "octoboard+ssh://me@my-mac.local:22?fingerprint=SHA256%3Aabc&octoboardd=%2FApplications%2FOctoboard.app&name=my-mac";
const WITH_DETAILS = { ...SERVING, connection: { text: DETAILS, qr: ["1010", "0101", "1010", "0101"] } };

/** Browser access alone, where the daemon serves the browser at a loopback port of its own (Windows, macOS). */
const BROWSER_SERVING = remoteAccessOf({
  browser_allowed: true,
  browser_state: "serving",
  browser_port: 3369,
  address: "http://127.0.0.1:3369/?key=abc",
  forward_command: "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:3369:127.0.0.1:3369 me@<server>",
});
const BROWSER_OFF = remoteAccessOf({ browser_port: 3369 });
/** Remote access serving the socket (macOS) and the browser port taken by another program. */
const PORT_TAKEN = remoteAccessOf({
  allowed: true,
  state: "serving",
  address: "http://127.0.0.1:3369/?key=abc",
  forward_command: "ssh -N -L 127.0.0.1:3369:/Users/me/.octoboard/browser.sock me@<server>",
  browser_allowed: true,
  browser_state: "failed",
  browser_port: 3369,
  browser_failure: { reason: "port_in_use", detail: "listening on 127.0.0.1:3369: address in use" },
});

async function mount(remoteAccess: RemoteAccess, platform: PlatformAdapter = DESKTOP, matches?: ComponentProps<typeof SettingsSearchProvider>["matches"]) {
  const daemon = testDaemon({ request, state: { connectionState: "open", remoteAccess } });
  let container!: HTMLElement;
  await act(async () => {
    container = mountHost(<LanguageProvider><SettingsSearchProvider matches={matches}><RemoteAccessSection /></SettingsSearchProvider></LanguageProvider>, { platform, daemon });
  });
  return { container, daemon };
}

const tabNamed = (container: HTMLElement, name: string) =>
  Array.from(container.querySelectorAll<HTMLElement>('[role="tab"]')).find((tab) => tab.textContent?.trim() === name)!;
/** Mounts the section and selects its Browser tab, which holds the steps. */
async function mountBrowserTab(remoteAccess: RemoteAccess, platform: PlatformAdapter = DESKTOP, matches?: ComponentProps<typeof SettingsSearchProvider>["matches"]) {
  const mounted = await mount(remoteAccess, platform, matches);
  await click(tabNamed(mounted.container, "Browser"));
  return mounted;
}
const switchIn = (container: HTMLElement, searchId: string) => container.querySelector<HTMLInputElement>(`[data-settings-search-id='${searchId}'] input[type=checkbox]`)!;
const switchOf = (container: HTMLElement) => switchIn(container, "remote-access.allow");
const browserSwitch = (container: HTMLElement) => switchIn(container, "remote-access.browser-allow");
const dialog = () => document.body.querySelector("[role=alertdialog]");
const buttonNamed = (root: ParentNode, name: string) =>
  Array.from(root.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent?.trim() === name);
const rowsOf = (container: HTMLElement) =>
  Array.from(container.querySelectorAll("[data-settings-search-id]")).map((row) => row.getAttribute("data-settings-search-id"));
const textField = () => document.body.querySelector<HTMLInputElement>("[role=dialog] input")!;
const chipsOf = (root: ParentNode) => [...root.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent);
const tabLabels = (container: HTMLElement) => [...container.querySelectorAll('[role="tab"]')].map((tab) => tab.textContent?.trim());
const groupsOf = (container: HTMLElement) =>
  [...container.querySelectorAll<HTMLElement>("[data-settings-search-group]")].map((group) => [group.dataset.settingsSearchGroup, [...group.querySelectorAll("[data-settings-search-id]")].map((row) => row.getAttribute("data-settings-search-id"))]);

async function typeInto(input: HTMLInputElement, value: string) {
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setter.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** What the first alert raised says, its title and its words together. */
const alertWords = () => {
  const [alert] = errorAlerts.getSnapshot();
  return alert ? `${alert.title} ${(alert.error as Error).message}` : "";
};

it("is offered only in the application's window, for a daemon that offers remote access", () => {
  expect(remoteAccessOffered(DESKTOP, remoteAccessOf())).toBe(true);
  expect(remoteAccessOffered(DESKTOP, null)).toBe(false);
  expect(remoteAccessOffered(BROWSER, remoteAccessOf())).toBe(false);
});

it("tells a daemon that serves the browser at a port of its own from one that does not", () => {
  expect(hasBrowserPort(BROWSER_OFF)).toBe(true);
  expect(hasBrowserPort(remoteAccessOf())).toBe(false);
  expect(hasBrowserPort(null)).toBe(false);
});

it("is not offered in a remote daemon's window, whose adapter has no Remote Login as its daemon is not this computer's", () => {
  expect(remoteAccessOffered({ ...DESKTOP, remoteLogin: undefined }, remoteAccessOf())).toBe(false);
});

it("asks before turning remote access on, and sends nothing when the confirmation is cancelled", async () => {
  const { container } = await mount(remoteAccessOf());
  await act(async () => switchOf(container).click());
  expect(request).not.toHaveBeenCalled();
  expect(dialog()?.textContent).toContain("including running any command");
  expect(dialog()?.textContent).toContain("The key lasts until you rotate it.");
  expect(dialog()?.textContent).not.toMatch(/browser|bookmarks/i);
  expect(dialog()?.textContent).toContain("SSH login");
  await act(async () => buttonNamed(dialog()!, "Cancel")!.click());
  expect(request).not.toHaveBeenCalled();
});

it("leaves the browser port out of the confirmation, where the daemon has one too", async () => {
  for (const platform of [WINDOWS, DESKTOP]) {
    const { container } = await mount(BROWSER_OFF, platform);
    await act(async () => switchOf(container).click());
    expect(dialog()?.textContent).toContain("SSH login");
    expect(dialog()?.textContent).not.toContain("browser port");
    await act(async () => buttonNamed(dialog()!, "Cancel")!.click());
    container.remove();
  }
});

it("turns remote access on once confirmed", async () => {
  const { container } = await mount(remoteAccessOf());
  await act(async () => switchOf(container).click());
  await act(async () => buttonNamed(dialog()!, "Allow")!.click());
  expect(request).toHaveBeenCalledWith({ type: "set_remote_access", allowed: true });
});

it("turns remote access off without asking", async () => {
  const { container } = await mount(SERVING);
  await act(async () => switchOf(container).click());
  expect(dialog()).toBeNull();
  expect(request).toHaveBeenCalledWith({ type: "set_remote_access", allowed: false });
});

it("puts the SSH server first, then the switch, then the connection details and Rotate key while remote access serves", async () => {
  const { container } = await mount(WITH_DETAILS);
  expect(container.querySelector('[role="tab"]')).toBeNull();
  expect(rowsOf(container)).toEqual(["remote-access.remote-login", "remote-access.allow", "remote-access.connection", "remote-access.rotate"]);
});

it("leads with a sentence about another computer's app that does not mention the browser", async () => {
  const { container } = await mount(WITH_DETAILS);
  const lead = [...container.querySelectorAll("p")].find((paragraph) => paragraph.textContent?.startsWith("Use this computer's app"))!;
  expect(lead.textContent).toContain("another computer's own app");
  expect(lead.textContent).not.toMatch(/browser/i);
});

it("holds nothing about the browser's own groups, rows or steps where the daemon has no browser port", async () => {
  const { container } = await mount(WITH_DETAILS);
  expect(groupsOf(container).map(([key]) => key)).toEqual(["settings.remoteAccess.group.others", "settings.remoteAccess.group.key"]);
  expect(container.textContent).not.toContain("Step 1");
  expect(container.textContent).not.toContain("Browser port");
  expect(container.textContent).not.toContain("Allow browser access");
  expect(container.textContent).not.toContain("Browser access is off");
});

it("shows the connection details as text and a QR code", async () => {
  const { container } = await mount(WITH_DETAILS);
  expect(container.textContent).toContain("Connection details");
  expect(container.textContent).toContain("Add remote daemon");
  expect(container.textContent).toContain(DETAILS);
  const qr = container.querySelector("svg[role=img]")!;
  expect(qr.getAttribute("aria-label")).toBe("QR code of the connection details");
  expect(qr.querySelectorAll("path")).toHaveLength(1);
});

it("shows the connection details as the single line another computer pastes, with no key in it", async () => {
  const { container } = await mount(WITH_DETAILS);
  const shown = [...container.querySelectorAll("pre")].map((pre) => pre.textContent);
  expect(shown).toContain(DETAILS);
  expect(DETAILS).not.toContain("key=");
  expect(DETAILS).not.toMatch(/\s/);
});

it("shows no connection details when the daemon gives none, or while off", async () => {
  expect((await mount(SERVING)).container.querySelector("svg[role=img]")).toBeNull();
  const off = await mount(remoteAccessOf());
  expect(off.container.textContent).not.toContain("Connection details");
  expect(off.container.querySelector("svg[role=img]")).toBeNull();
  expect(rowsOf(off.container)).toEqual(["remote-access.remote-login", "remote-access.allow"]);
});

it("names the Add remote daemon button and the Connection details field as references", async () => {
  const { container } = await mount(WITH_DETAILS);
  expect(chipsOf(container)).toEqual(expect.arrayContaining(["Add remote daemon", "Connection details"]));
  const description = [...container.querySelectorAll("[data-settings-search-id='remote-access.connection'] *")].find((node) => node.textContent?.startsWith("On the other computer"));
  expect(description?.querySelectorAll("[data-inline-reference]")).toHaveLength(2);
});

it("offers the connection details and no more when a search finds only them", async () => {
  const found = { ids: new Set(["remote-access.connection" as const]), groups: new Set<never>() };
  const { container } = await mount(WITH_DETAILS, DESKTOP, found);
  const hidden = [...container.querySelectorAll("[data-settings-search-id][hidden]")].map((row) => row.getAttribute("data-settings-search-id"));
  expect(hidden).toEqual(["remote-access.remote-login", "remote-access.allow", "remote-access.rotate"]);
});

it("offers Rotate key while either way in serves, and not while nothing does", async () => {
  for (const serving of [SERVING, BROWSER_SERVING]) {
    const { container } = await mount(serving);
    expect(buttonNamed(container, "Rotate key")).toBeDefined();
    container.remove();
  }
  expect(buttonNamed((await mount(BROWSER_OFF)).container, "Rotate key")).toBeUndefined();
});

it("words Rotate key as shared with browser access where there is a browser port and for other computers' apps alone where there is none, and rotates once confirmed", async () => {
  const appOnly = await mount(SERVING);
  expect(appOnly.container.textContent).toContain("A new key disconnects the apps of other computers");
  expect(appOnly.container.textContent).not.toMatch(/browser/i);
  await act(async () => buttonNamed(appOnly.container, "Rotate key")!.click());
  expect(dialog()?.textContent).toContain("The key that other computers' apps use is replaced.");
  expect(dialog()?.textContent).not.toMatch(/browser/i);
  await act(async () => buttonNamed(dialog()!, "Cancel")!.click());
  appOnly.container.remove();

  const { container } = await mount(BROWSER_SERVING);
  expect(container.textContent).toContain("Browser access and remote access share one key.");
  await act(async () => buttonNamed(container, "Rotate key")!.click());
  expect(request).not.toHaveBeenCalled();
  expect(dialog()?.textContent).toContain("share is replaced");
  expect(dialog()?.textContent).toContain("reconnect with the new key by themselves");
  await act(async () => buttonNamed(dialog()!, "Rotate")!.click());
  expect(request).toHaveBeenCalledWith({ type: "rotate_owner_key" });
});

it("says why the key could not be rotated, and keeps the confirmation", async () => {
  const { container } = await mount(SERVING);
  await act(async () => buttonNamed(container, "Rotate key")!.click());
  request.mockRejectedValueOnce(new Error("refused"));
  await act(async () => buttonNamed(dialog()!, "Rotate")!.click());
  await act(async () => {});
  expect(alertWords()).toContain("Could not rotate the key");
  expect(dialog()).not.toBeNull();
});

it("says why the Windows named pipe could not be opened", async () => {
  const { container } = await mount(remoteAccessOf({ allowed: true, state: "failed", failure: { reason: "pipe_failed", detail: "Access is denied" } }), WINDOWS);
  expect(container.textContent).toContain("could not open the named pipe");
  expect(container.textContent).not.toMatch(/browser/i);
  expect(buttonNamed(container, "Rotate key")).toBeUndefined();
});

it("words a key failure and a socket failure for another computer's app, not a browser", async () => {
  for (const reason of ["socket_failed", "owner_key_failed"]) {
    const { container } = await mount(remoteAccessOf({ allowed: true, state: "failed", failure: { reason, detail: "d" } }));
    expect(container.textContent).toContain("another computer's app");
    expect(container.textContent).not.toMatch(/a browser/i);
    container.remove();
  }
});

it("says why remote access is not served, beside the daemon's own words", async () => {
  const { container } = await mount(
    remoteAccessOf({ allowed: true, state: "failed", failure: { reason: "not_private", detail: "/Users/me/.octoboard is not private" } }),
  );
  expect(container.textContent).toContain("Remote access is on, but this computer cannot be reached");
  expect(container.textContent).toContain("is not private to your account");
  expect(container.textContent).toContain("another computer's app connects through");
  expect(container.textContent).not.toMatch(/browser/i);
  // The daemon's own words are behind the alert's Details button.
  await act(async () => buttonNamed(container, "Details")!.click());
  expect(document.body.textContent).toContain("/Users/me/.octoboard is not private");
});

it("shows only the daemon's own words for a failure reason this build does not know", async () => {
  const { container } = await mount(
    remoteAccessOf({ allowed: true, state: "failed", failure: { reason: "from_a_newer_daemon", detail: "the socket could not be served for a new reason" } }),
  );
  expect(container.textContent).toContain("Remote access is on, but this computer cannot be reached");
  for (const known of ["is not private to your account", "could not open the named pipe"]) expect(container.textContent).not.toContain(known);
  await act(async () => buttonNamed(container, "Details")!.click());
  expect(document.body.textContent).toContain("the socket could not be served for a new reason");
});

it("shows Remote Login on, or off with the way to System Settings, worded for another computer's app only", async () => {
  loginStatus.mockResolvedValue(true);
  const on = await mount(SERVING);
  expect(on.container.textContent).toContain("On");
  expect(buttonNamed(on.container, "Open System Settings")).toBeUndefined();

  loginStatus.mockResolvedValue(false);
  const off = await mount(SERVING);
  const offRow = off.container.querySelector("[data-settings-search-id='remote-access.remote-login']")!;
  expect(offRow.textContent).toMatch(/^Remote Login\s*Off\..*go to General > Sharing, then turn on Remote Login\./s);
  expect(offRow.textContent).toContain("Access from another computer through the app requires an SSH connection to this computer.");
  expect(offRow.textContent).not.toMatch(/browser/i);
  expect(offRow.textContent).not.toContain("macOS");
  await act(async () => buttonNamed(off.container, "Open System Settings")!.click());
  expect(openLoginSettings).toHaveBeenCalled();
});

it("words the SSH server row for the app alone without a browser port, and for the app and a browser with one", async () => {
  loginStatus.mockResolvedValue(true);
  const app = await mount(SERVING);
  const appRow = app.container.querySelector("[data-settings-search-id='remote-access.remote-login']")!;
  expect(appRow.textContent).toContain("through the app requires an SSH connection");
  expect(appRow.textContent).not.toMatch(/browser/i);
  app.container.remove();
  const both = await mount(BROWSER_OFF);
  const bothRow = both.container.querySelector("[data-settings-search-id='remote-access.remote-login']")!;
  expect(bothRow.textContent).toContain("through the app or a browser requires an SSH connection to this computer.");
});

it("names OpenSSH Server on Windows, with the way to Settings while it is not running", async () => {
  loginStatus.mockResolvedValue(false);
  const { container } = await mount(remoteAccessOf({ ...SERVING, browser_port: 3369 }), WINDOWS);
  expect(container.textContent).toContain("OpenSSH Server");
  expect(container.textContent).toContain("add OpenSSH Server under System > Optional features");
  expect(container.textContent).toContain("Both another computer's app and a browser log in to this one over SSH");
  expect(container.textContent).not.toContain("Remote Login");
  await act(async () => buttonNamed(container, "Open Settings")!.click());
  expect(openLoginSettings).toHaveBeenCalled();
});

it("sets the system's names and the steps to them as chips in a path, on macOS and on Windows", async () => {
  loginStatus.mockResolvedValue(false);
  const mac = await mount(SERVING);
  const macPath = mac.container.querySelector("[data-inline-path]")!;
  expect([...macPath.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toEqual(["General", "Sharing"]);
  expect(macPath.textContent).toBe("General > Sharing");
  expect(chipsOf(mac.container)).toEqual(expect.arrayContaining(["Remote Login"]));

  const windows = await mount(remoteAccessOf({ ...SERVING, browser_port: 3369 }), WINDOWS);
  const windowsPath = windows.container.querySelector("[data-inline-path]")!;
  expect(windowsPath.textContent).toBe("System > Optional features");
  expect(chipsOf(windows.container)).toEqual(expect.arrayContaining(["System", "Optional features", "OpenSSH Server", "OpenSSH SSH Server", "Services"]));
});

it("raises the failed opening of the settings as an alert whose steps are chips in a path", async () => {
  loginStatus.mockResolvedValue(false);
  openLoginSettings.mockRejectedValueOnce(new Error("no"));
  const { container } = await mount(SERVING);
  await act(async () => buttonNamed(container, "Open System Settings")!.click());
  const [alert] = errorAlerts.getSnapshot();
  // The error's own message stays text; the alert's body takes the path as chips.
  expect(alert!.message).toContain("go to General > Sharing > Remote Login");
  const body = mountHost(<LanguageProvider>{alert!.content}</LanguageProvider>, { platform: DESKTOP, daemon: testDaemon({ request }) });
  expect(body.querySelector("[data-inline-path]")?.textContent).toBe("General > Sharing > Remote Login");
  expect(body.querySelectorAll("[data-inline-path] [data-inline-reference]")).toHaveLength(3);
});

it("says it could not tell whether the SSH server is on, and still offers the way to the settings", async () => {
  loginStatus.mockResolvedValue(undefined);
  const { container } = await mount(SERVING);
  expect(container.textContent).toContain("could not tell whether it is on");
  expect(buttonNamed(container, "Open System Settings")).toBeDefined();
});

it("raises an alert, and keeps showing the daemon's own state, when turning remote access off fails", async () => {
  request.mockRejectedValueOnce(new Error("refused"));
  const { container } = await mount(SERVING);
  await act(async () => switchOf(container).click());
  await act(async () => {});
  expect(alertWords()).toContain("Could not change remote access");
  expect(switchOf(container).checked).toBe(true);
});

it("stays on the confirmation and says why when the daemon refuses turning remote access on", async () => {
  request.mockRejectedValueOnce(new Error("refused"));
  const { container } = await mount(remoteAccessOf());
  await act(async () => switchOf(container).click());
  await act(async () => buttonNamed(dialog()!, "Allow")!.click());
  await act(async () => {});
  expect(dialog()).not.toBeNull();
  expect(alertWords()).toContain("Could not change remote access");
  expect(switchOf(container).checked).toBe(false);
});

it("copies the connection details exactly as shown", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const { container } = await mount(WITH_DETAILS, { ...DESKTOP, clipboard: { writeText } });
  await act(async () => container.querySelector<HTMLButtonElement>('button[aria-label="Copy connection details"]')!.click());
  expect(writeText).toHaveBeenLastCalledWith(DETAILS);
});

it("says which copy failed when the clipboard refuses it", async () => {
  const writeText = vi.fn(async (_text: string) => Promise.reject(new Error("denied")));
  Object.assign(document, { execCommand: vi.fn(() => false) });
  const { container } = await mount(WITH_DETAILS, { ...DESKTOP, clipboard: { writeText } });
  await act(async () => container.querySelector<HTMLButtonElement>('button[aria-label="Copy connection details"]')!.click());
  await act(async () => {});
  expect(alertWords()).toContain("Could not copy the connection details");
});

it("renders nothing while the daemon has said nothing of remote access", async () => {
  const daemon = testDaemon({ request, state: { connectionState: "open", remoteAccess: null } });
  let container!: HTMLElement;
  await act(async () => {
    container = mountHost(<LanguageProvider><RemoteAccessSection /></LanguageProvider>, { platform: DESKTOP, daemon });
  });
  expect(container.querySelector("input[type=checkbox]")).toBeNull();
});

it("draws the QR code from the rows the daemon sent", async () => {
  const { container } = await mount({ ...WITH_DETAILS, connection: { text: DETAILS, qr: ["11", "10"] } });
  const svg = container.querySelector("svg[role=img]")!;
  // Two modules and a quiet zone of four on each side.
  expect(svg.getAttribute("viewBox")).toBe("0 0 10 10");
});

it("leads with a sentence that names the browser, then the browser's group, the other computers' group and, last, the key's, where the daemon has a browser port", async () => {
  const { container } = await mount(BROWSER_SERVING, WINDOWS);
  const lead = [...container.querySelectorAll("p")].find((paragraph) => paragraph.textContent?.startsWith("Use this computer's app in a browser"))!;
  expect(lead.textContent).toContain("that computer's own app");
  expect(groupsOf(container).map(([key]) => key)).toEqual(["settings.remoteAccess.group.browser", "settings.remoteAccess.group.others", "settings.remoteAccess.group.key"]);
  expect([...container.querySelectorAll("[data-settings-search-group] h3[id]")].map((heading) => heading.textContent)).toEqual(["This computer's browser", "Other computers", "Key"]);
  expect(tabLabels(container)).toEqual(["Octoboard app", "Browser"]);
  expect(tabNamed(container, "Octoboard app").getAttribute("aria-selected")).toBe("true");
});

it("puts the switch, the port and the local address in the browser's group, the SSH server above the tabs in the other computers', then Rotate key in the key's", async () => {
  const serving = await mount(BROWSER_SERVING, WINDOWS);
  expect(groupsOf(serving.container)).toEqual([
    ["settings.remoteAccess.group.browser", ["remote-access.browser-allow", "remote-access.browser-port", "remote-access.browser-local"]],
    ["settings.remoteAccess.group.others", ["remote-access.remote-login", "remote-access.allow"]],
    ["settings.remoteAccess.group.key", ["remote-access.rotate"]],
  ]);
  await click(tabNamed(serving.container, "Browser"));
  expect(groupsOf(serving.container)[1]).toEqual(["settings.remoteAccess.group.others", ["remote-access.remote-login", "remote-access.browser-forward", "remote-access.browser-address"]]);
  serving.container.remove();
  const off = await mount(BROWSER_OFF, WINDOWS);
  expect(groupsOf(off.container)).toEqual([
    ["settings.remoteAccess.group.browser", ["remote-access.browser-allow", "remote-access.browser-port"]],
    ["settings.remoteAccess.group.others", ["remote-access.remote-login", "remote-access.allow"]],
  ]);
  await click(tabNamed(off.container, "Browser"));
  expect(groupsOf(off.container)[1]).toEqual(["settings.remoteAccess.group.others", ["remote-access.remote-login", "remote-access.browser-off"]]);
  expect(browserSwitch(off.container).checked).toBe(false);
  expect(off.container.textContent).toContain("Port 3369");
});

it("keeps the remote switch above the tabs of neither way: it is the application tab's, with the failure and the connection details", async () => {
  const { container } = await mount({ ...BROWSER_OFF, allowed: true, state: "serving", connection: { text: DETAILS, qr: ["1"] } });
  expect(tabNamed(container, "Octoboard app").getAttribute("aria-selected")).toBe("true");
  expect(groupsOf(container)[1]).toEqual(["settings.remoteAccess.group.others", ["remote-access.remote-login", "remote-access.allow", "remote-access.connection"]]);
  expect(switchOf(container)).not.toBeNull();
});

it("keeps the Browser tab open while browser access is on but its port is not, with a line that points at the alert", async () => {
  const { container } = await mountBrowserTab(PORT_TAKEN);
  expect(tabNamed(container, "Browser").getAttribute("aria-disabled")).toBeNull();
  const row = container.querySelector("[data-settings-search-id='remote-access.browser-closed']")!;
  expect(row.textContent).toContain("The browser port is not open");
  expect(row.textContent).toContain("The alert under Allow browser access above says why.");
  expect(chipsOf(row)).toEqual(["Allow browser access"]);
  expect(container.textContent).not.toContain("Step 1");
});

it("shows the tab that holds what a search found, whichever was selected", async () => {
  const step = { ids: new Set(["remote-access.browser-forward" as const]), groups: new Set<never>() };
  const found = await mount(BROWSER_SERVING, WINDOWS, step);
  expect(tabNamed(found.container, "Browser").getAttribute("aria-selected")).toBe("true");
  expect(tabNamed(found.container, "Octoboard app").getAttribute("aria-disabled")).toBe("true");
  expect(rowsOf(found.container)).toContain("remote-access.browser-forward");
  found.container.remove();
  const allow = { ids: new Set(["remote-access.allow" as const]), groups: new Set<never>() };
  const other = await mountBrowserTab(BROWSER_SERVING, WINDOWS, allow);
  expect(tabNamed(other.container, "Octoboard app").getAttribute("aria-selected")).toBe("true");
  expect(tabNamed(other.container, "Browser").getAttribute("aria-disabled")).toBe("true");
});

it("shows no tabs when a search finds only the SSH server", async () => {
  const found = { ids: new Set(["remote-access.remote-login" as const]), groups: new Set<never>() };
  const { container } = await mount(BROWSER_SERVING, WINDOWS, found);
  expect(container.querySelector('[role="tab"]')).toBeNull();
});

it("says to turn on Allow browser access above, as a reference, in place of the steps while browser access is off", async () => {
  const { container } = await mountBrowserTab(BROWSER_OFF);
  const row = container.querySelector("[data-settings-search-id='remote-access.browser-off']")!;
  expect(row.textContent).toContain("Browser access is off");
  expect(row.textContent).toContain("Turn on Allow browser access above to open this app in a browser on another computer.");
  expect(chipsOf(row)).toEqual(["Allow browser access"]);
  expect(container.textContent).not.toContain("Step 1");
});

it("asks before turning browser access on, naming the cost of the browser port, on Windows and macOS", async () => {
  for (const platform of [WINDOWS, DESKTOP]) {
    const { container } = await mount(BROWSER_OFF, platform);
    await act(async () => browserSwitch(container).click());
    expect(request).not.toHaveBeenCalled();
    expect(dialog()?.textContent).toContain("Allow browser access?");
    expect(dialog()?.textContent).toContain("including running any command");
    expect(dialog()?.textContent).toContain("can take the browser port");
    expect(dialog()?.textContent).toContain("sends them the key");
    expect(dialog()?.textContent).toContain("bookmarks and sync");
    expect(dialog()?.textContent).not.toContain("SSH login");
    await act(async () => buttonNamed(dialog()!, "Cancel")!.click());
    expect(request).not.toHaveBeenCalled();
    container.remove();
  }
});

it("turns browser access on once confirmed, and off without asking, apart from remote access", async () => {
  const off = await mount(BROWSER_OFF);
  await act(async () => browserSwitch(off.container).click());
  await act(async () => buttonNamed(dialog()!, "Allow")!.click());
  expect(request).toHaveBeenCalledWith({ type: "set_browser_access", allowed: true });
  off.container.remove();
  request.mockClear();
  const on = await mount(BROWSER_SERVING);
  expect(browserSwitch(on.container).checked).toBe(true);
  await act(async () => browserSwitch(on.container).click());
  expect(dialog()).toBeNull();
  expect(request).toHaveBeenCalledWith({ type: "set_browser_access", allowed: false });
  expect(request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "set_remote_access" }));
});

it("raises an alert when turning browser access off fails, and keeps the daemon's own state", async () => {
  request.mockRejectedValueOnce(new Error("refused"));
  const { container } = await mount(BROWSER_SERVING);
  await act(async () => browserSwitch(container).click());
  await act(async () => {});
  expect(alertWords()).toContain("Could not change browser access");
  expect(browserSwitch(container).checked).toBe(true);
});

it("shows the local address, the SSH server, the numbered forward and address in the Browser tab, then Rotate key, while the port serves", async () => {
  const { container } = await mountBrowserTab(BROWSER_SERVING, WINDOWS);
  expect(rowsOf(container)).toEqual(["remote-access.browser-allow", "remote-access.browser-port", "remote-access.browser-local", "remote-access.remote-login", "remote-access.browser-forward", "remote-access.browser-address", "remote-access.rotate"]);
  expect(container.querySelector("h3:not([id])")).toBeNull();
  expect(container.textContent).toContain("Step 1: SSH forward");
  expect(container.textContent).toContain("Step 2: Open the address");
});

it("says in Browser port that a browser here and an SSH forward from another computer reach the app at it", async () => {
  const { container } = await mount(BROWSER_OFF);
  const row = container.querySelector("[data-settings-search-id='remote-access.browser-port']")!;
  expect(row.textContent).toContain("The port a browser on this computer, and an SSH forward from another computer, reach the app at.");
  expect(row.textContent).toContain("disconnects the pages open at the old port");
});

it("opens the local address in the default browser, copies it, and has Step 2 point at it", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const { container } = await mountBrowserTab({ ...BROWSER_SERVING, address: "http://127.0.0.1:4000/?key=abc", browser_port: 4000 }, { ...DESKTOP, clipboard: { writeText } });
  const local = container.querySelector("[data-settings-search-id='remote-access.browser-local']")!;
  expect(local.textContent).toContain("http://127.0.0.1:4000/?key=abc");
  expect(local.querySelector("[aria-label='Copy local address']")).not.toBeNull();
  // Opening sits in the address field, beside the copy button.
  const open = local.querySelector<HTMLButtonElement>("button[aria-label='Open in browser']")!;
  expect(open.closest("pre, div")?.parentElement?.contains(local.querySelector("[aria-label='Copy local address']"))).toBe(true);
  await act(async () => open.click());
  expect(openLink).toHaveBeenCalledWith("http://127.0.0.1:4000/?key=abc");
  await act(async () => local.querySelector<HTMLButtonElement>("[aria-label='Copy local address']")!.click());
  expect(writeText).toHaveBeenLastCalledWith("http://127.0.0.1:4000/?key=abc");
  const step2 = container.querySelector("[data-settings-search-id='remote-access.browser-address']")!;
  expect(step2.textContent).toContain("Step 2: Open the address");
  expect(step2.textContent).toContain("Open the Local address above");
  expect(chipsOf(step2)).toEqual(["Local address"]);
  expect(step2.textContent).not.toContain("http://127.0.0.1");
});

it("opens the local address once for a press made while the opening is out, and says the address when the browser could not be opened", async () => {
  let finish: () => void = () => {};
  openLink.mockImplementationOnce(() => new Promise<void>((resolve) => (finish = resolve)));
  const { container } = await mount(BROWSER_SERVING);
  const open = () => container.querySelector<HTMLButtonElement>("button[aria-label='Open in browser']")!;
  await act(async () => open().click());
  await act(async () => open().click());
  expect(openLink).toHaveBeenCalledTimes(1);
  await act(async () => finish());
  openLink.mockRejectedValueOnce(new Error("no browser"));
  await act(async () => open().click());
  const [alert] = errorAlerts.getSnapshot();
  expect(alert?.title).toBe("Could not open the link");
});

it("offers no local address, forward or step 2 where the port is not open, not even the socket's forward", async () => {
  const { container } = await mount(PORT_TAKEN);
  expect(rowsOf(container)).toEqual(["remote-access.browser-allow", "remote-access.browser-failed", "remote-access.browser-port", "remote-access.remote-login", "remote-access.allow", "remote-access.rotate"]);
  expect(container.textContent).not.toContain("browser.sock");
  expect(container.textContent).not.toContain("Browser access is off");
  expect(container.textContent).not.toContain("Browser address");
  expect(buttonNamed(container, "Change port")).toBeDefined();
});

it("says that the port could not be opened, beside the daemon's own words", async () => {
  const { container } = await mount(PORT_TAKEN);
  expect(container.textContent).toContain("Browser access is on, but the browser port is not open");
  expect(container.textContent).toContain("The browser port is already in use");
  expect(container.textContent).not.toContain("Remote access is on, but this computer cannot be reached");
  await act(async () => buttonNamed(container, "Details")!.click());
  expect(document.body.textContent).toContain("listening on 127.0.0.1:3369: address in use");
});

it("words a browser port that is the daemon's own port", async () => {
  const { container } = await mount(remoteAccessOf({ browser_port: 3369, browser_allowed: true, browser_state: "failed", browser_failure: { reason: "port_is_daemons", detail: "both are 3369" } }));
  expect(container.textContent).toContain("The browser port is the port the app itself uses.");
});

it("rules the failure alert with the flat hairline the rows round it take, never the groove", async () => {
  const { container } = await mount(remoteAccessOf({ allowed: true, state: "failed", failure: { reason: "pipe_failed", detail: "Access is denied" } }), WINDOWS);
  const ruled = [...container.querySelectorAll<HTMLElement>(".hairline-b")].filter((node) => node.textContent!.includes("could not open the named pipe"));
  expect(ruled.length).toBeGreaterThan(0);
  expect(ruled.at(-1)!.className).not.toContain("groove");
});

it("rules the App and Browser tab strip, shown once there is a browser port, with the flat hairline, never the groove", async () => {
  const { container } = await mount(remoteAccessOf({ ...SERVING, browser_port: 3369 }));
  const tablist = container.querySelector('[role="tablist"]')!;
  expect(tablist).not.toBeNull();
  const strip = tablist.parentElement!.closest<HTMLElement>("[class*='h-10']")!;
  expect(strip.classList.contains("hairline-b")).toBe(true);
  expect(strip.className).not.toContain("groove");
  expect(container.querySelectorAll("[class*='groove']")).toHaveLength(0);
});

it("words a key failure for a browser", async () => {
  const { container } = await mount(remoteAccessOf({ browser_port: 3369, browser_allowed: true, browser_state: "failed", browser_failure: { reason: "owner_key_failed", detail: "d" } }));
  expect(container.textContent).toContain("The key that signs a browser in could not be created or read.");
});

it("keeps browser access off, with its own line in the Browser tab, when only remote access serves, on Windows or macOS", async () => {
  const windows = await mount(remoteAccessOf({ allowed: true, state: "serving", browser_port: 3369 }), WINDOWS);
  expect(rowsOf(windows.container)).toEqual(["remote-access.browser-allow", "remote-access.browser-port", "remote-access.remote-login", "remote-access.allow", "remote-access.rotate"]);
  expect(browserSwitch(windows.container).checked).toBe(false);
  await click(tabNamed(windows.container, "Browser"));
  expect(rowsOf(windows.container)).toContain("remote-access.browser-off");
});

it("changes the browser port once a port is typed and saved, refusing what is no port", async () => {
  const { container } = await mount(BROWSER_SERVING, WINDOWS);
  await act(async () => buttonNamed(container, "Change port")!.click());
  expect(textField().value).toBe("3369");
  for (const wrong of ["0", "65536", "33a", ""]) {
    await typeInto(textField(), wrong);
    await act(async () => buttonNamed(document.body, "Save")!.click());
    expect(document.body.textContent, wrong).toContain("Enter a whole number from 1 to 65535.");
  }
  expect(request).not.toHaveBeenCalled();
  await typeInto(textField(), " 4000 ");
  await act(async () => buttonNamed(document.body, "Save")!.click());
  expect(request).toHaveBeenCalledWith({ type: "set_remote_access_port", port: 4000 });
  expect(document.body.querySelector("[role=dialog]")).toBeNull();
});

it("keeps the port dialog open and says why when the daemon refuses the port", async () => {
  const { container } = await mount(BROWSER_SERVING, WINDOWS);
  await act(async () => buttonNamed(container, "Change port")!.click());
  request.mockRejectedValueOnce(new Error("refused"));
  await typeInto(textField(), "4000");
  await act(async () => buttonNamed(document.body, "Save")!.click());
  await act(async () => {});
  expect(alertWords()).toContain("Could not change the browser port");
  expect(document.body.querySelector("[role=dialog]")).not.toBeNull();
});

it("sends nothing for the port when the dialog is cancelled", async () => {
  const { container } = await mount(BROWSER_SERVING, WINDOWS);
  await act(async () => buttonNamed(container, "Change port")!.click());
  await typeInto(textField(), "4000");
  await act(async () => buttonNamed(document.body, "Cancel")!.click());
  expect(request).not.toHaveBeenCalled();
});

it.each(["remote-access.browser-allow", "remote-access.browser-port", "remote-access.browser-local", "remote-access.browser-forward", "remote-access.browser-address", "remote-access.rotate"] as const)(
  "shows only the row a search found: %s",
  async (id) => {
    const found = { ids: new Set([id]), groups: new Set<never>() };
    const { container } = await mount(BROWSER_SERVING, WINDOWS, found);
    const shown = [...container.querySelectorAll("[data-settings-search-id]:not([hidden])")].map((row) => row.getAttribute("data-settings-search-id"));
    expect(shown).toEqual([id]);
  },
);

const withClipboard = (writeText: (text: string) => Promise<void>): PlatformAdapter => ({ ...DESKTOP, clipboard: { writeText } });
const copyButton = (container: HTMLElement, name: string) => container.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`)!;

const WITH_ADDRESSES = { ...BROWSER_SERVING, server_addresses: ["my-mac.local", "192.168.1.20", "fd00::20"] };
const tokenOf = (container: HTMLElement) => container.querySelector<HTMLButtonElement>('button[aria-label*="This computer\'s address"]');
const menuItems = () => Array.from(document.body.querySelectorAll<HTMLElement>('[role="menuitemradio"]'));
const commandWith = (address: string) => BROWSER_SERVING.forward_command?.replace("<server>", address);

it("shows the forward command as plain text, with its placeholder, where the daemon lists no address, and copies it", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const { container } = await mountBrowserTab(BROWSER_SERVING, withClipboard(writeText));
  expect(tokenOf(container)).toBeNull();
  expect(container.textContent).toContain("Run it on the other computer, replacing");
  expect(container.textContent).toContain(BROWSER_SERVING.forward_command);
  const chip = [...container.querySelectorAll("[data-inline-reference]")].find((reference) => reference.textContent === "<server>")!;
  expect(chip.querySelector("code")).not.toBeNull();
  await act(async () => copyButton(container, "Copy forward command").click());
  expect(writeText).toHaveBeenLastCalledWith(BROWSER_SERVING.forward_command);
});

it("treats a daemon that sends no list of addresses as one that lists none", async () => {
  const { server_addresses: _omitted, ...older } = BROWSER_SERVING;
  const { container } = await mountBrowserTab(older as RemoteAccess);
  expect(tokenOf(container)).toBeNull();
  expect(container.textContent).toContain(BROWSER_SERVING.forward_command);
});

it("puts the computer's name, the first address listed, in the command as a token that names itself, and copies it", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const { container } = await mountBrowserTab(WITH_ADDRESSES, withClipboard(writeText));
  expect(tokenOf(container)?.textContent).toBe("my-mac.local");
  expect(tokenOf(container)?.getAttribute("aria-label")).toContain("This computer's address");
  expect(container.textContent).toContain(commandWith("my-mac.local"));
  expect(container.textContent).toContain("press the address in it to choose");
  await act(async () => copyButton(container, "Copy forward command").click());
  expect(writeText).toHaveBeenLastCalledWith(commandWith("my-mac.local"));
});

it("opens a menu of the addresses and a last item to enter it yourself, and shows and copies the command with the one chosen", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const { container } = await mountBrowserTab(WITH_ADDRESSES, withClipboard(writeText));
  await click(tokenOf(container));
  expect(menuItems().map((item) => item.textContent)).toEqual(["my-mac.local", "192.168.1.20", "fd00::20", "Enter the address yourself"]);
  expect(menuItems()[0].getAttribute("aria-checked")).toBe("true");
  await click(menuItems().find((item) => item.textContent === "fd00::20"));
  expect(tokenOf(container)?.textContent).toBe("fd00::20");
  expect(container.textContent).toContain(commandWith("fd00::20"));
  await act(async () => copyButton(container, "Copy forward command").click());
  expect(writeText).toHaveBeenLastCalledWith(commandWith("fd00::20"));
});

it("puts the placeholder back, in the command and its copy, when the last item is chosen", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const { container } = await mountBrowserTab(WITH_ADDRESSES, withClipboard(writeText));
  await click(tokenOf(container));
  await click(menuItems().find((item) => item.textContent === "Enter the address yourself"));
  expect(tokenOf(container)?.textContent).toBe("<server>");
  await act(async () => copyButton(container, "Copy forward command").click());
  expect(writeText).toHaveBeenLastCalledWith(BROWSER_SERVING.forward_command);
});

it("opens its menu from the keyboard on the token", async () => {
  const { container } = await mountBrowserTab(WITH_ADDRESSES);
  await act(async () => {
    tokenOf(container)!.focus();
    tokenOf(container)!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }));
    tokenOf(container)!.dispatchEvent(new KeyboardEvent("keyup", { key: "ArrowDown", bubbles: true, cancelable: true }));
  });
  expect(menuItems()).toHaveLength(4);
});

it("falls back to the first address when the one chosen is no longer listed", async () => {
  const { container, daemon } = await mountBrowserTab(WITH_ADDRESSES);
  await click(tokenOf(container));
  await click(menuItems().find((item) => item.textContent === "192.168.1.20"));
  await act(async () => {
    daemon.store.setState({ remoteAccess: { ...WITH_ADDRESSES, server_addresses: ["my-mac.local"] } });
  });
  expect(tokenOf(container)?.textContent).toBe("my-mac.local");
});

it("keeps the chosen placeholder when the daemon's list changes, and marks that last item as the one chosen", async () => {
  const { container, daemon } = await mountBrowserTab(WITH_ADDRESSES);
  await click(tokenOf(container));
  await click(menuItems().find((item) => item.textContent === "Enter the address yourself"));
  await act(async () => {
    daemon.store.setState({ remoteAccess: { ...WITH_ADDRESSES, server_addresses: ["other.local", "10.0.0.5"] } });
  });
  expect(tokenOf(container)?.textContent).toBe("<server>");
  expect(container.textContent).toContain(BROWSER_SERVING.forward_command);
  await click(tokenOf(container));
  expect(menuItems().map((item) => item.textContent)).toEqual(["other.local", "10.0.0.5", "Enter the address yourself"]);
  expect(menuItems().map((item) => item.getAttribute("aria-checked"))).toEqual(["false", "false", "true"]);
});

it("keeps the address chosen while the list grows", async () => {
  const { container, daemon } = await mountBrowserTab(WITH_ADDRESSES);
  await click(tokenOf(container));
  await click(menuItems().find((item) => item.textContent === "192.168.1.20"));
  await act(async () => {
    daemon.store.setState({ remoteAccess: { ...WITH_ADDRESSES, server_addresses: [...WITH_ADDRESSES.server_addresses, "10.0.0.5"] } });
  });
  expect(tokenOf(container)?.textContent).toBe("192.168.1.20");
});

it("shows a command with no placeholder in it as it is, whatever addresses are listed", async () => {
  const plain = "ssh -N -L 127.0.0.1:3369:127.0.0.1:3369 me@host.example";
  const { container } = await mountBrowserTab({ ...WITH_ADDRESSES, forward_command: plain });
  expect(tokenOf(container)).toBeNull();
  expect(container.textContent).toContain(plain);
});
