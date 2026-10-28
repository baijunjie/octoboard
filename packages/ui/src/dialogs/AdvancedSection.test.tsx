// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { AdvancedSection, AdvancedTabs } from "./AdvancedSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(children: React.ReactNode) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(children));
  return { container, render: (next: React.ReactNode) => act(() => root.render(next)) };
}

/** The rule of `style.css` for `selector`, read from the source as jsdom loads no stylesheet (by directory: under
 * jsdom `import.meta.url` is not a file URL). */
function styleRule(selector: string): string {
  const css = readFileSync(resolve(import.meta.dirname, "../style.css"), "utf8");
  const body = css.match(new RegExp(`^${selector.replace(/[.[\]"*]/g, "\\$&")} \\{([^}]*)\\}`, "m"))?.[1];
  expect(body, `style.css rule ${selector}`).toBeDefined();
  return `${selector} {${body}}`;
}

// Neither Chrome nor WebKit reports the end of a transition started in content it skips, and react-aria's
// `runAfterTransition` holds every deferred focus move on the page until none is running. A tab indicator that takes
// over the place of an earlier one starts one even while folded away (as each one does under StrictMode, which mounts
// it twice), so `style.css`'s rule for folded content must keep it from sliding while the section is collapsed.
it("keeps the tab indicator of a collapsed section from sliding, and gives it its slide back once open", () => {
  const style = document.head.appendChild(document.createElement("style"));
  onTestFinished(() => style.remove());
  // HeroUI's indicator transition (`tabs.css`), ahead of the app's rules as its layer puts it.
  style.textContent = `.tabs__indicator { transition-property: translate; transition-duration: 250ms; }\n${styleRule('[hidden="until-found"] *')}`;
  const section = (expanded: boolean) => (
    <AdvancedSection expanded={expanded} onExpandedChange={() => {}}>
      <AdvancedTabs tabs={[{ id: "a", label: "A", content: null }, { id: "b", label: "B", content: null }]} />
    </AdvancedSection>
  );
  const { container, render } = mount(section(false));
  const indicator = () => container.querySelector<HTMLElement>(".tabs__indicator")!;
  expect(indicator().closest('[hidden="until-found"]'), "the indicator is folded away").not.toBeNull();
  expect(getComputedStyle(indicator()).transitionProperty).toBe("none");
  render(section(true));
  expect(indicator().closest("[hidden]")).toBeNull();
  expect(getComputedStyle(indicator()).transitionProperty).toBe("translate");
});
