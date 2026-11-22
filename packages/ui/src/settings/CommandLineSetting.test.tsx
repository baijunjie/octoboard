// @vitest-environment jsdom
import "../input/focusGuard";
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { LanguageProvider } from "../i18n/react";
import { CommandLineError, type CommandLineCapability, type CommandLineState, type PlatformAdapter } from "../platform";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { CommandLineSetting } from "./CommandLineSetting";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => acknowledgeAllAlerts());

function mount(commandLine: CommandLineCapability) {
  const platform: PlatformAdapter = { kind: "tauri", host: MAC_HOST, commandLine };
  return mountHost(
    <LanguageProvider>
      <CommandLineSetting />
    </LanguageProvider>,
    { platform },
  );
}

const capability = (state: CommandLineState, extra: Partial<CommandLineCapability> = {}): CommandLineCapability => ({
  status: async () => state,
  install: vi.fn(async () => state),
  remove: vi.fn(async () => ({ state: "notInstalled" }) as const),
  ...extra,
});

const buttons = (container: HTMLElement) => [...container.querySelectorAll("button")].map((button) => button.textContent);

it("offers Install alone when the tool is not installed", async () => {
  const container = mount(capability({ state: "notInstalled" }));
  await act(async () => {});
  expect(container.textContent).toContain("Not installed.");
  expect(buttons(container)).toEqual(["Install"]);
});

it("says what installing adds and where before the tool is installed", async () => {
  const container = mount(capability({ state: "notInstalled" }));
  await act(async () => {});
  const said = container.textContent ?? "";
  expect(said).toContain("Installing adds a link named");
  // Both places the shell may put the link, the system's first.
  expect(said.indexOf("/usr/local/bin")).toBeGreaterThan(said.indexOf("Installing adds a link named"));
  expect(said.indexOf("~/.local/bin")).toBeGreaterThan(said.indexOf("/usr/local/bin"));
});

it("does not say what installing adds once the tool is installed", async () => {
  const container = mount(capability({ state: "installed", path: "/usr/local/bin/octoboardd", pathNote: false }));
  await act(async () => {});
  expect(container.textContent).not.toContain("Installing adds a link named");
});

it.each<{ state: CommandLineState; text: string }>([
  { state: { state: "installed", path: "/usr/local/bin/octoboardd", pathNote: false }, text: "Installed at /usr/local/bin/octoboardd." },
  { state: { state: "otherCopy", path: "/usr/local/bin/octoboardd" }, text: "runs another copy" },
  { state: { state: "unresolved", path: "/usr/local/bin/octoboardd" }, text: "is no longer there" },
])("says where it stands when it is $state.state, and offers Remove and Reinstall", async ({ state, text }) => {
  const container = mount(capability(state));
  await act(async () => {});
  expect(container.textContent).toContain(text);
  expect(buttons(container)).toEqual(["Remove", "Reinstall"]);
});

it("says to add the directory to the PATH when the link is in the user's own", async () => {
  const container = mount(capability({ state: "installed", path: "/Users/me/.local/bin/octoboardd", pathNote: true }));
  await act(async () => {});
  expect(container.textContent).toContain("to your PATH");
});

it("shows the state an install answers", async () => {
  const installed: CommandLineState = { state: "installed", path: "/usr/local/bin/octoboardd", pathNote: false };
  const container = mount(capability({ state: "notInstalled" }, { install: vi.fn(async () => installed) }));
  await act(async () => {});
  await act(async () => container.querySelector("button")!.click());
  expect(container.textContent).toContain("Installed at /usr/local/bin/octoboardd.");
});

it("offers nothing where the platform has no command-line tool", async () => {
  const platform: PlatformAdapter = { kind: "tauri", host: MAC_HOST };
  const container = mountHost(
    <LanguageProvider>
      <CommandLineSetting />
    </LanguageProvider>,
    { platform },
  );
  await act(async () => {});
  expect(container.querySelector("[data-settings-search-id]")).toBeNull();
  expect(container.querySelector("button")).toBeNull();
});

it("removes the link and says the tool is not installed, then offers Install alone", async () => {
  const installed: CommandLineState = { state: "installed", path: "/usr/local/bin/octoboardd", pathNote: false };
  const setting = capability(installed);
  const container = mount(setting);
  await act(async () => {});
  expect(buttons(container)).toEqual(["Remove", "Reinstall"]);
  await act(async () => container.querySelector("button")!.click());
  expect(setting.remove).toHaveBeenCalledTimes(1);
  expect(setting.install).not.toHaveBeenCalled();
  expect(container.textContent).toContain("Not installed.");
  expect(buttons(container)).toEqual(["Install"]);
});

it("reads the state when it opens and again each time the window gets focus back", async () => {
  const status = vi
    .fn<CommandLineCapability["status"]>()
    .mockResolvedValueOnce({ state: "notInstalled" })
    .mockResolvedValue({ state: "installed", path: "/usr/local/bin/octoboardd", pathNote: false });
  const container = mount(capability({ state: "notInstalled" }, { status }));
  await act(async () => {});
  expect(status).toHaveBeenCalledTimes(1);
  expect(container.textContent).toContain("Not installed.");
  // Someone made the link from a terminal while the window was in the background.
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
  });
  expect(status).toHaveBeenCalledTimes(2);
  expect(container.textContent).toContain("Installed at /usr/local/bin/octoboardd.");
  expect(buttons(container)).toEqual(["Remove", "Reinstall"]);
});

it.each([
  { error: new CommandLineError("refused", "/usr/local/bin/octoboardd", "User canceled."), words: "did not allow the change to /usr/local/bin/octoboardd" },
  { error: new CommandLineError("not_a_link", "/usr/local/bin/octoboardd", "is a directory"), words: "/usr/local/bin/octoboardd is not a link" },
  { error: new CommandLineError("foreign_link", "/usr/local/bin/octoboardd", "/opt/other/bin/tool"), words: "is a link to /opt/other/bin/tool, not to this app" },
  { error: new CommandLineError("failed", null, "disk full"), words: "The change could not be made." },
  { error: new Error("no way"), words: "The change could not be made." },
])("says a failure in the row, as an alert and no dialog, keeping the state that stood: $error", async ({ error, words }) => {
  const install = vi.fn<CommandLineCapability["install"]>().mockRejectedValue(error);
  const container = mount(capability({ state: "notInstalled" }, { install }));
  await act(async () => {});
  await act(async () => container.querySelector("button")!.click());
  expect(container.textContent).toContain("Could not install the command-line tool");
  expect(container.textContent).toContain(words);
  expect(container.textContent).toContain("Not installed.");
  expect(document.querySelector('[role="dialog"], [role="alertdialog"]')).toBeNull();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("keeps the shell's own words behind Details", async () => {
  const install = vi.fn<CommandLineCapability["install"]>().mockRejectedValue(new CommandLineError("failed", null, "disk full"));
  const container = mount(capability({ state: "notInstalled" }, { install }));
  await act(async () => {});
  await act(async () => container.querySelector("button")!.click());
  expect(container.textContent).not.toContain("disk full");
  await act(async () => [...container.querySelectorAll("button")].find((button) => button.textContent === "Details")!.click());
  expect(document.body.textContent).toContain("disk full");
});

it("says when the state could not be read, and still offers Install", async () => {
  const status = vi.fn<CommandLineCapability["status"]>().mockRejectedValue(new Error("unreadable"));
  const container = mount(capability({ state: "notInstalled" }, { status }));
  await act(async () => {});
  expect(container.textContent).toContain("Could not tell whether the tool is installed.");
  expect(buttons(container)).toEqual(["Install"]);
  expect([...container.querySelectorAll("button")].some((button) => button.disabled)).toBe(false);
});

it("makes the pressed button pending and the other disabled while a change is out, and neither once it ends", async () => {
  let finish: (state: CommandLineState) => void = () => {};
  const remove = vi.fn<CommandLineCapability["remove"]>(() => new Promise((resolve) => (finish = resolve)));
  const container = mount(capability({ state: "installed", path: "/usr/local/bin/octoboardd", pathNote: false }, { remove }));
  await act(async () => {});
  const [removeButton, reinstallButton] = [...container.querySelectorAll("button")];
  await act(async () => removeButton!.click());
  expect(removeButton!.getAttribute("aria-busy")).toBe("true");
  expect(reinstallButton!.disabled).toBe(true);
  await act(async () => finish({ state: "notInstalled" }));
  expect([...container.querySelectorAll("button")].some((button) => button.disabled || button.getAttribute("aria-busy") === "true")).toBe(false);
});

it("drops the answer of a read that a change overtook, and reads nothing while one is out", async () => {
  let answerRead: (state: CommandLineState) => void = () => {};
  let finish: (state: CommandLineState) => void = () => {};
  const status = vi.fn<CommandLineCapability["status"]>().mockResolvedValueOnce({ state: "notInstalled" }).mockImplementationOnce(() => new Promise((resolve) => (answerRead = resolve)));
  const install = vi.fn<CommandLineCapability["install"]>(() => new Promise((resolve) => (finish = resolve)));
  const container = mount(capability({ state: "notInstalled" }, { status, install }));
  await act(async () => {});
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
  });
  await act(async () => container.querySelector("button")!.click());
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
  });
  expect(status).toHaveBeenCalledTimes(2);
  await act(async () => answerRead({ state: "unresolved", path: "/stale" }));
  expect(container.textContent).not.toContain("/stale");
  await act(async () => finish({ state: "installed", path: "/usr/local/bin/octoboardd", pathNote: false }));
  expect(container.textContent).toContain("Installed at /usr/local/bin/octoboardd.");
});
