// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { currentLanguage, setLanguage } from "../i18n/language";
import { LanguageProvider } from "../i18n/react";
import { DEFAULT_SETTINGS } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { LanguageSetting } from "./LanguageSetting";
import { errorAlerts } from "../components/alerts/errorAlerts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);
const raiseError = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});

beforeEach(() => {
  request.mockClear();
  raiseError.mockClear();
  setLanguage("en");
});

function mount(held: string | null = null) {
  const container = mountHost(
    <LanguageProvider>
      <LanguageSetting />
    </LanguageProvider>,
    { daemon: testDaemon({ request, state: { settings: { ...DEFAULT_SETTINGS, interface_language: held } } }) },
  );
  return container.querySelector("select")!;
}

function pick(select: HTMLSelectElement, value: string) {
  act(() => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

it("shows the language the user picks at once and sends it to the daemon", () => {
  const select = mount();
  pick(select, "zh-Hans");
  expect(currentLanguage()).toBe("zh-Hans");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", interface_language: "zh-Hans" });
});

it("takes back a language the daemon refuses, to the one it holds, and tells the user", async () => {
  request.mockImplementationOnce(() => Promise.reject(new Error("refused")));
  const select = mount("de");
  setLanguage("en");
  await act(async () => pick(select, "ja"));
  expect(currentLanguage()).toBe("de");
  expect(raiseError).toHaveBeenCalledOnce();
});

it("takes back a refused pick to the previous language when the daemon holds none", async () => {
  request.mockImplementationOnce(() => Promise.reject(new Error("refused")));
  const select = mount(null);
  await act(async () => pick(select, "ja"));
  expect(currentLanguage()).toBe("en");
  expect(raiseError).toHaveBeenCalledOnce();
});
