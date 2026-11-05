// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { WINDOWS_MACHINE } from "../fixtures/builders";
import { globalParam } from "../i18n/catalog";
import { currentDaemonHost, daemonOsName, platformParam } from "../i18n/platformTerms";
import { Message, useT } from "../i18n/react";
import type { HostFacts } from "../platform/host";
import type { HostInfo } from "../protocol";
import { withUiParams } from "../errors/daemonMessage";
import { daemonHostOf } from "./daemonHost";
import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import type { State } from "./state";

const MAC_MACHINE = { host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings" }, homeDir: "/Users/dev" } as const satisfies {
  host: HostInfo;
  homeDir: string;
};

/** Words from both hosts: the daemon's Trash through `useT` and through `Message`, the client's settings app and menu
 * bar icon. */
function Probe(): React.ReactElement {
  const t = useT();
  return (
    <>
      <p data-word="trash">{t("dialog.removeProject.trash")}</p>
      <p data-word="trashHint"><Message id="dialog.removeProject.trashHint" /></p>
      <p data-word="settings">{t("settings.notifications.openSettings")}</p>
      <p data-word="menuBarIcon">{t("settings.background.description.on")}</p>
    </>
  );
}

function mount(client: HostFacts, state: Partial<State>) {
  const daemon = testDaemon({ state });
  const container = mountHost(<Probe />, { daemon, platform: { kind: "browser", host: client } });
  const word = (name: string) => container.querySelector(`[data-word="${name}"]`)?.textContent ?? "";
  return { daemon, container, word };
}

it("words what happens on a Windows daemon's machine in Windows terms on a macOS client, and the client's own system in macOS terms", () => {
  const { word } = mount(MAC_HOST, { ...WINDOWS_MACHINE });
  expect(word("trash")).toBe("Also move the project folder to the Recycle Bin");
  expect(word("trashHint")).toContain("You can recover it from the Recycle Bin.");
  expect(globalParam("hostSettings", "en")).toBe("Settings");
  expect(word("settings")).toBe("Open System Settings");
  expect(word("menuBarIcon")).toContain("The menu bar icon brings the window back.");
  expect(withUiParams("directory_unreadable", {}).osName).toBe("Windows");
});

it("words what happens on a macOS daemon's machine in macOS terms on a Windows client, and the client's own system in Windows terms", () => {
  const { word } = mount(WINDOWS_HOST, { ...MAC_MACHINE });
  expect(word("trash")).toBe("Also move the project folder to the Trash");
  expect(word("trashHint")).toContain("You can recover it from the Trash.");
  expect(globalParam("hostSettings", "en")).toBe("System Settings");
  expect(word("settings")).toBe("Open Windows Settings");
  expect(word("menuBarIcon")).toContain("The notification area icon brings the window back.");
  expect(withUiParams("directory_unreadable", {}).osName).toBe("macOS");
});

it("words everything as before on a macOS client of a macOS daemon", () => {
  const { word } = mount(MAC_HOST, { ...MAC_MACHINE });
  expect(word("trash")).toBe("Also move the project folder to the Trash");
  expect(word("trashHint")).toBe("The whole folder goes, including anything not committed to Git. You can recover it from the Trash.");
  expect(globalParam("hostSettings", "en")).toBe("System Settings");
  expect(word("settings")).toBe("Open System Settings");
  expect(word("menuBarIcon")).toContain("The menu bar icon brings the window back.");
  expect(withUiParams("directory_unreadable", {}).osName).toBe("macOS");
});

it("takes the client's words until the first snapshot names the daemon's host, then words the messages already shown anew", () => {
  const { daemon, word } = mount(MAC_HOST, {});
  expect(currentDaemonHost()).toBeUndefined();
  expect(word("trash")).toBe("Also move the project folder to the Trash");
  act(() => daemon.store.setState({ host: WINDOWS_MACHINE.host }));
  expect(word("trash")).toBe("Also move the project folder to the Recycle Bin");
  expect(word("trashHint")).toContain("You can recover it from the Recycle Bin.");
  expect(word("settings")).toBe("Open System Settings");
});

it("announces no daemon host once the window is gone", () => {
  const root = createRoot(document.createElement("div"));
  act(() => root.render(<TestHost daemon={testDaemon({ state: { ...WINDOWS_MACHINE } })} platform={{ kind: "browser", host: MAC_HOST }}><Probe /></TestHost>));
  expect(currentDaemonHost()).toEqual({ os: "windows", terms: { trash: "recycleBin", settings: "settings" } });
  act(() => root.unmount());
  expect(currentDaemonHost()).toBeUndefined();
  expect(globalParam("trash", "en")).toBe("Trash");
});

it("falls back to the client's own words where an older daemon's snapshot names neither its Trash nor its settings app", () => {
  const olderDaemon = { os: "windows", path_style: "windows", file_links: true } as unknown as HostInfo;
  const { word } = mount(WINDOWS_HOST, { host: olderDaemon });
  expect(currentDaemonHost()).toEqual({ os: "windows", terms: {} });
  expect(word("trash")).toBe("Also move the project folder to the Recycle Bin");
  expect(word("trashHint")).toContain("You can recover it from the Recycle Bin.");
  expect(globalParam("hostSettings", "en")).toBe("Settings");
  expect(globalParam("trash", "en")).not.toContain("undefined");
});

it("takes each word the daemon names and the client's for the one it leaves out", () => {
  const partial = { os: "macos", path_style: "unix", file_links: true, trash: "trash" } as unknown as HostInfo;
  const { word } = mount(WINDOWS_HOST, { host: partial });
  expect(word("trash")).toBe("Also move the project folder to the Trash");
  expect(globalParam("hostSettings", "en")).toBe("Settings");
});

it("drops a value this client does not know from every field, the client's own word standing in", () => {
  const newer = { os: "plan9", path_style: "unix", file_links: true, trash: "compost", settings: "control_panel" } as unknown as HostInfo;
  expect(daemonHostOf(newer)).toEqual({ terms: {} });
  const { word } = mount(WINDOWS_HOST, { host: newer });
  expect(word("trash")).toBe("Also move the project folder to the Recycle Bin");
  expect(globalParam("hostSettings", "en")).toBe("Settings");
  expect(daemonOsName()).toBe("Windows");
});

it("maps the daemon host's settings app to the catalog's term, and leaves it out where an older daemon sent none", () => {
  expect(daemonHostOf(WINDOWS_MACHINE.host).terms.settings).toBe("settings");
  expect(daemonHostOf({ ...MAC_MACHINE.host }).terms.settings).toBe("systemSettings");
  const older = { os: "windows", path_style: "windows", file_links: true, trash: "recycle_bin" } as unknown as HostInfo;
  expect(daemonHostOf(older).terms).not.toHaveProperty("settings");
});

it("reads {hostSettings} from the daemon's host and {systemSettings} from the client's, the client's word standing in for a daemon that names none", () => {
  const { word } = mount(MAC_HOST, { ...WINDOWS_MACHINE });
  expect(platformParam("hostSettings", "en")).toBe("Settings");
  expect(platformParam("HostSettings", "en")).toBe("Settings");
  expect(platformParam("systemSettings", "en")).toBe("System Settings");
  expect(word("settings")).toBe("Open System Settings");
  expect(platformParam("hostSettings", "tr", undefined, { os: "windows", terms: { settings: "settings" } })).toBe("Ayarlar");
  expect(platformParam("hostSettingsAcc", "tr", undefined, { os: "windows", terms: { settings: "settings" } })).toBe("Ayarlar’ı");
  expect(platformParam("hostSettings", "en", WINDOWS_HOST, { os: "macos", terms: {} })).toBe("Settings");
  expect(platformParam("hostSettings", "en", MAC_HOST, { os: "windows", terms: {} })).toBe("System Settings");
});
