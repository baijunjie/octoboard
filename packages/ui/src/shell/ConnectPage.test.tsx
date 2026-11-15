// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { ActionableError } from "../errors/actionableError";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter, RemoteDaemonConnectOutcome, RemoteDaemonEntry, RemoteDaemonEntryInput, RemoteDaemonStatus } from "../platform";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { ConnectPage } from "./ConnectPage";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const STAGING: RemoteDaemonEntry = {
  id: "staging",
  name: "Staging",
  host: "staging.example.com",
  port: 22,
  user: "deploy",
  auth: "password",
  keyPath: null,
  fingerprint: null,
  octoboarddPath: null,
};
const KEYED: RemoteDaemonEntry = { ...STAGING, id: "keyed", name: "Keyed", auth: "key", keyPath: "~/.ssh/id_ed25519" };
const DEVICE: RemoteDaemonEntry = { ...STAGING, id: "device", name: "Device", auth: "device" };

const list = vi.fn(async (): Promise<RemoteDaemonEntry[]> => [STAGING]);
const status = vi.fn(async (): Promise<RemoteDaemonStatus[]> => []);
const save = vi.fn(async (entry: RemoteDaemonEntryInput): Promise<RemoteDaemonEntry> => ({ ...entry, id: entry.id ?? "new" }));
const remove = vi.fn(async (_id: string) => {});
const connect = vi.fn(async (_request: unknown): Promise<RemoteDaemonConnectOutcome> => ({ outcome: "opened" }));
const disconnect = vi.fn(async (_id: string) => {});
let changed: (() => void) | undefined;
const showUpdates = vi.fn(async () => {});
beforeEach(() => {
  acknowledgeAllAlerts();
  list.mockReset().mockResolvedValue([STAGING]);
  status.mockReset().mockResolvedValue([]);
  save.mockClear();
  remove.mockClear();
  disconnect.mockClear();
  showUpdates.mockClear();
  connect.mockReset().mockResolvedValue({ outcome: "opened" });
  changed = undefined;
});

const DESKTOP: PlatformAdapter = {
  kind: "tauri",
  host: MAC_HOST,
  remoteDaemons: {
    list: () => list(),
    status: () => status(),
    onChanged: (listener) => {
      changed = listener;
      return () => {};
    },
    save: (entry) => save(entry),
    remove: (id) => remove(id),
    connect: (request) => connect(request),
    disconnect: (id) => disconnect(id),
    newWindow: async () => {},
    openWindow: async () => {},
    showMainWindow: async () => {},
    showUpdates: () => showUpdates(),
  },
};

async function mount(entryId?: string) {
  let container!: HTMLElement;
  await act(async () => {
    container = mountHost(<LanguageProvider><ConnectPage entryId={entryId} /></LanguageProvider>, { platform: DESKTOP });
  });
  return container;
}

/** A button's accessible name without the isolates around the names substituted into it. */
const nameOf = (button: Element) => (button.getAttribute("aria-label") ?? "").replace(/[\u2068\u2069]/g, "");
const rowButton = (container: ParentNode, verb: "Connect to" | "Open" | "Edit" | "Remove", name = "Staging") =>
  Array.from(container.querySelectorAll<HTMLButtonElement>("button")).find((button) => nameOf(button) === `${verb} ${name}`)!;
const pressTile = (container: ParentNode, name?: string) => act(async () => rowButton(container, "Connect to", name).click());

const modal = () => [...document.body.querySelectorAll("[role=dialog], [role=alertdialog]")].at(-1) as HTMLElement | undefined;
const buttonNamed = (root: ParentNode, name: string) =>
  Array.from(root.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent?.trim() === name);
const press = (root: ParentNode, name: string) => act(async () => buttonNamed(root, name)!.click());
const firstInput = (root: ParentNode) => root.querySelector<HTMLInputElement>("input")!;
function type(input: HTMLInputElement, value: string) {
  return act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
const alert = () => errorAlerts.getSnapshot()[0];
const sentRequest = (call = -1) => connect.mock.calls.at(call)![0];

it("asks for the password first, sends it with the request, and refreshes the list once the window is open", async () => {
  const container = await mount();
  await pressTile(container);
  expect(connect).not.toHaveBeenCalled();
  expect(modal()!.textContent).toContain("It is not stored");
  await type(firstInput(modal()!), "hunter2");
  await press(modal()!, "Connect");
  expect(connect).toHaveBeenCalledTimes(1);
  expect(sentRequest()).toEqual({ id: "staging", secret: "hunter2", confirmFingerprint: null, action: null, browser: false });
  expect(list).toHaveBeenCalledTimes(2);
  expect(modal()).toBeUndefined();
});

it("tries a key without a secret and asks for the passphrase when the shell does", async () => {
  list.mockResolvedValue([KEYED]);
  connect.mockResolvedValueOnce({ outcome: "secret_needed", kind: "passphrase" });
  const container = await mount();
  await pressTile(container, "Keyed");
  expect(sentRequest()).toEqual({ id: "keyed", secret: null, confirmFingerprint: null, action: null, browser: false });
  expect(modal()!.textContent).toContain("Enter key passphrase");
  await type(firstInput(modal()!), "open sesame");
  await press(modal()!, "Connect");
  expect(connect).toHaveBeenCalledTimes(2);
  expect(sentRequest()).toEqual({ id: "keyed", secret: "open sesame", confirmFingerprint: null, action: null, browser: false });
});

it("connects with this computer's key without asking, and asks for the password once the server refuses the key", async () => {
  list.mockResolvedValue([DEVICE]);
  connect.mockResolvedValueOnce({ outcome: "device_key_refused" });
  const container = await mount();
  await pressTile(container, "Device");
  expect(sentRequest()).toEqual({ id: "device", secret: null, confirmFingerprint: null, action: null, browser: false });
  expect(modal()!.textContent).toContain("Enter password");
  expect(modal()!.textContent).toContain("no longer accepts this computer’s key");
  await type(firstInput(modal()!), "hunter2");
  await press(modal()!, "Connect");
  expect(connect).toHaveBeenCalledTimes(2);
  expect(sentRequest()).toEqual({ id: "device", secret: "hunter2", confirmFingerprint: null, action: null, browser: false });
});

it("shows the fingerprint of a host not pinned yet and retries with it once confirmed, keeping the password", async () => {
  connect.mockResolvedValueOnce({ outcome: "host_key_unknown", fingerprint: "SHA256:abc123" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(modal()!.textContent).toContain("SHA256:abc123");
  expect(modal()!.textContent).toContain("ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub");
  await press(modal()!, "Connect");
  expect(connect).toHaveBeenCalledTimes(2);
  expect(sentRequest()).toEqual({ id: "staging", secret: "pw", confirmFingerprint: "SHA256:abc123", action: null, browser: false });
});

it("refuses a host key that does not match, naming both fingerprints, and opens no dialog", async () => {
  connect.mockResolvedValueOnce({ outcome: "host_key_mismatch", expected: "SHA256:pinned", actual: "SHA256:presented" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(alert().title).toBe("The host key does not match");
  expect((alert().error as ActionableError).detail).toBe("Pinned: SHA256:pinned\nPresented: SHA256:presented");
  expect(connect).toHaveBeenCalledTimes(1);
});

it("offers to start the daemon where none is running, and retries with the start action", async () => {
  connect.mockResolvedValueOnce({ outcome: "no_daemon" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(modal()!.textContent).toContain("No daemon is running on");
  await press(modal()!, "Start daemon");
  expect(sentRequest()).toEqual({ id: "staging", secret: "pw", confirmFingerprint: null, action: "start", browser: false });
});

it("says the application has to be running where the daemon is the desktop application's, and retries on Try again", async () => {
  connect.mockResolvedValueOnce({ outcome: "application_not_running", detail: "" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(modal()!.textContent).toContain("is not running on");
  expect(modal()!.textContent).toContain("Open Octoboard on that computer");
  expect(buttonNamed(modal()!, "Start daemon")).toBeUndefined();
  connect.mockResolvedValueOnce({ outcome: "opened" });
  await press(modal()!, "Try again");
  expect(connect).toHaveBeenCalledTimes(2);
  expect(sentRequest()).toEqual({ id: "staging", secret: "pw", confirmFingerprint: null, action: null, browser: false });
});

it("names the daemon's own words beside the application not running, when it gives some", async () => {
  connect.mockResolvedValueOnce({ outcome: "application_not_running", detail: "no owner socket" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(modal()!.textContent).toContain("no owner socket");
});

const OLDER = { outcome: "generation_mismatch", remoteGeneration: 1, localGeneration: 2, remoteVersion: "1.4.0", localVersion: "2.0.0" } as const;
const NEWER = { outcome: "generation_mismatch", remoteGeneration: 3, localGeneration: 2, remoteVersion: "3.0.0", localVersion: "2.0.0" } as const;
const buttonLabels = (root: ParentNode) => Array.from(root.querySelectorAll<HTMLButtonElement>("button")).map((button) => button.textContent?.trim());

it("names both versions when the server's daemon is older, says it is updated on the server itself, and offers no update", async () => {
  connect.mockResolvedValueOnce(OLDER);
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(modal()!.textContent).toContain("1.4.0");
  expect(modal()!.textContent).toContain("2.0.0");
  expect(modal()!.textContent).toContain("Update it on the server itself.");
  expect(buttonNamed(modal()!, "Update daemon")).toBeUndefined();
  // Only the request that held the password has been made: nothing asked the server to update.
  expect(connect).toHaveBeenCalledTimes(1);
  expect(connect.mock.calls.every(([request]) => (request as { action: string | null }).action !== "update")).toBe(true);
});

it.each([
  ["older", OLDER, "Cancel"],
  ["newer", NEWER, "Go to Updates"],
] as const)("offers an %s daemon's mismatch dialog Open in browser as the primary action and %s beside it, and nothing else", async (_name, outcome, other) => {
  connect.mockResolvedValueOnce(outcome);
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  const labels = buttonLabels(modal()!).filter((label) => label);
  expect(labels).toHaveLength(2);
  expect(labels).toEqual(expect.arrayContaining(["Open in browser", other]));
  // The dialog's confirming (submit) button is the primary action.
  expect(modal()!.querySelector("button[type=submit]")!.textContent!.trim()).toBe("Open in browser");
});

it("takes the owner to this app's Updates from a newer daemon's mismatch dialog, closing it", async () => {
  connect.mockResolvedValueOnce(NEWER);
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  await press(modal()!, "Go to Updates");
  expect(showUpdates).toHaveBeenCalledOnce();
  expect(modal()).toBeUndefined();
});

it("opens an older daemon's own web interface in the browser through the shell, holding the connection with the same password", async () => {
  connect.mockResolvedValueOnce(OLDER).mockResolvedValueOnce({ outcome: "opened_in_browser" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  await press(modal()!, "Open in browser");
  expect(sentRequest()).toEqual({ id: "staging", secret: "pw", confirmFingerprint: null, action: null, browser: true });
  expect(modal()).toBeUndefined();
});

it("offers a newer daemon's own web interface and no update", async () => {
  connect.mockResolvedValueOnce(NEWER).mockResolvedValueOnce({ outcome: "opened_in_browser" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(modal()!.textContent).toContain("3.0.0");
  expect(modal()!.textContent).not.toContain("Updates section");
  expect(buttonNamed(modal()!, "Update daemon")).toBeUndefined();
  await press(modal()!, "Open in browser");
  expect(sentRequest()).toEqual({ id: "staging", secret: "pw", confirmFingerprint: null, action: null, browser: true });
});

it("connects an entry held for the browser without asking for the password again", async () => {
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "browser" }]);
  connect.mockResolvedValueOnce({ outcome: "opened_in_browser" });
  const container = await mount();
  expect(container.textContent).toContain("Opened in browser");
  // A connected one offers Open in place of Connect.
  expect(rowButton(container, "Connect to")).toBeUndefined();
  await act(async () => rowButton(container, "Open").click());
  expect(modal()).toBeUndefined();
  expect(sentRequest()).toEqual({ id: "staging", secret: null, confirmFingerprint: null, action: null, browser: false });
});

it("lists the saved entries with where they connect to, or says there are none", async () => {
  list.mockResolvedValue([STAGING, KEYED]);
  const container = await mount();
  expect(container.textContent).toContain("deploy@staging.example.com:22");
  expect(rowButton(container, "Connect to", "Keyed")).toBeDefined();
  list.mockResolvedValue([]);
  const empty = await mount();
  expect(empty.textContent).toContain("No remote daemons yet");
});

it("holds Add remote daemon in the empty state, and offers it below the list only once there are entries", async () => {
  list.mockResolvedValue([]);
  const empty = await mount();
  expect(empty.querySelectorAll("button").length).toBe(1);
  expect(buttonNamed(empty, "Add remote daemon")).toBeDefined();
  list.mockResolvedValue([STAGING]);
  const listed = await mount();
  expect(buttonNamed(listed, "Add remote daemon")).toBeDefined();
});

it("connects one entry at a time: the others' Connect is unavailable while one connects", async () => {
  list.mockResolvedValue([KEYED, { ...KEYED, id: "other", name: "Other" }]);
  let finish!: (outcome: RemoteDaemonConnectOutcome) => void;
  connect.mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)));
  const container = await mount();
  await pressTile(container, "Keyed");
  const other = rowButton(container, "Connect to", "Other")!;
  expect(other.disabled || other.getAttribute("aria-disabled") === "true").toBe(true);
  await act(async () => other.click());
  expect(connect).toHaveBeenCalledTimes(1);
  await act(async () => finish({ outcome: "opened" }));
  const freed = rowButton(container, "Connect to", "Other")!;
  expect(freed.disabled || freed.getAttribute("aria-disabled") === "true").toBe(false);
});

it("waits for the connection states before connecting to the entry the page was opened for, so a held one asks for no password", async () => {
  let answer!: (statuses: RemoteDaemonStatus[]) => void;
  status.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
  await mount("staging");
  expect(modal()).toBeUndefined();
  expect(connect).not.toHaveBeenCalled();
  await act(async () => answer([{ id: "staging", state: "connected", via: "browser" }]));
  expect(modal()).toBeUndefined();
  expect(sentRequest()).toEqual({ id: "staging", secret: null, confirmFingerprint: null, action: null, browser: false });
});

it("reads the list again when the shell says it changed, as a window opening or closing does", async () => {
  const container = await mount();
  list.mockResolvedValue([STAGING, KEYED]);
  await act(async () => changed!());
  expect(rowButton(container, "Connect to", "Keyed")).toBeDefined();
});

it("starts connecting to the entry the page was opened for at once", async () => {
  list.mockResolvedValue([KEYED]);
  await mount("keyed");
  expect(sentRequest()).toEqual({ id: "keyed", secret: null, confirmFingerprint: null, action: null, browser: false });
});

it("asks for the password of the entry the page was opened for, in this window", async () => {
  await mount("staging");
  expect(connect).not.toHaveBeenCalled();
  expect(modal()!.textContent).toContain("It is not stored");
});

it("asks where octoboardd is, saves the path in the entry and connects again", async () => {
  connect.mockResolvedValueOnce({ outcome: "octoboardd_not_found", tried: ["~/.octoboard/bin/octoboardd", "/usr/local/bin/octoboardd"] });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(modal()!.textContent).toContain("/usr/local/bin/octoboardd");
  await type(modal()!.querySelector<HTMLInputElement>("input")!, "/opt/octoboard/octoboardd");
  await press(modal()!, "Save and connect");
  expect(save).toHaveBeenCalledWith({ ...STAGING, octoboarddPath: "/opt/octoboard/octoboardd" });
  expect(sentRequest()).toEqual({ id: "staging", secret: "pw", confirmFingerprint: null, action: null, browser: false });
});

it.each([
  [{ outcome: "unreachable", detail: "connection refused" }, "Could not reach the server"],
  [{ outcome: "auth_failed", detail: "denied" }, "Could not sign in"],
  [{ outcome: "key_unreadable", detail: "bad key" }, "Could not read the key file"],
  [{ outcome: "no_owner_endpoint", detail: "no socket" }, "The daemon cannot be connected to"],
  [{ outcome: "endpoint_unreachable", detail: "timed out" }, "Could not reach the daemon"],
  [{ outcome: "action_failed", action: "start", detail: "exit 1" }, "Could not start the daemon"],
] as [RemoteDaemonConnectOutcome, string][])("raises an alert for %o, with the shell's detail kept apart", async (outcome, title) => {
  connect.mockResolvedValueOnce(outcome);
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(alert().title).toBe(title);
  expect((alert().error as ActionableError).detail).toBe((outcome as { detail: string }).detail);
});

it("raises a failure the app does not recognise as unexpected, with the shell's text as what failed", async () => {
  connect.mockResolvedValueOnce({ outcome: "failed", detail: "odd" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(alert().title).toBe("Could not connect to the remote daemon");
  expect(alert().error).not.toBeInstanceOf(ActionableError);
  expect((alert().error as Error).message).toBe("odd");
});

const addButton = (container: ParentNode) => buttonNamed(container, "Add remote daemon")!;

it("adds an entry, naming it after its host when the name is left empty", async () => {
  list.mockResolvedValue([]);
  const container = await mount();
  await act(async () => addButton(container).click());
  const inputs = modal()!.querySelectorAll<HTMLInputElement>("input[type=text], input:not([type])");
  await type(inputs[2]!, "box.example.com");
  await type(inputs[4]!, "me");
  await press(modal()!, "Save");
  expect(save).toHaveBeenCalledWith({
    id: null,
    name: "box.example.com",
    host: "box.example.com",
    port: 22,
    user: "me",
    auth: "password",
    keyPath: null,
    fingerprint: null,
    octoboarddPath: null,
  });
  expect(modal()).toBeUndefined();
});

it("fills an entry from pasted connection details, pinning their host key, and refuses text that is not any", async () => {
  list.mockResolvedValue([]);
  const container = await mount();
  await act(async () => addButton(container).click());
  const details = firstInput(modal()!);
  await type(details, "not details");
  await press(modal()!, "Save");
  expect(save).not.toHaveBeenCalled();
  expect(modal()!.textContent).toContain("These are not connection details.");

  await type(details, " octoboard+ssh://me@198.51.100.7:2200?fingerprint=SHA256:ab%2Bc/d&octoboardd=/home/me/.local/bin/octoboardd&name=box ");
  expect(modal()!.textContent).toContain("SHA256:ab+c/d");
  await press(modal()!, "Save");
  expect(save).toHaveBeenCalledWith({
    id: null,
    name: "box",
    host: "198.51.100.7",
    port: 2200,
    user: "me",
    auth: "password",
    keyPath: null,
    fingerprint: "SHA256:ab+c/d",
    octoboarddPath: "/home/me/.local/bin/octoboardd",
  });
});

it("refuses an entry without a host or user", async () => {
  list.mockResolvedValue([]);
  const container = await mount();
  await act(async () => addButton(container).click());
  await press(modal()!, "Save");
  expect(save).not.toHaveBeenCalled();
  expect(modal()!.textContent).toContain("Enter the host.");
});

it("edits an entry, clearing its pinned host key on request", async () => {
  list.mockResolvedValue([{ ...STAGING, fingerprint: "SHA256:pinned" }]);
  const container = await mount();
  await act(async () => rowButton(container, "Edit").click());
  expect(modal()!.textContent).toContain("SHA256:pinned");
  await press(modal()!, "Clear");
  expect(modal()!.textContent).toContain("Not pinned yet");
  await press(modal()!, "Save");
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ id: "staging", fingerprint: null }));
});

it("asks before removing an entry", async () => {
  const container = await mount();
  await act(async () => rowButton(container, "Remove").click());
  expect(remove).not.toHaveBeenCalled();
  expect(modal()!.textContent).toContain("will be removed from the remote daemons");
  await press(modal()!, "Remove");
  expect(remove).toHaveBeenCalledWith("staging");
});

it("never keeps the password beyond the connection it was typed for: a new connection asks again", async () => {
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "hunter2");
  await press(modal()!, "Connect");
  expect(modal()).toBeUndefined();
  await pressTile(container);
  expect(connect).toHaveBeenCalledTimes(1);
  expect(modal()!.textContent).toContain("It is not stored");
  expect(firstInput(modal()!).value).toBe("");
});

it("connects to nothing when the password dialog is cancelled", async () => {
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "hunter2");
  await press(modal()!, "Cancel");
  expect(modal()).toBeUndefined();
  expect(connect).not.toHaveBeenCalled();
});

it("connects to nothing, and pins nothing, when the host key is not confirmed", async () => {
  connect.mockResolvedValueOnce({ outcome: "host_key_unknown", fingerprint: "SHA256:abc123" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(modal()!.textContent).toContain("SHA256:abc123");
  await press(modal()!, "Cancel");
  expect(modal()).toBeUndefined();
  expect(connect).toHaveBeenCalledTimes(1);
  expect(save).not.toHaveBeenCalled();
});

it("confirms a host key for an entry that signs in with this computer's key without asking for a password", async () => {
  list.mockResolvedValue([DEVICE]);
  connect.mockResolvedValueOnce({ outcome: "host_key_unknown", fingerprint: "SHA256:abc123" });
  const container = await mount();
  await pressTile(container, "Device");
  await press(modal()!, "Connect");
  expect(connect).toHaveBeenCalledTimes(2);
  expect(sentRequest()).toEqual({ id: "device", secret: null, confirmFingerprint: "SHA256:abc123", action: null, browser: false });
});

it("takes the next outcome after a start, which can need the host key confirmed first, like any other", async () => {
  connect
    .mockResolvedValueOnce({ outcome: "no_daemon" })
    .mockResolvedValueOnce({ outcome: "action_failed", action: "start", detail: "exit 1" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  await press(modal()!, "Start daemon");
  expect(modal()).toBeUndefined();
  expect(alert().title).toBe("Could not start the daemon");
});

it("closes the start question and the version mismatch on Cancel without acting", async () => {
  connect.mockResolvedValueOnce({ outcome: "no_daemon" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  await press(modal()!, "Cancel");
  expect(modal()).toBeUndefined();
  expect(connect).toHaveBeenCalledTimes(1);

  connect.mockResolvedValueOnce(OLDER);
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  await press(modal()!, "Cancel");
  expect(modal()).toBeUndefined();
  expect(connect).toHaveBeenCalledTimes(2);
});

it("raises an alert when the connect command itself fails, and lets the entry be tried again", async () => {
  connect.mockRejectedValueOnce(new Error("no such command"));
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "pw");
  await press(modal()!, "Connect");
  expect(alert().title).toBe("Could not connect to the remote daemon");
  expect(rowButton(container, "Connect to").disabled).toBe(false);
});

it("shows an entry that is reconnecting as such, with Open in place of Connect", async () => {
  status.mockResolvedValue([{ id: "staging", state: "reconnecting", via: "window" }]);
  const container = await mount();
  expect(container.textContent).toContain("Reconnecting");
  expect(rowButton(container, "Open")).toBeDefined();
  expect(rowButton(container, "Connect to")).toBeUndefined();
});

// The shell focuses the window an entry already has when it is asked to connect (the rail relies on it, sending no
// secret), so there is nothing a password could be for; the page's Open button says it only brings the window forward.
it("brings a connected entry's window forward without asking for the password again", async () => {
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
  const container = await mount();
  expect(container.textContent).toContain("Connected");
  await act(async () => rowButton(container, "Open").click());
  expect(modal()).toBeUndefined();
  expect(sentRequest()).toEqual({ id: "staging", secret: null, confirmFingerprint: null, action: null, browser: false });
});

it("leaves the entry in place when removing it is cancelled, and raises an alert when removing it fails", async () => {
  const container = await mount();
  await act(async () => rowButton(container, "Remove").click());
  await press(modal()!, "Cancel");
  expect(remove).not.toHaveBeenCalled();
  expect(modal()).toBeUndefined();

  remove.mockRejectedValueOnce(new Error("store locked"));
  await act(async () => rowButton(container, "Remove").click());
  await press(modal()!, "Remove");
  expect(alert().title).toBe("Could not remove the remote daemon");
});

it("starts connecting to the entry it was opened for only once, however often the list is read again", async () => {
  list.mockResolvedValue([KEYED]);
  await mount("keyed");
  await act(async () => changed!());
  await act(async () => changed!());
  expect(connect).toHaveBeenCalledTimes(1);
});

it("connects to nothing at once for an entry it was opened for that is no longer saved", async () => {
  await mount("gone");
  expect(connect).not.toHaveBeenCalled();
  expect(modal()).toBeUndefined();
});

it("lists nothing and offers nothing where the shell has no remote daemons", async () => {
  let container!: HTMLElement;
  await act(async () => {
    container = mountHost(<LanguageProvider><ConnectPage /></LanguageProvider>, { platform: { kind: "browser", host: MAC_HOST } });
  });
  expect(container.querySelector("main")).toBeNull();
  expect(buttonNamed(container, "Add remote daemon")).toBeUndefined();
});
