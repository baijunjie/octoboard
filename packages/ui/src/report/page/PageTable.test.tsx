// @vitest-environment jsdom
import { act } from "react";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import type { CellValue, PageContent } from "../../protocol";
import { fakeTimers, runOutTimers } from "../../test/clock";
import { click, type, withoutIsolates } from "../../test/dom";
import { renderPage, type PageOptions } from "./pageTestSupport";
import { sortedRows } from "./PageTable";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Table = Extract<PageContent["body"][number], { type: "table" }>;
type Cell = CellValue | null;

/** Makes every scroller overflow sideways, for the region a wide table is while it does. */
function overflowing(): void {
  fakeTimers();
  for (const [key, value] of Object.entries({ clientWidth: 100, clientHeight: 100, scrollWidth: 300, scrollHeight: 100 })) {
    const mock = vi.spyOn(HTMLElement.prototype, key as "clientWidth", "get").mockReturnValue(value);
    onTestFinished(() => mock.mockRestore());
  }
}

const renderTable = (table: Omit<Table, "type">, options?: PageOptions) => renderPage({ body: [{ type: "table", ...table }] }, options);

const headers = (host: ParentNode) => [...host.querySelectorAll("th")];
const headerNamed = (host: ParentNode, label: string) => headers(host).find((th) => th.textContent === label)!;
const sortButton = (host: ParentNode, label: string) => headerNamed(host, label).querySelector("button")!;
const bodyRows = (host: ParentNode) => [...host.querySelectorAll("tbody tr")].map((row) => [...row.querySelectorAll("td")].map((cell) => cell.textContent));
const column = (host: ParentNode, index: number) => bodyRows(host).map((row) => row[index]);
const filterField = (host: ParentNode) => host.querySelector<HTMLInputElement>('input[aria-label="Filter rows"], input[placeholder="Search"]')!;
const countText = (host: ParentNode) => [...host.querySelectorAll("p")].map((p) => p.textContent).find((text) => text?.startsWith("Rows shown"));

const NUMBERS: Omit<Table, "type"> = {
  columns: [{ label: "Name" }, { label: "Size" }],
  rows: [["c", 10], ["a", 2], ["empty", null], ["b", 5]],
};

describe("the table's rows", () => {
  it("shows the rows in the order given, with the caption and the column labels", async () => {
    const { host } = await renderTable({ ...NUMBERS, caption: "Files" });
    expect(host.querySelector("caption")?.textContent).toBe("Files");
    expect(headers(host).map((th) => th.textContent)).toEqual(["Name", "Size"]);
    expect(bodyRows(host)).toEqual([["c", "10"], ["a", "2"], ["empty", ""], ["b", "5"]]);
    for (const th of headers(host)) expect(th.hasAttribute("aria-sort")).toBe(false);
  });

  it("shows numbers grouped in the UI language and booleans as Yes and No", async () => {
    const { host } = await renderTable({ columns: [{ label: "N" }, { label: "B" }], rows: [[1234567.5, true], [0, false]] });
    expect(bodyRows(host)).toEqual([["1,234,567.5", "Yes"], ["0", "No"]]);
  });

  it("shows a row that is shorter than the columns with empty cells", async () => {
    const { host } = await renderTable({ columns: [{ label: "A" }, { label: "B" }], rows: [["x"]] });
    expect(bodyRows(host)).toEqual([["x", ""]]);
  });
});

describe("sorting by a column", () => {
  it("sorts ascending on the first press and descending on the next, exposed as aria-sort, a numeric column by value with empty cells last in both directions", async () => {
    const { host } = await renderTable(NUMBERS);
    await click(sortButton(host, "Size"));
    expect(headerNamed(host, "Size").getAttribute("aria-sort")).toBe("ascending");
    expect(column(host, 1)).toEqual(["2", "5", "10", ""]);
    await click(sortButton(host, "Size"));
    expect(headerNamed(host, "Size").getAttribute("aria-sort")).toBe("descending");
    expect(column(host, 1)).toEqual(["10", "5", "2", ""]);
    expect(headerNamed(host, "Name").hasAttribute("aria-sort")).toBe(false);
  });

  it("starts a newly pressed column ascending and clears the sort marker of the previous one", async () => {
    const { host } = await renderTable(NUMBERS);
    await click(sortButton(host, "Size"));
    await click(sortButton(host, "Size"));
    await click(sortButton(host, "Name"));
    expect(headerNamed(host, "Name").getAttribute("aria-sort")).toBe("ascending");
    expect(headerNamed(host, "Size").hasAttribute("aria-sort")).toBe(false);
    expect(column(host, 0)).toEqual(["a", "b", "c", "empty"]);
  });

  it("orders texts with the language's collation reading digit runs as numbers, empty cells last either way", async () => {
    const { host } = await renderTable({ columns: [{ label: "Item" }], rows: [["item 10"], [null], ["item 2"], ["item 1"]] });
    await click(sortButton(host, "Item"));
    expect(column(host, 0)).toEqual(["item 1", "item 2", "item 10", ""]);
    await click(sortButton(host, "Item"));
    expect(column(host, 0)).toEqual(["item 10", "item 2", "item 1", ""]);
  });

  it("puts false before true", async () => {
    const { host } = await renderTable({ columns: [{ label: "Done" }], rows: [[true], [false], [true], [false]] });
    await click(sortButton(host, "Done"));
    expect(column(host, 0)).toEqual(["No", "No", "Yes", "Yes"]);
    await click(sortButton(host, "Done"));
    expect(column(host, 0)).toEqual(["Yes", "Yes", "No", "No"]);
  });

  it("makes the headers plain labels, not sort buttons, when the table is not sortable", async () => {
    const { host } = await renderTable({ ...NUMBERS, sortable: false });
    expect(host.querySelectorAll("thead button")).toHaveLength(0);
    expect(headers(host).map((th) => th.textContent)).toEqual(["Name", "Size"]);
    expect(column(host, 0)).toEqual(["c", "a", "empty", "b"]);
  });

  it("sorts when the table is sortable by default and when sortable is true", async () => {
    const { host } = await renderTable({ ...NUMBERS, sortable: true });
    expect(host.querySelectorAll("thead button")).toHaveLength(2);
  });
});

describe("filtering the rows", () => {
  const PEOPLE: Omit<Table, "type"> = {
    columns: [{ label: "Name" }, { label: "Age" }, { label: "Active" }],
    rows: [["Ada", 36, true], ["Grace", 85, false], ["Linus", 1234, true], ["Nobody", null, null]],
  };

  it("keeps the rows where any cell's text contains the query, ignoring case, and counts the rows shown of all", async () => {
    const { host } = await renderTable(PEOPLE);
    expect(countText(host)).toBeUndefined();
    type(filterField(host), "ADA");
    expect(column(host, 0)).toEqual(["Ada"]);
    expect(countText(host)).toBe("Rows shown: 1 of 4");
    type(filterField(host), "a");
    expect(column(host, 0)).toEqual(["Ada", "Grace"]);
    expect(countText(host)).toBe("Rows shown: 2 of 4");
    type(filterField(host), "DY");
    expect(column(host, 0)).toEqual(["Nobody"]);
  });

  it("matches on the text as displayed: a grouped number and a Yes or No", async () => {
    const { host } = await renderTable(PEOPLE);
    type(filterField(host), "1,234");
    expect(column(host, 0)).toEqual(["Linus"]);
    type(filterField(host), "yes");
    expect(column(host, 0)).toEqual(["Ada", "Linus"]);
    type(filterField(host), "no");
    expect(column(host, 0)).toEqual(["Grace", "Nobody"]);
  });

  it("shows a no-rows-match row when nothing matches, and all the rows again once the filter is cleared", async () => {
    const { host } = await renderTable(PEOPLE);
    type(filterField(host), "zzz");
    expect(bodyRows(host)).toEqual([["No rows match"]]);
    expect(host.querySelector("tbody td")?.getAttribute("colspan")).toBe("3");
    expect(countText(host)).toBe("Rows shown: 0 of 4");
    type(filterField(host), "");
    expect(column(host, 0)).toEqual(["Ada", "Grace", "Linus", "Nobody"]);
    expect(countText(host)).toBeUndefined();
  });

  it("keeps the chosen sort order while filtering and after the filter is cleared", async () => {
    const { host } = await renderTable(PEOPLE);
    await click(sortButton(host, "Name"));
    await click(sortButton(host, "Name"));
    type(filterField(host), "a");
    expect(column(host, 0)).toEqual(["Grace", "Ada"]);
    type(filterField(host), "");
    expect(column(host, 0)).toEqual(["Nobody", "Linus", "Grace", "Ada"]);
  });

  it("has no filter field when the table is not filterable, and has one when filterable is true", async () => {
    const without = await renderTable({ ...PEOPLE, filterable: false });
    expect(without.host.querySelector("input")).toBeNull();
    const withField = await renderTable({ ...PEOPLE, filterable: true });
    expect(filterField(withField.host)).not.toBeNull();
  });
});

describe("the alignment of a column", () => {
  const endAligned = (host: ParentNode, index: number) => headers(host)[index].classList.contains("text-end");
  const cellsEndAligned = (host: ParentNode, index: number) => [...host.querySelectorAll("tbody tr")].map((row) => row.querySelectorAll("td")[index].classList.contains("text-end"));

  it("ends a column with no align whose non-empty cells are all numbers, header and cells, and starts every other", async () => {
    const { host } = await renderTable({
      columns: [{ label: "Name" }, { label: "Size" }, { label: "Mixed" }, { label: "Flag" }],
      rows: [["a", 1, 1, true], ["b", null, "x", false]],
    });
    expect(endAligned(host, 0)).toBe(false);
    expect(endAligned(host, 1)).toBe(true);
    expect(cellsEndAligned(host, 1)).toEqual([true, true]);
    expect(endAligned(host, 2)).toBe(false);
    expect(endAligned(host, 3)).toBe(false);
    expect(cellsEndAligned(host, 0)).toEqual([false, false]);
  });

  it("follows the align the page names, whatever the cells hold", async () => {
    const { host } = await renderTable({
      columns: [{ label: "Name", align: "end" }, { label: "Size", align: "start" }],
      rows: [["a", 1], ["b", 2]],
    });
    expect(endAligned(host, 0)).toBe(true);
    expect(endAligned(host, 1)).toBe(false);
  });
});

describe.each<[string, PageOptions]>([
  ["a history page", { history: true }],
  ["a submitted page", { submission: {} }],
  ["a page whose submission is out", { pending: true }],
])("on %s", (_, options) => {
  it("still sorts and filters", async () => {
    const { host } = await renderTable(NUMBERS, options);
    await click(sortButton(host, "Size"));
    expect(column(host, 1)).toEqual(["2", "5", "10", ""]);
    type(filterField(host), "b");
    expect(column(host, 0)).toEqual(["b"]);
  });
});

describe("several tables on one page", () => {
  /** A control's accessible name as the page gives it: its `aria-label`, else the text of its labels. */
  const nameOf = (element: Element) => element.getAttribute("aria-label")
    ?? [...((element as HTMLInputElement).labels ?? [])].map((label) => label.textContent).join(" ")
    ?? element.getAttribute("aria-labelledby");
  const filterNames = (host: ParentNode) => [...host.querySelectorAll("input")].map(nameOf);
  const sortNames = (host: ParentNode) => [...host.querySelectorAll("th button")].map(nameOf);
  const regionNames = (host: ParentNode) => [...host.querySelectorAll('[role="region"]')].map(nameOf);
  const table = (caption?: string): Table => ({ type: "table", columns: [{ label: "Name" }], rows: [["a"]], ...(caption === undefined ? {} : { caption }) });

  it("names each table's filter field, sort buttons and region after its caption, else its place among the tables", async () => {
    overflowing();
    const { host } = await renderPage({ body: [table("Files"), { type: "section", children: [table()] }, table()] });
    await act(async () => {});
    runOutTimers();
    expect(filterNames(host).map(withoutIsolates)).toEqual(["Filter rows: Files", "Filter rows: Table 2", "Filter rows: Table 3"]);
    expect(sortNames(host).map(withoutIsolates)).toEqual(["Name, Files", "Name, Table 2", "Name, Table 3"]);
    expect(regionNames(host)).toEqual(["Files", "Table 2", "Table 3"]);
    // The visible header text stays the column's own label.
    expect(headers(host).map((th) => th.textContent)).toEqual(["Name", "Name", "Name"]);
  });

  it("keeps the plain names for the only table of a page", async () => {
    overflowing();
    const { host } = await renderPage({ body: [table("Files")] });
    await act(async () => {});
    runOutTimers();
    expect(filterNames(host)).toEqual(["Filter rows"]);
    expect(sortButton(host, "Name").getAttribute("aria-label")).toBeNull();
    expect(regionNames(host)).toEqual(["Files"]);
  });
});

describe("sortedRows", () => {
  const collator = new Intl.Collator("en", { numeric: true });
  const rows: Cell[][] = [["b", 2], ["a", 2], [null, 1], ["c", null]];

  it("returns a copy in the given order when no sort is picked", () => {
    const result = sortedRows(rows, undefined, collator);
    expect(result).toEqual(rows);
    expect(result).not.toBe(rows);
  });

  it("sorts by a column in either direction without changing its input, empty cells last whichever way", () => {
    const before = structuredClone(rows);
    expect(sortedRows(rows, { column: 1, descending: false }, collator).map((row) => row[1])).toEqual([1, 2, 2, null]);
    expect(sortedRows(rows, { column: 1, descending: true }, collator).map((row) => row[1])).toEqual([2, 2, 1, null]);
    expect(sortedRows(rows, { column: 0, descending: true }, collator).map((row) => row[0])).toEqual(["c", "b", "a", null]);
    expect(rows).toEqual(before);
  });

  it("keeps the given order of rows whose cells tie, in both directions", () => {
    expect(sortedRows(rows, { column: 1, descending: false }, collator).map((row) => row[0])).toEqual([null, "b", "a", "c"]);
    expect(sortedRows(rows, { column: 1, descending: true }, collator).map((row) => row[0])).toEqual(["b", "a", null, "c"]);
  });
});

describe("the table's own surface and filter", () => {
  it("draws the filter field in HeroUI's default field variant, as a control on the pane's own background", async () => {
    const { host } = await renderTable(NUMBERS);
    expect(host.querySelector(".search-field--primary")).not.toBeNull();
    expect(host.querySelector(".search-field--secondary")).toBeNull();
  });

  it("shows the caption as the block's label above the table and keeps it as the table's visually hidden caption", async () => {
    const { host } = await renderTable({ ...NUMBERS, caption: "Files" });
    const label = [...host.querySelectorAll("span")].find((span) => span.textContent === "Files")!;
    const table = host.querySelector("table")!;
    expect(label.compareDocumentPosition(table) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(label.closest("table")).toBeNull();
    const caption = table.querySelector("caption")!;
    expect(caption.textContent).toBe("Files");
    expect(caption.classList.contains("sr-only")).toBe(true);
  });

  it("has no label and no caption for a table without one", async () => {
    const { host } = await renderTable(NUMBERS);
    expect(host.querySelector("caption")).toBeNull();
  });
});

describe("sorting a column of mixed types", () => {
  const MIXED: Omit<Table, "type"> = {
    columns: [{ label: "Value" }],
    rows: [[true], ["beta"], [null], [3], ["alpha"], [false], [-1], [null]],
  };

  it("keeps the types together, numbers then texts then booleans, and the empty cells last", async () => {
    const { host } = await renderTable(MIXED);
    await click(sortButton(host, "Value"));
    expect(column(host, 0)).toEqual(["-1", "3", "alpha", "beta", "No", "Yes", "", ""]);
  });

  it("puts the empty cells last when descending too, with every type still in one group", async () => {
    const { host } = await renderTable(MIXED);
    await click(sortButton(host, "Value"));
    await click(sortButton(host, "Value"));
    const shown = column(host, 0);
    expect(shown.slice(-2)).toEqual(["", ""]);
    const kind = (text: string | null) => (text === "Yes" || text === "No" ? "boolean" : /^-?\d/.test(text!) ? "number" : "text");
    const kinds = shown.slice(0, -2).map(kind);
    expect(kinds.join(",")).toMatch(/^(boolean,boolean,text,text,number,number|number,number,text,text,boolean,boolean)$/);
    expect(shown.slice(0, -2).sort()).toEqual(["-1", "3", "No", "Yes", "alpha", "beta"].sort());
  });
});
