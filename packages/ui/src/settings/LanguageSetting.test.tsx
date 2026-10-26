// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { currentLanguage, setLanguage } from "../i18n/language";
import { LanguageProvider } from "../i18n/react";
import { LanguageSetting } from "./LanguageSetting";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);
const toastError = vi.fn();

vi.mock("../store", () => ({
  useDaemon: () => ({ request, toastError }),
}));

beforeEach(() => {
  request.mockClear();
  toastError.mockClear();
  setLanguage("en");
});

function mount() {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <LanguageProvider>
        <LanguageSetting />
      </LanguageProvider>,
    ),
  );
  return container.querySelector("select")!;
}

function pick(select: HTMLSelectElement, value: string) {
  act(() => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

it("reports the language the user picks to the daemon as it changes the interface", () => {
  const select = mount();
  pick(select, "zh-Hans");
  expect(currentLanguage()).toBe("zh-Hans");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", interface_language: "zh-Hans" });
});

it("tells the user when the daemon refuses the language", async () => {
  request.mockImplementationOnce(() => Promise.reject(new Error("refused")));
  const select = mount();
  await act(async () => pick(select, "ja"));
  expect(currentLanguage()).toBe("ja");
  expect(toastError).toHaveBeenCalledOnce();
});
