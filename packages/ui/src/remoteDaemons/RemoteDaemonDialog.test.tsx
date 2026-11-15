// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { LanguageProvider } from "../i18n/react";
import type { RemoteDaemonEntry, RemoteDaemonEntryInput } from "../platform";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { buttons, inputLabelled, type } from "../test/dom";
import { mountHost } from "../test/mountHost";
import { RemoteDaemonDialog } from "./RemoteDaemonDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SAVED: RemoteDaemonEntry = {
  id: "staging",
  name: "Staging",
  host: "staging.example.com",
  port: 2222,
  user: "deploy",
  auth: "password",
  keyPath: null,
  fingerprint: "SHA256:pinned",
  octoboarddPath: "/opt/octoboardd",
};

const onSave = vi.fn(async (_entry: RemoteDaemonEntryInput) => {});
const onClose = vi.fn();
beforeEach(() => {
  acknowledgeAllAlerts();
  onSave.mockReset().mockResolvedValue(undefined);
  onClose.mockClear();
});

function mount(entry?: RemoteDaemonEntry) {
  act(() => {
    mountHost(<LanguageProvider><RemoteDaemonDialog entry={entry} onSave={onSave} onClose={onClose} /></LanguageProvider>);
  });
}

const field = (label: string) => inputLabelled(label);
const press = (name: string) => act(async () => buttons().find((button) => button.textContent?.trim() === name)!.click());
const radio = (name: string) => [...document.body.querySelectorAll<HTMLInputElement>("input[type=radio]")].find((input) => input.labels?.[0]?.textContent?.startsWith(name));
const dialogText = () => document.body.textContent ?? "";
const saved = () => onSave.mock.calls.at(-1)![0];

it("starts a new entry on password sign-in and port 22, with no pinned host key to show", () => {
  mount();
  expect(field("SSH port").value).toBe("22");
  expect(radio("Password")!.checked).toBe(true);
  expect(radio("This computer")).toBeUndefined();
  expect(dialogText()).not.toContain("Pinned host key");
  expect(field("Key file path")).toBeUndefined();
});

it("saves a new entry named after its host when the name is blank, and closes once the shell has taken it", async () => {
  mount();
  type(field("Host"), "box.example.com");
  type(field("User"), "me");
  await press("Save");
  expect(saved()).toEqual({
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
  expect(onClose).toHaveBeenCalledOnce();
});

it("refuses a blank host or user and a port that is no port from 1 to 65535", async () => {
  mount();
  await press("Save");
  expect(dialogText()).toContain("Enter the host.");
  expect(dialogText()).toContain("Enter the user.");
  type(field("Host"), "box");
  type(field("User"), "me");
  for (const wrong of ["", "0", "65536", "22a", "-1", "2 2"]) {
    type(field("SSH port"), wrong);
    await press("Save");
    expect(dialogText(), wrong).toContain("Enter a port from 1 to 65535.");
  }
  expect(onSave).not.toHaveBeenCalled();
  type(field("SSH port"), "65535");
  await press("Save");
  expect(saved().port).toBe(65535);
});

it("asks for the key file's path when it signs in with one, and sends it only then", async () => {
  mount();
  type(field("Host"), "box");
  type(field("User"), "me");
  await act(async () => radio("Key file")!.click());
  await press("Save");
  expect(dialogText()).toContain("Enter the path of the key file.");
  expect(onSave).not.toHaveBeenCalled();
  type(field("Key file path"), "~/.ssh/id_ed25519");
  await press("Save");
  expect(saved()).toMatchObject({ auth: "key", keyPath: "~/.ssh/id_ed25519" });
});

it("drops the key file's path again when the entry goes back to a password", async () => {
  mount();
  type(field("Host"), "box");
  type(field("User"), "me");
  await act(async () => radio("Key file")!.click());
  type(field("Key file path"), "~/.ssh/id_ed25519");
  await act(async () => radio("Password")!.click());
  await press("Save");
  expect(saved()).toMatchObject({ auth: "password", keyPath: null });
});

it("offers no use of this computer's own key to an entry the shell has not switched to it", () => {
  mount(SAVED);
  expect(radio("This computer")).toBeUndefined();
});

it("keeps this computer's own key on an entry the shell has switched to it, which it offers to be saved with", async () => {
  mount({ ...SAVED, auth: "device" });
  expect(radio("This computer")!.checked).toBe(true);
  await press("Save");
  expect(saved()).toMatchObject({ id: "staging", auth: "device", keyPath: null });
});

it("edits an entry as it is saved, keeping its id and sending every field back", async () => {
  mount(SAVED);
  expect(document.body.textContent).toContain("Edit remote daemon");
  expect(field("Name").value).toBe("Staging");
  expect(field("SSH port").value).toBe("2222");
  expect(field("Path of octoboardd").value).toBe("/opt/octoboardd");
  type(field("User"), "ops");
  await press("Save");
  expect(saved()).toEqual({ ...SAVED, user: "ops" });
});

it("shows the pinned host key of an entry and clears it on request, so that the next connection confirms one again", async () => {
  mount(SAVED);
  expect(dialogText()).toContain("SHA256:pinned");
  await press("Clear");
  expect(dialogText()).toContain("Not pinned yet");
  expect(dialogText()).not.toContain("SHA256:pinned");
  await press("Save");
  expect(saved().fingerprint).toBeNull();
});

it("says a saved entry has no pinned host key yet when it has none", () => {
  mount({ ...SAVED, fingerprint: null });
  expect(dialogText()).toContain("Not pinned yet");
});

it("fills the fields from pasted connection details, pinning the host key they carry, and keeps the typed name where they give none", async () => {
  mount();
  type(field("Name"), "My box");
  type(field("Connection details"), "octoboard+ssh://me@198.51.100.7:2200?fingerprint=SHA256:abc&octoboardd=/srv/octoboardd");
  expect(field("Host").value).toBe("198.51.100.7");
  expect(field("SSH port").value).toBe("2200");
  expect(field("User").value).toBe("me");
  expect(field("Path of octoboardd").value).toBe("/srv/octoboardd");
  expect(dialogText()).toContain("SHA256:abc");
  await press("Save");
  expect(saved()).toMatchObject({ name: "My box", host: "198.51.100.7", port: 2200, user: "me", fingerprint: "SHA256:abc", octoboarddPath: "/srv/octoboardd" });
});

it("takes the machine's own name from pasted details as the entry's", async () => {
  mount();
  type(field("Connection details"), "octoboard+ssh://me@box:22?name=Build%20box");
  await press("Save");
  expect(saved().name).toBe("Build box");
});

it("keeps the pinned host key when pasted details carry none, removing a pin only through Clear", async () => {
  mount(SAVED);
  type(field("Connection details"), "octoboard+ssh://me@new-host:22");
  await press("Save");
  expect(saved()).toMatchObject({ host: "new-host", fingerprint: "SHA256:pinned", octoboarddPath: null });
});

it("never types a host key in: pasting details with none into a new entry leaves it unpinned", async () => {
  mount();
  type(field("Connection details"), "octoboard+ssh://me@new-host:22");
  await press("Save");
  expect(saved()).toMatchObject({ host: "new-host", fingerprint: null });
});

it("refuses text that is not connection details, and pasting nothing is fine", async () => {
  mount();
  type(field("Host"), "box");
  type(field("User"), "me");
  type(field("Connection details"), "ssh me@box");
  await press("Save");
  expect(dialogText()).toContain("These are not connection details.");
  expect(onSave).not.toHaveBeenCalled();
  type(field("Connection details"), "   ");
  await press("Save");
  expect(saved()).toMatchObject({ host: "box", user: "me" });
});

it("stays open and shows why when the shell refuses the entry", async () => {
  onSave.mockRejectedValueOnce(new Error("name taken"));
  mount();
  type(field("Host"), "box");
  type(field("User"), "me");
  await press("Save");
  expect(onClose).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()[0]!.title).toBe("Could not save the remote daemon");
  expect(document.body.querySelector("[role=dialog]")).not.toBeNull();
});

it("closes without saving on Cancel", async () => {
  mount();
  await press("Cancel");
  expect(onClose).toHaveBeenCalledOnce();
  expect(onSave).not.toHaveBeenCalled();
});
