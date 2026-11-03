// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { FADE_SIZE } from "../components/scroll/scrollFade";
import { ROW_HEIGHT, scrollRowIntoView } from "./treeRow";

function listAt(scrollTop: number, clientHeight = 300): HTMLElement {
  const list = document.createElement("div");
  Object.defineProperty(list, "clientHeight", { value: clientHeight });
  list.scrollTop = scrollTop;
  return list;
}

describe("scrollRowIntoView", () => {
  it("leaves a row that is in view, clear of the fades, where it is", () => {
    const list = listAt(100);
    scrollRowIntoView(list, 100 + FADE_SIZE);
    expect(list.scrollTop).toBe(100);
  });

  it("scrolls up until the row clears the fade at the top", () => {
    const list = listAt(500);
    scrollRowIntoView(list, 200);
    expect(list.scrollTop).toBe(200 - FADE_SIZE);
  });

  it("scrolls down until the row clears the fade at the bottom", () => {
    const list = listAt(0);
    scrollRowIntoView(list, 400);
    expect(list.scrollTop).toBe(400 + ROW_HEIGHT + FADE_SIZE - 300);
  });

  it("also clears a heading pinned at the top", () => {
    const list = listAt(500);
    scrollRowIntoView(list, 200, { stickyTop: 28 });
    expect(list.scrollTop).toBe(200 - 28 - FADE_SIZE);
  });
});
