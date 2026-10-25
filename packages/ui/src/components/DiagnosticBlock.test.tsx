// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { DiagnosticBlock } from "./DiagnosticBlock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(wrap: boolean) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<LanguageProvider><DiagnosticBlock wrap={wrap}>{"a long message ".repeat(20)}</DiagnosticBlock></LanguageProvider>));
  return container;
}

it("wraps a block asked to, which has no horizontal scroller to name", () => {
  const pre = mount(true).querySelector("pre")!;
  expect(pre.className).toContain("whitespace-pre-wrap");
  expect(pre.parentElement?.closest("[class*=overflow-x-auto]")).toBeNull();
});

it("keeps a block unwrapped by default, inside its scroller", () => {
  const pre = mount(false).querySelector("pre")!;
  expect(pre.className).toContain("whitespace-pre");
  expect(pre.className).not.toContain("whitespace-pre-wrap");
  expect(pre.closest("[class*=overflow-x-auto]")).not.toBeNull();
});
