// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { DiagnosticBlock } from "./DiagnosticBlock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(wrap: boolean, tall = false) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<LanguageProvider><DiagnosticBlock wrap={wrap} tall={tall}>{"a long message ".repeat(20)}</DiagnosticBlock></LanguageProvider>));
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

it("scrolls a wrapped block asked to be tall only vertically, within its height limit", () => {
  const pre = mount(true, true).querySelector("pre")!;
  expect(pre.className).toContain("whitespace-pre-wrap");
  const scroller = pre.closest<HTMLElement>("[data-slot=scroll-shadow]")!;
  expect(scroller.className).toContain("max-h-[40vh]");
  expect(scroller.className).toContain("overflow-y-auto");
  expect(scroller.className).toContain("overflow-x-hidden");
});
