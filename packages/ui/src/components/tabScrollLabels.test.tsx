// @vitest-environment jsdom
import { Tabs } from "@heroui/react";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { setLanguage } from "../i18n/language";
import { LanguageProvider } from "../i18n/react";
import { useTabScrollLabels } from "./tabScrollLabels";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Strip({ orientation }: { orientation: "vertical" | "horizontal" }): React.ReactElement {
  const tabScrollLabels = useTabScrollLabels();
  return (
    <Tabs orientation={orientation} defaultSelectedKey="one">
      <Tabs.ListContainer render={(props) => <div {...props}>{tabScrollLabels(props.children)}</div>}>
        <Tabs.List aria-label="sections">
          <Tabs.Tab id="one">One</Tabs.Tab>
          <Tabs.Tab id="two">Two</Tabs.Tab>
        </Tabs.List>
      </Tabs.ListContainer>
    </Tabs>
  );
}

function mount(orientation: "vertical" | "horizontal"): { container: HTMLDivElement; rerender: (orientation: "vertical" | "horizontal") => void } {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => { root.unmount(); setLanguage("en"); });
    container.remove();
    vi.unstubAllGlobals();
  });
  const rerender = (orientation: "vertical" | "horizontal") => act(() => root.render(<LanguageProvider><Strip orientation={orientation} /></LanguageProvider>));
  act(() => setLanguage("zh-Hans"));
  rerender(orientation);
  return { container, rerender };
}

it.each([
  ["vertical", ["向上滚动标签页", "向下滚动标签页"], ["Scroll tabs up", "Scroll tabs down"]],
  ["horizontal", ["向左滚动标签页", "向右滚动标签页"], ["Scroll tabs left", "Scroll tabs right"]],
] as const)("localizes the real HeroUI %s arrows and preserves scrolling across language and orientation changes", (orientation, chinese, english) => {
  const { container, rerender } = mount(orientation);
  const buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("[data-slot=tabs-list-container] > button"));
  expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual(chinese);
  const scroller = container.querySelector<HTMLElement>("[data-slot=scroll-shadow]")!;
  expect(scroller.querySelector("[role=tablist]")).not.toBeNull();
  const scrollTo = vi.fn();
  Object.defineProperties(scroller, {
    clientWidth: { value: 100 }, clientHeight: { value: 100 },
    scrollWidth: { value: 500 }, scrollHeight: { value: 500 },
    scrollTo: { value: scrollTo },
  });
  scroller.scrollLeft = scroller.scrollTop = 100;
  act(() => { buttons[0].click(); buttons[1].click(); });
  const axis = orientation === "vertical" ? "top" : "left";
  expect(scrollTo.mock.calls).toEqual([[{ behavior: "smooth", [axis]: 20 }], [{ behavior: "smooth", [axis]: 180 }]]);
  act(() => setLanguage("en"));
  expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual(english);
  expect(buttons.map((button) => button.tabIndex)).toEqual([-1, -1]);
  rerender(orientation === "vertical" ? "horizontal" : "vertical");
  expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual(
    orientation === "vertical" ? ["Scroll tabs left", "Scroll tabs right"] : ["Scroll tabs up", "Scroll tabs down"],
  );
});
