// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter, RemoteDaemonEntry, RemoteDaemonEntryInput } from "../platform";
import { BROWSER_HOST } from "../test/hosts";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { buttons, inputLabelled, type } from "../test/dom";
import { mountHost } from "../test/mountHost";
import { RemoteDaemonForm } from "./RemoteDaemonForm";

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
  deviceKey: { declined: false, file: null, mark: null },
};

const onSave = vi.fn(async (_entry: RemoteDaemonEntryInput) => {});
const onCancel = vi.fn();
beforeEach(() => {
  acknowledgeAllAlerts();
  onSave.mockReset().mockResolvedValue(undefined);
  onCancel.mockClear();
});

/** The form for `entry` (a new device's blank one without), whose `render(entry)` shows it again with another entry, as
 * the page does when the shell changes what it holds. */
function mount(entry?: RemoteDaemonEntry, { cancellable = entry === undefined, readText }: { cancellable?: boolean; readText?: () => Promise<string> } = {}) {
  const view = (shown: RemoteDaemonEntry | undefined) => (
    <LanguageProvider><RemoteDaemonForm entry={shown} onSave={onSave} onCancel={cancellable ? onCancel : undefined} /></LanguageProvider>
  );
  const platform: PlatformAdapter = { kind: "browser", host: BROWSER_HOST, clipboard: { writeText: async () => {}, readText } };
  const container = mountHost(view(entry), { platform });
  return { render: (shown: RemoteDaemonEntry | undefined) => container.render(view(shown)) };
}

const field = (label: string) => inputLabelled(label)!;
const button = (name: string) => buttons().find((candidate) => candidate.textContent?.trim() === name);
const isDisabled = (name: string) => {
  const target = button(name)!;
  return target.disabled || target.getAttribute("aria-disabled") === "true";
};
const press = (name: string) => act(async () => button(name)!.click());
const radio = (name: string) => [...document.body.querySelectorAll<HTMLInputElement>("input[type=radio]")].find((input) => input.labels?.[0]?.textContent?.startsWith(name));
/** Leaves the field, as the user does by moving on. */
const leave = (label: string) => act(() => { field(label).focus(); field(label).blur(); });
const text = () => document.body.textContent ?? "";
const saved = () => onSave.mock.calls.at(-1)![0];
/** Fills in what a new entry needs. */
function fillNew() {
  type(field("Host"), "box.example.com");
  type(field("User"), "me");
}

it("starts a new entry on password sign-in and port 22, with no pinned host key to show and nothing to save yet", () => {
  mount();
  expect(field("SSH port").value).toBe("22");
  expect(radio("Password")!.checked).toBe(true);
  expect(radio("This computer")).toBeUndefined();
  expect(text()).not.toContain("Pinned host key");
  expect(inputLabelled("Key file path")).toBeUndefined();
  expect(isDisabled("Save")).toBe(true);
});

it("saves a new entry named after its host when the name is blank", async () => {
  mount();
  fillNew();
  expect(isDisabled("Save")).toBe(false);
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
});

it("saves a new entry under the name that was typed", async () => {
  mount();
  fillNew();
  type(field("Name"), "Build box");
  await press("Save");
  expect(saved()).toMatchObject({ id: null, name: "Build box", host: "box.example.com" });
});

it("cancels a new entry through Cancel, and offers no Revert for it", async () => {
  mount();
  expect(button("Revert")).toBeUndefined();
  fillNew();
  await press("Cancel");
  expect(onCancel).toHaveBeenCalledOnce();
  expect(onSave).not.toHaveBeenCalled();
});

it("saves nothing while a field is wrong, says so and puts focus there, and a port is a number from 1 to 65535", async () => {
  mount();
  fillNew();
  for (const wrong of ["", "0", "65536", "22a", "-1", "2 2"]) {
    type(field("SSH port"), wrong);
    await press("Save");
    expect(text(), wrong).toContain("Enter a port from 1 to 65535.");
    expect(document.activeElement, wrong).toBe(field("SSH port"));
  }
  expect(onSave).not.toHaveBeenCalled();
  type(field("SSH port"), "65535");
  expect(isDisabled("Save")).toBe(false);
  await press("Save");
  expect(saved().port).toBe(65535);
});

it("says nothing of a field until it was edited and left, and of every invalid one once Save is pressed", async () => {
  mount();
  expect(text()).not.toContain("Enter the host.");
  type(field("Host"), "box");
  type(field("Host"), "");
  type(field("User"), "me");
  // Edited but not left yet, and nothing said of a field because another control changed.
  expect(text()).not.toContain("Enter the host.");
  await leave("Host");
  expect(text()).toContain("Enter the host.");
  type(field("User"), "");
  type(field("Name"), "Box");
  expect(text()).not.toContain("Enter the user.");
  expect(isDisabled("Save")).toBe(false);
  await press("Save");
  expect(onSave).not.toHaveBeenCalled();
  expect(text()).toContain("Enter the user.");
  // Focus goes to the first invalid field, in the order the address is written.
  expect(document.activeElement).toBe(field("User"));
});

it("asks for the key file's path when it signs in with one, only once Save finds it blank, and sends it only then", async () => {
  mount();
  fillNew();
  await act(async () => radio("Key file")!.click());
  // Choosing the way to sign in says nothing of the path it has not been asked for yet.
  expect(text()).not.toContain("Enter the path of the key file.");
  expect(isDisabled("Save")).toBe(false);
  await press("Save");
  expect(onSave).not.toHaveBeenCalled();
  expect(text()).toContain("Enter the path of the key file.");
  expect(document.activeElement).toBe(field("Key file path"));
  type(field("Key file path"), "~/.ssh/id_ed25519");
  await press("Save");
  expect(saved()).toMatchObject({ auth: "key", keyPath: "~/.ssh/id_ed25519" });
});

it("drops the key file's path again when the entry goes back to a password", async () => {
  mount();
  fillNew();
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

it("says an entry the shell has switched to this computer's own key signs in with it, in place of the choice, and saves it so", async () => {
  mount({ ...SAVED, auth: "device" });
  // The key is turned off at its own switch, not by choosing another way here.
  expect(radio("This computer")).toBeUndefined();
  expect(radio("Password")).toBeUndefined();
  expect(radio("Key file")).toBeUndefined();
  expect(text()).toContain("Connections sign in with this computer’s key. To use a password or a key file instead, turn the key off.");
  type(field("User"), "ops");
  await press("Save");
  expect(saved()).toMatchObject({ id: "staging", auth: "device", keyPath: null, user: "ops" });
});

/** What describes `input`, by the ids its `aria-describedby` names, without the isolates around what is substituted. */
const describedBy = (input: HTMLInputElement) =>
  (input.getAttribute("aria-describedby") ?? "")
    .split(" ")
    .map((id) => document.getElementById(id)?.textContent ?? "")
    .join(" ")
    .replace(/[\u2068\u2069]/g, "");
const REPOINT_WARNING = "Saving removes this computer’s key from deploy@staging.example.com.";

it("warns that saving another host, port or user removes this computer's key from where it was added, before the save", () => {
  mount({ ...SAVED, auth: "device" });
  const shown = () => text().replace(/[\u2068\u2069]/g, "");
  // Nothing warns while the destination is the saved one, whatever else is edited.
  expect(shown()).not.toContain(REPOINT_WARNING);
  type(field("Name"), "Renamed");
  expect(shown()).not.toContain(REPOINT_WARNING);
  for (const [label, changed, back] of [["User", "ops", "deploy"], ["Host", "other.example.com", "staging.example.com"], ["SSH port", "22", "2222"]] as const) {
    type(field(label), changed);
    // The warning names where the key is now, says what the next connection does, and describes the three fields.
    expect(shown()).toContain("The key does not move to the new address");
    expect(shown()).toContain(REPOINT_WARNING);
    expect(shown()).toContain("The next connection asks before adding it at the new address.");
    for (const described of ["User", "Host", "SSH port"]) expect(describedBy(field(described))).toContain(REPOINT_WARNING);
    // Back at the saved destination, the save removes nothing and nothing warns.
    type(field(label), back);
    expect(shown()).not.toContain(REPOINT_WARNING);
    expect(describedBy(field(label))).not.toContain(REPOINT_WARNING);
  }
});

it("warns of nothing when a daemon without this computer's key is pointed elsewhere", () => {
  mount(SAVED);
  type(field("Host"), "other.example.com");
  expect(text()).not.toContain("removes this computer’s key");
  expect(describedBy(field("Host"))).not.toContain("removes this computer’s key");
});

it("shows an entry as it is saved, with Save and Revert unavailable until something is changed", () => {
  mount(SAVED);
  expect(field("Name").value).toBe("Staging");
  expect(field("Host").value).toBe("staging.example.com");
  expect(field("SSH port").value).toBe("2222");
  expect(field("User").value).toBe("deploy");
  expect(field("Path of octoboardd").value).toBe("/opt/octoboardd");
  expect(isDisabled("Save")).toBe(true);
  expect(isDisabled("Revert")).toBe(true);
});

it("edits an entry as it is saved, keeping its id and sending every field back", async () => {
  mount(SAVED);
  type(field("User"), "ops");
  expect(isDisabled("Save")).toBe(false);
  expect(isDisabled("Revert")).toBe(false);
  await press("Save");
  // What the entry keeps about this computer's key is not the form's to send.
  const { deviceKey: _deviceKey, ...sent } = SAVED;
  expect(saved()).toEqual({ ...sent, user: "ops" });
});

it("takes the fields back to what is saved on Revert, and Save with them", async () => {
  mount(SAVED);
  type(field("User"), "ops");
  type(field("Name"), "Prod");
  await press("Revert");
  expect(field("User").value).toBe("deploy");
  expect(field("Name").value).toBe("Staging");
  expect(isDisabled("Save")).toBe(true);
  expect(isDisabled("Revert")).toBe(true);
  expect(onSave).not.toHaveBeenCalled();
});

it("takes back a change typed over, so that what equals the saved entry is not a change", () => {
  mount(SAVED);
  type(field("User"), "ops");
  type(field("User"), "deploy");
  expect(isDisabled("Save")).toBe(true);
});

it("shows the pinned host key of an entry and clears it on request, so that the next connection confirms one again", async () => {
  mount(SAVED);
  expect(text()).toContain("SHA256:pinned");
  await press("Clear");
  expect(text()).toContain("Not pinned yet");
  expect(text()).not.toContain("SHA256:pinned");
  await press("Save");
  expect(saved().fingerprint).toBeNull();
});

it("says a saved entry has no pinned host key yet when it has none", () => {
  mount({ ...SAVED, fingerprint: null });
  expect(text()).toContain("Not pinned yet");
});

it("lets a field the user has not touched follow the saved entry, and keeps what was typed", () => {
  const form = mount({ ...SAVED, fingerprint: null });
  type(field("Name"), "Mine");
  form.render({ ...SAVED, name: "Renamed elsewhere", host: "other.example.com", fingerprint: "SHA256:fresh", octoboarddPath: "/srv/octoboardd" });
  // Typed over: kept.
  expect(field("Name").value).toBe("Mine");
  // Not touched: the shell's change shows, a newly pinned host key and a path saved from a question included.
  expect(field("Host").value).toBe("other.example.com");
  expect(field("Path of octoboardd").value).toBe("/srv/octoboardd");
  expect(text()).toContain("SHA256:fresh");
});

it("has nothing left to save once the shell hands back the entry that was saved", async () => {
  const form = mount(SAVED);
  type(field("Name"), "");
  type(field("User"), "ops");
  await press("Save");
  // A blank name was saved as the host, which the form then shows.
  expect(saved()).toMatchObject({ name: "staging.example.com", user: "ops" });
  form.render({ ...SAVED, name: "staging.example.com", user: "ops" });
  expect(field("Name").value).toBe("staging.example.com");
  expect(isDisabled("Save")).toBe(true);
  expect(isDisabled("Revert")).toBe(true);
});

it("fills the fields from pasted connection details, pinning the host key they carry, and keeps the typed name where they give none", async () => {
  mount();
  type(field("Name"), "My box");
  type(field("Connection details"), "octoboard+ssh://me@198.51.100.7:2200?fingerprint=SHA256:abc&octoboardd=/srv/octoboardd");
  expect(field("Host").value).toBe("198.51.100.7");
  expect(field("SSH port").value).toBe("2200");
  expect(field("User").value).toBe("me");
  expect(field("Path of octoboardd").value).toBe("/srv/octoboardd");
  expect(text()).toContain("SHA256:abc");
  await press("Save");
  expect(saved()).toMatchObject({ name: "My box", host: "198.51.100.7", port: 2200, user: "me", fingerprint: "SHA256:abc", octoboarddPath: "/srv/octoboardd" });
});

it("takes the machine's own name from pasted details as the entry's", async () => {
  mount();
  type(field("Connection details"), "octoboard+ssh://me@box:22?name=Build%20box");
  expect(field("Name").value).toBe("Build box");
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

it("refuses text that is not connection details once it is left, and pasting nothing is fine", async () => {
  mount();
  fillNew();
  type(field("Connection details"), "ssh me@box");
  await leave("Connection details");
  expect(text()).toContain("These are not connection details.");
  await press("Save");
  expect(onSave).not.toHaveBeenCalled();
  type(field("Connection details"), "   ");
  expect(text()).not.toContain("These are not connection details.");
  await press("Save");
  expect(saved()).toMatchObject({ host: "box.example.com", user: "me" });
});

it("shows why when the shell refuses the entry, and keeps what was typed", async () => {
  onSave.mockRejectedValueOnce(new Error("name taken"));
  mount();
  fillNew();
  type(field("Name"), "Taken");
  await press("Save");
  expect(errorAlerts.getSnapshot()[0]!.title).toBe("Could not save the remote daemon");
  expect(field("Name").value).toBe("Taken");
  expect(field("Host").value).toBe("box.example.com");
  expect(isDisabled("Save")).toBe(false);
});

it("enables Save for any change to save, and for nothing else: what saves the same is not a change", () => {
  mount({ ...SAVED, name: "staging.example.com" });
  expect(isDisabled("Save")).toBe(true);
  // Blanks around the text, the port as typed, a blank name (which is saved as the host) and a blank path of `octoboardd`
  // (none) come to what is held.
  type(field("User"), " deploy ");
  expect(field("User").value).toBe(" deploy ");
  expect(isDisabled("Save")).toBe(true);
  type(field("SSH port"), "02222");
  type(field("Name"), "");
  type(field("Path of octoboardd"), "/opt/octoboardd  ");
  expect(isDisabled("Save")).toBe(true);
  expect(isDisabled("Revert")).toBe(true);
  type(field("User"), "ops");
  expect(isDisabled("Save")).toBe(false);
});

it("takes a key file's path that is no longer the way to sign in for no change", async () => {
  mount(SAVED);
  await act(async () => radio("Key file")!.click());
  type(field("Key file path"), "~/.ssh/id_ed25519");
  expect(isDisabled("Save")).toBe(false);
  await act(async () => radio("Password")!.click());
  expect(isDisabled("Save")).toBe(true);
});

it("saves what was typed trimmed, and the blank name as the trimmed host", async () => {
  mount();
  type(field("Host"), "  box.example.com ");
  type(field("User"), " me ");
  type(field("Name"), "   ");
  await press("Save");
  expect(saved()).toMatchObject({ name: "box.example.com", host: "box.example.com", user: "me" });
});

it("enables Save as soon as something changed, even where the form is not valid, and a Save then says what is wrong of every field and puts focus on the first", async () => {
  mount();
  expect(isDisabled("Save")).toBe(true);
  type(field("Name"), "Box");
  expect(isDisabled("Save")).toBe(false);
  expect(text()).not.toContain("Enter the host.");
  await press("Save");
  expect(onSave).not.toHaveBeenCalled();
  expect(text()).toContain("Enter the host.");
  expect(text()).toContain("Enter the user.");
  // The address is written user@host:port, so the user's field is the first.
  expect(document.activeElement).toBe(field("User"));
});

it("says nothing of a field that was focused and left unchanged", async () => {
  mount();
  type(field("Name"), "Box");
  await leave("Host");
  await leave("User");
  expect(text()).not.toContain("Enter the host.");
  expect(text()).not.toContain("Enter the user.");
});

it("says what is wrong with the key file's path once it is edited and left, not when Key file is chosen", async () => {
  mount();
  fillNew();
  await act(async () => radio("Key file")!.click());
  expect(text()).not.toContain("Enter the path of the key file.");
  type(field("Key file path"), "x");
  type(field("Key file path"), "");
  await leave("Key file path");
  expect(text()).toContain("Enter the path of the key file.");
});

it("takes back what Save said once the fields are reverted", async () => {
  mount(SAVED);
  type(field("User"), "");
  await press("Save");
  expect(text()).toContain("Enter the user.");
  await press("Revert");
  expect(text()).not.toContain("Enter the user.");
  // Nor does a change after it say anything before the field is left again.
  type(field("User"), "");
  expect(text()).not.toContain("Enter the user.");
});

it("follows the saved entry again for a field whose draft the entry came to equal, and for a field that was saved", () => {
  const form = mount(SAVED);
  type(field("User"), "ops");
  form.render({ ...SAVED, user: "ops" });
  expect(field("User").value).toBe("ops");
  expect(isDisabled("Save")).toBe(true);
  // Followed again, so a later change of the entry shows.
  form.render({ ...SAVED, user: "root" });
  expect(field("User").value).toBe("root");
});

it("shows the entry's own name again after a blank name was saved as the host and the entry is renamed elsewhere", async () => {
  const form = mount(SAVED);
  type(field("Name"), "");
  await press("Save");
  expect(saved()).toMatchObject({ name: "staging.example.com" });
  form.render({ ...SAVED, name: "staging.example.com" });
  form.render({ ...SAVED, name: "Renamed elsewhere" });
  expect(field("Name").value).toBe("Renamed elsewhere");
});

const pasteButton = () => buttons().find((candidate) => candidate.getAttribute("aria-label") === "Paste");
const pressPaste = () => act(async () => pasteButton()!.click());

it("offers Paste beside the connection details only where the clipboard can be read", () => {
  mount();
  expect(pasteButton()).toBeUndefined();
  mount(undefined, { readText: async () => "" });
  expect(pasteButton()).toBeDefined();
});

it("fills the fields from the clipboard as a manual paste does, replacing what the details field held", async () => {
  mount(undefined, { readText: async () => "octoboard+ssh://me@198.51.100.7:2200?fingerprint=SHA256:abc&octoboardd=/srv/octoboardd&name=Build%20box" });
  type(field("Connection details"), "old text");
  await pressPaste();
  expect(field("Connection details").value).toBe("octoboard+ssh://me@198.51.100.7:2200?fingerprint=SHA256:abc&octoboardd=/srv/octoboardd&name=Build%20box");
  expect(field("Host").value).toBe("198.51.100.7");
  expect(field("SSH port").value).toBe("2200");
  expect(field("User").value).toBe("me");
  expect(field("Name").value).toBe("Build box");
  expect(field("Path of octoboardd").value).toBe("/srv/octoboardd");
  expect(text()).toContain("SHA256:abc");
  await press("Save");
  expect(saved()).toMatchObject({ name: "Build box", host: "198.51.100.7", port: 2200, user: "me", fingerprint: "SHA256:abc", octoboarddPath: "/srv/octoboardd" });
});

it("puts text that is not connection details into the field, and fills nothing from it", async () => {
  mount(undefined, { readText: async () => "ssh me@box" });
  await pressPaste();
  expect(field("Connection details").value).toBe("ssh me@box");
  expect(field("Host").value).toBe("");
  expect(field("User").value).toBe("");
});

it("leaves the form as it was for an empty clipboard", async () => {
  mount(undefined, { readText: async () => "" });
  type(field("Name"), "Box");
  await pressPaste();
  expect(field("Connection details").value).toBe("");
  expect(field("Name").value).toBe("Box");
});

it("raises the alert titled for the connection details when the clipboard cannot be read", async () => {
  mount(undefined, { readText: async () => { throw new Error("denied"); } });
  await pressPaste();
  expect(errorAlerts.getSnapshot()[0]).toMatchObject({ title: "Could not paste the connection details", message: "The clipboard could not be read." });
});

it("shows at once what a pasted text that is not connection details is, and puts the trimmed text in the field", async () => {
  mount(undefined, { readText: async () => "  ssh me@box \n" });
  await pressPaste();
  expect(field("Connection details").value).toBe("ssh me@box");
  expect(text()).toContain("These are not connection details.");
});

it("fills the fields from pasted connection details with blanks around them, and says nothing of them", async () => {
  mount(undefined, { readText: async () => "\n  octoboard+ssh://me@198.51.100.7:2200  " });
  await pressPaste();
  expect(field("Connection details").value).toBe("octoboard+ssh://me@198.51.100.7:2200");
  expect(field("Host").value).toBe("198.51.100.7");
  expect(text()).not.toContain("These are not connection details.");
});

it("does not overwrite a field being edited because an unrelated reload gave the form a new entry object", () => {
  const entry = { ...SAVED, name: "staging.example.com" };
  const form = mount(entry);
  // Cleared, which saves as the host: the same as what is held, so the draft saves nothing different.
  type(field("Name"), "");
  expect(field("Name").value).toBe("");
  form.render({ ...entry });
  expect(field("Name").value).toBe("");
  // Nor does it when another field's saved value changes.
  form.render({ ...entry, user: "root" });
  expect(field("Name").value).toBe("");
  expect(field("User").value).toBe("root");
});

it("takes back what Save said, and what was left, once a save has succeeded", async () => {
  mount(SAVED);
  type(field("User"), "");
  await press("Save");
  expect(text()).toContain("Enter the user.");
  type(field("User"), "ops");
  type(field("Host"), "");
  await leave("Host");
  expect(text()).toContain("Enter the host.");
  type(field("Host"), "box");
  await press("Save");
  expect(saved()).toMatchObject({ user: "ops", host: "box" });
  // Nothing of the earlier form is said of a field edited now, before it is left.
  type(field("Host"), "");
  type(field("User"), "");
  expect(text()).not.toContain("Enter the host.");
  expect(text()).not.toContain("Enter the user.");
});
