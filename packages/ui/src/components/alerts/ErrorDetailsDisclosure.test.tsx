// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers, runOutTimers } from "../../test/clock";
import { mountHost } from "../../test/mountHost";
import { DiagnosticBlock } from "./DiagnosticBlock";
import { DetailsDisclosure } from "./ErrorDetailsDisclosure";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(ui: React.ReactElement): HTMLElement {
  // Before anything mounts: the frame HeroUI's overflow watch reports from is armed on the faked clock.
  fakeTimers();
  return mountHost(ui);
}

/** Has every element report more content than room, so a diagnostic block's scroller becomes a named region. */
function overflowing(): void {
  for (const [key, value] of Object.entries({ clientWidth: 100, clientHeight: 100, scrollWidth: 300, scrollHeight: 300 })) {
    const mock = vi.spyOn(HTMLElement.prototype, key as "clientWidth", "get").mockReturnValue(value);
    onTestFinished(() => mock.mockRestore());
  }
}

async function settle(): Promise<void> {
  await act(async () => {});
  runOutTimers();
}

const triggers = (container: HTMLElement) => [...container.querySelectorAll<HTMLButtonElement>("button[aria-expanded]")];

it("is a button named Error details, collapsed to begin with, that opens what the caller laid out", async () => {
  const container = mount(<DetailsDisclosure><p>the system said so</p></DetailsDisclosure>);
  const [trigger] = triggers(container);
  expect(triggers(container)).toHaveLength(1);
  expect(trigger.textContent).toBe("Error details");
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  expect(container.querySelector("[data-slot=disclosure-content]")!.hasAttribute("hidden")).toBe(true);
  await act(async () => trigger.click());
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  expect(container.querySelector("p")?.textContent).toBe("the system said so");
  expect(container.querySelector("[data-slot=disclosure-content]")!.hasAttribute("hidden")).toBe(false);
});

it("sets what it holds flush with what stands above, as a code block, and not indented past the triangle", () => {
  const container = mount(<DetailsDisclosure><p>the system said so</p></DetailsDisclosure>);
  const body = container.querySelector(".disclosure__body")!;
  expect(body.hasAttribute("data-indent")).toBe(false);
  expect(body.hasAttribute("data-flush")).toBe(true);
});

it("names the region of a diagnostic block inside it by its own trigger, without the caller passing an id", async () => {
  overflowing();
  const container = mount(<DetailsDisclosure><DiagnosticBlock>{"Error: boom\n    at run (x.ts:1:1)"}</DiagnosticBlock></DetailsDisclosure>);
  const [trigger] = triggers(container);
  await act(async () => trigger.click());
  await settle();
  const region = container.querySelector<HTMLElement>("[data-slot=scroll-shadow]")!;
  expect(trigger.id).not.toBe("");
  expect(region.getAttribute("aria-labelledby")).toBe(trigger.id);
  expect(region.hasAttribute("aria-label")).toBe(false);
  expect(document.getElementById(region.getAttribute("aria-labelledby")!)).toBe(trigger);
});

it("labels each of several diagnostic blocks inside it by the trigger, and leaves a block with a label of its own to it", async () => {
  overflowing();
  const container = mount(
    <DetailsDisclosure>
      <DiagnosticBlock>first</DiagnosticBlock>
      <h3 id="second-heading">Session b</h3>
      <DiagnosticBlock labelledBy="second-heading">second</DiagnosticBlock>
    </DetailsDisclosure>,
  );
  const [trigger] = triggers(container);
  await act(async () => trigger.click());
  await settle();
  const regions = [...container.querySelectorAll<HTMLElement>("[data-slot=scroll-shadow]")];
  expect(regions.map((region) => region.getAttribute("aria-labelledby"))).toEqual([trigger.id, "second-heading"]);
});

it("gives each disclosure its own trigger id, so a block is named by the disclosure it stands in", async () => {
  overflowing();
  const container = mount(
    <>
      <DetailsDisclosure><DiagnosticBlock>first</DiagnosticBlock></DetailsDisclosure>
      <DetailsDisclosure><DiagnosticBlock>second</DiagnosticBlock></DetailsDisclosure>
    </>,
  );
  const [first, second] = triggers(container);
  await act(async () => first.click());
  await act(async () => second.click());
  await settle();
  expect(first.id).not.toBe("");
  expect(second.id).not.toBe("");
  expect(first.id).not.toBe(second.id);
  const regions = [...container.querySelectorAll<HTMLElement>("[data-slot=scroll-shadow]")];
  expect(regions.map((region) => region.getAttribute("aria-labelledby"))).toEqual([first.id, second.id]);
});
