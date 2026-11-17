// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { FIRST_WRITE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { activate } from "../components/hint/tooltipTestSupport";
import { fakeTimers } from "../test/clock";
import { mountHost } from "../test/mountHost";
import { BodyView } from "./BodyView";
import type { ViewerBody } from "./content";
import { load, stubScrollerLayout } from "./image/zoomTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => {
  stubScrollerLayout();
  fakeTimers();
});

const png = (id: string): ViewerBody => ({ kind: "image", mediaType: "image/png", url: `data:image/png;base64,${id}`, size: 4 });

function mount(resetKey: string, body: ViewerBody): HTMLElement & { render: (resetKey: string, body: ViewerBody) => void } {
  const element = (key: string, content: ViewerBody) => (
    <div role="dialog">
      <BodyView resetKey={key} name="photo.png" body={content} theme="light" />
    </div>
  );
  const container = mountHost(element(resetKey, body));
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  const renderInto = container.render;
  return Object.assign(container, { render: (key: string, content: ViewerBody) => renderInto(element(key, content)) }) as HTMLElement & { render: (resetKey: string, body: ViewerBody) => void };
}

const readout = (root: ParentNode) => root.querySelector('[role=group] span[aria-hidden="true"]')?.textContent;
const button = (root: ParentNode, name: string) => root.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`)!;

it("shows an image as a zoomable image: fitted once it has loaded", () => {
  const view = mount("a", png("a"));
  expect(readout(view)).toBeUndefined();
  load(view.querySelector("img")!, 2000, 1000);
  expect(readout(view)).toBe("50%");
  expect(view.querySelector("img")!.getAttribute("alt")).toBe("photo.png");
});

it("opens another image fitted, whatever the previous one was zoomed to", () => {
  const view = mount("a", png("a"));
  load(view.querySelector("img")!, 2000, 1000);
  activate(button(view, "Actual size"));
  expect(readout(view)).toBe("100%");

  view.render("b", png("b"));
  // A new image: not decoded yet, so no bar; then it opens fitted.
  expect(readout(view)).toBeUndefined();
  load(view.querySelector("img")!, 2000, 1000);
  expect(readout(view)).toBe("50%");
});

it("falls back to the empty-state failure when a raster image cannot be decoded, and does not carry it over to the next image", () => {
  const view = mount("a", png("a"));
  act(() => void view.querySelector("img")!.dispatchEvent(new Event("error")));
  expect(view.querySelector("img")).toBeNull();
  expect(view.querySelector('[data-slot="empty-state"]')?.textContent).toContain("This image cannot be displayed");
  expect(view.querySelector("[role=group]")).toBeNull();

  view.render("b", png("b"));
  expect(view.querySelector('[data-slot="empty-state"]')).toBeNull();
  expect(view.querySelector("img")).not.toBeNull();
});
