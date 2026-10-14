import { webFiles, type RepositoryExample } from "./types";

const checkout = `import { useRef, useState } from "react";
import { placeOrder } from "../api/orders";
import type { MealDeal, Order } from "../domain/meal-deal";

export function Checkout({ deal, onOrdered }: { deal: MealDeal; onOrdered: (order: Order) => void }) {
  const requestId = useRef(crypto.randomUUID());
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState("");
  async function submit() {
    if (pending) return;
    setPending(true);
    try {
      const order = await placeOrder({ packageId: deal.id, storeId: deal.storeId,
        quantity: 1, requestId: requestId.current });
      onOrdered(order);
    } catch { setMessage("Unable to confirm your order. Retry with the same request."); }
    finally { setPending(false); }
  }
  return <section aria-label="Checkout">
    <p>Pay the current package price. Member discounts do not stack.</p>
    <button disabled={pending || deal.remaining === 0} onClick={submit}>
      {pending ? "Confirming…" : "Place order"}
    </button>
    <p role="alert">{message}</p>
  </section>;
}
`;
const orderApiBefore = `import type { Order, OrderInput } from "../domain/meal-deal";

export async function placeOrder(input: OrderInput): Promise<Order> {
  const response = await fetch("/api/orders", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  return response.json();
}
`;
const orderApi = orderApiBefore.replace(
  "  return response.json();",
  '  if (!response.ok) throw new Error("Order was not confirmed");\n  return response.json();',
);
export const customer: RepositoryExample = {
  id: "customer",
  branch: "feature/meal-deal-checkout",
  files: {
    ...webFiles("customer", "MealDeals", "pages/MealDeals"),
    "README.md":
      "# Bistro Customer — fictional preview repository\n\nBrowse meal deals for a store, place an order and inspect its payment status. The browser submits package, store, quantity and a stable request ID; only the server sets the final price. A failed checkout keeps its request ID so a retry cannot create another order.\n\nIllustrative source only; the website preview does not execute these requests.\n",
    "src/domain/meal-deal.ts": `export interface MealDeal {
  id: string;
  title: string;
  storeId: string;
  priceCents: number;
  remaining: number;
  endsAt: string;
}
export interface OrderInput {
  packageId: string;
  storeId: string;
  quantity: number;
  requestId: string;
}
export interface Order extends OrderInput {
  id: string;
  totalCents: number;
  status: "pending_payment" | "paid" | "redeemed";
}
`,
    "src/api/meal-deals.ts": `import type { MealDeal } from "../domain/meal-deal";

export async function listMealDeals(storeId: string): Promise<MealDeal[]> {
  const response = await fetch("/api/meal-deals?storeId=" + encodeURIComponent(storeId));
  if (!response.ok) throw new Error("Unable to load meal deals");
  return response.json();
}
`,
    "src/api/orders.ts": orderApi,
    "src/pages/MealDeals.tsx": `import { useEffect, useState } from "react";
import { listMealDeals } from "../api/meal-deals";
import type { MealDeal, Order } from "../domain/meal-deal";
import { MealDealCard } from "../components/MealDealCard";
import { Checkout } from "../components/Checkout";
import { OrderSummary } from "../components/OrderSummary";

export function MealDeals() {
  const [deals, setDeals] = useState<MealDeal[]>([]);
  const [selected, setSelected] = useState<MealDeal>();
  const [order, setOrder] = useState<Order>();
  const [error, setError] = useState("");
  useEffect(() => {
    void listMealDeals("store-riverside").then(setDeals)
      .catch(() => setError("Meal deals could not be loaded."));
  }, []);
  return <main><h1>Meal deals at Riverside</h1><p role="alert">{error}</p>
    {order ? <OrderSummary order={order} /> : <>
      {deals.map((deal) => <MealDealCard key={deal.id} deal={deal} onSelect={() => setSelected(deal)} />)}
      {selected && <Checkout key={selected.id} deal={selected} onOrdered={setOrder} />}
    </>}
  </main>;
}
`,
    "src/components/MealDealCard.tsx": `import type { MealDeal } from "../domain/meal-deal";
import { money } from "../format/money";

export function MealDealCard({ deal, onSelect }: { deal: MealDeal; onSelect: () => void }) {
  return <article><h2>{deal.title}</h2><strong>{money(deal.priceCents)}</strong>
    <p>{deal.remaining > 0 ? "Available today" : "Sold out"}</p>
    <button disabled={deal.remaining === 0} onClick={onSelect}>View package</button>
  </article>;
}
`,
    "src/components/Checkout.tsx": checkout,
    "src/components/OrderSummary.tsx": `import type { Order } from "../domain/meal-deal";
import { money } from "../format/money";

export function OrderSummary({ order }: { order: Order }) {
  return <article aria-label="Order details"><h2>Order {order.id}</h2>
    <dl><dt>Total</dt><dd>{money(order.totalCents)}</dd>
      <dt>Status</dt><dd>{order.status.replaceAll("_", " ")}</dd>
      <dt>Quantity</dt><dd>{order.quantity}</dd></dl>
    <p>Pay before collecting your meal deal at the selected store.</p>
  </article>;
}
`,
    "src/format/money.ts": `const currency = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });
export const money = (cents: number): string => currency.format(cents / 100);
`,
    "tests/orders.test.ts": `import { afterEach, expect, it, vi } from "vitest";
import { placeOrder } from "../src/api/orders";

const input = { packageId: "lunch-two", storeId: "store-riverside", quantity: 1, requestId: "retry-123" };
afterEach(() => vi.unstubAllGlobals());

it("submits the same request ID on retry and never sends a price", async () => {
  const fetch = vi.fn().mockRejectedValueOnce(new Error("Network interrupted"))
    .mockResolvedValueOnce({ ok: true, json: async () => ({ ...input, id: "order-1", totalCents: 2800 }) });
  vi.stubGlobal("fetch", fetch);
  await expect(placeOrder(input)).rejects.toThrow();
  await expect(placeOrder(input)).resolves.toMatchObject({ id: "order-1", totalCents: 2800 });
  const bodies = fetch.mock.calls.map((call) => JSON.parse(call[1].body));
  expect(bodies).toEqual([input, input]);
  expect(bodies[0]).not.toHaveProperty("priceCents");
});
it("does not treat an unavailable offer as an order", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 409 }));
  await expect(placeOrder(input)).rejects.toThrow("Order was not confirmed");
});
`,
  },
  edited: {
    "src/components/Checkout.tsx": checkout
      .replace("useRef, ", "")
      .replace("  const requestId = useRef(crypto.randomUUID());\n", "")
      .replace(
        "requestId: requestId.current",
        "requestId: crypto.randomUUID()",
      ),
    "src/api/orders.ts": orderApiBefore,
  },
  added: ["tests/orders.test.ts"],
};
