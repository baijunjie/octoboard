import { webFiles, type RepositoryExample } from "./types";

export const mobile: RepositoryExample = {
  id: "mobile",
  branch: "feat/mobile-redemption",
  files: {
    ...webFiles("mobile", "MobileApp", "MobileApp"),
    "README.md": `# Bistro Mobile — fictional preview repository

A mobile web companion for browsing nearby meal deals and collecting paid orders. The API owns prices, payment status and redemption codes. Codes are fetched only for paid orders and are never derived from order IDs. Retry an interrupted order with its original request ID; a new request starts only after confirmation.

Built as a React DOM and TypeScript web client with Vite and Vitest. The website displays this source without executing it. Run pnpm dev against the Bistro API to develop this illustrative client.
`,
    "src/domain/mobile-order.ts": `export interface MobileDeal {
  id: string;
  storeId: string;
  title: string;
  remaining: number;
  priceCents: number;
}
export interface MobileOrder {
  id: string;
  storeId: string;
  status: "pending_payment" | "paid" | "redeemed" | "cancelled";
  totalCents: number;
}
export interface RedemptionPass {
  orderId: string;
  code: string;
  expiresAt: string;
}
`,
    "src/domain/redemption.ts": `import type { MobileOrder, RedemptionPass } from "./mobile-order";

export function canDisplayPass(order: MobileOrder, pass: RedemptionPass, now: number): boolean {
  return order.status === "paid" && order.id === pass.orderId
    && pass.code.length > 0 && Date.parse(pass.expiresAt) > now;
}
`,
    "src/api/deals.ts": `import type { MobileDeal } from "../domain/mobile-order";

export async function nearbyDeals(storeId: string): Promise<MobileDeal[]> {
  const response = await fetch("/api/meal-deals?storeId=" + encodeURIComponent(storeId));
  if (!response.ok) throw new Error("Nearby deals are unavailable");
  return response.json();
}
`,
    "src/api/mobile-orders.ts": `import type { MobileOrder, RedemptionPass } from "../domain/mobile-order";

export async function orderDeal(dealId: string, storeId: string, requestId: string): Promise<MobileOrder> {
  const response = await fetch("/api/orders", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ packageId: dealId, storeId, quantity: 1, requestId }),
  });
  if (!response.ok) throw new Error("Order was not confirmed");
  return response.json();
}
export async function loadOrder(orderId: string): Promise<MobileOrder> {
  const response = await fetch("/api/orders/" + encodeURIComponent(orderId));
  if (!response.ok) throw new Error("Order status is unavailable");
  return response.json();
}
export async function loadPass(orderId: string): Promise<RedemptionPass> {
  const response = await fetch("/api/orders/" + encodeURIComponent(orderId) + "/redemption", { cache: "no-store" });
  if (!response.ok) throw new Error("No active collection pass");
  return response.json();
}
`,
    "src/MobileApp.tsx": `import { useEffect, useState } from "react";
import { nearbyDeals } from "./api/deals";
import type { MobileDeal, MobileOrder } from "./domain/mobile-order";
import { DealList } from "./components/DealList";
import { PlaceOrder } from "./components/PlaceOrder";
import { OrderWallet } from "./components/OrderWallet";

export function MobileApp() {
  const [deals, setDeals] = useState<MobileDeal[]>([]);
  const [selected, setSelected] = useState<MobileDeal>();
  const [order, setOrder] = useState<MobileOrder>();
  const [error, setError] = useState("");
  useEffect(() => {
    void nearbyDeals("store-riverside").then(setDeals)
      .catch(() => setError("Unable to load today's nearby offers."));
  }, []);
  return <main><h1>Lunch near Riverside</h1><p role="alert">{error}</p>
    {order ? <OrderWallet initialOrder={order} /> : <>
      <DealList deals={deals} onSelect={setSelected} />
      {selected && <PlaceOrder key={selected.id} deal={selected} onOrdered={setOrder} />}
    </>}
  </main>;
}
`,
    "src/components/DealList.tsx": `import type { MobileDeal } from "../domain/mobile-order";
import { money } from "../format/money";

export function DealList({ deals, onSelect }: { deals: MobileDeal[]; onSelect: (deal: MobileDeal) => void }) {
  return <section aria-label="Nearby offers">{deals.map((deal) => <article key={deal.id}>
    <h2>{deal.title}</h2><p>{money(deal.priceCents)}</p>
    <button disabled={deal.remaining < 1} onClick={() => onSelect(deal)}>Choose deal</button>
  </article>)}</section>;
}
`,
    "src/components/PlaceOrder.tsx": `import { useRef, useState } from "react";
import { orderDeal } from "../api/mobile-orders";
import type { MobileDeal, MobileOrder } from "../domain/mobile-order";

export function PlaceOrder({ deal, onOrdered }: { deal: MobileDeal; onOrdered: (order: MobileOrder) => void }) {
  const requestId = useRef(crypto.randomUUID());
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  async function submit() {
    if (pending) return;
    setPending(true);
    try { onOrdered(await orderDeal(deal.id, deal.storeId, requestId.current)); }
    catch { setError("Confirmation interrupted. Retry to check the same order."); }
    finally { setPending(false); }
  }
  return <section><h2>{deal.title}</h2><p>Final price is confirmed by the store.</p>
    <button disabled={pending || deal.remaining < 1} onClick={submit}>Confirm order</button>
    <p role="alert">{error}</p>
  </section>;
}
`,
    "src/components/OrderWallet.tsx": `import { useState } from "react";
import { loadOrder, loadPass } from "../api/mobile-orders";
import { canDisplayPass } from "../domain/redemption";
import type { MobileOrder, RedemptionPass } from "../domain/mobile-order";
import { CollectionPass } from "./CollectionPass";

export function OrderWallet({ initialOrder }: { initialOrder: MobileOrder }) {
  const [order, setOrder] = useState(initialOrder);
  const [pass, setPass] = useState<RedemptionPass>();
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  async function refresh() {
    setPending(true); setPass(undefined); setError("");
    try {
      const current = await loadOrder(order.id);
      setOrder(current);
      if (current.status === "paid") {
        const issued = await loadPass(current.id);
        if (canDisplayPass(current, issued, Date.now())) setPass(issued);
        else setError("Collection pass expired. Refresh to request the latest pass.");
      }
    } catch { setError("Unable to verify payment or collection status."); }
    finally { setPending(false); }
  }
  return <section><h2>Your order {order.id}</h2><p>{order.status.replaceAll("_", " ")}</p>
    <button disabled={pending} onClick={refresh}>Refresh payment and pass</button>
    {pass && <CollectionPass pass={pass} />}<p role="alert">{error}</p>
  </section>;
}
`,
    "src/components/CollectionPass.tsx": `import { useEffect, useState } from "react";
import type { RedemptionPass } from "../domain/mobile-order";

export function CollectionPass({ pass }: { pass: RedemptionPass }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  if (Date.parse(pass.expiresAt) <= now) return <p>Pass expired. Refresh before collection.</p>;
  return <article><h3>Show at the collection counter</h3><output>{pass.code}</output>
    <p>Valid until {new Date(pass.expiresAt).toLocaleTimeString()}</p>
    <p>Staff must confirm redemption before handing over your meal.</p>
  </article>;
}
`,
    "src/format/money.ts": `const currency = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });
export const money = (cents: number): string => currency.format(cents / 100);
`,
    "tests/redemption.test.ts": `import { expect, it } from "vitest";
import { canDisplayPass } from "../src/domain/redemption";

const order = { id: "order-482", storeId: "store-riverside", status: "paid" as const, totalCents: 2800 };
const pass = { orderId: order.id, code: "RX7M2Q", expiresAt: "2026-10-15T04:30:00Z" };
const now = Date.parse("2026-10-15T04:00:00Z");
it("requires payment, matching order and a live server-issued pass", () => {
  expect(canDisplayPass(order, pass, now)).toBe(true);
  expect(canDisplayPass({ ...order, status: "pending_payment" }, pass, now)).toBe(false);
  expect(canDisplayPass(order, { ...pass, orderId: "another-order" }, now)).toBe(false);
  expect(canDisplayPass(order, pass, Date.parse(pass.expiresAt))).toBe(false);
});
`,
  },
  edited: {
    "src/domain/redemption.ts": `import type { MobileOrder, RedemptionPass } from "./mobile-order";

export function canDisplayPass(order: MobileOrder, pass: RedemptionPass, now: number): boolean {
  return order.status === "paid" && pass.code.length > 0;
}
`,
    "src/components/PlaceOrder.tsx": `import { useState } from "react";
import { orderDeal } from "../api/mobile-orders";
import type { MobileDeal, MobileOrder } from "../domain/mobile-order";

export function PlaceOrder({ deal, onOrdered }: { deal: MobileDeal; onOrdered: (order: MobileOrder) => void }) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  async function submit() {
    if (pending) return;
    setPending(true);
    try { onOrdered(await orderDeal(deal.id, deal.storeId, crypto.randomUUID())); }
    catch { setError("Confirmation interrupted. Retry to check the same order."); }
    finally { setPending(false); }
  }
  return <section><h2>{deal.title}</h2><p>Final price is confirmed by the store.</p>
    <button disabled={pending || deal.remaining < 1} onClick={submit}>Confirm order</button>
    <p role="alert">{error}</p>
  </section>;
}
`,
  },
  added: ["tests/redemption.test.ts"],
};
