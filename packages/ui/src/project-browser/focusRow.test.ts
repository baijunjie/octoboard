// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";

import { fakeTimers, runOutTimers } from "../test/clock";
import { focusedRowKey, focusRow, focusRowBeforeFold } from "./focusRow";

afterEach(() => {
  document.body.replaceChildren();
});

/** A row of `list` with `key`, holding a button, focusable by script. */
function addRow(list: HTMLElement, key: string): HTMLElement {
  const row = list.appendChild(document.createElement("div"));
  row.setAttribute("role", "row");
  row.dataset.key = key;
  row.tabIndex = -1;
  row.appendChild(document.createElement("button"));
  return row;
}

/** A list of rows with the given keys, attached to the document. */
function mountList(...keys: string[]): { list: HTMLElement; row: (key: string) => HTMLElement; control: (key: string) => HTMLButtonElement } {
  const list = document.body.appendChild(document.createElement("div"));
  list.setAttribute("role", "treegrid");
  keys.forEach((key) => addRow(list, key));
  const row = (key: string) => list.querySelector<HTMLElement>(`[role=row][data-key="${key}"]`)!;
  return { list, row, control: (key) => row(key).querySelector("button")! };
}

/** A button outside every list. */
function outsideButton(): HTMLButtonElement {
  return document.body.appendChild(document.createElement("button"));
}

describe("focusedRowKey", () => {
  it("names the row that holds focus itself", () => {
    const { list, row } = mountList("a", "b");
    row("b").focus();
    expect(focusedRowKey(list)).toBe("b");
  });

  it("names the row whose control holds focus", () => {
    const { list, control } = mountList("a", "b");
    control("a").focus();
    expect(focusedRowKey(list)).toBe("a");
  });

  it("gives nothing while focus is outside the list", () => {
    const { list } = mountList("a");
    const other = mountList("x");
    other.row("x").focus();
    expect(focusedRowKey(list)).toBeUndefined();
    outsideButton().focus();
    expect(focusedRowKey(list)).toBeUndefined();
  });

  it("gives nothing while nothing holds focus", () => {
    const { list } = mountList("a");
    expect(document.activeElement).toBe(document.body);
    expect(focusedRowKey(list)).toBeUndefined();
  });

  it("gives nothing without a list", () => {
    const { row } = mountList("a");
    row("a").focus();
    expect(focusedRowKey(null)).toBeUndefined();
  });

  it("gives nothing for a keyed element that is not a row", () => {
    const { list } = mountList("a");
    const keyed = list.appendChild(document.createElement("div"));
    keyed.dataset.key = "heading";
    keyed.tabIndex = -1;
    keyed.focus();
    expect(document.activeElement).toBe(keyed);
    expect(focusedRowKey(list)).toBeUndefined();
  });
});

describe("focusRowBeforeFold", () => {
  it("focuses a rendered row at once, before any frame or timer can run", () => {
    fakeTimers();
    const { list, row } = mountList("a", "b");
    row("a").focus();
    focusRowBeforeFold(list, "b");
    expect(document.activeElement).toBe(row("b"));
  });

  it("focuses a rendered row from a control inside another row", () => {
    fakeTimers();
    const { list, row, control } = mountList("a", "b");
    control("a").focus();
    focusRowBeforeFold(list, "b");
    expect(document.activeElement).toBe(row("b"));
  });

  it("leaves focus where it is until the collection settles when the row is not rendered yet, then focuses it once it is", () => {
    fakeTimers();
    const { list, control } = mountList("a");
    control("a").focus();
    focusRowBeforeFold(list, "late");
    expect(document.activeElement).toBe(control("a"));
    const late = addRow(list, "late");
    runOutTimers();
    expect(document.activeElement).toBe(late);
  });

  it("does not take focus the user put outside the list while it waits for the row", () => {
    fakeTimers();
    const { list } = mountList("a");
    focusRowBeforeFold(list, "late");
    const outside = outsideButton();
    outside.focus();
    addRow(list, "late");
    runOutTimers();
    expect(document.activeElement).toBe(outside);
  });

  it("takes focus from the page when the row turns up after the wait", () => {
    fakeTimers();
    const { list } = mountList("a");
    expect(document.activeElement).toBe(document.body);
    focusRowBeforeFold(list, "late");
    const late = addRow(list, "late");
    runOutTimers();
    expect(document.activeElement).toBe(late);
  });

  it("does nothing when the row never turns up, or without a list", () => {
    fakeTimers();
    const { list, control } = mountList("a");
    control("a").focus();
    expect(() => focusRowBeforeFold(list, "missing")).not.toThrow();
    expect(() => focusRowBeforeFold(null, "missing")).not.toThrow();
    runOutTimers();
    expect(document.activeElement).toBe(control("a"));
  });

  describe("with a way to scroll the row into view", () => {
    /** A list whose virtualizer renders the row `key` only in answer to a scroll event, as react-aria's does once a
     * scroll has brought the row into view; `scrolls` records each call of the scroll function, by whether it was for
     * the rows as they stand after the fold. */
    function virtualized(key: string) {
      const { list, row, control } = mountList("a");
      const scrolls: boolean[] = [];
      const scrollIntoView = (afterFold: boolean) => void scrolls.push(afterFold);
      list.addEventListener("scroll", () => {
        if (scrolls.length > 0 && !list.querySelector(`[data-key="${key}"]`)) addRow(list, key);
      });
      return { list, row, control, scrolls, scrollIntoView };
    }

    it("scrolls a row not rendered into view and focuses it at once, before any frame or timer can run", () => {
      fakeTimers();
      const { list, row, control, scrolls, scrollIntoView } = virtualized("far");
      control("a").focus();
      focusRowBeforeFold(list, "far", scrollIntoView);
      expect(scrolls).toEqual([false]);
      expect(document.activeElement).toBe(row("far"));
    });

    it("scrolls the row into view again once the collection has settled after the fold", () => {
      fakeTimers();
      const { list, row, scrolls, scrollIntoView } = virtualized("far");
      row("a").focus();
      focusRowBeforeFold(list, "far", scrollIntoView);
      runOutTimers();
      expect(scrolls).toEqual([false, true]);
      expect(document.activeElement).toBe(row("far"));
    });

    it("scrolls nothing before the fold for a rendered row, and keeps it in view after it", () => {
      fakeTimers();
      const { list, row, scrolls, scrollIntoView } = virtualized("b");
      addRow(list, "b");
      row("a").focus();
      focusRowBeforeFold(list, "b", scrollIntoView);
      expect(document.activeElement).toBe(row("b"));
      expect(scrolls).toEqual([]);
      runOutTimers();
      expect(scrolls).toEqual([true]);
    });

    it("does not scroll after the fold once focus has left the row", () => {
      fakeTimers();
      const { list, row, scrolls, scrollIntoView } = virtualized("far");
      row("a").focus();
      focusRowBeforeFold(list, "far", scrollIntoView);
      outsideButton().focus();
      runOutTimers();
      expect(scrolls).toEqual([false]);
    });

    it("waits for the collection to settle when the scroll still leaves the row unrendered", () => {
      fakeTimers();
      const { list, control } = mountList("a");
      const scrolls: boolean[] = [];
      control("a").focus();
      focusRowBeforeFold(list, "late", (afterFold) => void scrolls.push(afterFold));
      expect(document.activeElement).toBe(control("a"));
      const late = addRow(list, "late");
      runOutTimers();
      expect(document.activeElement).toBe(late);
      expect(scrolls).toEqual([false, true]);
    });
  });
});

describe("focusRow", () => {
  it("does not take focus at once, only once the collection has settled", () => {
    fakeTimers();
    const { list, row } = mountList("a", "b");
    row("a").focus();
    focusRow(list, "b");
    expect(document.activeElement).toBe(row("a"));
    runOutTimers();
    expect(document.activeElement).toBe(row("b"));
  });
});
