// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { LanguageProvider } from "../i18n/react";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { buttons, type } from "../test/dom";
import { mountHost } from "../test/mountHost";
import { RemoteDaemonPathDialog, RemoteDaemonSecretDialog } from "./RemoteDaemonPrompts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const onSubmit = vi.fn();
const onSave = vi.fn(async (_path: string) => {});
const onClose = vi.fn();
beforeEach(() => {
  acknowledgeAllAlerts();
  onSubmit.mockClear();
  onSave.mockReset().mockResolvedValue(undefined);
  onClose.mockClear();
});

const input = () => document.body.querySelector<HTMLInputElement>("input")!;
const press = (name: string) => act(async () => buttons().find((button) => button.textContent?.trim() === name)!.click());
const text = () => document.body.textContent ?? "";
/** `text` without the bidirectional isolation marks around the values filled in. */
const plain = (value: string) => value.replace(/[\u2066-\u2069]/g, "");
const mount = (ui: React.ReactNode) => act(() => void mountHost(<LanguageProvider>{ui}</LanguageProvider>));

it("asks for a password in a masked field, saying it is used for this connection only, and hands it to onSubmit unstored", async () => {
  mount(<RemoteDaemonSecretDialog kind="password" name="Staging" user="dev" onSubmit={onSubmit} onClose={onClose} />);
  expect(text()).toContain("Enter password");
  expect(plain(text())).toContain("Sign in to Staging as dev.");
  expect(plain(input().placeholder)).toBe("Password on the server");
  expect(text()).toContain("Used for this connection only. It is not stored.");
  expect(input().type).toBe("password");
  type(input(), "hunter2");
  await press("Connect");
  expect(onSubmit).toHaveBeenCalledExactlyOnceWith("hunter2");
});

it("asks for a key's passphrase in its own words", async () => {
  mount(<RemoteDaemonSecretDialog kind="passphrase" name="Staging" user="dev" onSubmit={onSubmit} onClose={onClose} />);
  expect(text()).toContain("Enter key passphrase");
  await press("Connect");
  expect(text()).toContain("Enter the passphrase.");
  expect(onSubmit).not.toHaveBeenCalled();
  type(input(), "open sesame");
  await press("Connect");
  expect(onSubmit).toHaveBeenCalledExactlyOnceWith("open sesame");
});

it("sends nothing for an empty password and says so", async () => {
  mount(<RemoteDaemonSecretDialog kind="password" name="Staging" user="dev" onSubmit={onSubmit} onClose={onClose} />);
  await press("Connect");
  expect(text()).toContain("Enter the password.");
  expect(onSubmit).not.toHaveBeenCalled();
});

it("says why it asks, where that is not plain", () => {
  mount(<RemoteDaemonSecretDialog kind="password" name="Staging" user="dev" message="The server no longer accepts this computer’s key." onSubmit={onSubmit} onClose={onClose} />);
  expect(text()).toContain("The server no longer accepts this computer’s key.");
});

it("closes the secret dialog on Cancel without submitting", async () => {
  mount(<RemoteDaemonSecretDialog kind="password" name="Staging" user="dev" onSubmit={onSubmit} onClose={onClose} />);
  type(input(), "typed");
  await press("Cancel");
  expect(onClose).toHaveBeenCalledOnce();
  expect(onSubmit).not.toHaveBeenCalled();
});

it("lists where octoboardd was looked for and saves the path typed, closing once the entry is saved", async () => {
  mount(<RemoteDaemonPathDialog tried={["~/.octoboard/bin/octoboardd", "/usr/local/bin/octoboardd"]} onSave={onSave} onClose={onClose} />);
  expect(text()).toContain("Looked in");
  expect(text()).toContain("~/.octoboard/bin/octoboardd");
  expect(text()).toContain("/usr/local/bin/octoboardd");
  type(input(), "/opt/octoboardd");
  await press("Save and connect");
  expect(onSave).toHaveBeenCalledExactlyOnceWith("/opt/octoboardd");
  expect(onClose).toHaveBeenCalledOnce();
});

it("has no list of places when it was told none, and refuses an empty path", async () => {
  mount(<RemoteDaemonPathDialog tried={[]} onSave={onSave} onClose={onClose} />);
  expect(text()).not.toContain("Looked in");
  await press("Save and connect");
  expect(text()).toContain("Enter the path.");
  expect(onSave).not.toHaveBeenCalled();
  expect(onClose).not.toHaveBeenCalled();
});

it("stays open and shows why when the entry could not be saved", async () => {
  onSave.mockRejectedValueOnce(new Error("disk full"));
  mount(<RemoteDaemonPathDialog tried={[]} onSave={onSave} onClose={onClose} />);
  type(input(), "/opt/octoboardd");
  await press("Save and connect");
  expect(onClose).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()[0]!.title).toBe("Could not save the remote daemon");
});
