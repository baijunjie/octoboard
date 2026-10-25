import { previewRepositories } from "./repositories";
import type { SessionTerminalCopy, TerminalQuestion, TerminalStep } from "./terminals";

/** What a step's output is. Test runs count the cases of the project's own test files, so the
 * output agrees with the repository a visitor can open beside the terminal. */
type RunOutput =
  | string
  | { tests: string[]; filter?: string }
  | { shortstat: string };

/** One thing a session's agent did or said, in terms that do not depend on the language: the
 * paths, commands and counts are read from the sample repositories, and `say` names a block of
 * the agent's prose by its key in the locale. */
type ScriptStep =
  | { say: string }
  | { read: string }
  /** `diff` picks the line pair to show by a fragment each of the removed and the added line
   * contains; `null` for a change that removes nothing. */
  | { edit: string; diff?: [removed: string | null, added: string] }
  | { run: string; output: RunOutput };

interface SessionScript {
  /** The project whose repository the paths belong to; the coordinator has none. */
  project?: string;
  /** The steps on screen when the page opens. */
  done: ScriptStep[];
  /** The steps appended one by one while the session works. */
  live: ScriptStep[];
}

/** The agent's words and the user's prompt for one session, in one language. */
export interface SessionProse {
  prompt: string;
  /** The status line's verb, such as "Checking". */
  verb: string;
  say: Record<string, string[]>;
  /** What a session waiting for the user asks, below its `done` steps. */
  question?: TerminalQuestion;
}

export const terminalScripts: Record<string, SessionScript> = {
  "preview-coordinator": {
    done: [{ say: "plan" }, { say: "shared" }, { say: "status" }, { say: "decision" }],
    live: [
      { run: "git -C /demo/bistro/server diff --cached --shortstat", output: { shortstat: "preview-server" } },
      { say: "pricesAgree" },
      { say: "collecting" },
    ],
  },
  "preview-admin-session": {
    project: "preview-admin",
    done: [
      { read: "src/domain/promotion.ts" },
      { edit: "src/domain/promotion.ts", diff: ["z.array(z.string())", "min(1, "] },
      { edit: "src/pages/PromotionEditor.tsx", diff: ["Check the promotion details", "issues[0].message"] },
      { edit: "tests/promotion.test.ts" },
      { run: "pnpm test tests/promotion.test.ts", output: { tests: ["tests/promotion.test.ts"] } },
      { say: "summary" },
    ],
    live: [],
  },
  "preview-merchant-session": {
    project: "preview-merchant",
    done: [
      { read: "src/domain/store-offer.ts" },
      { edit: "src/domain/availability.ts", diff: ['offer.status === "draft";', "offer.remaining > 0"] },
      { edit: "src/pages/StoreOffers.tsx", diff: [null, "<RedemptionForm />"] },
      { edit: "src/components/RedemptionForm.tsx" },
      { edit: "tests/availability.test.ts" },
      { say: "summary" },
    ],
    live: [
      { say: "checking" },
      { read: "src/api/redemptions.ts" },
      { run: "pnpm test tests/availability.test.ts", output: { tests: ["tests/availability.test.ts"] } },
      { run: "pnpm tsc --noEmit", output: "No errors" },
    ],
  },
  "preview-customer-session": {
    project: "preview-customer",
    done: [
      { read: "src/domain/meal-deal.ts" },
      { edit: "src/components/Checkout.tsx", diff: ["requestId: crypto.randomUUID()", "useRef(crypto.randomUUID())"] },
      { edit: "src/api/orders.ts", diff: [null, "Order was not confirmed"] },
      { edit: "tests/orders.test.ts" },
      { run: "pnpm test tests/orders.test.ts", output: { tests: ["tests/orders.test.ts"] } },
      { say: "summary" },
    ],
    live: [
      { say: "checking" },
      { read: "src/components/OrderSummary.tsx" },
      { read: "src/api/meal-deals.ts" },
      { run: "pnpm tsc --noEmit", output: "No errors" },
    ],
  },
  "preview-server-session": {
    project: "preview-server",
    done: [
      { read: "src/domain/order.ts" },
      { edit: "src/meal-deal-orders.ts", diff: ["totalCents = (input.priceCents", "totalCents = offer.priceCents"] },
      { edit: "db/queries/reserve-stock.sql" },
      { edit: "tests/meal-deal-orders.test.ts" },
      { run: "pnpm test tests/meal-deal-orders.test.ts", output: { tests: ["tests/meal-deal-orders.test.ts"] } },
      { say: "summary" },
    ],
    live: [
      { say: "checking" },
      { read: "src/http/create-order.ts" },
      { read: "src/http/order-schema.ts" },
      { run: "pnpm tsc --noEmit", output: "No errors" },
    ],
  },
  "preview-review": {
    project: "preview-server",
    done: [
      { read: "tests/meal-deal-orders.test.ts" },
      { read: "src/domain/order.ts" },
      { say: "finding" },
      { edit: "src/meal-deal-orders.ts", diff: ["totalCents = (input.priceCents", "totalCents = offer.priceCents"] },
      { run: "pnpm test --watch", output: "Running in the background" },
      { say: "summary" },
    ],
    live: [],
  },
  "preview-order-checks": {
    project: "preview-server",
    done: [
      { run: "pnpm test tests/meal-deal-orders.test.ts", output: { tests: ["tests/meal-deal-orders.test.ts"] } },
      { run: "pnpm test tests/meal-deal-orders.test.ts -t stock", output: { tests: ["tests/meal-deal-orders.test.ts"], filter: "stock" } },
      { say: "summary" },
    ],
    live: [],
  },
  "preview-mobile-session": {
    project: "preview-mobile",
    done: [
      { read: "src/domain/mobile-order.ts" },
      { edit: "src/domain/redemption.ts", diff: ["pass.code.length > 0;", "order.id === pass.orderId"] },
      { edit: "src/components/PlaceOrder.tsx", diff: ["crypto.randomUUID()", "useRef(crypto.randomUUID())"] },
      { edit: "tests/redemption.test.ts" },
      { run: "pnpm test tests/redemption.test.ts", output: { tests: ["tests/redemption.test.ts"] } },
      { say: "summary" },
    ],
    live: [
      { say: "checking" },
      { read: "src/components/DealList.tsx" },
      { read: "src/components/CollectionPass.tsx" },
      { run: "pnpm tsc --noEmit", output: "No errors" },
    ],
  },
  "preview-tablet-session": {
    project: "preview-tablet",
    done: [
      { read: "src/domain/table-order.ts" },
      { edit: "src/domain/add-ons.ts", diff: ["selected.some", "> extra.maximum"] },
      { edit: "src/api/table-orders.ts", diff: ['"Content-Type": "application/json" }', '"If-Match"'] },
      { edit: "tests/add-ons.test.ts" },
      { run: "pnpm test tests/add-ons.test.ts", output: { tests: ["tests/add-ons.test.ts"] } },
      { say: "summary" },
    ],
    live: [
      { say: "checking" },
      { read: "src/api/table-menu.ts" },
      { edit: "tests/table-sync.test.ts" },
      { run: "pnpm test tests/table-sync.test.ts", output: { tests: ["tests/table-sync.test.ts"] } },
    ],
  },
  "preview-pos-session": {
    project: "preview-pos",
    done: [
      { read: "src/domain/redemption.ts" },
      { edit: "src/domain/checkout.ts", diff: ['if (memberRequested) return "member"', 'if (hasMealDeal) return "meal_deal"'] },
      { edit: "src/api/redemptions.ts", diff: [null, "Redemption was not confirmed"] },
      { edit: "tests/checkout.test.ts" },
      { edit: "tests/redemptions.test.ts" },
      {
        run: "pnpm test tests/checkout.test.ts tests/redemptions.test.ts",
        output: { tests: ["tests/checkout.test.ts", "tests/redemptions.test.ts"] },
      },
      { say: "summary" },
    ],
    live: [],
  },
  "preview-kitchen-session": {
    project: "preview-kitchen",
    done: [
      { read: "src/domain/ticket.ts" },
      { edit: "src/domain/print-job.ts", diff: ['case "queued": return "Paper ticket printed"', "Queued at the store gateway"] },
      { edit: "src/api/tickets.ts", diff: ['"Content-Type": "application/json" }', '"If-Match"'] },
      { edit: "tests/print-job.test.ts" },
      { run: "pnpm test tests/print-job.test.ts", output: { tests: ["tests/print-job.test.ts"] } },
      { say: "summary" },
    ],
    live: [
      { say: "checking" },
      { read: "src/api/printer.ts" },
      { edit: "tests/tickets.test.ts" },
      { run: "pnpm test tests/tickets.test.ts", output: { tests: ["tests/tickets.test.ts"] } },
    ],
  },
};

const repositoryOf = (project: string) => {
  const repository = previewRepositories.get(project);
  if (!repository) throw new Error(`No sample repository for ${project}`);
  return repository;
};

const textOf = (project: string, path: string) => {
  const file = repositoryOf(project).files[path];
  if (!file || !("text" in file)) throw new Error(`${project} has no file ${path}`);
  return file.text;
};

const linesOf = (text: string) => text.split("\n").length - 1;

/** The lines the project's staged patch for `path` adds and removes. */
function patchOf(project: string, path: string): { added: string[]; removed: string[] } {
  const repository = repositoryOf(project);
  const changes = repository.git.changes[`${project}-feature`];
  const patch = Array.isArray(changes)
    ? changes.find((change) => change.patch?.startsWith(`diff --git a/${path} `))?.patch
    : undefined;
  if (patch === undefined) throw new Error(`${project} has no staged change to ${path}`);
  const lines = patch.split("\n");
  const body = lines.slice(lines.findIndex((line) => line.startsWith("@@")) + 1);
  return {
    added: body.filter((line) => line.startsWith("+")).map((line) => line.slice(1)),
    removed: body.filter((line) => line.startsWith("-")).map((line) => line.slice(1)),
  };
}

const lineWith = (lines: string[], fragment: string, what: string) => {
  const found = lines.find((line) => line.includes(fragment));
  if (found === undefined) throw new Error(`No ${what} line contains ${fragment}`);
  return found.trim();
};

/** The vitest summary for the cases of `files` (those whose title has `filter` run, the rest are
 * skipped). An `it.each` counts one case per row of its table, each row being one flat object
 * literal; a form this does not count throws, so a fixture edit cannot leave the summary wrong. */
function testSummary(project: string, files: string[], filter?: string): string {
  const titles = files.flatMap((file) => {
    const text = textOf(project, file);
    if (/^\s*(test[.(]|describe\.|it\.(?!each\())/m.test(text)) throw new Error(`${file} has cases this summary cannot count`);
    const single = [...text.matchAll(/^\s*it\("([^"]*)"/gm)].map((match) => match[1]);
    const tables = [...text.matchAll(/^\s*it\.each\(\[([^\]]*)\]\)\("([^"]*)"/gm)];
    if (tables.length !== (text.match(/^\s*it\.each\(/gm) ?? []).length)
      throw new Error(`${file} has an it.each table this summary cannot count`);
    const table = tables.flatMap((match) => Array.from(match[1].matchAll(/\{[^{}]*\}/g), () => match[2]));
    return [...single, ...table];
  });
  const run = filter === undefined ? titles.length : titles.filter((title) => title.includes(filter)).length;
  const skipped = titles.length - run;
  return `Tests  ${run} passed${skipped > 0 ? ` | ${skipped} skipped` : ""} (${titles.length})`;
}

function shortstat(project: string): string {
  const changes = repositoryOf(project).git.changes[`${project}-feature`];
  if (!Array.isArray(changes)) throw new Error(`${project} has no change list`);
  let added = 0;
  let removed = 0;
  for (const change of changes) {
    const path = /^diff --git a\/(\S+) /.exec(change.patch ?? "")?.[1];
    if (path === undefined) continue;
    const lines = patchOf(project, path);
    added += lines.added.length;
    removed += lines.removed.length;
  }
  const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? "" : "s"}`;
  return ` ${plural(changes.length, "file")} changed, ${plural(added, "insertion")}(+), ${plural(removed, "deletion")}(-)`;
}

function resolve(script: SessionScript, prose: SessionProse, step: ScriptStep): TerminalStep {
  const { project } = script;
  if ("say" in step) {
    const lines = prose.say[step.say];
    if (!lines) throw new Error(`No prose for ${step.say}`);
    return { say: lines };
  }
  if ("read" in step) {
    if (!project) throw new Error("A read needs a project");
    return { read: step.read, lines: linesOf(textOf(project, step.read)) };
  }
  if ("edit" in step) {
    if (!project) throw new Error("An edit needs a project");
    const { added, removed } = patchOf(project, step.edit);
    const diff = step.diff && [
      ...(step.diff[0] === null ? [] : [{ removed: lineWith(removed, step.diff[0], "removed") }]),
      { added: lineWith(added, step.diff[1], "added") },
    ];
    return { edit: step.edit, added: added.length, removed: removed.length, ...(diff && { diff }) };
  }
  const { output } = step;
  if (typeof output === "string") return { run: step.run, result: output };
  if ("shortstat" in output) return { run: step.run, result: shortstat(output.shortstat) };
  if (!project) throw new Error("A test run needs a project");
  return { run: step.run, result: testSummary(project, output.tests, output.filter) };
}

/** What the session's terminal shows: its script with the repository facts filled in and the
 * agent's prose in the language of `prose`. */
export function terminalCopy(id: string, prose: SessionProse): SessionTerminalCopy {
  const script = terminalScripts[id];
  if (!script) throw new Error(`No terminal script for ${id}`);
  return {
    prompt: prose.prompt,
    verb: prose.verb,
    done: script.done.map((step) => resolve(script, prose, step)),
    live: script.live.map((step) => resolve(script, prose, step)),
    question: prose.question,
  };
}
