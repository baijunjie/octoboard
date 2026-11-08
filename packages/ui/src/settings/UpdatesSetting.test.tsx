// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { press } from "../components/alerts/detailsTestSupport";
import { LanguageProvider } from "../i18n/react";
import type { AvailableUpdate, PlatformAdapter, UpdaterCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import { UpdaterError } from "../platform/updaterError";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { updateChannel } from "../updates/updateChannel";
import type { UpdateStore } from "../updates/updateStore";
import { UpdatesProvider, useUpdates } from "../updates/UpdatesProvider";
import { UpdatesSetting } from "./UpdatesSetting";
import { BROWSER_HOST, MAC_HOST, WINDOWS_HOST } from "../test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => updateChannel.set("beta"));
afterEach(() => localStorage.clear());

/** Mounts the provider with the setting shown from the start, or, with `opened: false`, without it until `open()`: the
 * update state belongs to the provider and is there before Settings is. */
function mount(platform: PlatformAdapter, { opened = true }: { opened?: boolean } = {}) {
  let store: UpdateStore | undefined;
  const Capture = () => {
    store = useUpdates()?.store;
    return null;
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (withSetting: boolean) =>
    act(() =>
      root.render(
        <PlatformProvider value={platform}>
          <LanguageProvider>
            <UpdatesProvider>
              <Capture />
              {withSetting && <UpdatesSetting />}
            </UpdatesProvider>
          </LanguageProvider>
        </PlatformProvider>,
      ),
    );
  render(opened);
  return { container, store: () => store!, open: () => render(true), close: () => render(false) };
}

const FOUND: AvailableUpdate = { version: "0.2.0" };

const updaterOf = (check: UpdaterCapability["check"], install: UpdaterCapability["install"] = async () => {}, version = "0.1.0"): UpdaterCapability => ({
  currentVersion: async () => version,
  check,
  install,
});

const textOf = (element: Element) => (element.textContent ?? "").replace(/[⁦-⁩]/g, "");
const button = (container: HTMLElement, name: string) => Array.from(container.querySelectorAll("button")).find((candidate) => candidate.textContent === name);

/** What the setting's status region says, once it has had its first write delay (`StatusAnnouncer`). */
async function spoken(container: HTMLElement): Promise<string> {
  await act(async () => new Promise((resolve) => setTimeout(resolve, 150)));
  return textOf(container.querySelector("[role=status]")!);
}

const failedMark = (container: HTMLElement) => container.querySelector<HTMLElement>("button[aria-label='Last update check']");

it("shows a check that did not finish as a warning mark before the button, its popover holding the sentences, not as an alert under the row", async () => {
  const check = vi.fn(async () => { throw new Error("offline"); });
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(check) });
  await act(async () => void button(container, "Check for updates")!.click());
  expect(container.querySelector(".alert")).toBeNull();
  const mark = failedMark(container)!;
  expect(mark).not.toBeNull();
  // Before the Check for updates button, in the row's control area.
  expect(mark.compareDocumentPosition(button(container, "Check for updates")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  press(mark);
  const popover = document.querySelector("[data-status-mark-popover]")!;
  expect(textOf(popover)).toContain("The last check for updates did not finish");
  expect(textOf(popover)).toContain("It is not known whether a newer version exists.");
  expect(textOf(popover)).toContain("Press Check for updates to try again.");
  // No retry button of its own (the buttons react-aria adds for dismissing are unnamed).
  expect([...popover.querySelectorAll("button")].filter((b) => b.textContent !== "")).toHaveLength(0);
  // The row's own sentence does not carry the failure.
  expect(textOf(container.querySelector("[class*=text-app-muted]")!)).toBe("You are using version 0.1.0.");
});

it("hands focus to the Check for updates button when the mark that holds it goes as a check starts, and leaves other focus alone", async () => {
  let fail = true;
  let finish: () => void = () => {};
  const check = vi.fn(() => (fail ? Promise.reject(new Error("offline")) : new Promise<undefined>((resolve) => (finish = () => resolve(undefined)))));
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(check) });
  await act(async () => void button(container, "Check for updates")!.click());
  fail = false;
  act(() => failedMark(container)!.focus());
  await act(async () => void store().check("automatic"));
  expect(failedMark(container)).toBeNull();
  expect(document.activeElement).toBe(button(container, "Check for updates"));
  await act(async () => finish());
  // Focus that was elsewhere stays there.
  fail = true;
  await act(async () => store().check("automatic"));
  const outside = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => outside.remove());
  act(() => outside.focus());
  fail = false;
  await act(async () => void store().check("automatic"));
  expect(failedMark(container)).toBeNull();
  expect(document.activeElement).toBe(outside);
  await act(async () => finish());
});

it("leaves a check pressed here to the error alert dialog: the mark appears without being spoken", async () => {
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => { throw new Error("offline"); }) });
  await act(async () => void button(container, "Check for updates")!.click());
  expect(failedMark(container)).not.toBeNull();
  expect(await spoken(container)).toBe("");
  expect(container.querySelector("[role=alert]")).toBeNull();
});

it("speaks an automatic check that does not finish, though an earlier check was pressed here", async () => {
  let fail = false;
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => { if (fail) throw new Error("offline"); return undefined; }) });
  await act(async () => void button(container, "Check for updates")!.click());
  expect(failedMark(container)).toBeNull();
  fail = true;
  await act(async () => store().check("automatic"));
  expect(failedMark(container)).not.toBeNull();
  expect(await spoken(container)).toBe("The last check for updates did not finish.");
});

it("has no update rows without the capability", () => {
  const container = mount({ kind: "browser", host: BROWSER_HOST }).container;
  expect(textOf(container)).not.toContain("Update channel");
});

it("checks the chosen channel on request, saying how it ended", async () => {
  const check = vi.fn(async () => undefined);
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(check) });
  await act(async () => {});
  expect(textOf(container)).toContain("You are using version 0.1.0.");
  await act(async () => button(container, "Check for updates")!.click());
  expect(check).toHaveBeenCalledExactlyOnceWith("beta");
  expect(textOf(container)).toContain("Version 0.1.0 is the latest on this channel.");
});

it("offers the download once a newer version is found", async () => {
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => FOUND) });
  await act(async () => {});
  await act(async () => button(container, "Check for updates")!.click());
  expect(textOf(container)).toContain("Version 0.2.0 is available. You are using version 0.1.0.");
  expect(button(container, "Download and install")).toBeDefined();
  expect(button(container, "Check for updates")).toBeUndefined();
});

it("downloads from Settings, then offers the restart, which asks the window's exit flow", async () => {
  let finish: () => void = () => {};
  const install = vi.fn(() => new Promise<void>((resolve) => (finish = resolve)));
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => FOUND, install) });
  await act(async () => {});
  await act(async () => button(container, "Check for updates")!.click());
  await act(async () => button(container, "Download and install")!.click());
  expect(install).toHaveBeenCalledOnce();
  expect(textOf(container)).toContain("Downloading version 0.2.0…");
  await act(async () => finish());
  expect(textOf(container)).toContain("Version 0.2.0 is installed.");
  const restart = vi.fn();
  store().onRestartRequested(restart);
  await act(async () => button(container, "Restart now")!.click());
  expect(restart).toHaveBeenCalledOnce();
});

it("keeps the install button enabled while a check runs, so focus on it is not dropped", async () => {
  let settle: (found: AvailableUpdate | undefined) => void = () => {};
  const check = vi.fn<UpdaterCapability["check"]>().mockResolvedValueOnce(FOUND).mockImplementation(() => new Promise((resolve) => (settle = resolve)));
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(check) });
  await act(async () => {});
  await act(async () => store().check("automatic"));
  act(() => void store().check("automatic"));
  const install = button(container, "Download and install")!;
  expect(install.disabled).toBe(false);
  expect(install.getAttribute("aria-disabled")).toBe("true");
  await act(async () => settle(FOUND));
});

it("speaks the result of a check pressed here", async () => {
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => FOUND) });
  expect(await spoken(container)).toBe("");
  await act(async () => button(container, "Check for updates")!.click());
  expect(await spoken(container)).toContain("Version 0.2.0 is available.");
});

it("says nothing of an update found before Settings opened, until the user acts on it", async () => {
  let finish: () => void = () => {};
  const install = vi.fn(() => new Promise<void>((resolve) => (finish = resolve)));
  const { container, store, open } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => FOUND, install) }, { opened: false });
  await act(async () => {});
  await act(async () => store().check("automatic"));
  open();
  expect(await spoken(container)).toBe("");
  await act(async () => button(container, "Download and install")!.click());
  expect(await spoken(container)).toContain("Downloading version 0.2.0…");
  await act(async () => finish());
  expect(await spoken(container)).toContain("Version 0.2.0 is installed.");
});

it("speaks the result of a check that starts while Settings is open, an automatic one included", async () => {
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => undefined) });
  await act(async () => {});
  expect(await spoken(container)).toBe("");
  await act(async () => store().check("automatic"));
  expect(await spoken(container)).toContain("Version 0.1.0 is the latest on this channel.");
});

it("keeps the channel the user picks and looks again on the new one", async () => {
  const check = vi.fn(async () => undefined);
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(check) });
  await act(async () => {});
  expect(updateChannel.get()).toBe("beta");
  expect(check).not.toHaveBeenCalled();
  await act(async () => updateChannel.set("stable"));
  expect(localStorage.getItem(PREFERENCE_KEYS.updateChannel)).toBe("stable");
  expect(container.textContent).toContain("Stable");
  expect(check).toHaveBeenCalledExactlyOnceWith("stable");
});

/** The channel the Update channel row shows as chosen. */
const shownChannel = (container: HTMLElement) => container.querySelector("select")?.value;

it.each([
  ["0.1.0-beta.3", "beta"],
  ["0.1.0", "stable"],
])("shows the default channel of build %s while nothing is chosen, without saving it or looking again", async (version, expected) => {
  updateChannel.set(undefined);
  const check = vi.fn(async () => undefined);
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(check, undefined, version) });
  await act(async () => {});
  expect(shownChannel(container)).toBe(expected);
  expect(localStorage.getItem(PREFERENCE_KEYS.updateChannel)).toBeNull();
  expect(check).not.toHaveBeenCalled();
});

it("looks on a channel picked before the running version is known", async () => {
  updateChannel.set(undefined);
  const check = vi.fn(async () => undefined);
  mount({ kind: "tauri", host: BROWSER_HOST, updater: { currentVersion: () => new Promise<string>(() => {}), check, install: async () => {} } });
  await act(async () => {});
  await act(async () => updateChannel.set("beta"));
  expect(check).toHaveBeenCalledExactlyOnceWith("beta");
});

it("checks the running build's default channel while nothing is chosen", async () => {
  updateChannel.set(undefined);
  const check = vi.fn(async () => undefined);
  const { store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(check, undefined, "0.1.0-beta.3") });
  await act(async () => {});
  await act(async () => store().check("manual"));
  expect(check).toHaveBeenCalledExactlyOnceWith("beta");
});

it("keeps Stable chosen on a beta build", async () => {
  updateChannel.set("stable");
  const check = vi.fn(async () => undefined);
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(check, undefined, "0.1.0-beta.3") });
  await act(async () => {});
  expect(shownChannel(container)).toBe("stable");
  await act(async () => store().check("manual"));
  expect(check).toHaveBeenCalledExactlyOnceWith("stable");
});

it("keeps focus on the one button, now Download and install, once a check pressed from it finds an update", async () => {
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => FOUND) });
  await act(async () => {});
  const check = button(container, "Check for updates")!;
  act(() => check.focus());
  await act(async () => check.click());
  const install = button(container, "Download and install")!;
  expect(install).toBeDefined();
  expect(document.activeElement).toBe(install);
});

it("speaks an install that finishes while Settings is open, though the notice started it before", async () => {
  let finish: () => void = () => {};
  const install = vi.fn(() => new Promise<void>((resolve) => (finish = resolve)));
  const { container, store, open } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => FOUND, install) }, { opened: false });
  await act(async () => {});
  await act(async () => store().check("automatic"));
  act(() => void store().install());
  open();
  expect(await spoken(container)).toBe("");
  await act(async () => finish());
  expect(await spoken(container)).toContain("Version 0.2.0 is installed.");
});

it("speaks the result of a check that was already running when Settings opened", async () => {
  let settle: (found: AvailableUpdate | undefined) => void = () => {};
  const { container, store, open } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(() => new Promise((resolve) => (settle = resolve))) }, { opened: false });
  await act(async () => {});
  act(() => void store().check("automatic"));
  open();
  await act(async () => settle(undefined));
  expect(await spoken(container)).toContain("Version 0.1.0 is the latest on this channel.");
});

const notesLink = (container: HTMLElement) =>
  Array.from(container.querySelectorAll<HTMLAnchorElement>("a[data-external-link]")).find((candidate) => candidate.textContent === "What is new");
const releaseOf = (version: string) => `https://github.com/baijunjie/octoboard/releases/tag/desktop-v${version}`;

it("links What is new to the GitHub release of the version the row is about, before the warning mark and the button", async () => {
  let finish: () => void = () => {};
  const check = vi.fn<UpdaterCapability["check"]>().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(FOUND);
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(check, () => new Promise<void>((resolve) => (finish = resolve))) });
  await act(async () => {});
  expect(notesLink(container)!.href).toBe(releaseOf("0.1.0"));
  await act(async () => button(container, "Check for updates")!.click());
  const mark = failedMark(container)!;
  expect(notesLink(container)!.compareDocumentPosition(mark) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(mark.compareDocumentPosition(button(container, "Check for updates")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  await act(async () => button(container, "Check for updates")!.click());
  expect(notesLink(container)!.href).toBe(releaseOf("0.2.0"));
  await act(async () => button(container, "Download and install")!.click());
  expect(notesLink(container)!.href).toBe(releaseOf("0.2.0"));
  await act(async () => finish());
  expect(textOf(container)).toContain("Version 0.2.0 is installed.");
  expect(notesLink(container)!.href).toBe(releaseOf("0.2.0"));
});

it("leaves What is new out while the running version is not known", async () => {
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: { ...updaterOf(async () => undefined), currentVersion: () => new Promise<string>(() => {}) } });
  await act(async () => {});
  expect(notesLink(container)).toBeUndefined();
});

it("leaves What is new out for an installed update whose version is not known, rather than pointing at the running one", async () => {
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => { throw new UpdaterError("installed"); }) });
  await act(async () => {});
  await act(async () => button(container, "Check for updates")!.click());
  expect(textOf(container)).toContain("An update is installed.");
  expect(notesLink(container)).toBeUndefined();
});

it("moves focus held on What is new to the row's action when an installed update of no known version takes the link away", async () => {
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => { throw new UpdaterError("installed"); }) });
  await act(async () => {});
  act(() => notesLink(container)!.focus());
  await act(async () => void store().check("automatic"));
  expect(notesLink(container)).toBeUndefined();
  expect(document.activeElement).toBe(button(container, "Restart now"));
});

it("leaves focus to the dialog when the whole row goes while What is new holds it", async () => {
  const { container, close } = mount({ kind: "tauri", host: BROWSER_HOST, updater: updaterOf(async () => undefined) });
  await act(async () => {});
  act(() => notesLink(container)!.focus());
  const check = button(container, "Check for updates")!;
  const focus = vi.spyOn(check, "focus");
  await act(async () => close());
  expect(focus).not.toHaveBeenCalled();
});

it.each([
  ["installer", WINDOWS_HOST, "Version 0.2.0 is downloaded. It is installed when Octoboard quits or restarts.", "An update is downloaded. It is installed when Octoboard quits or restarts."],
  ["in-place", MAC_HOST, "Version 0.2.0 is installed. It takes effect after Octoboard restarts.", "An update is installed. It takes effect after Octoboard restarts."],
])("words a finished download as the %s updater leaves it, with and without a known version", async (_updater, host, known, unknown) => {
  const found = mount({ kind: "tauri", host, updater: updaterOf(async () => FOUND) });
  await act(async () => {});
  await act(async () => button(found.container, "Check for updates")!.click());
  await act(async () => button(found.container, "Download and install")!.click());
  expect(textOf(found.container)).toContain(known);
  const blind = mount({ kind: "tauri", host, updater: updaterOf(async () => { throw new UpdaterError("installed"); }) });
  await act(async () => {});
  await act(async () => button(blind.container, "Check for updates")!.click());
  expect(textOf(blind.container)).toContain(unknown);
});
