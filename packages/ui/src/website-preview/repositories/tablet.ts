import { webFiles, type RepositoryExample } from "./types";

export const tablet: RepositoryExample = {
  id: "tablet",
  branch: "feat/table-orders",
  files: {
    ...webFiles("tablet", "TabletApp", "TabletApp"),
    "README.md": `# Bistro Tablet — fictional preview repository

A table-side ordering station for staff: inspect the current table revision, add meal deals and allowed extras, then submit with optimistic concurrency. A stale revision requires reloading the table before another submission. The server validates inventory and returns the authoritative updated order; prices are never submitted by the tablet.

Built as a React DOM and TypeScript web client with Vite and Vitest. This is display-only source for the website preview. The illustrative API is proxied by Vite during local development.
`,
    "src/domain/table-order.ts": `export interface Extra { id: string; label: string; maximum: number; }
export interface TableOffer { id: string; title: string; extras: Extra[]; }
export interface TableLine { id: string; title: string; quantity: number; extras: string[]; }
export interface TableOrder { id: string; table: string; revision: number; lines: TableLine[]; }
export interface AddLine { offerId: string; quantity: number; extras: string[]; requestId: string; }
`,
    "src/domain/add-ons.ts": `import type { TableOffer } from "./table-order";

export function validateExtras(offer: TableOffer, selected: string[]): string | undefined {
  for (const id of new Set(selected)) {
    const extra = offer.extras.find((item) => item.id === id);
    if (!extra) return "This extra is unavailable for the selected package.";
    if (selected.filter((value) => value === id).length > extra.maximum)
      return "Extra quantity exceeds the package limit.";
  }
  return undefined;
}
`,
    "src/api/table-orders.ts": `import type { AddLine, TableOrder } from "../domain/table-order";

export class RevisionConflict extends Error {}
export async function loadTable(table: string): Promise<TableOrder> {
  const response = await fetch("/api/tables/" + encodeURIComponent(table) + "/order", { cache: "no-store" });
  if (!response.ok) throw new Error("Table order is unavailable");
  return response.json();
}
export async function addLine(order: TableOrder, line: AddLine): Promise<TableOrder> {
  const response = await fetch("/api/table-orders/" + encodeURIComponent(order.id) + "/lines", {
    method: "POST", headers: { "Content-Type": "application/json", "If-Match": String(order.revision) },
    body: JSON.stringify(line),
  });
  if (response.status === 409) throw new RevisionConflict("Table changed on another device");
  if (!response.ok) throw new Error("Line was not accepted");
  return response.json();
}
`,
    "src/api/table-menu.ts": `import type { TableOffer } from "../domain/table-order";

export async function loadTableMenu(): Promise<TableOffer[]> {
  const response = await fetch("/api/stores/store-riverside/table-menu");
  if (!response.ok) throw new Error("Table menu is unavailable");
  return response.json();
}
`,
    "src/TabletApp.tsx": `import { useEffect, useState } from "react";
import { loadTable } from "./api/table-orders";
import { loadTableMenu } from "./api/table-menu";
import type { TableOffer, TableOrder } from "./domain/table-order";
import { TableSummary } from "./components/TableSummary";
import { AddMeal } from "./components/AddMeal";

export function TabletApp() {
  const [order, setOrder] = useState<TableOrder>();
  const [offers, setOffers] = useState<TableOffer[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    void Promise.all([loadTable("T12"), loadTableMenu()]).then(([table, menu]) => {
      setOrder(table); setOffers(menu);
    }).catch(() => setError("Unable to open table T12."));
  }, []);
  return <main><h1>Table T12</h1><p role="alert">{error}</p>
    {order && <><TableSummary order={order} />
      <AddMeal order={order} offers={offers} onUpdated={setOrder} /></>}
  </main>;
}
`,
    "src/components/TableSummary.tsx": `import type { TableOrder } from "../domain/table-order";

export function TableSummary({ order }: { order: TableOrder }) {
  return <section aria-label="Current table order"><p>Revision {order.revision}</p>
    <ul>{order.lines.map((line) => <li key={line.id}>{line.quantity} × {line.title}
      {line.extras.length > 0 && <small> — {line.extras.join(", ")}</small>}
    </li>)}</ul>
  </section>;
}
`,
    "src/components/AddMeal.tsx": `import { useRef, useState } from "react";
import { addLine, loadTable, RevisionConflict } from "../api/table-orders";
import { validateExtras } from "../domain/add-ons";
import type { TableOffer, TableOrder } from "../domain/table-order";
import { ExtraPicker } from "./ExtraPicker";

export function AddMeal({ order, offers, onUpdated }: {
  order: TableOrder; offers: TableOffer[]; onUpdated: (order: TableOrder) => void;
}) {
  const [offerId, setOfferId] = useState("");
  const [extras, setExtras] = useState<string[]>([]);
  const [message, setMessage] = useState("");
  const [pending, setPending] = useState(false);
  const [needsReload, setNeedsReload] = useState(false);
  const requestId = useRef(crypto.randomUUID());
  const [uncertain, setUncertain] = useState(false);
  const offer = offers.find((item) => item.id === offerId);
  async function submit() {
    if (!offer || pending || needsReload) return;
    const error = validateExtras(offer, extras);
    if (error) { setMessage(error); return; }
    setPending(true);
    try {
      onUpdated(await addLine(order, { offerId, extras, quantity: 1, requestId: requestId.current }));
      requestId.current = crypto.randomUUID(); setExtras([]); setUncertain(false); setMessage("Meal added to the table.");
    } catch (error) {
      if (error instanceof RevisionConflict) { setNeedsReload(true); setUncertain(false); setMessage("Table changed. Reload before adding meals."); }
      else { setUncertain(true); setMessage("Unable to confirm. Retry the same meal selection."); }
    } finally { setPending(false); }
  }
  async function reload() {
    setPending(true);
    try { onUpdated(await loadTable(order.table)); setNeedsReload(false); requestId.current = crypto.randomUUID(); }
    catch { setMessage("Reload failed. Keep this table open and retry."); }
    finally { setPending(false); }
  }
  return <section><h2>Add a meal</h2><select aria-label="Meal package" disabled={pending || uncertain || needsReload} value={offerId}
    onChange={(event) => { setOfferId(event.target.value); setExtras([]); }}>
    <option value="">Choose a package</option>{offers.map((item) => <option key={item.id} value={item.id}>{item.title}</option>)}
    </select>
    {offer && <ExtraPicker offer={offer} selected={extras} disabled={pending || uncertain || needsReload} onChange={setExtras} />}
    <button disabled={pending || !offer || needsReload} onClick={submit}>Add meal</button>
    {needsReload && <button disabled={pending} onClick={reload}>Reload table</button>}<p role="alert">{message}</p>
  </section>;
}
`,
    "src/components/ExtraPicker.tsx": `import type { TableOffer } from "../domain/table-order";

export function ExtraPicker({ offer, selected, disabled, onChange }: {
  offer: TableOffer; selected: string[]; disabled: boolean; onChange: (extras: string[]) => void;
}) {
  return <fieldset disabled={disabled}><legend>Package extras</legend>{offer.extras.filter((extra) => extra.maximum > 0).map((extra) =>
    <label key={extra.id}><input type="checkbox" checked={selected.includes(extra.id)} onChange={(event) =>
      onChange(event.target.checked ? [...selected, extra.id] : selected.filter((id) => id !== extra.id))} />
      {extra.label}</label>)}
  </fieldset>;
}
`,
    "tests/add-ons.test.ts": `import { expect, it } from "vitest";
import { validateExtras } from "../src/domain/add-ons";

const offer = { id: "lunch-two", title: "Lunch for two", extras: [{ id: "tea", label: "Iced tea", maximum: 1 }] };
it("accepts included extras and rejects unknown or excessive quantities", () => {
  expect(validateExtras(offer, ["tea"])).toBeUndefined();
  expect(validateExtras(offer, ["soup"])).toContain("unavailable");
  expect(validateExtras(offer, ["tea", "tea"])).toContain("limit");
});
`,
    "tests/table-sync.test.ts": `import { afterEach, expect, it, vi } from "vitest";
import { addLine, RevisionConflict } from "../src/api/table-orders";

afterEach(() => vi.unstubAllGlobals());
it("sends the last observed revision and reports a conflict without claiming success", async () => {
  const fetch = vi.fn().mockResolvedValue({ ok: false, status: 409 });
  vi.stubGlobal("fetch", fetch);
  const order = { id: "table-order-12", table: "T12", revision: 7, lines: [] };
  await expect(addLine(order, { offerId: "lunch-two", quantity: 1, extras: [], requestId: "line-12" }))
    .rejects.toBeInstanceOf(RevisionConflict);
  expect(fetch.mock.calls[0][1].headers["If-Match"]).toBe("7");
});
`,
  },
  edited: {
    "src/api/table-orders.ts": `import type { AddLine, TableOrder } from "../domain/table-order";

export class RevisionConflict extends Error {}
export async function loadTable(table: string): Promise<TableOrder> {
  const response = await fetch("/api/tables/" + encodeURIComponent(table) + "/order", { cache: "no-store" });
  if (!response.ok) throw new Error("Table order is unavailable");
  return response.json();
}
export async function addLine(order: TableOrder, line: AddLine): Promise<TableOrder> {
  const response = await fetch("/api/table-orders/" + encodeURIComponent(order.id) + "/lines", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify(line),
  });
  if (!response.ok) throw new Error("Line was not accepted");
  return response.json();
}
`,
    "src/domain/add-ons.ts": `import type { TableOffer } from "./table-order";

export function validateExtras(offer: TableOffer, selected: string[]): string | undefined {
  if (selected.some((id) => !offer.extras.some((extra) => extra.id === id)))
    return "This extra is unavailable for the selected package.";
  return undefined;
}
`,
  },
  added: ["tests/add-ons.test.ts", "tests/table-sync.test.ts"],
};
