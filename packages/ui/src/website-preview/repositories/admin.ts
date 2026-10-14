import { webFiles, type RepositoryExample } from "./types";

const schemaBefore = `import { z } from "zod";

export const promotionSchema = z.object({
  title: z.string().trim().min(1),
  priceCents: z.number().int().positive(),
  startsAt: z.iso.datetime(),
  endsAt: z.iso.datetime(),
  storeIds: z.array(z.string()),
  stackWithMemberDiscount: z.literal(false),
});
export type PromotionDraft = z.infer<typeof promotionSchema>;
`;
const schema = schemaBefore
  .replace(
    "z.array(z.string())",
    'z.array(z.string().min(1)).min(1, "Select at least one store")',
  )
  .replace(
    "});\nexport type",
    `}).refine((draft) => Date.parse(draft.endsAt) > Date.parse(draft.startsAt), {
  path: ["endsAt"],
  message: "The promotion must end after it starts",
});
export type`,
  );
const editor = `import { useState, type FormEvent } from "react";
import { savePromotion } from "../api/promotions";
import { promotionSchema } from "../domain/promotion";
import { StorePicker } from "../components/StorePicker";

export function PromotionEditor() {
  const [storeIds, setStoreIds] = useState<string[]>([]);
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState("");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const fields = new FormData(event.currentTarget);
    const parsed = promotionSchema.safeParse({
      title: fields.get("title"),
      priceCents: Math.round(Number(fields.get("price")) * 100),
      startsAt: new Date(String(fields.get("startsAt"))).toISOString(),
      endsAt: new Date(String(fields.get("endsAt"))).toISOString(),
      storeIds,
      stackWithMemberDiscount: false,
    });
    if (!parsed.success) return setMessage(parsed.error.issues[0].message);
    setPending(true);
    try {
      await savePromotion(parsed.data);
      setMessage("Promotion saved. Stores can now prepare their listings.");
    } catch { setMessage("Unable to save. Please try again."); }
    finally { setPending(false); }
  }
  return <main><h1>Lunch meal deal</h1><form onSubmit={submit}>
    <label>Package name<input name="title" defaultValue="Lunch for two" required /></label>
    <label>Package price<input name="price" type="number" min="0.01" step="0.01" required /></label>
    <label>Starts<input name="startsAt" type="datetime-local" required /></label>
    <label>Ends<input name="endsAt" type="datetime-local" required /></label>
    <StorePicker value={storeIds} onChange={setStoreIds} />
    <p>Member discounts are not combined with this offer.</p>
    <button disabled={pending}>{pending ? "Saving…" : "Save promotion"}</button>
    <p role="status">{message}</p>
  </form></main>;
}
`;
export const admin: RepositoryExample = {
  id: "admin",
  branch: "feature/promotion-rules",
  files: {
    ...webFiles("admin", "PromotionEditor", "pages/PromotionEditor"),
    "README.md":
      "# Bistro Admin — fictional preview repository\n\nConfigure meal deal pricing, UTC promotion windows and eligible stores. The editor validates before POST /api/admin/promotions; the service repeats validation and enforces staff permissions. Member discounts remain separate pending the product decision.\n\nThe source shown here is illustrative and never runs in the website preview.\n",
    "src/pages/PromotionEditor.tsx": editor,
    "src/domain/promotion.ts": schema,
    "src/api/promotions.ts": `import type { PromotionDraft } from "../domain/promotion";

export async function savePromotion(draft: PromotionDraft): Promise<{ id: string }> {
  const response = await fetch("/api/admin/promotions", {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(draft),
  });
  if (!response.ok) throw new Error("Promotion could not be saved");
  return response.json();
}
`,
    "src/components/StorePicker.tsx": `import { stores } from "../data/stores";

type Props = { value: string[]; onChange: (ids: string[]) => void };
export function StorePicker({ value, onChange }: Props) {
  return <fieldset><legend>Eligible stores</legend>{stores.map((store) =>
    <label key={store.id}><input type="checkbox" checked={value.includes(store.id)}
      onChange={(event) => onChange(event.target.checked
        ? [...value, store.id] : value.filter((id) => id !== store.id))} />
      {store.name}
    </label>
  )}</fieldset>;
}
`,
    "src/data/stores.ts": `export const stores = [
  { id: "store-riverside", name: "Riverside" },
  { id: "store-station", name: "Station Square" },
  { id: "store-market", name: "Market Street" },
];
`,
    "tests/promotion.test.ts": `import { describe, expect, it } from "vitest";
import { promotionSchema } from "../src/domain/promotion";

const draft = { title: "Lunch for two", priceCents: 2800,
  startsAt: "2026-10-15T02:00:00Z", endsAt: "2026-10-31T14:00:00Z",
  storeIds: ["store-riverside"], stackWithMemberDiscount: false };

describe("promotion publishing rules", () => {
  it("accepts a priced offer with a store and valid window", () => {
    expect(promotionSchema.safeParse(draft).success).toBe(true);
  });
  it("requires an eligible store", () => {
    expect(promotionSchema.safeParse({ ...draft, storeIds: [] }).success).toBe(false);
  });
  it("rejects a reversed promotion window", () => {
    expect(promotionSchema.safeParse({ ...draft, endsAt: draft.startsAt }).success).toBe(false);
  });
  it("does not combine membership discounts", () => {
    expect(promotionSchema.safeParse({ ...draft, stackWithMemberDiscount: true }).success).toBe(false);
  });
});
`,
  },
  edited: {
    "src/domain/promotion.ts": schemaBefore,
    "src/pages/PromotionEditor.tsx": editor.replace(
      "    if (!parsed.success) return setMessage(parsed.error.issues[0].message);",
      '    if (!parsed.success) return setMessage("Check the promotion details.");',
    ),
  },
  added: ["tests/promotion.test.ts"],
};
