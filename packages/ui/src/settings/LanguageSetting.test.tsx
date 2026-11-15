// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { currentLanguage, setLanguage } from "../i18n/language";
import { LanguageProvider } from "../i18n/react";
import { remoteDaemonName } from "../platform";
import { DEFAULT_SETTINGS } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { AgentLanguageSetting, InterfaceLanguageSetting } from "./LanguageSetting";
import { errorAlerts } from "../components/alerts/errorAlerts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);
const raiseError = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});

beforeEach(() => {
  request.mockClear();
  raiseError.mockClear();
  setLanguage("en");
});

/** Mounts both rows, the interface language's first, over a daemon that holds `held` as its agent language, and
 * returns their selects in that order. */
function mount(held: string | null = null) {
  const container = mountHost(
    <LanguageProvider>
      <InterfaceLanguageSetting />
      <AgentLanguageSetting />
    </LanguageProvider>,
    { daemon: testDaemon({ request, state: { settings: { ...DEFAULT_SETTINGS, agent_language: held } } }) },
  );
  const [interfaceSelect, agentSelect] = Array.from(container.querySelectorAll("select"));
  return { container, interfaceSelect, agentSelect };
}

function pick(select: HTMLSelectElement, value: string) {
  act(() => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

/** Makes this window the application's window of a daemon elsewhere, as the desktop shell opens it. */
function inRemoteWindow() {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  window.history.replaceState({}, "", "/?remote=server");
  onTestFinished(() => {
    window.history.replaceState({}, "", "/");
    vi.unstubAllGlobals();
  });
  expect(remoteDaemonName()).toBe("server");
}

it("shows the interface language row first and the agent language row below it, each described", () => {
  const { container } = mount("de");
  const text = container.textContent ?? "";
  expect(text.indexOf("Interface language")).toBeGreaterThanOrEqual(0);
  expect(text.indexOf("Interface language")).toBeLessThan(text.indexOf("Agent language"));
  expect(text).toContain("The language of the interface on this device.");
  expect(text).toContain("The language agents talk to you in.");
});

it("shows the interface language the user picks at once, and sends nothing to the daemon", () => {
  const { interfaceSelect } = mount("de");
  pick(interfaceSelect, "zh-Hans");
  expect(currentLanguage()).toBe("zh-Hans");
  expect(request).not.toHaveBeenCalled();
});

it("shows the agent language the daemon holds, whatever the interface language", () => {
  setLanguage("ja");
  const { interfaceSelect, agentSelect } = mount("de");
  expect(interfaceSelect.value).toBe("ja");
  expect(agentSelect.value).toBe("de");
});

it("shows the interface language as the agent language until the daemon holds one this build offers", () => {
  expect(mount(null).agentSelect.value).toBe("en");
});

it("sends a picked agent language to the daemon and leaves the interface language as it is", async () => {
  const { agentSelect } = mount("de");
  await act(async () => pick(agentSelect, "ko"));
  expect(request).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ type: "update_settings", agent_language: "ko" }));
  expect(currentLanguage()).toBe("en");
});

it("tells the user when the daemon refuses an agent language", async () => {
  request.mockImplementationOnce(() => Promise.reject(new Error("refused")));
  const { agentSelect } = mount("de");
  await act(async () => pick(agentSelect, "ja"));
  expect(raiseError).toHaveBeenCalledOnce();
  expect(currentLanguage()).toBe("en");
});

it("in the window of a daemon elsewhere, shows and changes that daemon's agent language, and keeps the interface language to the client", async () => {
  inRemoteWindow();
  const { interfaceSelect, agentSelect } = mount("fr");
  expect(agentSelect.value).toBe("fr");
  await act(async () => pick(agentSelect, "it"));
  expect(request).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ type: "update_settings", agent_language: "it" }));
  request.mockClear();
  pick(interfaceSelect, "de");
  expect(currentLanguage()).toBe("de");
  expect(request).not.toHaveBeenCalled();
});
