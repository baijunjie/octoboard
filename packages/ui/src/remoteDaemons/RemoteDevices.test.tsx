// @vitest-environment jsdom
import React, { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { UnsavedChangesProvider, useUnsavedChangesGuard } from "../components/dialog/UnsavedChanges";
import { closeOpenTooltip, enter, openTooltip } from "../components/hint/tooltipTestSupport";
import { ActionableError } from "../errors/actionableError";
import { setLanguage } from "../i18n/language";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter, RemoteDaemonConnectOutcome, RemoteDaemonHandedOutcome, RemoteDaemonEntry, RemoteDaemonEntryInput, RemoteDaemonSaved, RemoteDaemonStatus, RemoteDeviceKeyRemoval } from "../platform";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { fakeTimers, runOutTimers } from "../test/clock";
import { MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { RemoteDevicePage } from "./RemoteDevicePage";
import { RemoteDevicesProvider } from "./RemoteDevices";
import { RemoteDevicesSidebar } from "./RemoteDevicesSidebar";
import { useRemoteDaemons } from "./useRemoteDaemons";

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
  deviceKey: { declined: false, file: null, mark: null },
};
const KEYED: RemoteDaemonEntry = { ...STAGING, id: "keyed", name: "Keyed", auth: "key", keyPath: "~/.ssh/id_ed25519" };
const DEVICE: RemoteDaemonEntry = { ...STAGING, id: "device", name: "Device", auth: "device" };

const list = vi.fn(async (): Promise<RemoteDaemonEntry[]> => [STAGING]);
const status = vi.fn(async (): Promise<RemoteDaemonStatus[]> => []);
const NOT_INSTALLED: RemoteDeviceKeyRemoval = { line: { result: "not_installed" }, privateKeyLeft: null };
const REMOVED: RemoteDeviceKeyRemoval = { line: { result: "removed" }, privateKeyLeft: null };
const LEFT_LINE = { result: "left_on_host", mark: "Octoboard on Desk", file: "~/.ssh/authorized_keys", host: "staging.example.com", user: "deploy", detail: "Unreachable(\"timed out\")" } as const;
const LEFT: RemoteDeviceKeyRemoval = { line: LEFT_LINE, privateKeyLeft: null };
const KEY_KEPT = { service: "app.octoboard.desktop.remote-daemon-key", account: "device", host: "staging.example.com", user: "deploy", detail: "the keychain is locked" };
const save = vi.fn(async (entry: RemoteDaemonEntryInput): Promise<RemoteDaemonSaved> => ({ entry: { ...entry, id: entry.id ?? "new", deviceKey: { declined: false, file: null, mark: null } }, keyRemoval: NOT_INSTALLED }));
const remove = vi.fn(async (_id: string): Promise<RemoteDeviceKeyRemoval> => NOT_INSTALLED);
const setDeviceKey = vi.fn(async (_id: string, _enabled: boolean): Promise<RemoteDeviceKeyRemoval> => NOT_INSTALLED);
const connect = vi.fn(async (_request: unknown): Promise<RemoteDaemonConnectOutcome> => ({ outcome: "opened" }));
const disconnect = vi.fn(async (_id: string) => {});
const showUpdates = vi.fn(async () => {});
beforeEach(() => {
  setLanguage("en");
  acknowledgeAllAlerts();
  list.mockReset().mockResolvedValue([STAGING]);
  status.mockReset().mockResolvedValue([]);
  save.mockClear();
  remove.mockReset().mockResolvedValue(NOT_INSTALLED);
  setDeviceKey.mockReset().mockResolvedValue(NOT_INSTALLED);
  disconnect.mockClear();
  showUpdates.mockClear();
  connect.mockReset().mockResolvedValue({ outcome: "opened" });
  handOver = undefined;
  onShow.mockClear();
  onAttention.mockClear();
  onSelect.mockClear();
});

const onShow = vi.fn();
const onAttention = vi.fn();
const onSelect = vi.fn();
let askForPage: (() => void) | undefined;
let handOver: ((items: RemoteDaemonHandedOutcome[]) => void) | undefined;

const DESKTOP: PlatformAdapter = {
  kind: "tauri",
  host: MAC_HOST,
  remoteDaemons: {
    list: () => list(),
    status: () => status(),
    onChanged: (listener) => {
      void listener;
      return () => {};
    },
    save: (entry) => save(entry),
    remove: (id) => remove(id),
    setDeviceKey: (id, enabled) => setDeviceKey(id, enabled),
    connect: (request) => connect(request),
    disconnect: (id) => disconnect(id),
    onPageRequested: (listener) => {
      askForPage = listener;
      return () => {};
    },
    onHandedOver: (listener) => {
      handOver = listener;
      return () => {};
    },
    showMainWindow: async () => {},
    showUpdates: () => showUpdates(),
  },
};

/** The window's part of the page: the list it reads, the device it selects, whether a new device's form is open, whether
 * the page shows (a page that is not shown ends the form, as the window's does), and the guard of changes not saved. */
function Harness({ capability, shown = true }: { capability: NonNullable<PlatformAdapter["remoteDaemons"]>; shown?: boolean }) {
  const list = useRemoteDaemons(capability);
  const unsaved = useUnsavedChangesGuard();
  const [chosen, setChosen] = React.useState<string>();
  const [adding, setAdding] = React.useState(false);
  React.useEffect(() => {
    if (!shown) setAdding(false);
  }, [shown]);
  const selectedId = (list.entries.find((entry) => entry.id === chosen) ?? list.entries[0])?.id;
  return (
    <UnsavedChangesProvider guard={unsaved.guard}>
      <RemoteDevicesProvider capability={capability} list={list} selectedId={selectedId} adding={adding} onShow={onShow} onAttention={onAttention} onSelect={(id) => { onSelect(id); setChosen(id); setAdding(false); }} onAdd={() => setAdding(true)} onCancelAdd={() => setAdding(false)}>
        <RemoteDevicesSidebar />
        {shown && <RemoteDevicePage />}
      </RemoteDevicesProvider>
      {unsaved.confirmation}
    </UnsavedChangesProvider>
  );
}

async function mount(platform: PlatformAdapter = DESKTOP) {
  let container!: HTMLElement;
  await act(async () => {
    container = mountHost(<LanguageProvider><Harness capability={platform.remoteDaemons!} /></LanguageProvider>, { platform });
  });
  return container;
}

/** A button's accessible name without the isolates around the names substituted into it. */
const nameOf = (button: Element) => (button.getAttribute("aria-label") ?? "").replace(/[\u2068\u2069]/g, "");
const rowButton = (container: ParentNode, verb: "Connect to" | "Open" | "Remove", name = "Staging") =>
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
  expect(modal()!.textContent).toContain("The server no longer accepts this computer’s key. Sign in with the password instead.");
  await type(firstInput(modal()!), "hunter2");
  await press(modal()!, "Connect");
  expect(connect).toHaveBeenCalledTimes(2);
  expect(sentRequest()).toEqual({ id: "device", secret: "hunter2", confirmFingerprint: null, action: null, browser: false });
});

/** Signs in to Staging with the password, which the shell answers by asking about this computer's key. */
async function offeredKey(container: HTMLElement) {
  connect.mockResolvedValueOnce({ outcome: "device_key_offered", mark: "Octoboard on Desk", file: "~/.ssh/authorized_keys" });
  await pressTile(container);
  await type(firstInput(modal()!), "hunter2");
  await press(modal()!, "Connect");
}

it("asks before this computer's key is installed, naming the line, the file and the server, and installs it on a yes", async () => {
  const container = await mount();
  await offeredKey(container);
  // Asked without the answer: the shell wrote nothing.
  expect(sentRequest()).toEqual({ id: "staging", secret: "hunter2", confirmFingerprint: null, action: null, browser: false });
  const question = modal()!.textContent!.replace(/[\u2068\u2069]/g, "");
  expect(question).toContain("Add this computer’s key");
  expect(question).toContain("A key of this computer’s own can be added to Staging");
  expect(question).toContain("Adding it writes one line to ~/.ssh/authorized_keys for deploy@staging.example.com: this computer’s public key, marked Octoboard on Desk.");
  expect(question).toContain("Removing this remote daemon, or turning its key off, removes the line and the private key.");
  expect(question).toContain("you are not asked again");
  // Its buttons are the two answers; the way out is the close button.
  expect(buttonNamed(modal()!, "Cancel")).toBeUndefined();
  // It came up by itself after the connection's seconds, so an Enter meant for something else answers nothing.
  expect(document.activeElement).not.toBe(buttonNamed(modal()!, "Add key"));
  await press(modal()!, "Add key");
  expect(connect).toHaveBeenCalledTimes(2);
  expect(sentRequest()).toEqual({ id: "staging", secret: "hunter2", confirmFingerprint: null, action: null, browser: false, deviceKey: true });
  expect(modal()).toBeUndefined();
});

it("installs nothing on a no, connecting again with the password and the answer for the shell to remember", async () => {
  const container = await mount();
  await offeredKey(container);
  await press(modal()!, "Do not add");
  expect(connect).toHaveBeenCalledTimes(2);
  expect(sentRequest()).toEqual({ id: "staging", secret: "hunter2", confirmFingerprint: null, action: null, browser: false, deviceKey: false });
  expect(modal()).toBeUndefined();
});

it("ends the connection when the question about this computer's key is closed, sending no answer", async () => {
  const container = await mount();
  await offeredKey(container);
  await act(async () => modal()!.querySelector<HTMLButtonElement>('button[aria-label="Close"]')!.click());
  expect(modal()).toBeUndefined();
  expect(connect).toHaveBeenCalledTimes(1);
});

it("asks about this computer's key for the browser's connection too, and holds the answered one for the browser", async () => {
  connect.mockResolvedValueOnce({ outcome: "generation_mismatch", remoteGeneration: 1, localGeneration: 2, remoteVersion: "0.9.0", localVersion: "1.0.0" });
  connect.mockResolvedValueOnce({ outcome: "device_key_offered", mark: "Octoboard on Desk", file: "~/.ssh/authorized_keys" });
  const container = await mount();
  await pressTile(container);
  await type(firstInput(modal()!), "hunter2");
  await press(modal()!, "Connect");
  await press(modal()!, "Open in browser");
  await press(modal()!, "Do not add");
  expect(sentRequest()).toEqual({ id: "staging", secret: "hunter2", confirmFingerprint: null, action: null, browser: true, deviceKey: false });
});

it("takes up the question about this computer's key a connection made from a menu was left with", async () => {
  list.mockResolvedValue([KEYED]);
  await mount();
  await act(async () => handOver!([{ id: "keyed", outcome: { outcome: "device_key_offered", mark: "Octoboard on Desk", file: "~/.ssh/authorized_keys" } }]));
  expect(modal()!.textContent).toContain("Add this computer’s key");
  await press(modal()!, "Add key");
  expect(sentRequest()).toEqual({ id: "keyed", secret: null, confirmFingerprint: null, action: null, browser: false, deviceKey: true });
});

const keySwitch = (container: ParentNode) => container.querySelector<HTMLInputElement>('main input[role="switch"]')!;
/** The accessible description of the key's switch: the text of what its `aria-describedby` names. */
const keyDescription = (container: ParentNode) => document.getElementById(keySwitch(container).getAttribute("aria-describedby") ?? "")?.textContent?.replace(/[\u2068\u2069]/g, "");
const INSTALLED: RemoteDaemonEntry = { ...DEVICE, deviceKey: { declined: false, file: "~/.ssh/authorized_keys", mark: "Octoboard on Desk" } };

it("says where this computer's key is installed, with its switch on, and how a daemon without it signs in", async () => {
  list.mockResolvedValue([INSTALLED, STAGING]);
  const container = await mount();
  const page = () => container.querySelector("main")!.textContent!.replace(/[\u2068\u2069]/g, "");
  expect(keySwitch(container).checked).toBe(true);
  const INSTALLED_LINE = "Added for deploy@staging.example.com. Connections from this computer sign in with it without asking for a password or passphrase.";
  expect(page()).toContain(INSTALLED_LINE);
  // The switch is on in two states: its line is its accessible description, which tells them apart.
  expect(keyDescription(container)).toBe(INSTALLED_LINE);
  expect(page()).toContain("Connections sign in with this computer’s key.");
  expect(container.querySelector("main input[type=radio]")).toBeNull();
  // One without the key says how it signs in, and that the next connection asks.
  await act(async () => rowNamed(container, "Staging").click());
  expect(keySwitch(container).checked).toBe(true);
  expect(page()).toContain("Not added yet. The next connection asks before adding it to the server.");
  expect(keyDescription(container)).toBe("Not added yet. The next connection asks before adding it to the server.");
  expect([...container.querySelectorAll<HTMLInputElement>("main input[type=radio]")].find((radio) => radio.checked)!.labels![0].textContent).toBe("Password");
});

it("counts a key an earlier version installed as installed, though nothing says where its line is", async () => {
  list.mockResolvedValue([DEVICE]);
  const container = await mount();
  expect(keySwitch(container).checked).toBe(true);
  expect(container.querySelector("main")!.textContent).toContain("Added for");
});

it("turns an installed key off only after asking, removing it from the server and keeping the remote daemon", async () => {
  list.mockResolvedValue([INSTALLED]);
  setDeviceKey.mockResolvedValue(REMOVED);
  const container = await mount();
  await act(async () => keySwitch(container).click());
  expect(setDeviceKey).not.toHaveBeenCalled();
  const question = modal()!.textContent!;
  expect(question).toContain("Turn off this computer’s key");
  expect(question).toContain("This computer’s key will be removed from");
  expect(question).toContain("Its line is taken out of the server’s authorized keys, and the private key is removed from this computer.");
  list.mockResolvedValue([{ ...STAGING, id: "device", name: "Device", deviceKey: { declined: true, file: null, mark: null } }]);
  await press(modal()!, "Turn off and remove");
  expect(setDeviceKey).toHaveBeenCalledExactlyOnceWith("device", false);
  expect(remove).not.toHaveBeenCalled();
  expect(modal()).toBeUndefined();
  expect(errorAlerts.getSnapshot()).toEqual([]);
  // The remote daemon stays, its switch off, saying what turning it on does.
  expect(heading(container)).toBe("Device");
  expect(keySwitch(container).checked).toBe(false);
  const OFF_LINE = "Once this is on, the next connection asks before adding this computer’s key to the server. Once it is added, later connections sign in with it.";
  expect(container.querySelector("main")!.textContent).toContain(OFF_LINE);
  expect(keyDescription(container)).toBe(OFF_LINE);
});

it("leaves an installed key as it is when turning it off is cancelled", async () => {
  list.mockResolvedValue([INSTALLED]);
  const container = await mount();
  await act(async () => keySwitch(container).click());
  await press(modal()!, "Cancel");
  expect(setDeviceKey).not.toHaveBeenCalled();
  expect(keySwitch(container).checked).toBe(true);
});

it("turns a declined key on again at once, which installs nothing until the next connection asks", async () => {
  const declined = { ...STAGING, deviceKey: { declined: true, file: null, mark: null } };
  list.mockResolvedValue([declined]);
  const container = await mount();
  expect(keySwitch(container).checked).toBe(false);
  list.mockResolvedValue([STAGING]);
  await act(async () => keySwitch(container).click());
  expect(setDeviceKey).toHaveBeenCalledExactlyOnceWith("staging", true);
  expect(modal()).toBeUndefined();
  expect(keySwitch(container).checked).toBe(true);
  // Off again, with nothing installed, asks nothing either.
  await act(async () => keySwitch(container).click());
  expect(setDeviceKey).toHaveBeenLastCalledWith("staging", false);
  expect(modal()).toBeUndefined();
});

it("raises an alert when the switch of this computer's key fails", async () => {
  const declined = { ...STAGING, deviceKey: { declined: true, file: null, mark: null } };
  list.mockResolvedValue([declined]);
  setDeviceKey.mockRejectedValue(new Error("cannot write"));
  const container = await mount();
  await act(async () => keySwitch(container).click());
  expect(alert().title).toBe("Could not change this computer’s key");
});

/** What the alert about a line left on a server says, without the isolates around what is substituted into it. */
const leftMessage = () => (alert().error as ActionableError).message.replace(/[\u2068\u2069]/g, "");

it("tells which line stays on a server that could not be reached when a key is turned off, the switch done all the same", async () => {
  list.mockResolvedValue([INSTALLED]);
  setDeviceKey.mockResolvedValue(LEFT);
  const container = await mount();
  await act(async () => keySwitch(container).click());
  await press(modal()!, "Turn off and remove");
  expect(alert().title).toBe("Could not remove this computer’s key from the server");
  expect(leftMessage()).toBe(
    "This computer’s key could not be removed from staging.example.com, so the line marked Octoboard on Desk may still be in ~/.ssh/authorized_keys for deploy there. If it is, delete it on the server to remove the key. The private key was removed from this computer.",
  );
  expect((alert().error as ActionableError).detail).toBe("Unreachable(\"timed out\")");
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
});

it("tells of a private key the credential store did not remove, naming its item, and never says it was removed", async () => {
  list.mockResolvedValue([INSTALLED]);
  // The line went from the server; the keychain kept the private key.
  setDeviceKey.mockResolvedValue({ line: { result: "removed" }, privateKeyLeft: KEY_KEPT });
  const container = await mount();
  await act(async () => keySwitch(container).click());
  await press(modal()!, "Turn off and remove");
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(alert().title).toBe("Could not remove this computer’s private key");
  expect(leftMessage()).toBe(
    "The system’s credential store did not remove the private key kept for deploy on staging.example.com. Remove it there yourself: its item is app.octoboard.desktop.remote-daemon-key, with the account device.",
  );
  expect((alert().error as ActionableError).detail).toBe("the keychain is locked");
});

it("tells of a server's file left incomplete, naming the copy the server kept, as an alert of its own", async () => {
  list.mockResolvedValue([INSTALLED]);
  setDeviceKey.mockResolvedValue({ line: { result: "file_incomplete", host: "staging.example.com", user: "deploy", copy: "/tmp/tmp.Ab12/before", detail: ".ssh/authorized_keys may be incomplete" }, privateKeyLeft: null });
  const container = await mount();
  await act(async () => keySwitch(container).click());
  await press(modal()!, "Turn off and remove");
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(alert().title).toBe("The server’s authorized keys may be incomplete");
  expect(leftMessage()).toBe(
    "Removing this computer’s key from staging.example.com was cut short while the authorized keys of deploy were being rewritten, so keys may be missing from them. Their earlier content is kept on the server in /tmp/tmp.Ab12/before. Restore the file from that copy: until then, the keys it held may not sign in.",
  );
  expect((alert().error as ActionableError).detail).toBe(".ssh/authorized_keys may be incomplete");
});

it("tells of both where the line stays on the server and the private key stays on this computer", async () => {
  list.mockResolvedValue([INSTALLED]);
  remove.mockResolvedValue({ line: LEFT_LINE, privateKeyLeft: KEY_KEPT });
  const container = await mount();
  await act(async () => rowButton(container, "Remove", "Device").click());
  list.mockResolvedValue([]);
  await press(modal()!, "Remove");
  const [line, key] = errorAlerts.getSnapshot();
  expect(line.title).toBe("Could not remove this computer’s key from the server");
  // The private key was not removed, so the alert about the line does not say it was.
  expect((line.error as ActionableError).message).not.toContain("The private key was removed from this computer.");
  expect(key.title).toBe("Could not remove this computer’s private key");
});

it("says removing a remote daemon with this computer's key removes the key too, and tells of a line left behind", async () => {
  list.mockResolvedValue([INSTALLED]);
  // A key an earlier version installed: the shell does not know its file.
  remove.mockResolvedValue({ line: { ...LEFT_LINE, file: null }, privateKeyLeft: null });
  const container = await mount();
  await act(async () => rowButton(container, "Remove", "Device").click());
  expect(modal()!.textContent).toContain("This computer’s key is removed from the server, and its private key from this computer.");
  list.mockResolvedValue([]);
  await press(modal()!, "Remove");
  expect(remove).toHaveBeenCalledExactlyOnceWith("device");
  expect(alert().title).toBe("Could not remove this computer’s key from the server");
  expect(leftMessage()).toContain("the line marked Octoboard on Desk may still be in the authorized keys of deploy there. If it is, delete it on the server");
  // The remote daemon is removed all the same.
  expect(heading(container)).toBe("No remote daemons yet");
});

it("says nothing about a key when removing a remote daemon that has none", async () => {
  const container = await mount();
  await act(async () => rowButton(container, "Remove").click());
  expect(modal()!.textContent).toContain("The server and its daemon stay the same.");
  expect(modal()!.textContent).not.toContain("private key");
  await press(modal()!, "Remove");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("tells of the line left on the server a saved daemon was pointed away from, the change saved all the same", async () => {
  list.mockResolvedValue([INSTALLED]);
  save.mockResolvedValueOnce({ entry: { ...STAGING, id: "device", name: "Device", user: "ops" }, keyRemoval: LEFT });
  const container = await mount();
  await type(field("User"), "ops");
  await press(container, "Save");
  expect(save).toHaveBeenCalledOnce();
  expect(alert().title).toBe("Could not remove this computer’s key from the server");
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

it("shows a question the shell handed over, and sends what the owner gives with the retry", async () => {
  await mount();
  await act(async () => handOver!([{ id: "staging", outcome: { outcome: "secret_needed", kind: "password" } }]));
  expect(modal()!.textContent).toContain("It is not stored");
  await type(firstInput(modal()!), "hunter2");
  await press(modal()!, "Connect");
  expect(sentRequest()).toEqual({ id: "staging", secret: "hunter2", confirmFingerprint: null, action: null, browser: false });
});

it("raises the alert of a failure the shell handed over", async () => {
  await mount();
  await act(async () => handOver!([{ id: "staging", outcome: { outcome: "auth_failed", detail: "denied" } }]));
  expect(alert().title).toBe("Could not sign in");
});

it("takes what was handed over only once the list has been read, and one question at a time", async () => {
  let answer!: (entries: RemoteDaemonEntry[]) => void;
  list.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
  list.mockResolvedValue([STAGING, KEYED]);
  await mount();
  await act(async () => handOver!([
    { id: "staging", outcome: { outcome: "secret_needed", kind: "password" } },
    { id: "keyed", outcome: { outcome: "no_daemon" } },
  ]));
  expect(modal()).toBeUndefined();
  await act(async () => answer([STAGING, KEYED]));
  expect(modal()!.textContent).toContain("It is not stored");
  await act(async () => buttonNamed(modal()!, "Cancel")!.click());
  expect(modal()!.textContent).toContain("Start");
});

it("drops what was handed over for an entry that is no longer saved", async () => {
  await mount();
  await act(async () => handOver!([{ id: "gone", outcome: { outcome: "no_daemon" } }]));
  expect(modal()).toBeUndefined();
  expect(connect).not.toHaveBeenCalled();
});

it("holds what was handed over while a connection of this page is under way, and shows it once that ends", async () => {
  list.mockResolvedValue([KEYED, { ...KEYED, id: "other", name: "Other" }]);
  let finish!: (outcome: RemoteDaemonConnectOutcome) => void;
  connect.mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)));
  const container = await mount();
  await pressTile(container, "Keyed");
  await act(async () => handOver!([{ id: "other", outcome: { outcome: "no_daemon" } }]));
  expect(modal()).toBeUndefined();
  await act(async () => finish({ outcome: "opened" }));
  expect(modal()!.textContent).toContain("No daemon is running on");
});

it("shows each hand-over that arrives after another was dealt with", async () => {
  await mount();
  await act(async () => handOver!([{ id: "staging", outcome: { outcome: "no_daemon" } }]));
  await press(modal()!, "Cancel");
  expect(modal()).toBeUndefined();
  await act(async () => handOver!([{ id: "staging", outcome: { outcome: "auth_failed", detail: "denied" } }]));
  expect(alert().title).toBe("Could not sign in");
});

it("answers a handed-over host key question by retrying that entry with it confirmed", async () => {
  await mount();
  await act(async () => handOver!([{ id: "staging", outcome: { outcome: "host_key_unknown", fingerprint: "SHA256:abc123" } }]));
  expect(modal()!.textContent).toContain("SHA256:abc123");
  await press(modal()!, "Connect");
  expect(sentRequest()).toEqual({ id: "staging", secret: null, confirmFingerprint: "SHA256:abc123", action: null, browser: false });
});

it("holds a question handed over while the Remove confirmation is open", async () => {
  const container = await mount();
  await act(async () => rowButton(container, "Remove").click());
  await act(async () => handOver!([{ id: "staging", outcome: { outcome: "no_daemon" } }]));
  expect(modal()!.textContent).toContain("will be removed from the remote daemons");
  await press(modal()!, "Cancel");
  expect(modal()!.textContent).toContain("No daemon is running on");
});

it("draws neither the list nor the empty state until the list has been read", async () => {
  let answer!: (entries: RemoteDaemonEntry[]) => void;
  list.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
  const container = await mount();
  expect(container.textContent).not.toContain("No remote daemons yet");
  expect(container.querySelector("h1")).toBeNull();
  await act(async () => answer([STAGING]));
  expect(container.textContent).toContain("Staging");
});


it("asks for the page to be shown when the shell asks for it, and draws attention to it when it hands something over", async () => {
  await mount();
  await act(async () => askForPage!());
  expect(onShow).toHaveBeenCalledTimes(1);
  await act(async () => handOver!([{ id: "staging", outcome: { outcome: "no_daemon" } }]));
  // Once for the hand-over, and once for the question it raises.
  expect(onAttention.mock.calls.length).toBeGreaterThan(1);
  expect(onShow).toHaveBeenCalledTimes(1);
});


it("draws attention to the page when a connection started on the page ends in a question or a failure, and not when it opens", async () => {
  list.mockResolvedValue([KEYED]);
  const container = await mount();
  await pressTile(container, "Keyed");
  expect(onAttention).not.toHaveBeenCalled();
  connect.mockResolvedValueOnce({ outcome: "no_daemon" });
  await pressTile(container, "Keyed");
  expect(onAttention).toHaveBeenCalledTimes(1);
  await press(modal()!, "Cancel");
  onAttention.mockClear();
  connect.mockResolvedValueOnce({ outcome: "auth_failed", detail: "denied" });
  await pressTile(container, "Keyed");
  expect(onAttention).toHaveBeenCalledTimes(1);
  onAttention.mockClear();
  connect.mockResolvedValueOnce({ outcome: "opened_in_browser" });
  await pressTile(container, "Keyed");
  expect(onAttention).not.toHaveBeenCalled();
});


// The Remote devices page as the window lays it out: the sidebar's list, and the content area's page of the selected
// device, which share the provider's state.
const byLabel = (root: ParentNode, label: string) => root.querySelector<HTMLElement>(`[aria-label="${label}"]`)!;
const deviceRows = (container: ParentNode) => [...container.querySelectorAll<HTMLElement>("[data-list-motion]")].map((wrapper) => wrapper.querySelector<HTMLElement>("[role=button]")!);
const rowNamed = (container: ParentNode, name: string) => deviceRows(container).find((row) => row.textContent?.startsWith(name))!;
const heading = (container: ParentNode) => container.querySelector("h1")?.textContent;
const field = (label: string) => [...document.body.querySelectorAll<HTMLInputElement>("input")].find((input) => input.labels?.[0]?.textContent === label)!;
const addFromSidebar = (container: ParentNode) => act(async () => byLabel(container, "Add remote daemon").click());
const actionsOf = (root: ParentNode, name: string) => [...root.querySelectorAll<HTMLElement>("button[aria-label]")].find((one) => one.getAttribute("aria-label")!.replace(/[\u2068\u2069]/g, "") === `Actions for remote daemon ${name}`)!;
const menuItem = (name: string) => [...document.body.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent?.trim() === name);
const isDisabled = (element: HTMLElement) => (element as HTMLButtonElement).disabled || element.getAttribute("aria-disabled") === "true";

it("heads the sidebar with Remote devices and Add remote daemon", async () => {
  const container = await mount();
  // The device's own page has its name for a heading, so this is the sidebar's.
  expect(container.querySelector("h2")?.textContent).toBe("Remote devices");
  expect(byLabel(container, "Add remote daemon")).not.toBeNull();
});

it("lists the saved devices in the order saved, each with its state where it is held, and the first one is shown", async () => {
  const lab = { ...STAGING, id: "lab", name: "Home lab" };
  list.mockResolvedValue([KEYED, STAGING, lab]);
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }, { id: "lab", state: "reconnecting", via: "window" }]);
  const container = await mount();
  const rows = deviceRows(container);
  expect(rows.map((row) => row.textContent)).toEqual(["Keyed", "Staging", "Home lab"]);
  expect(rows[0].getAttribute("aria-label")).toBe("Keyed");
  expect(rows[0].querySelector("[role=presentation]")).toBeNull();
  expect(heading(container)).toBe("Keyed");
});

it.each([
  ["connected", "window", "Connected", "lucide-link"],
  ["connected", "browser", "Opened in browser", "lucide-app-window"],
  ["reconnecting", "window", "Reconnecting", undefined],
] as const)("shows the %s connection via %s as a glyph with a tooltip and names it on the row", async (state, via, label, glyph) => {
  list.mockResolvedValue([KEYED, STAGING]);
  status.mockResolvedValue([{ id: "staging", state, via }]);
  const container = await mount();
  const row = rowNamed(container, "Staging");
  expect(row.textContent).toBe("Staging");
  expect(row.getAttribute("aria-label")).toContain("Staging");
  expect(row.getAttribute("aria-label")).toContain(label);

  const trigger = row.querySelector("[role=presentation]")!;
  expect((trigger as HTMLElement).tabIndex).toBe(-1);
  if (glyph !== undefined) {
    const icon = trigger.querySelector(`svg.${glyph}`)!;
    expect(icon.closest('[aria-hidden="true"]')).not.toBeNull();
    expect(icon.classList.contains("text-[var(--tint-success-fg)]")).toBe(true);
    expect(icon.classList.contains("size-4")).toBe(true);
    expect(trigger.querySelector("[data-slot=spinner]")).toBeNull();
  }

  fakeTimers({ beforeDrain: closeOpenTooltip });
  enter(trigger);
  runOutTimers();
  expect(openTooltip()?.textContent).toBe(label);
});

it("draws a reconnecting device's row with the grey spinner, nothing amber", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  status.mockResolvedValue([{ id: "staging", state: "reconnecting", via: "window" }]);
  const container = await mount();
  const trigger = rowNamed(container, "Staging").querySelector("[role=presentation]")!;
  const spinner = trigger.querySelector("[data-slot=spinner]")!;
  expect(spinner.closest('[aria-hidden="true"]')).not.toBeNull();
  expect(spinner.classList.contains("text-app-muted")).toBe(true);
  expect(trigger.innerHTML).not.toContain("tint-warning-fg");
});

it("marks the selected device's row, and a press on another row shows that device in its place", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  const container = await mount();
  const current = () => deviceRows(container).filter((row) => row.getAttribute("aria-current") === "true").map((row) => row.textContent);
  expect(current()).toEqual(["Keyed"]);
  await act(async () => rowNamed(container, "Staging").click());
  expect(current()).toEqual(["Staging"]);
  expect(heading(container)).toBe("Staging");
  expect(field("Host").value).toBe("staging.example.com");
});

it("names where the shown device connects to and how it is connected in its header", async () => {
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
  const container = await mount();
  const header = container.querySelector("header")!;
  expect(header.querySelector("h1")!.textContent).toBe("Staging");
  expect(header.textContent).toContain("Connected");
  expect(header.querySelector("[data-slot=spinner]")).toBeNull();
  expect(header.textContent).toContain("deploy@staging.example.com:22");
  // A device that is not held has no state to name.
  list.mockResolvedValue([KEYED]);
  status.mockResolvedValue([]);
  const idle = await mount();
  expect(idle.querySelector("header")!.textContent).not.toContain("Connected");
});

it("keeps the reconnecting label in a warning badge beside a spinner that takes the badge's own text colour", async () => {
  status.mockResolvedValue([{ id: "staging", state: "reconnecting", via: "window" }]);
  const container = await mount();
  const badge = container.querySelector('header [data-color="warning"][data-variant="soft"]')!;
  expect(badge.textContent).toBe("Reconnecting");
  const spinner = badge.querySelector("[data-slot=spinner]")!;
  expect(spinner.getAttribute("aria-hidden")).toBe("true");
  expect(spinner.classList.contains("size-3.5")).toBe(true);
  expect(spinner.classList.contains("text-app-muted")).toBe(false);
});

it("connects one device at a time: the shown device's Connect is unavailable while another connects, and pending on the one that does", async () => {
  list.mockResolvedValue([KEYED, { ...KEYED, id: "other", name: "Other" }]);
  let finish!: (outcome: RemoteDaemonConnectOutcome) => void;
  connect.mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)));
  const container = await mount();
  await pressTile(container, "Keyed");
  await act(async () => rowNamed(container, "Other").click());
  const other = rowButton(container, "Connect to", "Other");
  expect(isDisabled(other)).toBe(true);
  await act(async () => other.click());
  expect(connect).toHaveBeenCalledTimes(1);
  await act(async () => finish({ outcome: "opened" }));
  expect(isDisabled(rowButton(container, "Connect to", "Other"))).toBe(false);
});

it("connects to a device from its row's menu, which offers Open where it is held and Remove", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
  const container = await mount();
  await act(async () => actionsOf(container, "Keyed").click());
  expect(menuItem("Connect")).toBeDefined();
  expect(menuItem("Remove")).toBeDefined();
  expect(menuItem("Open")).toBeUndefined();
  await act(async () => menuItem("Connect")!.click());
  expect(sentRequest()).toEqual({ id: "keyed", secret: null, confirmFingerprint: null, action: null, browser: false });

  await act(async () => actionsOf(container, "Staging").click());
  expect(menuItem("Open")).toBeDefined();
  expect(menuItem("Connect")).toBeUndefined();
});

it("removes a device from its row's menu after asking, though it is not the one shown", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  const container = await mount();
  await act(async () => actionsOf(container, "Staging").click());
  await act(async () => menuItem("Remove")!.click());
  expect(remove).not.toHaveBeenCalled();
  expect(modal()!.textContent).toContain("will be removed from the remote daemons");
  expect(modal()!.textContent).toContain("Staging");
  await press(modal()!, "Remove");
  expect(remove).toHaveBeenCalledWith("staging");
});

it("opens the same menu on a right-click of a row", async () => {
  const container = await mount();
  await act(async () => void rowNamed(container, "Staging").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 10, clientY: 10 })));
  expect(menuItem("Connect")).toBeDefined();
  expect(menuItem("Remove")).toBeDefined();
});

it("removes the shown device from the page after asking", async () => {
  const container = await mount();
  await act(async () => rowButton(container, "Remove").click());
  expect(remove).not.toHaveBeenCalled();
  await press(modal()!, "Remove");
  expect(remove).toHaveBeenCalledExactlyOnceWith("staging");
});

it("shows the next device once the shown one is removed", async () => {
  list.mockResolvedValue([STAGING, KEYED]);
  const container = await mount();
  await act(async () => rowButton(container, "Remove").click());
  list.mockResolvedValue([KEYED]);
  await press(modal()!, "Remove");
  expect(heading(container)).toBe("Keyed");
});

it("shows the empty state alone as the page, and a compact empty row in the sidebar, which starts Add", async () => {
  list.mockResolvedValue([]);
  const container = await mount();
  const headings = [...container.querySelectorAll("main :is(h1, h2, h3)")];
  expect(headings.map((one) => [one.tagName, one.textContent])).toEqual([["H1", "No remote daemons yet"]]);
  expect(container.textContent).toContain("A remote daemon is the daemon on another computer, reached over SSH.");
  expect(deviceRows(container)).toEqual([]);
  expect(container.querySelector("form")).toBeNull();
  const emptyRow = [...container.querySelectorAll<HTMLButtonElement>("button")].find((one) => one.textContent?.includes("No remote daemons yet") && !one.closest("main"))!;
  expect(emptyRow).toBeDefined();
  await act(async () => emptyRow.click());
  expect(heading(container)).toBe("Add remote daemon");
  expect(container.querySelector("form")).not.toBeNull();
  // The row is there while the form is open too.
  expect([...container.querySelectorAll("button")].some((one) => one.textContent?.includes("No remote daemons yet") && !one.closest("main"))).toBe(true);
});

it("starts Add from the page's empty state too", async () => {
  list.mockResolvedValue([]);
  const container = await mount();
  await act(async () => buttonNamed(container.querySelector("main")!, "Add remote daemon")!.click());
  expect(heading(container)).toBe("Add remote daemon");
});

it("shows a blank form titled Add remote daemon, with no device selected, and Cancel returns to the device shown before", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  const container = await mount();
  await act(async () => rowNamed(container, "Staging").click());
  await addFromSidebar(container);
  expect(heading(container)).toBe("Add remote daemon");
  expect(field("Host").value).toBe("");
  expect(field("SSH port").value).toBe("22");
  expect(deviceRows(container).some((row) => row.getAttribute("aria-current") === "true")).toBe(false);
  await press(container, "Cancel");
  expect(heading(container)).toBe("Staging");
  expect(save).not.toHaveBeenCalled();
});

it("returns to the empty state when Add is cancelled with nothing saved", async () => {
  list.mockResolvedValue([]);
  const container = await mount();
  await addFromSidebar(container);
  await press(container, "Cancel");
  expect(heading(container)).toBe("No remote daemons yet");
});

it("saves a new device and shows it selected", async () => {
  list.mockResolvedValue([STAGING]);
  const container = await mount();
  await addFromSidebar(container);
  await type(field("Host"), "box.example.com");
  await type(field("User"), "me");
  const created = { id: "new", name: "box.example.com", host: "box.example.com", port: 22, user: "me", auth: "password" as const, keyPath: null, fingerprint: null, octoboarddPath: null, deviceKey: { declined: false, file: null, mark: null } };
  list.mockResolvedValue([STAGING, created]);
  await press(container, "Save");
  expect(save).toHaveBeenCalledExactlyOnceWith({ id: null, name: "box.example.com", host: "box.example.com", port: 22, user: "me", auth: "password", keyPath: null, fingerprint: null, octoboarddPath: null });
  expect(container.querySelector("form")!.parentElement!.querySelector("h1")!.textContent).toBe("box.example.com");
  expect(deviceRows(container).map((row) => row.textContent)).toEqual(["Staging", "box.example.com"]);
  expect(deviceRows(container).filter((row) => row.getAttribute("aria-current") === "true").map((row) => row.textContent)).toEqual(["box.example.com"]);
});

it("saves a change to the shown device without changing what is shown", async () => {
  const container = await mount();
  await type(field("User"), "ops");
  await press(container, "Save");
  const { deviceKey: _deviceKey, ...sent } = STAGING;
  expect(save).toHaveBeenCalledExactlyOnceWith({ ...sent, user: "ops" });
  expect(heading(container)).toBe("Staging");
});

const asking = () => document.body.querySelector<HTMLElement>('[role="alertdialog"]');
const answerQuestion = (name: string) => act(async () => buttonNamed(asking()!, name)!.click());

it("asks before what was changed and not saved is dropped when another device is chosen or Add is started, and keeps it on Keep editing", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  const container = await mount();
  await type(field("User"), "ops");
  await act(async () => rowNamed(container, "Staging").click());
  expect(asking()!.textContent).toContain("The changes in this form have not been saved. They will be lost.");
  expect(heading(container)).toBe("Keyed");
  await answerQuestion("Keep editing");
  expect(asking()).toBeNull();
  expect(field("User").value).toBe("ops");
  expect(onSelect).not.toHaveBeenCalled();
  await addFromSidebar(container);
  expect(asking()).not.toBeNull();
  await answerQuestion("Keep editing");
  expect(heading(container)).toBe("Keyed");
  expect(field("User").value).toBe("ops");
});

it("shows the other device once Discard is answered, and the new device's blank form for Add", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  const container = await mount();
  await type(field("User"), "ops");
  await act(async () => rowNamed(container, "Staging").click());
  await answerQuestion("Discard");
  expect(asking()).toBeNull();
  expect(heading(container)).toBe("Staging");
  expect(field("User").value).toBe("deploy");
  expect(save).not.toHaveBeenCalled();

  await type(field("User"), "ops");
  await addFromSidebar(container);
  await answerQuestion("Discard");
  expect(heading(container)).toBe("Add remote daemon");
  expect(field("Host").value).toBe("");
});

it("asks nothing for Revert, Cancel, an untouched form, the device already shown or a new form's Add pressed again", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  const container = await mount();
  await act(async () => rowNamed(container, "Staging").click());
  expect(asking()).toBeNull();
  await act(async () => rowNamed(container, "Keyed").click());
  expect(asking()).toBeNull();
  await type(field("User"), "ops");
  await act(async () => rowNamed(container, "Keyed").click());
  expect(asking()).toBeNull();
  await press(container, "Revert");
  expect(field("User").value).toBe("deploy");

  await addFromSidebar(container);
  await type(field("Host"), "box");
  await addFromSidebar(container);
  expect(asking()).toBeNull();
  await press(container, "Cancel");
  expect(asking()).toBeNull();
  expect(heading(container)).toBe("Keyed");
});

it("does nothing for another device chosen while a save is in flight", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  save.mockImplementationOnce(() => new Promise(() => {}));
  const container = await mount();
  await type(field("User"), "ops");
  await press(container, "Save");
  await act(async () => rowNamed(container, "Staging").click());
  expect(asking()).toBeNull();
  expect(heading(container)).toBe("Keyed");
});

it("selects the device a hand-over is for, over the one shown and over a new form nothing was typed in, and shows its question", async () => {
  list.mockResolvedValue([STAGING, KEYED]);
  const container = await mount();
  expect(heading(container)).toBe("Staging");
  await addFromSidebar(container);
  await act(async () => handOver!([{ id: "keyed", outcome: { outcome: "no_daemon" } }]));
  expect(heading(container)).toBe("Keyed");
  expect(container.querySelector("form")!.querySelector("input[type=radio]")).not.toBeNull();
  expect(modal()!.textContent).toContain("No daemon is running on");
});

it("shows a hand-over's question over a form with changes not saved, without selecting its device or discarding the form", async () => {
  list.mockResolvedValue([STAGING, KEYED]);
  const container = await mount();
  await addFromSidebar(container);
  await type(field("Host"), "box");
  await act(async () => handOver!([{ id: "keyed", outcome: { outcome: "no_daemon" } }]));
  expect(onSelect).not.toHaveBeenCalled();
  expect(heading(container)).toBe("Add remote daemon");
  expect(modal()!.textContent).toContain("No daemon is running on");
  expect(modal()!.textContent).not.toContain("have not been saved");
  await act(async () => buttonNamed(modal()!, "Cancel")!.click());
  expect(field("Host").value).toBe("box");
});

it("selects nothing for a hand-over of a device that is no longer saved", async () => {
  list.mockResolvedValue([STAGING, KEYED]);
  const container = await mount();
  await act(async () => handOver!([{ id: "gone", outcome: { outcome: "no_daemon" } }]));
  expect(heading(container)).toBe("Staging");
  expect(modal()).toBeUndefined();
});

/** Answers the next read of the list when `answer` is called, in place of the list at once. */
function holdNextListRead() {
  let answer!: (entries: RemoteDaemonEntry[]) => void;
  list.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
  return (entries: RemoteDaemonEntry[]) => answer(entries);
}
const filledNew = async (container: ParentNode) => {
  await addFromSidebar(container);
  await type(field("Host"), "box.example.com");
  await type(field("User"), "me");
};
const CREATED: RemoteDaemonEntry = { id: "new", name: "box.example.com", host: "box.example.com", port: 22, user: "me", auth: "password", keyPath: null, fingerprint: null, octoboarddPath: null, deviceKey: { declined: false, file: null, mark: null } };

it("selects a device that was added only once the list holds it, and selects nothing in between", async () => {
  list.mockResolvedValue([STAGING, KEYED]);
  const container = await mount();
  await filledNew(container);
  const answer = holdNextListRead();
  await press(container, "Save");
  // Saved, and the list is being read again: the form is still what shows, and nothing was selected.
  expect(save).toHaveBeenCalledOnce();
  expect(onSelect).not.toHaveBeenCalled();
  expect(heading(container)).toBe("Add remote daemon");
  await act(async () => answer([STAGING, KEYED, CREATED]));
  expect(onSelect).toHaveBeenCalledExactlyOnceWith("new");
  expect(heading(container)).toBe("box.example.com");
});

it("keeps a new device's form as it is while its save is in flight, cancelling and choosing another device doing nothing, and selects the new device once the list holds it", async () => {
  list.mockResolvedValue([STAGING, KEYED]);
  const container = await mount();
  await filledNew(container);
  const answer = holdNextListRead();
  await press(container, "Save");
  expect(buttonNamed(container, "Cancel")!.disabled).toBe(true);
  await act(async () => rowNamed(container, "Keyed").click());
  expect(asking()).toBeNull();
  expect(onSelect).not.toHaveBeenCalled();
  expect(heading(container)).toBe("Add remote daemon");
  await act(async () => answer([STAGING, KEYED, CREATED]));
  expect(onSelect.mock.calls).toEqual([["new"]]);
  expect(heading(container)).toBe("box.example.com");
});

it("selects nothing for a device added when the page was left before the list answered", async () => {
  list.mockResolvedValue([STAGING]);
  const view = (shown: boolean) => <LanguageProvider><Harness capability={DESKTOP.remoteDaemons!} shown={shown} /></LanguageProvider>;
  let container!: ReturnType<typeof mountHost>;
  await act(async () => {
    container = mountHost(view(true), { platform: DESKTOP });
  });
  await filledNew(container);
  const answer = holdNextListRead();
  await press(container, "Save");
  await act(async () => container.render(view(false)));
  await act(async () => answer([STAGING, CREATED]));
  expect(onSelect).not.toHaveBeenCalled();
});

it("keeps a device that is changed, not added, where it is, and reads the list once more", async () => {
  const container = await mount();
  const reads = list.mock.calls.length;
  await type(field("User"), "ops");
  await press(container, "Save");
  expect(list.mock.calls.length).toBe(reads + 1);
  expect(onSelect).not.toHaveBeenCalled();
});

it("shows the empty state only when nothing is saved, whichever device is selected", async () => {
  list.mockResolvedValue([STAGING]);
  const container = await mount();
  expect(container.textContent).not.toContain("No remote daemons yet");
  expect(heading(container)).toBe("Staging");
});

/** An item of the menu opened last, which one still closing does not hide. */
const newestMenuItem = (name: string) => [...([...document.body.querySelectorAll("[role=menu]")].at(-1)?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? [])].find((item) => item.textContent?.trim() === name);

it("makes the row menu's Connect unavailable while another device is being connected, and Remove stays", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  connect.mockImplementationOnce(() => new Promise(() => {}));
  const container = await mount();
  await act(async () => actionsOf(container, "Staging").click());
  expect(isDisabled(newestMenuItem("Connect")!)).toBe(false);
  await act(async () => document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await pressTile(container, "Keyed");
  await act(async () => actionsOf(container, "Staging").click());
  expect(isDisabled(newestMenuItem("Connect")!)).toBe(true);
  expect(isDisabled(newestMenuItem("Remove")!)).toBe(false);
  // The one being connected is still its own to press.
  await act(async () => document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await act(async () => actionsOf(container, "Keyed").click());
  expect(isDisabled(newestMenuItem("Connect")!)).toBe(false);
});

it.each([
  ["a device", [STAGING]],
  ["no device saved", []],
])("casts the docked sidebar's edge shadow from the page's main area as the sessions' does, with %s", async (_, entries) => {
  list.mockResolvedValue(entries);
  const container = await mount();
  const main = container.querySelector("main[data-region=devices]")!;
  expect(main).not.toBeNull();
  for (const name of ["pane-docked:shadow-(--panel-shadow)", "relative"]) expect(main.classList.contains(name), name).toBe(true);
});
