import { webFiles, type RepositoryExample } from "./types";

export const pos: RepositoryExample = {
  id: "pos",
  branch: "feat/checkout",
  files: {
    ...webFiles("pos", "PosApp", "PosApp"),
    "README.md": `# Bistro POS — fictional preview repository

A cashier checkout station for quoting table orders, taking payment and redeeming prepaid collection codes. Quotes come from the server and select exactly one discount: meal deals cannot stack with member discounts. Payment is tied to a quote and a stable attempt ID. Redemption requires an explicit cashier action, an active code and server confirmation; scanning alone never marks an order collected.

Built as a React DOM and TypeScript web client with Vite and Vitest. Illustrative display-only source. Device payment and receipt hardware are represented by API contracts.
`,
    "src/domain/checkout.ts": `export type Discount = "none" | "member" | "meal_deal";
export interface Quote { id: string; totalCents: number; discount: Discount; expiresAt: string; }
export interface Receipt { id: string; orderId: string; paidCents: number; paymentStatus: "captured"; }

export function checkoutDiscount(hasMealDeal: boolean, memberRequested: boolean): Discount {
  if (hasMealDeal) return "meal_deal";
  return memberRequested ? "member" : "none";
}
export function quoteUsable(quote: Quote, now: number): boolean {
  return quote.totalCents >= 0 && Date.parse(quote.expiresAt) > now;
}
`,
    "src/domain/redemption.ts": `export interface RedemptionCheck {
  orderId: string;
  storeId: string;
  status: "redeemable" | "already_redeemed" | "unpaid" | "expired";
  title: string;
}
export interface RedemptionReceipt { orderId: string; redeemedAt: string; receiptId: string; }

export function normalizeCode(value: string): string {
  return value.trim().toUpperCase();
}
`,
    "src/api/checkout.ts": `import type { Discount, Quote, Receipt } from "../domain/checkout";

export async function quoteOrder(orderId: string, discount: Discount): Promise<Quote> {
  const response = await fetch("/api/pos/quotes", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ orderId, discount }),
  });
  if (!response.ok) throw new Error("Quote could not be calculated");
  return response.json();
}
export async function capturePayment(quoteId: string, requestId: string): Promise<Receipt> {
  const response = await fetch("/api/pos/payments", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ quoteId, requestId }),
  });
  if (!response.ok) throw new Error("Payment was not confirmed; check this attempt before retrying");
  return response.json();
}
`,
    "src/api/redemptions.ts": `import type { RedemptionCheck, RedemptionReceipt } from "../domain/redemption";

export async function inspectCode(code: string, storeId: string): Promise<RedemptionCheck> {
  const response = await fetch("/api/pos/redemptions/check", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code, storeId }),
  });
  if (!response.ok) throw new Error("Collection code could not be verified");
  return response.json();
}
export async function redeemCode(code: string, storeId: string, requestId: string): Promise<RedemptionReceipt> {
  const response = await fetch("/api/pos/redemptions", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code, storeId, requestId }),
  });
  if (!response.ok) throw new Error("Redemption was not confirmed");
  return response.json();
}
`,
    "src/PosApp.tsx": `import { useState } from "react";
import { CheckoutPanel } from "./components/CheckoutPanel";
import { RedemptionPanel } from "./components/RedemptionPanel";

export function PosApp() {
  const [mode, setMode] = useState<"checkout" | "collection">("checkout");
  return <main><h1>Riverside · Register 02</h1><nav aria-label="Register mode">
    <button onClick={() => setMode("checkout")}>Table checkout</button>
    <button onClick={() => setMode("collection")}>Prepaid collection</button>
  </nav><section hidden={mode !== "checkout"}><CheckoutPanel /></section>
    <section hidden={mode !== "collection"}><RedemptionPanel storeId="store-riverside" /></section>
  </main>;
}
`,
    "src/components/CheckoutPanel.tsx": `import { useRef, useState } from "react";
import { capturePayment, quoteOrder } from "../api/checkout";
import { checkoutDiscount, quoteUsable, type Quote, type Receipt } from "../domain/checkout";
import { PaymentReceipt } from "./PaymentReceipt";
import { money } from "../format/money";

export function CheckoutPanel() {
  const [orderId, setOrderId] = useState("");
  const [hasDeal, setHasDeal] = useState(false);
  const [member, setMember] = useState(false);
  const [quote, setQuote] = useState<Quote>();
  const [receipt, setReceipt] = useState<Receipt>();
  const [pending, setPending] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [error, setError] = useState("");
  const requestId = useRef(crypto.randomUUID());
  async function calculate() {
    setPending(true); setQuote(undefined); setError("");
    try { setQuote(await quoteOrder(orderId, checkoutDiscount(hasDeal, member))); }
    catch { setError("Unable to quote this order. Check its current status."); }
    finally { setPending(false); }
  }
  async function pay() {
    if (!quote || pending) return;
    if (!attempted && !quoteUsable(quote, Date.now())) { setQuote(undefined); setError("Quote expired. Calculate again."); return; }
    setPending(true); setAttempted(true);
    try { setReceipt(await capturePayment(quote.id, requestId.current)); }
    catch { setError("Payment outcome unknown. Recheck this attempt before starting another checkout."); }
    finally { setPending(false); }
  }
  if (receipt) return <PaymentReceipt receipt={receipt} />;
  return <section><h2>Take payment</h2><fieldset disabled={pending || attempted}>
    <label>Table order ID<input value={orderId} onChange={(event) => { setOrderId(event.target.value); setQuote(undefined); }} /></label>
    <label><input type="checkbox" checked={hasDeal} onChange={(event) => { setHasDeal(event.target.checked); setQuote(undefined); }} />Meal deal present</label>
    <label><input type="checkbox" disabled={hasDeal} checked={member && !hasDeal}
      onChange={(event) => { setMember(event.target.checked); setQuote(undefined); }} />Member discount</label>
    <button disabled={!orderId.trim()} onClick={calculate}>Calculate server quote</button>
    </fieldset>
    {quote && <><p>{money(quote.totalCents)} · {quote.discount.replaceAll("_", " ")}</p>
      <button disabled={pending} onClick={pay}>{attempted ? "Recheck payment attempt" : "Take payment"}</button></>}
    <p role="alert">{error}</p>
  </section>;
}
`,
    "src/components/RedemptionPanel.tsx": `import { useRef, useState } from "react";
import { inspectCode, redeemCode } from "../api/redemptions";
import { normalizeCode, type RedemptionCheck, type RedemptionReceipt } from "../domain/redemption";

export function RedemptionPanel({ storeId }: { storeId: string }) {
  const [input, setInput] = useState("");
  const [checked, setChecked] = useState<RedemptionCheck>();
  const [receipt, setReceipt] = useState<RedemptionReceipt>();
  const [pending, setPending] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [error, setError] = useState("");
  const requestId = useRef(crypto.randomUUID());
  async function inspect() {
    setPending(true); setError(""); setChecked(undefined);
    try { setChecked(await inspectCode(normalizeCode(input), storeId)); }
    catch { setError("Code verification failed. Do not hand over the order."); }
    finally { setPending(false); }
  }
  async function confirm() {
    if (pending || checked?.status !== "redeemable" || checked.storeId !== storeId) return;
    setPending(true); setAttempted(true);
    try { setReceipt(await redeemCode(normalizeCode(input), storeId, requestId.current)); }
    catch { setError("Collection not confirmed. Retry this attempt before handing over food."); }
    finally { setPending(false); }
  }
  return <section><h2>Collect prepaid order</h2>
    <label>Collection code<input value={input} disabled={pending || attempted || !!receipt}
      onChange={(event) => { setInput(event.target.value); setChecked(undefined); }} /></label>
    <button disabled={pending || attempted || !!receipt || !input.trim()} onClick={inspect}>Verify code</button>
    {checked && <><p>{checked.title} · {checked.status.replaceAll("_", " ")}</p>
      <button disabled={pending || !!receipt || checked.status !== "redeemable" || checked.storeId !== storeId}
        onClick={confirm}>{attempted ? "Recheck collection attempt" : "Confirm handover"}</button></>}
    {receipt && <p role="status">Collected order {receipt.orderId} · Receipt {receipt.receiptId}</p>}
    <p role="alert">{error}</p>
  </section>;
}
`,
    "src/components/PaymentReceipt.tsx": `import type { Receipt } from "../domain/checkout";
import { money } from "../format/money";

export function PaymentReceipt({ receipt }: { receipt: Receipt }) {
  return <article><h2>Payment captured</h2><dl>
    <dt>Receipt</dt><dd>{receipt.id}</dd><dt>Order</dt><dd>{receipt.orderId}</dd>
    <dt>Paid</dt><dd>{money(receipt.paidCents)}</dd></dl>
  </article>;
}
`,
    "src/format/money.ts": `const currency = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });
export const money = (cents: number): string => currency.format(cents / 100);
`,
    "tests/checkout.test.ts": `import { expect, it } from "vitest";
import { checkoutDiscount, quoteUsable } from "../src/domain/checkout";

it("does not stack a member discount on a meal deal", () => {
  expect(checkoutDiscount(true, true)).toBe("meal_deal");
  expect(checkoutDiscount(false, true)).toBe("member");
  expect(checkoutDiscount(false, false)).toBe("none");
});
it("requires a current server quote before the first payment attempt", () => {
  const quote = { id: "quote-17", totalCents: 2800, discount: "meal_deal" as const, expiresAt: "2026-10-15T04:05:00Z" };
  expect(quoteUsable(quote, Date.parse("2026-10-15T04:04:00Z"))).toBe(true);
  expect(quoteUsable(quote, Date.parse(quote.expiresAt))).toBe(false);
});
`,
    "tests/redemptions.test.ts": `import { afterEach, expect, it, vi } from "vitest";
import { redeemCode } from "../src/api/redemptions";

afterEach(() => vi.unstubAllGlobals());
it("requires server confirmation and preserves the attempt on retry", async () => {
  const fetch = vi.fn().mockResolvedValueOnce({ ok: false, status: 409 })
    .mockResolvedValueOnce({ ok: true, json: async () => ({ orderId: "order-482", receiptId: "receipt-91" }) });
  vi.stubGlobal("fetch", fetch);
  await expect(redeemCode("RX7M2Q", "store-riverside", "handover-91")).rejects.toThrow("not confirmed");
  await expect(redeemCode("RX7M2Q", "store-riverside", "handover-91")).resolves.toMatchObject({ orderId: "order-482" });
  expect(fetch.mock.calls.map((call) => JSON.parse(call[1].body).requestId)).toEqual(["handover-91", "handover-91"]);
});
`,
  },
  edited: {
    "src/domain/checkout.ts": `export type Discount = "none" | "member" | "meal_deal";
export interface Quote { id: string; totalCents: number; discount: Discount; expiresAt: string; }
export interface Receipt { id: string; orderId: string; paidCents: number; paymentStatus: "captured"; }

export function checkoutDiscount(hasMealDeal: boolean, memberRequested: boolean): Discount {
  if (memberRequested) return "member";
  return hasMealDeal ? "meal_deal" : "none";
}
export function quoteUsable(quote: Quote, now: number): boolean {
  return quote.totalCents >= 0 && Date.parse(quote.expiresAt) > now;
}
`,
    "src/api/redemptions.ts": `import type { RedemptionCheck, RedemptionReceipt } from "../domain/redemption";

export async function inspectCode(code: string, storeId: string): Promise<RedemptionCheck> {
  const response = await fetch("/api/pos/redemptions/check", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code, storeId }),
  });
  if (!response.ok) throw new Error("Collection code could not be verified");
  return response.json();
}
export async function redeemCode(code: string, storeId: string, requestId: string): Promise<RedemptionReceipt> {
  const response = await fetch("/api/pos/redemptions", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code, storeId, requestId }),
  });
  return response.json();
}
`,
  },
  added: ["tests/checkout.test.ts", "tests/redemptions.test.ts"],
};
