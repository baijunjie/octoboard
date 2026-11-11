// A page's table: its rows sorted by a column the user picks and narrowed by a text filter. A plain semantic table
// rather than the application's `Table` primitive, which has no sorted columns. Sorting and filtering change only what is
// shown, never the page, so they work on a page that is read-only, in history or submitted.
import { ArrowDown, ArrowUp, ChevronsUpDown, SearchX } from "lucide-react";
import React, { createContext, useContext, useMemo, useState } from "react";

import { StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { EmptyPanel } from "../../components/panel/EmptyPanel";
import { FadedScroller, useFadeRegion } from "../../components/scroll/scrollFade";
import { isolate } from "../../i18n/isolate";
import { useCurrentLanguage, useT } from "../../i18n/react";
import { BareButton, SearchField } from "../../primitives";
import type { CellValue, Component } from "../../protocol";
import { useCellText } from "./cells";
import { BlockLabel } from "./copyable";
import { walkComponents } from "./formModel";
import type { ComponentProps } from "./registry";

type Cell = CellValue | null;

interface Sorting {
  column: number;
  descending: boolean;
}

/** Which group a value sorts in, so a column of mixed types keeps the types together: numbers, then texts, then
 * booleans. */
function typeRank(value: CellValue): number {
  return typeof value === "number" ? 0 : typeof value === "string" ? 1 : 2;
}

/** The order of two values that are not empty: numbers by value, texts by the language's collation with digit runs read
 * as numbers, booleans false before true. */
function compareValues(collator: Intl.Collator, a: CellValue, b: CellValue): number {
  const byType = typeRank(a) - typeRank(b);
  if (byType !== 0) return byType;
  if (typeof a === "number" && typeof b === "number") return a - b;
  if (typeof a === "string" && typeof b === "string") return collator.compare(a, b);
  return Number(a) - Number(b);
}

/** The rows by `sorting`, an empty cell last whichever way it goes; the given order where none is picked or cells tie. */
export function sortedRows(rows: readonly (readonly Cell[])[], sorting: Sorting | undefined, collator: Intl.Collator): (readonly Cell[])[] {
  if (!sorting) return [...rows];
  const sign = sorting.descending ? -1 : 1;
  return rows
    .map((row, index) => ({ row, index }))
    .sort((a, b) => {
      const x = a.row[sorting.column];
      const y = b.row[sorting.column];
      if (x === null || x === undefined || y === null || y === undefined) return Number(x === null || x === undefined) - Number(y === null || y === undefined) || a.index - b.index;
      return sign * compareValues(collator, x, y) || a.index - b.index;
    })
    .map(({ row }) => row);
}

/** Where each table of a page stands among the page's tables, counted from 1, when the page has more than one; empty
 * otherwise. A table of several takes a name of its own (its caption, else its number) into its filter field's and
 * sort buttons' names, so they are told apart from another table's. */
export function tableNumbers(body: readonly Component[]): ReadonlyMap<Component, number> {
  const tables = [...walkComponents(body)].filter((component) => component.type === "table");
  return new Map(tables.length > 1 ? tables.map((table, index) => [table, index + 1]) : []);
}

export const PageTablesContext = createContext<ReadonlyMap<Component, number>>(new Map());

export function TableView({ component }: ComponentProps<"table">): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const cellText = useCellText();
  const collator = useMemo(() => new Intl.Collator(language, { numeric: true }), [language]);
  const [sorting, setSorting] = useState<Sorting>();
  const [query, setQuery] = useState("");
  const number = useContext(PageTablesContext).get(component);
  // The table's own name where the page has several: its caption, else its place among them.
  const name = number === undefined ? undefined : (component.caption ?? t("report.table.labelNumbered", { index: number }));
  const regionName = name ?? component.caption ?? t("report.table.label");
  const region = useFadeRegion({ label: regionName });
  const sortable = component.sortable !== false;
  const filterable = component.filterable !== false;

  const needle = query.trim().toLocaleLowerCase(language);
  const shown = useMemo(() => {
    const sorted = sortedRows(component.rows, sortable ? sorting : undefined, collator);
    return needle === "" ? sorted : sorted.filter((row) => row.some((cell) => cell !== null && [cellText(cell), String(cell)].some((text) => text.toLocaleLowerCase(language).includes(needle))));
  }, [component.rows, sortable, sorting, collator, needle, language, cellText]);

  const count = needle === "" ? undefined : t("report.table.count", { shown: shown.length, total: component.rows.length });
  // Numbers sit at the end of their column unless the page says where.
  const alignEnd = component.columns.map((column, index) => column.align === undefined
    ? component.rows.some((row) => row[index] !== null && row[index] !== undefined) && component.rows.every((row) => row[index] === null || row[index] === undefined || typeof row[index] === "number")
    : column.align === "end");

  // A column of running text keeps room for most of a note on a line or two, so its rows stay even beside short cells.
  const prose = component.columns.map((_, index) => component.rows.some((row) => typeof row[index] === "string" && row[index].length > 24));

  const sortBy = (column: number) => setSorting((held) => ({ column, descending: held?.column === column && !held.descending }));

  return (
    <div className="flex flex-col gap-1">
      {/* The caption is drawn as every block's label, above the block; the table keeps it as its caption for assistive
        technology. */}
      <BlockLabel label={component.caption} decorative />
      <div className="flex flex-col gap-2">
        {filterable && (
          <SearchField fullWidth label={name === undefined ? t("report.table.filterLabel") : t("report.table.filterLabelNamed", { table: isolate(name) })} placeholder={t("common.search")} value={query} onChange={setQuery} />
        )}
        {count !== undefined && <p className="text-xs text-app-muted">{count}</p>}
        <StatusAnnouncer text={count} clearAfterRead />
        <div className={`rounded-lg border border-separator bg-field ${region.ring}`}>
          {/* The frame is on HeroUI's field fill (white in light, the fields' recessed dark), so it stands off the panel; the
            header row is tinted. Cells wrap at word breaks, so a column never narrows below its longest word or the minimum
            width of its column: a table wider than the region scrolls inside it, and is a region of its own while it does. A
            token longer than a cell's maximum width still breaks. */}
          <FadedScroller orientation="horizontal" region={region}>
            <table className="w-full border-collapse text-sm">
              {component.caption !== undefined && <caption className="sr-only">{component.caption}</caption>}
              <thead>
                <tr className="border-b border-separator bg-default/40">
                  {component.columns.map((column, index) => {
                    const active = sortable && sorting?.column === index;
                    const Arrow = active ? (sorting.descending ? ArrowDown : ArrowUp) : ChevronsUpDown;
                    return (
                      <th
                        key={index}
                        scope="col"
                        aria-sort={active ? (sorting.descending ? "descending" : "ascending") : undefined}
                        className={`px-3 py-2 font-medium whitespace-nowrap ${alignEnd[index] ? "text-end" : "text-start"}`}
                      >
                        {sortable ? (
                          <BareButton onPress={() => sortBy(index)} label={name === undefined ? undefined : t("report.table.sortNamed", { column: isolate(column.label), table: isolate(name) })} className={`inline-flex items-center gap-1 rounded-sm outline-none ui-keyboard-focus:ring-2 ui-keyboard-focus:ring-focus ${alignEnd[index] ? "flex-row-reverse" : ""}`}>
                            <span dir="auto" className="align-match-parent">{column.label}</span>
                            <Arrow aria-hidden="true" className={`size-3.5 shrink-0 ${active ? "" : "text-app-muted"}`} />
                          </BareButton>
                        ) : (
                          <span dir="auto" className="align-match-parent">{column.label}</span>
                        )}
                      </th>
                    );
                  })}
                </tr>
              </thead>
              <tbody>
                {shown.length === 0 ? (
                  <tr>
                    <td colSpan={component.columns.length} className="p-2">
                      <EmptyPanel compact icon={SearchX} message={t("report.table.noMatch")} />
                    </td>
                  </tr>
                ) : shown.map((row, rowIndex) => (
                  <tr key={rowIndex} className="border-b border-separator last:border-b-0">
                    {component.columns.map((_, index) => {
                      const cell = row[index];
                      return (
                        <td key={index} dir={alignEnd[index] ? undefined : "auto"} className={`px-3 py-2 align-top max-w-80 ${prose[index] ? "min-w-80" : "min-w-28"} [overflow-wrap:break-word] ${alignEnd[index] ? "text-end tabular-nums" : "align-match-parent text-start"}`}>
                          {cell === null || cell === undefined ? "" : cellText(cell)}
                        </td>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </FadedScroller>
        </div>
      </div>
    </div>
  );
}
