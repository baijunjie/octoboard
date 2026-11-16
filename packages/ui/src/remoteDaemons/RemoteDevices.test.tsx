// @vitest-environment jsdom
import React, { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { ActionableError } from "../errors/actionableError";
import { setLanguage } from "../i18n/language";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter, RemoteDaemonConnectOutcome, RemoteDaemonHandedOutcome, RemoteDaemonEntry, RemoteDaemonEntryInput, RemoteDaemonStatus } from "../platform";
import { acknowledgeAllAlerts } from "../test/alertQueue";
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
};
const KEYED: RemoteDaemonEntry = { ...STAGING, id: "keyed", name: "Keyed", auth: "key", keyPath: "~/.ssh/id_ed25519" };
const DEVICE: RemoteDaemonEntry = { ...STAGING, id: "device", name: "Device", auth: "device" };

const list = vi.fn(async (): Promise<RemoteDaemonEntry[]> => [STAGING]);
const status = vi.fn(async (): Promise<RemoteDaemonStatus[]> => []);
const save = vi.fn(async (entry: RemoteDaemonEntryInput): Promise<RemoteDaemonEntry> => ({ ...entry, id: entry.id ?? "new" }));
const remove = vi.fn(async (_id: string) => {});
const connect = vi.fn(async (_request: unknown): Promise<RemoteDaemonConnectOutcome> => ({ outcome: "opened" }));
const disconnect = vi.fn(async (_id: string) => {});
const showUpdates = vi.fn(async () => {});
beforeEach(() => {
  setLanguage("en");
  acknowledgeAllAlerts();
  list.mockReset().mockResolvedValue([STAGING]);
  status.mockReset().mockResolvedValue([]);
  save.mockClear();
  remove.mockClear();
  disconnect.mockClear();
  showUpdates.mockClear();
  connect.mockReset().mockResolvedValue({ outcome: "opened" });
  handOver = undefined;
  onShow.mockClear();
  onAddingChange.mockClear();
  onSelect.mockClear();
});

const onShow = vi.fn();
const onAddingChange = vi.fn();
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

/** The window's part of the page: the list it reads, the device it selects, and whether the page shows. */
function Harness({ capability, shown = true }: { capability: NonNullable<PlatformAdapter["remoteDaemons"]>; shown?: boolean }) {
  const list = useRemoteDaemons(capability);
  const [chosen, setChosen] = React.useState<string>();
  const selectedId = (list.entries.find((entry) => entry.id === chosen) ?? list.entries[0])?.id;
  return (
    <RemoteDevicesProvider capability={capability} list={list} selectedId={selectedId} shown={shown} onShow={onShow} onSelect={(id) => { onSelect(id); setChosen(id); }} onAdd={() => {}} onAddingChange={onAddingChange}>
      <RemoteDevicesSidebar />
      {shown && <RemoteDevicePage />}
    </RemoteDevicesProvider>
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


it("asks for the page to be shown when the shell asks for it, and when it hands something over", async () => {
  await mount();
  await act(async () => askForPage!());
  expect(onShow).toHaveBeenCalledTimes(1);
  await act(async () => handOver!([{ id: "staging", outcome: { outcome: "no_daemon" } }]));
  // Once for the hand-over, and once for the question it raises.
  expect(onShow.mock.calls.length).toBeGreaterThan(1);
});


it("asks for the page back when a connection started on the page ends in a question or a failure, and not when it opens", async () => {
  list.mockResolvedValue([KEYED]);
  const container = await mount();
  await pressTile(container, "Keyed");
  expect(onShow).not.toHaveBeenCalled();
  connect.mockResolvedValueOnce({ outcome: "no_daemon" });
  await pressTile(container, "Keyed");
  expect(onShow).toHaveBeenCalledTimes(1);
  await press(modal()!, "Cancel");
  onShow.mockClear();
  connect.mockResolvedValueOnce({ outcome: "auth_failed", detail: "denied" });
  await pressTile(container, "Keyed");
  expect(onShow).toHaveBeenCalledTimes(1);
  onShow.mockClear();
  connect.mockResolvedValueOnce({ outcome: "opened_in_browser" });
  await pressTile(container, "Keyed");
  expect(onShow).not.toHaveBeenCalled();
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
  expect(container.textContent).toContain("Remote devices");
  expect(byLabel(container, "Add remote daemon")).not.toBeNull();
});

it("lists the saved devices in the order saved, each with its state where it is held, and the first one is shown", async () => {
  const lab = { ...STAGING, id: "lab", name: "Home lab" };
  list.mockResolvedValue([KEYED, STAGING, lab]);
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }, { id: "lab", state: "reconnecting", via: "window" }]);
  const container = await mount();
  const rows = deviceRows(container);
  expect(rows.map((row) => row.textContent!.startsWith("Keyed") ? "Keyed" : row.textContent!.startsWith("Staging") ? "Staging" : "Home lab")).toEqual(["Keyed", "Staging", "Home lab"]);
  expect(rows.map((row) => row.textContent!.includes("Connected"))).toEqual([false, true, false]);
  expect(rows.map((row) => row.textContent!.includes("Reconnecting"))).toEqual([false, false, true]);
  expect(heading(container)).toBe("Keyed");
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
  expect(header.textContent).toContain("deploy@staging.example.com:22");
  // A device that is not held has no state to name.
  list.mockResolvedValue([KEYED]);
  status.mockResolvedValue([]);
  const idle = await mount();
  expect(idle.querySelector("header")!.textContent).not.toContain("Connected");
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
  const headings = [...container.querySelectorAll("h1, h2, h3")];
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
  const created = { id: "new", name: "box.example.com", host: "box.example.com", port: 22, user: "me", auth: "password" as const, keyPath: null, fingerprint: null, octoboarddPath: null };
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
  expect(save).toHaveBeenCalledExactlyOnceWith({ ...STAGING, user: "ops" });
  expect(heading(container)).toBe("Staging");
});

it("drops what was changed and not saved without asking when another device is chosen, and when Add is started", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  const container = await mount();
  await type(field("User"), "ops");
  await act(async () => rowNamed(container, "Staging").click());
  expect(modal()).toBeUndefined();
  expect(field("User").value).toBe("deploy");
  await type(field("User"), "ops");
  await act(async () => rowNamed(container, "Keyed").click());
  expect(modal()).toBeUndefined();
  expect(field("User").value).toBe("deploy");
  expect(save).not.toHaveBeenCalled();

  await type(field("User"), "ops");
  await addFromSidebar(container);
  await press(container, "Cancel");
  expect(field("User").value).toBe("deploy");
});

it("selects the device a hand-over is for, over the one shown and over a form being filled in, and shows its question", async () => {
  list.mockResolvedValue([STAGING, KEYED]);
  const container = await mount();
  expect(heading(container)).toBe("Staging");
  await addFromSidebar(container);
  await type(field("Host"), "box");
  await act(async () => handOver!([{ id: "keyed", outcome: { outcome: "no_daemon" } }]));
  expect(heading(container)).toBe("Keyed");
  expect(container.querySelector("form")!.querySelector("input[type=radio]")).not.toBeNull();
  expect(modal()!.textContent).toContain("No daemon is running on");
});

it("selects nothing for a hand-over of a device that is no longer saved", async () => {
  list.mockResolvedValue([STAGING, KEYED]);
  const container = await mount();
  await act(async () => handOver!([{ id: "gone", outcome: { outcome: "no_daemon" } }]));
  expect(heading(container)).toBe("Staging");
  expect(modal()).toBeUndefined();
});

it("tells whenever a new device's form opens and closes, for what names the page elsewhere", async () => {
  list.mockResolvedValue([KEYED, STAGING]);
  const container = await mount();
  expect(onAddingChange.mock.calls.at(-1)).toEqual([false]);
  await addFromSidebar(container);
  expect(onAddingChange.mock.calls.at(-1)).toEqual([true]);
  await press(container, "Cancel");
  expect(onAddingChange.mock.calls.at(-1)).toEqual([false]);
  await addFromSidebar(container);
  await act(async () => rowNamed(container, "Staging").click());
  expect(onAddingChange.mock.calls.at(-1)).toEqual([false]);
  await addFromSidebar(container);
  await type(field("Host"), "box");
  await type(field("User"), "me");
  await press(container, "Save");
  expect(onAddingChange.mock.calls.at(-1)).toEqual([false]);
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
const CREATED: RemoteDaemonEntry = { id: "new", name: "box.example.com", host: "box.example.com", port: 22, user: "me", auth: "password", keyPath: null, fingerprint: null, octoboarddPath: null };

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

it("selects nothing for a device added when the form was cancelled before the list answered", async () => {
  list.mockResolvedValue([STAGING, KEYED]);
  const container = await mount();
  await act(async () => rowNamed(container, "Keyed").click());
  onSelect.mockClear();
  await filledNew(container);
  const answer = holdNextListRead();
  await press(container, "Save");
  await press(container, "Cancel");
  await act(async () => answer([STAGING, KEYED, CREATED]));
  expect(onSelect).not.toHaveBeenCalled();
  expect(heading(container)).toBe("Keyed");
});

it("selects nothing for a device added when another device was chosen before the list answered", async () => {
  list.mockResolvedValue([STAGING, KEYED]);
  const container = await mount();
  await filledNew(container);
  const answer = holdNextListRead();
  await press(container, "Save");
  await act(async () => rowNamed(container, "Keyed").click());
  await act(async () => answer([STAGING, KEYED, CREATED]));
  expect(onSelect.mock.calls).toEqual([["keyed"]]);
  expect(heading(container)).toBe("Keyed");
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
