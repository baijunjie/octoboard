// @vitest-environment jsdom

import { expect, it, onTestFinished } from "vitest";

import { fakeTimers, runOutTimers } from "../../test/clock";
import { FADE_SIZE } from "./scrollFade";
import { watchScrollRowFade } from "./scrollRowFade";

const ROW_HEIGHT = FADE_SIZE + 8;
const VIEWPORT_HEIGHT = 3 * ROW_HEIGHT;
const TOP = 12;
const EDGE_MASK = "--scroll-row-fade-mask";
const CONTENT_MASK = "--scroll-row-content-mask";

function watch(rowCount: number) {
  const scroller = document.createElement("div");
  const content = document.createElement("div");
  const columns = [document.createElement("div"), document.createElement("div")];
  const rows = columns.map((column) => Array.from({ length: rowCount }, (_, index) => {
    const row = document.createElement("div");
    row.getBoundingClientRect = () => new DOMRect(0, TOP + index * ROW_HEIGHT - scroller.scrollTop, 100, ROW_HEIGHT);
    column.append(row);
    return row;
  }));
  content.append(...columns);
  scroller.append(content);
  Object.defineProperties(scroller, {
    clientHeight: { value: VIEWPORT_HEIGHT },
    scrollHeight: { value: rowCount * ROW_HEIGHT },
  });
  scroller.getBoundingClientRect = () => new DOMRect(0, TOP, 200, VIEWPORT_HEIGHT);

  fakeTimers();
  const stop = watchScrollRowFade(scroller, content, columns);
  onTestFinished(stop);
  return {
    rows,
    stop,
    scrollTo(top: number) {
      scroller.scrollTop = top;
      scroller.dispatchEvent(new Event("scroll"));
    },
  };
}

function expectFadedRows(columns: HTMLElement[][], expected: number[][]): void {
  for (const property of [EDGE_MASK, CONTENT_MASK]) {
    const fadedRows = columns.map((rows) =>
      rows.flatMap((row, index) => row.style.getPropertyValue(property) ? [index] : []),
    );
    expect(fadedRows).toEqual(expected);
  }
}

it("fades only rows at an edge that hides more lines, in the gutter and code", () => {
  const { rows, scrollTo } = watch(5);
  expectFadedRows(rows, [[2], [2]]);

  scrollTo(ROW_HEIGHT);
  runOutTimers();
  expectFadedRows(rows, [[1, 3], [1, 3]]);
  for (const column of rows) {
    expect(column[1].style.getPropertyValue(CONTENT_MASK)).toBe(
      `linear-gradient(to bottom, transparent 0, #000 ${FADE_SIZE}px, #000 calc(100% - ${FADE_SIZE}px), transparent 100%)`,
    );
    expect(column[1].style.getPropertyValue("--scroll-row-fade-offset")).toBe("0px");
    expect(column[3].style.getPropertyValue("--scroll-row-fade-offset")).toBe(`-${2 * ROW_HEIGHT}px`);
  }

  scrollTo(2 * ROW_HEIGHT);
  runOutTimers();
  expectFadedRows(rows, [[2], [2]]);
});

it("leaves rows fully visible when the content fits", () => {
  const { rows } = watch(3);
  expectFadedRows(rows, [[], []]);
});

it("clears the fade on teardown even with a scroll frame pending", () => {
  const { rows, scrollTo, stop } = watch(5);
  expectFadedRows(rows, [[2], [2]]);
  scrollTo(ROW_HEIGHT);
  stop();
  runOutTimers();
  expectFadedRows(rows, [[], []]);
  for (const column of rows) {
    expect(column[2].style.getPropertyValue("--scroll-row-fade-offset")).toBe("");
    expect(column[2].style.getPropertyValue("--scroll-row-fade-size")).toBe("");
  }
});
