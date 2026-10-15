import { webFiles, type RepositoryExample } from "./types";

export const kitchen: RepositoryExample = {
  id: "kitchen",
  branch: "feat/kitchen-tickets",
  files: {
    ...webFiles("kitchen", "KitchenApp", "KitchenApp"),
    "README.md": `# Bistro Kitchen — fictional preview repository

The kitchen display lists accepted meal tickets by station, queues printer jobs and marks food ready. A queued print request is not proof of paper delivery: staff inspect job status and the printer agent acknowledges actual printing. Print retries keep their attempt ID. Ready updates use the ticket revision and only update the screen after server confirmation; concurrent edits require reloading.

Built as a React DOM and TypeScript web client with Vite and Vitest. This source is illustrative and displayed by the website without executing requests. The printer transport itself runs on the store gateway.
`,
    "src/domain/ticket.ts": `export interface KitchenItem { name: string; quantity: number; notes: string[]; }
export interface KitchenTicket {
  id: string;
  table: string;
  station: "hot" | "cold";
  revision: number;
  status: "preparing" | "ready" | "collected";
  receivedAt: string;
  items: KitchenItem[];
}
`,
    "src/domain/print-job.ts": `export interface PrintJob {
  id: string;
  ticketId: string;
  status: "queued" | "printing" | "printed" | "failed";
  error?: string;
}

export function printStatus(job: PrintJob): string {
  switch (job.status) {
    case "queued": return "Queued at the store gateway";
    case "printing": return "Printer is working";
    case "printed": return "Paper ticket confirmed by printer";
    case "failed": return "Print failed: " + (job.error ?? "Inspect the printer");
  }
}
`,
    "src/domain/station.ts": `import type { KitchenTicket } from "./ticket";

export function stationQueue(tickets: KitchenTicket[], station: KitchenTicket["station"]): KitchenTicket[] {
  return tickets.filter((ticket) => ticket.station === station && ticket.status !== "collected")
    .sort((left, right) => Date.parse(left.receivedAt) - Date.parse(right.receivedAt));
}
`,
    "src/api/tickets.ts": `import type { KitchenTicket } from "../domain/ticket";

export class TicketConflict extends Error {}
export async function listTickets(): Promise<KitchenTicket[]> {
  const response = await fetch("/api/kitchen/tickets?storeId=store-riverside", { cache: "no-store" });
  if (!response.ok) throw new Error("Kitchen queue is unavailable");
  return response.json();
}
export async function markReady(ticket: KitchenTicket): Promise<KitchenTicket> {
  const response = await fetch("/api/kitchen/tickets/" + encodeURIComponent(ticket.id) + "/ready", {
    method: "POST", headers: { "Content-Type": "application/json", "If-Match": String(ticket.revision) },
    body: JSON.stringify({ status: "ready" }),
  });
  if (response.status === 409) throw new TicketConflict("Ticket changed; reload the queue");
  if (!response.ok) throw new Error("Food-ready status was not saved");
  return response.json();
}
`,
    "src/api/printer.ts": `import type { PrintJob } from "../domain/print-job";

export async function enqueuePrint(ticketId: string, requestId: string): Promise<PrintJob> {
  const response = await fetch("/api/kitchen/print-jobs", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ ticketId, printerId: "riverside-hot-01", requestId }),
  });
  if (!response.ok) throw new Error("Print request was not accepted");
  return response.json();
}
export async function inspectPrint(jobId: string): Promise<PrintJob> {
  const response = await fetch("/api/kitchen/print-jobs/" + encodeURIComponent(jobId), { cache: "no-store" });
  if (!response.ok) throw new Error("Printer status is unavailable");
  return response.json();
}
`,
    "src/KitchenApp.tsx": `import { useEffect, useState } from "react";
import { listTickets } from "./api/tickets";
import { stationQueue } from "./domain/station";
import type { KitchenTicket } from "./domain/ticket";
import { StationBoard } from "./components/StationBoard";

export function KitchenApp() {
  const [tickets, setTickets] = useState<KitchenTicket[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  async function refresh() {
    setLoading(true);
    try { setTickets(await listTickets()); setError(""); }
    catch { setError("Queue refresh failed. Verify tickets before handing over food."); }
    finally { setLoading(false); }
  }
  useEffect(() => { void refresh(); }, []);
  return <main><h1>Riverside · Hot kitchen</h1><p role="alert">{error}</p>
    <button disabled={loading} onClick={refresh}>Refresh queue</button>
    <StationBoard tickets={stationQueue(tickets, "hot")} onUpdated={(updated) =>
      setTickets((current) => current.map((ticket) => ticket.id === updated.id ? updated : ticket))} />
  </main>;
}
`,
    "src/components/StationBoard.tsx": `import type { KitchenTicket } from "../domain/ticket";
import { TicketCard } from "./TicketCard";

export function StationBoard({ tickets, onUpdated }: {
  tickets: KitchenTicket[]; onUpdated: (ticket: KitchenTicket) => void;
}) {
  return <section aria-label="Station queue">{tickets.length === 0 ? <p>No open tickets.</p> :
    tickets.map((ticket) => <TicketCard key={ticket.id + ":" + ticket.revision} ticket={ticket} onUpdated={onUpdated} />)}
  </section>;
}
`,
    "src/components/TicketCard.tsx": `import { useState } from "react";
import { markReady, TicketConflict } from "../api/tickets";
import type { KitchenTicket } from "../domain/ticket";
import { PrintTicket } from "./PrintTicket";

export function TicketCard({ ticket, onUpdated }: { ticket: KitchenTicket; onUpdated: (ticket: KitchenTicket) => void }) {
  const [pending, setPending] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [error, setError] = useState("");
  async function ready() {
    if (pending || ticket.status !== "preparing" || conflict) return;
    setPending(true);
    try { onUpdated(await markReady(ticket)); }
    catch (error) {
      setConflict(error instanceof TicketConflict);
      setError(error instanceof TicketConflict ? "Ticket changed. Refresh the queue." : "Ready update failed. Check and retry.");
    } finally { setPending(false); }
  }
  return <article><h2>{ticket.table} · {ticket.id}</h2><p>{ticket.status}</p>
    <ul>{ticket.items.map((item, index) => <li key={index}>{item.quantity} × {item.name}
      <ul>{item.notes.map((note) => <li key={note}>{note}</li>)}</ul></li>)}</ul>
    <PrintTicket ticketId={ticket.id} />
    <button disabled={pending || conflict || ticket.status !== "preparing"} onClick={ready}>Mark food ready</button>
    <p role="alert">{error}</p>
  </article>;
}
`,
    "src/components/PrintTicket.tsx": `import { useRef, useState } from "react";
import { enqueuePrint, inspectPrint } from "../api/printer";
import { printStatus, type PrintJob } from "../domain/print-job";

export function PrintTicket({ ticketId }: { ticketId: string }) {
  const requestId = useRef(crypto.randomUUID());
  const [job, setJob] = useState<PrintJob>();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  async function print() {
    setPending(true); setError("");
    try { setJob(await enqueuePrint(ticketId, requestId.current)); }
    catch { setError("Print request interrupted. Retry the same job; inspect the printer for duplicates."); }
    finally { setPending(false); }
  }
  async function inspect() {
    if (!job) return;
    setPending(true);
    try { setJob(await inspectPrint(job.id)); setError(""); }
    catch { setError("Cannot read printer status. Inspect the station printer."); }
    finally { setPending(false); }
  }
  return <section aria-label="Paper ticket">
    {!job && <button disabled={pending} onClick={print}>Queue paper ticket</button>}
    {job && <><p role="status">{printStatus(job)}</p><button disabled={pending} onClick={inspect}>Check printer status</button>
      {job.status === "failed" && <p>Inspect the printer and use the store gateway to retry this failed job.</p>}</>}
    <p role="alert">{error}</p>
  </section>;
}
`,
    "tests/print-job.test.ts": `import { expect, it } from "vitest";
import { printStatus } from "../src/domain/print-job";

it("does not report a queued request as a printed paper ticket", () => {
  const job = { id: "print-184", ticketId: "ticket-36", status: "queued" as const };
  expect(printStatus(job)).toBe("Queued at the store gateway");
  expect(printStatus({ ...job, status: "printed" })).toBe("Paper ticket confirmed by printer");
  expect(printStatus({ ...job, status: "failed", error: "Out of paper" })).toContain("Out of paper");
});
`,
    "tests/tickets.test.ts": `import { afterEach, expect, it, vi } from "vitest";
import { markReady, TicketConflict } from "../src/api/tickets";
import { stationQueue } from "../src/domain/station";
import type { KitchenTicket } from "../src/domain/ticket";

const ticket: KitchenTicket = { id: "ticket-36", table: "T12", station: "hot", revision: 4,
  status: "preparing", receivedAt: "2026-10-15T03:50:00Z", items: [{ name: "Grilled chicken", quantity: 2, notes: ["No peanuts"] }] };
afterEach(() => vi.unstubAllGlobals());
it("reports revision conflicts instead of moving stale tickets to ready", async () => {
  const fetch = vi.fn().mockResolvedValue({ ok: false, status: 409 });
  vi.stubGlobal("fetch", fetch);
  await expect(markReady(ticket)).rejects.toBeInstanceOf(TicketConflict);
  expect(fetch.mock.calls[0][1].headers["If-Match"]).toBe("4");
});
it("keeps collected tickets off the station board", () => {
  expect(stationQueue([ticket, { ...ticket, id: "old", status: "collected" }], "hot")).toEqual([ticket]);
});
`,
  },
  edited: {
    "src/domain/print-job.ts": `export interface PrintJob {
  id: string;
  ticketId: string;
  status: "queued" | "printing" | "printed" | "failed";
  error?: string;
}

export function printStatus(job: PrintJob): string {
  switch (job.status) {
    case "queued": return "Paper ticket printed";
    case "printing": return "Paper ticket printed";
    case "printed": return "Paper ticket confirmed by printer";
    case "failed": return "Print failed: " + (job.error ?? "Inspect the printer");
  }
}
`,
    "src/api/tickets.ts": `import type { KitchenTicket } from "../domain/ticket";

export class TicketConflict extends Error {}
export async function listTickets(): Promise<KitchenTicket[]> {
  const response = await fetch("/api/kitchen/tickets?storeId=store-riverside", { cache: "no-store" });
  if (!response.ok) throw new Error("Kitchen queue is unavailable");
  return response.json();
}
export async function markReady(ticket: KitchenTicket): Promise<KitchenTicket> {
  const response = await fetch("/api/kitchen/tickets/" + encodeURIComponent(ticket.id) + "/ready", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ status: "ready" }),
  });
  if (!response.ok) throw new Error("Food-ready status was not saved");
  return response.json();
}
`,
  },
  added: ["tests/print-job.test.ts", "tests/tickets.test.ts"],
};
