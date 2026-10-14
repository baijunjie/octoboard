import type { RepositoryExample } from "./types";

// The source examples are inert strings; the actual website has no database or order endpoint.
const orderBefore = `import type { OrderDatabase, OrderInput } from "./domain/order";
import { requirePositiveInteger, replaySameRequest } from "./domain/order";

export async function placeMealDealOrder(db: OrderDatabase, customer: { id: string }, input: OrderInput) {
  const { packageId, storeId, quantity, requestId } = input;
  requirePositiveInteger(quantity);

  return db.transaction(async (tx) => {
    const totalCents = (input.priceCents ?? 0) * quantity;
    await tx.stock.decrement(storeId, packageId, quantity);

    const order = await tx.orders.create({
      customerId: customer.id,
      packageId,
      storeId,
      quantity,
      totalCents,
      requestId,
      status: "pending_payment",
    });
    await tx.outbox.add("meal_deal_order_created", order.id);
    return order;
  });
}
`;
const orderAfter = `import type { OrderDatabase, OrderInput } from "./domain/order";
import { requirePositiveInteger, replaySameRequest } from "./domain/order";

export async function placeMealDealOrder(db: OrderDatabase, customer: { id: string }, input: OrderInput) {
  const { packageId, storeId, quantity, requestId } = input;
  requirePositiveInteger(quantity);

  return db.transaction(async (tx) => {
    await tx.lockRequest(customer.id, requestId);
    const prior = await tx.orders.findByRequest(customer.id, requestId);
    if (prior) return replaySameRequest(prior, input);

    const offer = await tx.packages.findActive(packageId, storeId, new Date());
    if (!offer) throw new Error("Meal deal unavailable at this store");
    const totalCents = offer.priceCents * quantity;
    const reserved = await tx.stock.decrementIfAvailable(storeId, packageId, quantity);
    if (!reserved) throw new Error("Insufficient stock");

    const order = await tx.orders.create({
      customerId: customer.id,
      packageId,
      storeId,
      quantity,
      totalCents,
      requestId,
      status: "pending_payment",
    });
    await tx.outbox.add("meal_deal_order_created", order.id);
    return order;
  });
}
`;
export const server: RepositoryExample = {
  id: "server",
  branch: "feature/atomic-meal-orders",
  files: {
    "README.md":
      "# Bistro Server — fictional preview repository\n\nThe order service validates promotion dates and store eligibility, calculates the authoritative price, reserves stock and writes an order plus outbox event in one transaction. The authenticated customer and request ID form the idempotency scope; a repeated key with a different payload is rejected.\n\nThese are review examples, not a running API. The OrderDatabase port describes the PostgreSQL adapter contract; tests use a local fake. Admin, catalogue and redemption HTTP handlers are outside this excerpt.\n",
    "package.json":
      JSON.stringify(
        {
          name: "@bistro/server",
          private: true,
          type: "module",
          scripts: { test: "vitest run", typecheck: "tsc --noEmit" },
          dependencies: { zod: "^4.0.0" },
          devDependencies: { typescript: "^5.9.0", vitest: "^3.0.0" },
        },
        null,
        2,
      ) + "\n",
    "tsconfig.json":
      '{\n  "compilerOptions": { "target": "ES2022", "module": "ESNext", "moduleResolution": "Bundler", "strict": true, "noEmit": true },\n  "include": ["src", "tests"]\n}\n',
    "src/meal-deal-orders.ts": orderAfter,
    "src/domain/order.ts": `export interface OrderInput {
  packageId: string;
  storeId: string;
  quantity: number;
  requestId: string;
  priceCents?: number; // Legacy clients may send this; it is never authoritative.
}
export interface Order extends OrderInput {
  id: string;
  customerId: string;
  totalCents: number;
  status: "pending_payment";
}
export interface OrderTransaction {
  lockRequest(customerId: string, requestId: string): Promise<void>;
  packages: { findActive(id: string, store: string, at: Date): Promise<{ priceCents: number } | null> };
  stock: {
    decrement(store: string, id: string, quantity: number): Promise<void>;
    decrementIfAvailable(store: string, id: string, quantity: number): Promise<boolean>;
  };
  orders: {
    findByRequest(customer: string, request: string): Promise<Order | null>;
    create(input: Omit<Order, "id">): Promise<Order>;
  };
  outbox: { add(event: string, orderId: string): Promise<void> };
}
export interface OrderDatabase {
  transaction<T>(work: (tx: OrderTransaction) => Promise<T>): Promise<T>;
}
export function requirePositiveInteger(value: number): void {
  if (!Number.isSafeInteger(value) || value < 1 || value > 20) throw new Error("Invalid quantity");
}
export function replaySameRequest(order: Order, input: OrderInput): Order {
  if (order.packageId !== input.packageId || order.storeId !== input.storeId || order.quantity !== input.quantity)
    throw new Error("Request ID already used with another order");
  return order;
}
`,
    "src/http/order-schema.ts": `import { z } from "zod";

export const orderSchema = z.object({
  packageId: z.string().min(1),
  storeId: z.string().min(1),
  quantity: z.number().int().min(1).max(20),
  requestId: z.uuid(),
});
// Unknown properties, including a client-supplied price, are stripped on parsing.
`,
    "src/http/create-order.ts": `import { placeMealDealOrder } from "../meal-deal-orders";
import type { OrderDatabase } from "../domain/order";
import { orderSchema } from "./order-schema";

export async function createOrder(db: OrderDatabase, customer: { id: string }, body: unknown) {
  const input = orderSchema.parse(body);
  const order = await placeMealDealOrder(db, customer, input);
  return { status: 201, body: order };
}
// The HTTP adapter supplies the customer from the authenticated session, never from the body.
`,
    "db/migrations/014_meal_deal_orders.sql": `CREATE TABLE meal_deal_orders (
  id uuid PRIMARY KEY,
  customer_id uuid NOT NULL,
  package_id uuid NOT NULL REFERENCES meal_deal_packages(id),
  store_id uuid NOT NULL REFERENCES stores(id),
  request_id uuid NOT NULL,
  quantity integer NOT NULL CHECK (quantity BETWEEN 1 AND 20),
  total_cents integer NOT NULL CHECK (total_cents > 0),
  status text NOT NULL DEFAULT 'pending_payment',
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (customer_id, request_id)
);
CREATE TABLE order_outbox (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  order_id uuid NOT NULL REFERENCES meal_deal_orders(id),
  event_type text NOT NULL,
  delivered_at timestamptz
);
`,
    "db/queries/active-package.sql": `-- findActive: dates and store membership are evaluated by the service clock.
SELECT p.price_cents
FROM meal_deal_packages p
JOIN package_stores s ON s.package_id = p.id
WHERE p.id = $1 AND s.store_id = $2
  AND p.starts_at <= $3 AND $3 < p.ends_at
  AND p.status = 'published';
`,
    "db/queries/reserve-stock.sql": `-- decrementIfAvailable: a single conditional update prevents overselling.
UPDATE package_stock
SET remaining = remaining - $3
WHERE store_id = $1 AND package_id = $2 AND remaining >= $3
RETURNING remaining;
`,
    "db/queries/lock-request.sql": `-- Hold the request lock until the surrounding transaction commits or rolls back.
-- A retry waits here, then sees the first order through findByRequest.
SELECT pg_advisory_xact_lock(hashtextextended($1 || ':' || $2, 0));
`,
    "tests/fixtures/order-database.ts": `import { vi } from "vitest";
import type { OrderDatabase, OrderTransaction } from "../../src/domain/order";

export function orderDatabase() {
  const tx: OrderTransaction = {
    lockRequest: vi.fn().mockResolvedValue(undefined),
    packages: { findActive: vi.fn().mockResolvedValue({ priceCents: 2800 }) },
    stock: { decrement: vi.fn(), decrementIfAvailable: vi.fn().mockResolvedValue(true) },
    orders: {
      findByRequest: vi.fn().mockResolvedValue(null),
      create: vi.fn(async (input) => ({ ...input, id: "order-1" })),
    },
    outbox: { add: vi.fn().mockResolvedValue(undefined) },
  };
  const db: OrderDatabase = { transaction: async (work) => work(tx) };
  return { db, tx };
}
`,
    "tests/meal-deal-orders.test.ts": `import { expect, it, vi } from "vitest";
import { placeMealDealOrder } from "../src/meal-deal-orders";
import { orderDatabase } from "./fixtures/order-database";

const customer = { id: "customer-1" };
const input = { packageId: "lunch-two", storeId: "store-riverside", quantity: 2, requestId: "request-1" };

it("uses the active package price instead of the client's price", async () => {
  const { db, tx } = orderDatabase();
  const order = await placeMealDealOrder(db, customer, { ...input, priceCents: 1 });
  expect(order.totalCents).toBe(5600);
  expect(tx.stock.decrementIfAvailable).toHaveBeenCalledWith(input.storeId, input.packageId, 2);
});
it("replays an existing request without reserving more stock", async () => {
  const { db, tx } = orderDatabase();
  const first = await placeMealDealOrder(db, customer, input);
  vi.mocked(tx.orders.findByRequest).mockResolvedValue(first);
  vi.mocked(tx.stock.decrementIfAvailable).mockClear();
  expect(await placeMealDealOrder(db, customer, input)).toEqual(first);
  expect(tx.stock.decrementIfAvailable).not.toHaveBeenCalled();
  await expect(placeMealDealOrder(db, customer, { ...input, quantity: 3 })).rejects.toThrow("Request ID");
});
it("rejects an inactive package or an ineligible store", async () => {
  const { db, tx } = orderDatabase();
  vi.mocked(tx.packages.findActive).mockResolvedValue(null);
  await expect(placeMealDealOrder(db, customer, input)).rejects.toThrow("unavailable");
  expect(tx.orders.create).not.toHaveBeenCalled();
});
it("does not write an order or event when stock is insufficient", async () => {
  const { db, tx } = orderDatabase();
  vi.mocked(tx.stock.decrementIfAvailable).mockResolvedValue(false);
  await expect(placeMealDealOrder(db, customer, input)).rejects.toThrow("Insufficient stock");
  expect(tx.orders.create).not.toHaveBeenCalled();
  expect(tx.outbox.add).not.toHaveBeenCalled();
});
`,
    "docs/transaction-contract.md":
      "# Order transaction contract\n\nAll operations use the same database transaction. Acquire the advisory request lock before looking up an existing order; the unique customer/request constraint is a final guard. Validate active package and store eligibility, then decrement stock with a conditional UPDATE. Write the order and outbox event before commit. Any failure rolls back the reservation.\n\nThe test fake checks service call decisions only; it does not establish SQL isolation or concurrent correctness. A real adapter requires PostgreSQL integration tests. Discount stacking is disabled pending product confirmation.\n",
  },
  edited: { "src/meal-deal-orders.ts": orderBefore },
  added: ["tests/meal-deal-orders.test.ts", "db/queries/reserve-stock.sql"],
};
