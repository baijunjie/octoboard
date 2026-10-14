import { webFiles, type RepositoryExample } from "./types";

const availabilityBefore = `import type { StoreOffer } from "./store-offer";

export function canPublish(offer: StoreOffer, now: Date): boolean {
  return offer.status === "draft";
}
`;
const availability = `import type { StoreOffer } from "./store-offer";

export function canPublish(offer: StoreOffer, now: Date): boolean {
  return offer.status === "draft"
    && offer.eligible
    && offer.remaining > 0
    && Date.parse(offer.startsAt) <= now.getTime()
    && now.getTime() < Date.parse(offer.endsAt);
}
`;
const storeOffers = `import { useEffect, useState } from "react";
import { listOffers, publishOffer } from "../api/store-offers";
import { canPublish } from "../domain/availability";
import type { StoreOffer } from "../domain/store-offer";
import { StockEditor } from "../components/StockEditor";
import { RedemptionForm } from "../components/RedemptionForm";

export function StoreOffers() {
  const [offers, setOffers] = useState<StoreOffer[]>([]);
  const [error, setError] = useState("");
  const refresh = () => listOffers().then(setOffers);
  useEffect(() => { void refresh().catch(() => setError("Unable to load offers")); }, []);
  async function publish(id: string) {
    try { await publishOffer(id); await refresh(); }
    catch { setError("Check availability and stock before publishing."); }
  }
  return <main><h1>Store meal deals</h1><p role="alert">{error}</p>
    {offers.map((offer) => <article key={offer.id}>
      <h2>{offer.title}</h2><p>{offer.status} · {offer.remaining} remaining</p>
      <StockEditor offer={offer} onSaved={refresh} />
      <button disabled={!canPublish(offer, new Date())} onClick={() => publish(offer.id)}>Publish</button>
    </article>)}
    <RedemptionForm />
  </main>;
}
`;

export const merchant: RepositoryExample = {
  id: "merchant",
  branch: "feature/store-availability",
  files: {
    ...webFiles("merchant", "StoreOffers", "pages/StoreOffers"),
    "README.md":
      "# Bistro Merchant — fictional preview repository\n\nPrepare store listings, adjust inventory and redeem paid meal deal orders. Listings require stock, store eligibility and an active promotion window. Publish and redeem endpoints repeat these checks against the authenticated merchant's store.\n\nThese are display-only source examples; no merchant service is contacted.\n",
    "src/domain/store-offer.ts": `export interface StoreOffer {
  id: string;
  title: string;
  storeId: string;
  eligible: boolean;
  remaining: number;
  startsAt: string;
  endsAt: string;
  status: "draft" | "published" | "paused";
}
`,
    "src/domain/availability.ts": availability,
    "src/api/store-offers.ts": `import type { StoreOffer } from "../domain/store-offer";

export async function listOffers(): Promise<StoreOffer[]> {
  const response = await fetch("/api/merchant/offers");
  if (!response.ok) throw new Error("Unable to load store offers");
  return response.json();
}
export async function publishOffer(id: string): Promise<void> {
  const response = await fetch("/api/merchant/offers/" + encodeURIComponent(id) + "/publish", { method: "POST" });
  if (!response.ok) throw new Error("Offer is no longer available to publish");
}
export async function setStock(id: string, remaining: number): Promise<void> {
  if (!Number.isSafeInteger(remaining) || remaining < 0) throw new Error("Invalid stock");
  const response = await fetch("/api/merchant/offers/" + encodeURIComponent(id) + "/stock", {
    method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ remaining }),
  });
  if (!response.ok) throw new Error("Unable to update stock");
}
`,
    "src/pages/StoreOffers.tsx": storeOffers,
    "src/components/StockEditor.tsx": `import { useState } from "react";
import { setStock } from "../api/store-offers";
import type { StoreOffer } from "../domain/store-offer";

export function StockEditor({ offer, onSaved }: { offer: StoreOffer; onSaved: () => Promise<void> }) {
  const [quantity, setQuantity] = useState(offer.remaining);
  const [message, setMessage] = useState("");
  return <form onSubmit={async (event) => {
    event.preventDefault();
    try { await setStock(offer.id, quantity); await onSaved(); setMessage("Stock saved"); }
    catch { setMessage("Stock could not be updated"); }
  }}>
    <label>Available portions<input type="number" min="0" step="1" value={quantity}
      onChange={(event) => setQuantity(event.target.valueAsNumber)} required /></label>
    <button>Save stock</button><span role="status">{message}</span>
  </form>;
}
`,
    "src/api/redemptions.ts": `export async function redeemOrder(code: string): Promise<{ orderId: string }> {
  const response = await fetch("/api/merchant/redemptions", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code: code.trim() }),
  });
  if (!response.ok) throw new Error("Code expired, already redeemed or belongs to another store");
  return response.json();
}
`,
    "src/components/RedemptionForm.tsx": `import { useState } from "react";
import { redeemOrder } from "../api/redemptions";

export function RedemptionForm() {
  const [code, setCode] = useState("");
  const [message, setMessage] = useState("");
  const [pending, setPending] = useState(false);
  return <form onSubmit={async (event) => {
    event.preventDefault(); setPending(true);
    try { const order = await redeemOrder(code); setMessage("Redeemed order " + order.orderId); setCode(""); }
    catch (error) { setMessage(error instanceof Error ? error.message : "Redemption failed"); }
    finally { setPending(false); }
  }}><h2>Redeem a meal deal</h2>
    <label>Redemption code<input value={code} onChange={(event) => setCode(event.target.value)} required /></label>
    <button disabled={pending}>Confirm redemption</button><p role="status">{message}</p>
  </form>;
}
`,
    "tests/availability.test.ts": `import { expect, it } from "vitest";
import { canPublish } from "../src/domain/availability";
import type { StoreOffer } from "../src/domain/store-offer";

const now = new Date("2026-10-15T04:00:00Z");
const offer: StoreOffer = { id: "lunch-two", title: "Lunch for two", storeId: "store-riverside",
  eligible: true, remaining: 24, status: "draft",
  startsAt: "2026-10-15T02:00:00Z", endsAt: "2026-10-31T14:00:00Z" };

it("allows a stocked offer at an eligible store", () => expect(canPublish(offer, now)).toBe(true));
it.each([
  { remaining: 0 }, { eligible: false }, { status: "published" as const },
  { startsAt: "2026-11-01T00:00:00Z" }, { endsAt: now.toISOString() },
])("blocks an unavailable listing: %j", (patch) => {
  expect(canPublish({ ...offer, ...patch }, now)).toBe(false);
});
`,
  },
  edited: {
    "src/domain/availability.ts": availabilityBefore,
    "src/pages/StoreOffers.tsx": storeOffers
      .replace(
        'import { RedemptionForm } from "../components/RedemptionForm";\n',
        "",
      )
      .replace("    <RedemptionForm />\n", ""),
  },
  added: ["src/components/RedemptionForm.tsx", "tests/availability.test.ts"],
};
