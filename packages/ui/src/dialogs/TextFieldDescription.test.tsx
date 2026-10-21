// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { currentLanguage, setLanguage, t } from "../i18n/language";
import { TextFieldDescription } from "./TextFieldDescription";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it.each(["en", "zh-Hans", "ar"] as const)("keeps only the drawn ratio LTR in %s while retaining the description and count threshold", (language) => {
  const previous = currentLanguage();
  setLanguage(language);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); setLanguage(previous); });
  const render = (value: string) => act(() => root.render(<TextFieldDescription value={value} characterLimit={10} description="Description" />));
  render("short");
  expect(container.textContent).toBe("Description");
  expect(container.querySelector("bdi")).toBeNull();
  render("123456789");
  const ratio = container.querySelector("bdi")!;
  expect(ratio.getAttribute("dir")).toBe("ltr");
  expect(ratio.textContent).toBe(t("dialog.characterRatio", { count: 9, limit: 10 }));
  expect(ratio.parentElement?.hasAttribute("dir")).toBe(false);
  expect(container.textContent).toBe(`Description${t("dialog.characterCount", { ratio: ratio.textContent! })}`);
  expect(document.documentElement.dir).toBe(language === "ar" ? "rtl" : "ltr");
});
