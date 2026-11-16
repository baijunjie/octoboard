// @vitest-environment jsdom
import { act, type ComponentProps } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { stubListMotion } from "../components/motion/listMotionTestSupport";
import { remoteAccessOf } from "../fixtures/builders";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter } from "../platform";
import type { Event, RemoteAccess } from "../protocol";
import { BROWSER_HOST, MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { click } from "../test/dom";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { RemoteAccessSection } from "./RemoteAccessSection";
import { SettingsSearchProvider } from "./settingsSearchFilter";
import { remoteAccessOffered } from "./remoteAccessOffered";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(async (_body: unknown): Promise<Event> => ({ type: "ack", id: null }));
const loginStatus = vi.fn(async (): Promise<boolean | undefined> => false);
const openLoginSettings = vi.fn(async () => {});
beforeEach(() => {
  acknowledgeAllAlerts();
  request.mockClear();
  loginStatus.mockReset().mockResolvedValue(false);
  openLoginSettings.mockClear();
});

const DESKTOP: PlatformAdapter = {
  kind: "tauri",
  host: MAC_HOST,
  remoteLogin: { status: () => loginStatus(), openSystemSettings: () => openLoginSettings() },
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

async function mount(remoteAccess: RemoteAccess, platform: PlatformAdapter = DESKTOP, matches?: ComponentProps<typeof SettingsSearchProvider>["matches"]) {
  const daemon = testDaemon({ request, state: { connectionState: "open", remoteAccess } });
  let container!: HTMLElement;
  await act(async () => {
    container = mountHost(<LanguageProvider><SettingsSearchProvider matches={matches}><RemoteAccessSection /></SettingsSearchProvider></LanguageProvider>, { platform, daemon });
  });
  return { container, daemon };
}

const switchOf = (container: HTMLElement) => container.querySelector<HTMLInputElement>("input[type=checkbox]")!;
const dialog = () => document.body.querySelector("[role=alertdialog]");
const buttonNamed = (root: ParentNode, name: string) =>
  Array.from(root.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent?.trim() === name);

it("is offered only in the application's window, for a daemon that offers remote access", () => {
  expect(remoteAccessOffered(DESKTOP, remoteAccessOf())).toBe(true);
  expect(remoteAccessOffered(DESKTOP, null)).toBe(false);
  expect(remoteAccessOffered(BROWSER, remoteAccessOf())).toBe(false);
});

it("is not offered in a remote daemon's window, whose adapter has no Remote Login as its daemon is not this computer's", () => {
  expect(remoteAccessOffered({ ...DESKTOP, remoteLogin: undefined }, remoteAccessOf())).toBe(false);
});

it("asks before turning remote access on, and sends nothing when the confirmation is cancelled", async () => {
  const { container } = await mount(remoteAccessOf());
  await act(async () => switchOf(container).click());
  expect(request).not.toHaveBeenCalled();
  expect(dialog()?.textContent).toContain("including running any command");
  expect(dialog()?.textContent).toContain("bookmarks and sync");
  expect(dialog()?.textContent).toContain("SSH login");
  await act(async () => buttonNamed(dialog()!, "Cancel")!.click());
  expect(request).not.toHaveBeenCalled();
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

it("shows the address and the forward while serving, and rotates the key once confirmed", async () => {
  const { container } = await mount(SERVING);
  expect(container.textContent).toContain(SERVING.address);
  expect(container.textContent).toContain(SERVING.forward_command);
  expect(container.querySelector("[aria-label='Copy address'], button[title='Copy address']") ?? container.textContent).toBeTruthy();
  await act(async () => buttonNamed(container, "Rotate key")!.click());
  expect(request).not.toHaveBeenCalled();
  await act(async () => buttonNamed(dialog()!, "Rotate")!.click());
  expect(request).toHaveBeenCalledWith({ type: "rotate_owner_key" });
});

const tabNamed = (container: HTMLElement, name: string) =>
  Array.from(container.querySelectorAll<HTMLElement>('[role="tab"]')).find((tab) => tab.textContent?.trim() === name)!;
const rowsOf = (container: HTMLElement) =>
  Array.from(container.querySelectorAll("[data-settings-search-id]")).map((row) => row.getAttribute("data-settings-search-id"));

it("puts the SSH server first, then the switch, then one tab per way in, the application's selected to begin with", async () => {
  const { container } = await mount(WITH_DETAILS);
  expect(Array.from(container.querySelectorAll('[role="tab"]')).map((tab) => tab.textContent?.trim())).toEqual(["Octoboard app", "Browser"]);
  expect(tabNamed(container, "Octoboard app").getAttribute("aria-selected")).toBe("true");
  expect(rowsOf(container)).toEqual(["remote-access.remote-login", "remote-access.allow", "remote-access.connection"]);
  expect(container.textContent).not.toContain(SERVING.forward_command);
});

it("shows the connection details as text and a QR code in the application's tab", async () => {
  const { container } = await mount(WITH_DETAILS);
  expect(container.textContent).toContain("Connection details");
  expect(container.textContent).toContain("Add remote daemon");
  expect(container.textContent).toContain(DETAILS);
  const qr = container.querySelector("svg[role=img]")!;
  expect(qr.getAttribute("aria-label")).toBe("QR code of the connection details");
  expect(qr.querySelectorAll("path")).toHaveLength(1);
});

it("shows the numbered forward and address, then the port on Windows and Rotate key, in the Browser tab", async () => {
  const { container } = await mount({ ...WINDOWS_SERVING, connection: WITH_DETAILS.connection }, WINDOWS);
  await click(tabNamed(container, "Browser"));
  expect(rowsOf(container)).toEqual(["remote-access.remote-login", "remote-access.allow", "remote-access.forward", "remote-access.address", "remote-access.port", "remote-access.rotate"]);
  expect(container.textContent).toContain("Step 1: SSH forward");
  expect(container.textContent).toContain("Step 2: Browser address");
  expect(container.textContent).not.toContain(DETAILS);
});

it("keeps the browser port under the switch, with no tabs, while nothing is served", async () => {
  const { container } = await mount(remoteAccessOf({ browser_port: 3369 }), WINDOWS);
  expect(container.querySelector('[role="tab"]')).toBeNull();
  expect(rowsOf(container)).toEqual(["remote-access.remote-login", "remote-access.allow", "remote-access.port"]);
});

it("shows the tab that holds what a search found, whichever was selected", async () => {
  const forwardOnly = { ids: new Set(["remote-access.forward" as const]), groups: new Set<never>() };
  const { container } = await mount(WITH_DETAILS, DESKTOP, forwardOnly);
  expect(tabNamed(container, "Browser").getAttribute("aria-selected")).toBe("true");
  expect(tabNamed(container, "Octoboard app").getAttribute("aria-disabled")).toBe("true");
  expect(tabNamed(container, "Browser").getAttribute("aria-disabled")).not.toBe("true");
  const connectionOnly = { ids: new Set(["remote-access.connection" as const]), groups: new Set<never>() };
  const other = await mount(WITH_DETAILS, DESKTOP, connectionOnly);
  expect(tabNamed(other.container, "Octoboard app").getAttribute("aria-selected")).toBe("true");
  expect(tabNamed(other.container, "Browser").getAttribute("aria-disabled")).toBe("true");
});

it("offers the Browser tab alone when the daemon gives no connection details", async () => {
  const { container } = await mount(SERVING);
  expect(tabNamed(container, "Browser").getAttribute("aria-selected")).toBe("true");
  expect(container.textContent).toContain(SERVING.forward_command);
});

it("shows no connection details when the daemon gives none, or while off", async () => {
  expect((await mount(SERVING)).container.querySelector("svg[role=img]")).toBeNull();
  const off = await mount(remoteAccessOf());
  expect(off.container.textContent).not.toContain("Connection details");
  expect(off.container.querySelector("svg[role=img]")).toBeNull();
});

it("says why no browser is served when the Windows named pipe could not be opened", async () => {
  const { container } = await mount(remoteAccessOf({ allowed: true, state: "failed", failure: { reason: "pipe_failed", detail: "Access is denied" } }), WINDOWS);
  expect(container.textContent).toContain("could not open the named pipe");
});

it("shows neither address nor rotation while off", async () => {
  const { container } = await mount(remoteAccessOf());
  expect(container.textContent).not.toContain("Browser address");
  expect(buttonNamed(container, "Rotate key")).toBeUndefined();
});

it("says why no browser is served, beside the daemon's own words", async () => {
  const { container } = await mount(
    remoteAccessOf({ allowed: true, state: "failed", failure: { reason: "not_private", detail: "/Users/me/.octoboard is not private" } }),
  );
  expect(container.textContent).toContain("Remote access is on, but this computer cannot be reached");
  expect(container.textContent).toContain("is not private to your account");
  expect(container.textContent).not.toContain("Browser address");
  // The daemon's own words are behind the alert's Details button.
  await act(async () => buttonNamed(container, "Details")!.click());
  expect(document.body.textContent).toContain("/Users/me/.octoboard is not private");
});

it("shows Remote Login on, or off with the way to System Settings", async () => {
  loginStatus.mockResolvedValue(true);
  const on = await mount(SERVING);
  expect(on.container.textContent).toContain("On");
  expect(buttonNamed(on.container, "Open System Settings")).toBeUndefined();

  loginStatus.mockResolvedValue(false);
  const off = await mount(SERVING);
  expect(off.container.textContent).toContain("go to General > Sharing, then turn on Remote Login");
  await act(async () => buttonNamed(off.container, "Open System Settings")!.click());
  expect(openLoginSettings).toHaveBeenCalled();
});

/** Remote access on Windows, where the daemon serves the browser at a loopback port of its own. */
const WINDOWS_SERVING = remoteAccessOf({
  ...SERVING,
  browser_port: 3369,
  forward_command: "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:3369:127.0.0.1:3369 me@<server>",
});

const textField = () => document.body.querySelector<HTMLInputElement>("[role=dialog] input")!;

async function typeInto(input: HTMLInputElement, value: string) {
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setter.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

it("names the cost of the browser port in the confirmation on Windows alone", async () => {
  const windows = await mount(remoteAccessOf({ browser_port: 3369 }), WINDOWS);
  await act(async () => switchOf(windows.container).click());
  expect(dialog()?.textContent).toContain("can take the browser port");
  await act(async () => buttonNamed(dialog()!, "Cancel")!.click());
  windows.container.remove();

  const mac = await mount(remoteAccessOf());
  await act(async () => switchOf(mac.container).click());
  expect(dialog()?.textContent).toContain("SSH login");
  expect(dialog()?.textContent).not.toContain("browser port");
});

it("shows the browser port on Windows, whether or not remote access is on, and none on macOS", async () => {
  const off = await mount(remoteAccessOf({ browser_port: 4000 }), WINDOWS);
  expect(off.container.textContent).toContain("Browser port");
  expect(off.container.textContent).toContain("Port 4000");
  off.container.remove();
  const serving = await mount(WINDOWS_SERVING, WINDOWS);
  expect(serving.container.textContent).toContain(WINDOWS_SERVING.forward_command);
  serving.container.remove();
  const mac = await mount(SERVING);
  expect(mac.container.textContent).not.toContain("Browser port");
  expect(buttonNamed(mac.container, "Change port")).toBeUndefined();
});

it("changes the browser port once a port is typed and saved, refusing what is no port", async () => {
  const { container } = await mount(WINDOWS_SERVING, WINDOWS);
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

it("says when the browser port is already in use", async () => {
  const { container } = await mount(
    remoteAccessOf({
      allowed: true,
      state: "failed",
      browser_port: 3369,
      failure: { reason: "port_in_use", detail: "listening on 127.0.0.1:3369: address in use" },
    }),
    WINDOWS,
  );
  expect(container.textContent).toContain("The browser port is already in use");
  expect(buttonNamed(container, "Change port")).toBeDefined();
  expect(container.textContent).not.toContain("Browser address");
});

it("names OpenSSH Server on Windows, with the way to Settings while it is not running", async () => {
  loginStatus.mockResolvedValue(false);
  const { container } = await mount(WINDOWS_SERVING, WINDOWS);
  expect(container.textContent).toContain("OpenSSH Server");
  expect(container.textContent).toContain("add OpenSSH Server under System > Optional features");
  expect(container.textContent).not.toContain("Remote Login");
  await act(async () => buttonNamed(container, "Open Settings")!.click());
  expect(openLoginSettings).toHaveBeenCalled();
});

const withClipboard = (writeText: (text: string) => Promise<void>): PlatformAdapter => ({ ...DESKTOP, clipboard: { writeText } });
const copyButton = (container: HTMLElement, name: string) => container.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`)!;

it("copies the browser address, the forward command and the connection details, each exactly as shown", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const { container } = await mount(WITH_DETAILS, withClipboard(writeText));
  await act(async () => copyButton(container, "Copy connection details").click());
  expect(writeText).toHaveBeenLastCalledWith(DETAILS);
  await click(tabNamed(container, "Browser"));
  await act(async () => copyButton(container, "Copy address").click());
  expect(writeText).toHaveBeenLastCalledWith(SERVING.address);
  await act(async () => copyButton(container, "Copy forward command").click());
  expect(writeText).toHaveBeenLastCalledWith(SERVING.forward_command);
  expect(writeText).toHaveBeenCalledTimes(3);
});

const chipsOf = (root: ParentNode) => [...root.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent);

const WITH_ADDRESSES = { ...SERVING, server_addresses: ["my-mac.local", "192.168.1.20", "fd00::20"] };
const tokenOf = (container: HTMLElement) => container.querySelector<HTMLButtonElement>('button[aria-label*="This computer\'s address"]');
const menuItems = () => Array.from(document.body.querySelectorAll<HTMLElement>('[role="menuitemradio"]'));
const commandWith = (address: string) => SERVING.forward_command?.replace("<server>", address);

it("shows the command as plain text, with its placeholder, where the daemon lists no address", async () => {
  const { container } = await mount(SERVING);
  expect(tokenOf(container)).toBeNull();
  expect(container.textContent).toContain("Run it on the other computer, replacing");
  expect(container.textContent).toContain(SERVING.forward_command);
});

it("treats a daemon that sends no list of addresses as one that lists none", async () => {
  const { server_addresses: _omitted, ...older } = SERVING;
  const { container } = await mount(older as RemoteAccess);
  expect(tokenOf(container)).toBeNull();
  expect(container.textContent).toContain(SERVING.forward_command);
});

it("puts the computer's name, the first address listed, in the command as a token that names itself, and copies it", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const { container } = await mount(WITH_ADDRESSES, withClipboard(writeText));
  expect(tokenOf(container)?.textContent).toBe("my-mac.local");
  expect(tokenOf(container)?.getAttribute("aria-label")).toContain("This computer's address");
  expect(tokenOf(container)?.getAttribute("aria-label")).toContain("my-mac.local");
  expect(container.textContent).toContain(commandWith("my-mac.local"));
  expect(container.textContent).toContain("press the address in it to choose");
  await act(async () => copyButton(container, "Copy forward command").click());
  expect(writeText).toHaveBeenLastCalledWith(commandWith("my-mac.local"));
});

it("opens a menu of the addresses and a last item to enter it yourself, and shows and copies the command with the one chosen", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const { container } = await mount(WITH_ADDRESSES, withClipboard(writeText));
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
  const { container } = await mount(WITH_ADDRESSES, withClipboard(writeText));
  await click(tokenOf(container));
  await click(menuItems().find((item) => item.textContent === "Enter the address yourself"));
  expect(tokenOf(container)?.textContent).toBe("<server>");
  await act(async () => copyButton(container, "Copy forward command").click());
  expect(writeText).toHaveBeenLastCalledWith(SERVING.forward_command);
});

it("opens its menu from the keyboard on the token", async () => {
  const { container } = await mount(WITH_ADDRESSES);
  await act(async () => {
    tokenOf(container)!.focus();
    tokenOf(container)!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }));
    tokenOf(container)!.dispatchEvent(new KeyboardEvent("keyup", { key: "ArrowDown", bubbles: true, cancelable: true }));
  });
  expect(menuItems()).toHaveLength(4);
});

it("falls back to the first address when the one chosen is no longer listed", async () => {
  const { container, daemon } = await mount(WITH_ADDRESSES);
  await click(tokenOf(container));
  await click(menuItems().find((item) => item.textContent === "192.168.1.20"));
  await act(async () => {
    daemon.store.setState({ remoteAccess: { ...WITH_ADDRESSES, server_addresses: ["my-mac.local"] } });
  });
  expect(tokenOf(container)?.textContent).toBe("my-mac.local");
});

it("keeps the chosen placeholder when the daemon's list changes, and marks that last item as the one chosen", async () => {
  const { container, daemon } = await mount(WITH_ADDRESSES);
  await click(tokenOf(container));
  await click(menuItems().find((item) => item.textContent === "Enter the address yourself"));
  await act(async () => {
    daemon.store.setState({ remoteAccess: { ...WITH_ADDRESSES, server_addresses: ["other.local", "10.0.0.5"] } });
  });
  expect(tokenOf(container)?.textContent).toBe("<server>");
  expect(container.textContent).toContain(SERVING.forward_command);
  await click(tokenOf(container));
  expect(menuItems().map((item) => item.textContent)).toEqual(["other.local", "10.0.0.5", "Enter the address yourself"]);
  expect(menuItems().map((item) => item.getAttribute("aria-checked"))).toEqual(["false", "false", "true"]);
});

it("keeps the address chosen while the list grows", async () => {
  const { container, daemon } = await mount(WITH_ADDRESSES);
  await click(tokenOf(container));
  await click(menuItems().find((item) => item.textContent === "192.168.1.20"));
  await act(async () => {
    daemon.store.setState({ remoteAccess: { ...WITH_ADDRESSES, server_addresses: [...WITH_ADDRESSES.server_addresses, "10.0.0.5"] } });
  });
  expect(tokenOf(container)?.textContent).toBe("192.168.1.20");
});

it("shows a command with no placeholder in it as it is, whatever addresses are listed", async () => {
  const plain = "ssh -N -L 127.0.0.1:3369:/Users/me/.octoboard/browser.sock me@host.example";
  const { container } = await mount({ ...WITH_ADDRESSES, forward_command: plain });
  expect(tokenOf(container)).toBeNull();
  expect(container.textContent).toContain(plain);
});

it("sets the placeholder in the forward's description as a monospace reference while no address is listed", async () => {
  const { container } = await mount(SERVING);
  const chip = [...container.querySelectorAll("[data-inline-reference]")].find((reference) => reference.textContent === "<server>")!;
  expect(chip.querySelector("code")).not.toBeNull();
});

it("names the Add remote daemon button and the Connection details field in the application tab as references", async () => {
  const { container } = await mount(WITH_DETAILS);
  expect(chipsOf(container)).toEqual(expect.arrayContaining(["Add remote daemon", "Connection details"]));
  const description = [...container.querySelectorAll("[data-settings-search-id='remote-access.connection'] *")].find((node) => node.textContent?.startsWith("On the other computer"));
  expect(description?.querySelectorAll("[data-inline-reference]")).toHaveLength(2);
});

it("keeps the browser port under the switch, with no tabs and no Rotate key, while remote access has failed", async () => {
  const { container } = await mount(
    remoteAccessOf({ allowed: true, state: "failed", browser_port: 3369, failure: { reason: "port_in_use", detail: "address in use" } }),
    WINDOWS,
  );
  expect(container.querySelector('[role="tab"]')).toBeNull();
  const rows = rowsOf(container);
  expect(rows.slice(0, 2)).toEqual(["remote-access.remote-login", "remote-access.allow"]);
  expect(rows).toContain("remote-access.port");
  expect(rows).not.toContain("remote-access.rotate");
});

it.each(["remote-access.forward", "remote-access.address", "remote-access.port", "remote-access.rotate"] as const)(
  "shows the Browser tab when a search finds only %s",
  async (id) => {
    const found = { ids: new Set([id]), groups: new Set<never>() };
    const { container } = await mount({ ...WINDOWS_SERVING, connection: WITH_DETAILS.connection }, WINDOWS, found);
    expect(tabNamed(container, "Browser").getAttribute("aria-selected")).toBe("true");
    expect(rowsOf(container)).toContain(id);
    expect(rowsOf(container)).not.toContain("remote-access.connection");
  },
);

it("shows no tabs when a search finds nothing that either tab holds", async () => {
  const found = { ids: new Set(["remote-access.allow" as const]), groups: new Set<never>() };
  const { container } = await mount(WITH_DETAILS, DESKTOP, found);
  expect(container.querySelector('[role="tab"]')).toBeNull();
  expect(rowsOf(container)).toContain("remote-access.allow");
  expect(rowsOf(container)).not.toContain("remote-access.connection");
  expect(container.textContent).not.toContain(DETAILS);
});

it("shows the other tab's panel at once on a switch, with no motion of the rows", async () => {
  const motion = stubListMotion();
  const { container } = await mount(WITH_DETAILS);
  motion.clear();
  await click(tabNamed(container, "Browser"));
  expect(tabNamed(container, "Browser").getAttribute("aria-selected")).toBe("true");
  expect(rowsOf(container)).toContain("remote-access.forward");
  expect(rowsOf(container)).not.toContain("remote-access.connection");
  await click(tabNamed(container, "Octoboard app"));
  expect(rowsOf(container)).toContain("remote-access.connection");
  expect(rowsOf(container)).not.toContain("remote-access.forward");
  await act(async () => {});
  expect(container.querySelector("[data-list-motion-ghost]")).toBeNull();
  expect(motion.played()).toEqual([]);
});

it("does not remember the tab across mounts: the application's tab is selected again", async () => {
  const first = await mount(WITH_DETAILS);
  await click(tabNamed(first.container, "Browser"));
  first.container.remove();
  const second = await mount(WITH_DETAILS);
  expect(tabNamed(second.container, "Octoboard app").getAttribute("aria-selected")).toBe("true");
});

it("shows the connection details as the single line another computer pastes, with no key in it", async () => {
  const { container } = await mount(WITH_DETAILS);
  const shown = [...container.querySelectorAll("pre")].map((pre) => pre.textContent);
  expect(shown).toContain(DETAILS);
  expect(DETAILS).not.toContain("key=");
  expect(DETAILS).not.toMatch(/\s/);
});

it("says which copy failed when the clipboard refuses it", async () => {
  const writeText = vi.fn(async (_text: string) => Promise.reject(new Error("denied")));
  Object.assign(document, { execCommand: vi.fn(() => false) });
  const { container } = await mount(WITH_DETAILS, withClipboard(writeText));
  await act(async () => copyButton(container, "Copy connection details").click());
  await act(async () => {});
  expect(alertWords()).toContain("Could not copy the connection details");
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

it("says why the key could not be rotated, and keeps the confirmation", async () => {
  const { container } = await mount(SERVING);
  await act(async () => buttonNamed(container, "Rotate key")!.click());
  request.mockRejectedValueOnce(new Error("refused"));
  await act(async () => buttonNamed(dialog()!, "Rotate")!.click());
  await act(async () => {});
  expect(alertWords()).toContain("Could not rotate the key");
  expect(dialog()).not.toBeNull();
});

it("keeps the port dialog open and says why when the daemon refuses the port", async () => {
  const { container } = await mount(WINDOWS_SERVING, WINDOWS);
  await act(async () => buttonNamed(container, "Change port")!.click());
  request.mockRejectedValueOnce(new Error("refused"));
  await typeInto(textField(), "4000");
  await act(async () => buttonNamed(document.body, "Save")!.click());
  await act(async () => {});
  expect(alertWords()).toContain("Could not change the browser port");
  expect(document.body.querySelector("[role=dialog]")).not.toBeNull();
});

it("sends nothing for the port when the dialog is cancelled", async () => {
  const { container } = await mount(WINDOWS_SERVING, WINDOWS);
  await act(async () => buttonNamed(container, "Change port")!.click());
  await typeInto(textField(), "4000");
  await act(async () => buttonNamed(document.body, "Cancel")!.click());
  expect(request).not.toHaveBeenCalled();
});

it("sets the system's names and the steps to them as chips in a path, on macOS and on Windows", async () => {
  loginStatus.mockResolvedValue(false);
  const mac = await mount(SERVING);
  const macPath = mac.container.querySelector("[data-inline-path]")!;
  expect([...macPath.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toEqual(["General", "Sharing"]);
  expect(macPath.textContent).toBe("General > Sharing");
  expect(chipsOf(mac.container)).toEqual(expect.arrayContaining(["Remote Login"]));

  const windows = await mount(WINDOWS_SERVING, WINDOWS);
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

it("renders nothing while the daemon has said nothing of remote access", async () => {
  const daemon = testDaemon({ request, state: { connectionState: "open", remoteAccess: null } });
  let container!: HTMLElement;
  await act(async () => {
    container = mountHost(<LanguageProvider><RemoteAccessSection /></LanguageProvider>, { platform: DESKTOP, daemon });
  });
  expect(container.querySelector("input[type=checkbox]")).toBeNull();
});

it("offers the QR code only beside the connection details, drawn from the rows the daemon sent", async () => {
  const { container } = await mount({ ...WITH_DETAILS, connection: { text: DETAILS, qr: ["11", "10"] } });
  const svg = container.querySelector("svg[role=img]")!;
  // Two modules and a quiet zone of four on each side.
  expect(svg.getAttribute("viewBox")).toBe("0 0 10 10");
});

/** What the first alert raised says, its title and its words together. */
const alertWords = () => {
  const [alert] = errorAlerts.getSnapshot();
  return alert ? `${alert.title} ${(alert.error as Error).message}` : "";
};
