// @vitest-environment jsdom
import React, { act } from "react";
import { expect, it, vi } from "vitest";

import { type PrimitiveLabels, PrimitiveLabelsProvider } from "./labels";
import { partSelector } from "./parts";
import { Tab, TabList, TabListContainer, Tabs } from "./Tabs";
import { mount } from "./testSupport";

type Orientation = "vertical" | "horizontal";

const FIRST: PrimitiveLabels = { scrollTabs: { up: "Up 1", down: "Down 1", left: "Left 1", right: "Right 1" } };
const SECOND: PrimitiveLabels = { scrollTabs: { up: "Up 2", down: "Down 2", left: "Left 2", right: "Right 2" } };

function Strip({ orientation, labels }: { orientation: Orientation; labels: PrimitiveLabels }): React.ReactElement {
  return (
    <PrimitiveLabelsProvider labels={labels}>
      <Tabs orientation={orientation} defaultSelectedKey="one">
        <TabListContainer>
          <TabList label="sections">
            <Tab id="one">One</Tab>
            <Tab id="two">Two</Tab>
          </TabList>
        </TabListContainer>
      </Tabs>
    </PrimitiveLabelsProvider>
  );
}

function mountStrip(orientation: Orientation, labels: PrimitiveLabels) {
  const { container, render } = mount(<Strip orientation={orientation} labels={labels} />);
  return { container, rerender: (next: Orientation, nextLabels: PrimitiveLabels) => render(<Strip orientation={next} labels={nextLabels} />) };
}

it.each([
  ["vertical", ["Up 1", "Down 1"], ["Up 2", "Down 2"]],
  ["horizontal", ["Left 1", "Right 1"], ["Left 2", "Right 2"]],
] as const)("names the real HeroUI %s arrows with the provided labels and preserves scrolling across label and orientation changes", (orientation, first, second) => {
  const { container, rerender } = mountStrip(orientation, FIRST);
  const buttons = Array.from(container.querySelectorAll<HTMLButtonElement>(`${partSelector.tabListContainer} > button`));
  expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual(first);
  const scroller = container.querySelector<HTMLElement>(partSelector.scrollShadow)!;
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
  rerender(orientation, SECOND);
  expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual(second);
  expect(buttons.map((button) => button.tabIndex)).toEqual([-1, -1]);
  rerender(orientation === "vertical" ? "horizontal" : "vertical", SECOND);
  expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual(
    orientation === "vertical" ? ["Left 2", "Right 2"] : ["Up 2", "Down 2"],
  );
});
