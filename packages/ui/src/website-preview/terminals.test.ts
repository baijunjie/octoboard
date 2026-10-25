// @vitest-environment jsdom
/* eslint-disable no-control-regex -- the tests read escape sequences. */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cells } from "../gallery/fakeTerminal";
import { createPreviewScenario } from "./scenarios";
import { previewCopy as english } from "./locales/en";
import { previewCopy as chinese } from "./locales/zh-Hans";
import { createTerminalScript, liveSchedule, type TerminalSession } from "./terminals";
import { terminalScripts } from "./terminalScripts";
import { previewRepositories } from "./repositories";

beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(1_000_000); });
afterEach(() => vi.useRealTimers());

const plain = (text: string) => text.replace(/\x1b\[[0-9;]*m/g, "");

const sessionsOf = (language: "en" | "zh-Hans") => {
  const { scenario, output } = createPreviewScenario("overview", language);
  return [...scenario.state!.sessions!.keys()].map((id) => {
    const script = output(id);
    if (typeof script !== "object") throw new Error(`${id} has no script`);
    return { id, session: scenario.state!.sessions!.get(id)!, script };
  });
};

describe("the preview's agent terminals", () => {
  it("has the same sessions, prose keys and options in both languages", () => {
    expect(Object.keys(english.terminals)).toEqual(Object.keys(terminalScripts));
    expect(Object.keys(chinese.terminals)).toEqual(Object.keys(english.terminals));
    for (const [id, prose] of Object.entries(english.terminals)) {
      const other = chinese.terminals[id];
      expect(Object.keys(other.say), id).toEqual(Object.keys(prose.say));
      expect(other.question?.options.length ?? 0, id).toBe(prose.question?.options.length ?? 0);
    }
  });

  it("prints what a tool prints in English in every language", () => {
    const toolRows = (language: "en" | "zh-Hans") =>
      sessionsOf(language).map(({ script }) =>
        plain(script.screen(80, 0)).split("\r\n").filter((row) => /Read \d+ lines|Tests {2}\d|Running in the background|additions|No errors/.test(row)),
      );
    expect(toolRows("zh-Hans")).toEqual(toolRows("en"));
    expect(toolRows("zh-Hans").flat().length).toBeGreaterThan(10);
  });

  it("agrees with the sample repositories the visitor can open", () => {
    const server = previewRepositories.get("preview-server")!;
    const order = (server.files["src/domain/order.ts"] as { text: string }).text;
    const sessions = Object.fromEntries(sessionsOf("en").map(({ id, script }) => [id, plain(script.screen(80, 0))]));
    const review = sessions["preview-review"];
    expect(review).toContain(`Read(src/domain/order.ts)\r\n  ⎿  Read ${order.split("\n").length - 1} lines`);
    expect(review).toContain("- const totalCents = (input.priceCents ?? 0) * quantity;");
    expect(review).toContain("+ const totalCents = offer.priceCents * quantity;");
    const patch = (server.git.changes["preview-server-feature"] as { patch?: string }[]).find((change) =>
      change.patch?.startsWith("diff --git a/src/meal-deal-orders.ts "))!.patch!;
    const count = (sign: string) => patch.split("\n").filter((line) => line.startsWith(sign) && !line.startsWith(sign.repeat(3))).length;
    expect(review).toContain(`with ${count("+")} additions and ${count("-")} removals`);
    expect(sessions["preview-order-checks"]).toContain("Tests  4 passed (4)");
    expect(sessions["preview-order-checks"]).toContain("Tests  2 passed | 2 skipped (4)");
  });

  it.each(["en", "zh-Hans"] as const)("keeps every row of the working tail within the width in %s", (language) => {
    for (const { id, session, script } of sessionsOf(language)) {
      if (session.status !== "working") continue;
      for (const cols of [40, 60, 100]) {
        for (const step of [1, 3, 500]) {
          const bytes = script.tick!(cols, step);
          const up = Number(/^\x1b\[(\d+)A/.exec(bytes)?.[1]);
          const rows = bytes.split("\x1b[J")[1].split("\r\n");
          const tail = rows.slice(-(up + 1));
          expect(tail, `${id} @${cols}`).toHaveLength(up + 1);
          for (const row of tail) expect(cells(row), `${id} @${cols}: ${plain(row)}`).toBeLessThanOrEqual(cols);
        }
      }
    }
  });

  it("keeps the input box and rules of every session within the width", () => {
    for (const language of ["en", "zh-Hans"] as const)
      for (const { script } of sessionsOf(language))
        for (const cols of [40, 100])
          for (const row of script.screen(cols, 0).split("\r\n"))
            if (/^(?:\x1b\[[0-9;]*m)*[╭│╰─]/.test(row)) expect(cells(row)).toBeLessThanOrEqual(cols);
  });

  it("appends a live step once it is due and none after the list is spent", () => {
    const id = "preview-merchant-session";
    const { script } = sessionsOf("en").find((entry) => entry.id === id)!;
    const schedule = liveSchedule(id, 4);
    expect(schedule.map((at, index) => at - (schedule[index - 1] ?? 0))).toSatisfy((gaps: number[]) =>
      gaps.every((gap) => gap >= 4000 && gap <= 7000));
    const rewritten = (bytes: string) => plain(bytes.split("\x1b[J")[1]);
    script.screen(80, 0);
    vi.setSystemTime(1_000_000 + schedule[0] - 1);
    expect(rewritten(script.tick!(80, 1))).not.toContain("Checking sold-out");
    vi.setSystemTime(1_000_000 + schedule[0]);
    expect(rewritten(script.tick!(80, 2))).toContain("Checking sold-out and already-redeemed states.");
    // A step is written once: the next tick rewrites the tail alone.
    expect(rewritten(script.tick!(80, 3))).not.toContain("Checking sold-out");
    vi.setSystemTime(1_000_000 + schedule[1]);
    expect(rewritten(script.tick!(80, 4))).toContain("Read(src/api/redemptions.ts)");
    vi.setSystemTime(1_000_000 + schedule.at(-1)! + 60_000);
    script.tick!(80, 5);
    const reads = () => plain(script.screen(80, 6)).split("Read(").length;
    const spent = reads();
    vi.setSystemTime(1_000_000 + schedule.at(-1)! + 120_000);
    expect(reads()).toBe(spent);
  });

  it("keeps its progress when the session is opened again", () => {
    const id = "preview-merchant-session";
    const { scenario, output } = createPreviewScenario("overview", "en");
    expect(scenario).toBeDefined();
    const screenOf = () => {
      const script = output(id);
      if (typeof script !== "object") throw new Error("no script");
      return plain(script.screen(80, 0));
    };
    expect(screenOf()).not.toContain("Read(src/api/redemptions.ts)");
    vi.setSystemTime(1_000_000 + liveSchedule(id, 4)[1]);
    expect(screenOf()).toContain("Read(src/api/redemptions.ts)");
    expect(screenOf()).toContain("Checking sold-out and already-redeemed states.");
  });

  it("catches up on every step that came due while a tab was hidden", () => {
    const id = "preview-merchant-session";
    const { script } = sessionsOf("en").find((entry) => entry.id === id)!;
    script.screen(80, 0);
    vi.setSystemTime(1_000_000 + liveSchedule(id, 4)[2]);
    const bytes = plain(script.tick!(80, 1).split("\x1b[J")[1]);
    expect(bytes).toContain("Checking sold-out");
    expect(bytes).toContain("Read(src/api/redemptions.ts)");
    expect(bytes).toContain("Bash(pnpm test tests/availability.test.ts)");
  });

  it("counts each row of an it.each table as a test", () => {
    const id = "preview-merchant-session";
    const { script } = sessionsOf("en").find((entry) => entry.id === id)!;
    vi.setSystemTime(1_000_000 + liveSchedule(id, 4).at(-1)!);
    expect(plain(script.screen(80, 0))).toContain("Tests  6 passed (6)");
  });

  it("counts the elapsed time from the page load, from a different base per session", () => {
    const elapsed = (id: string) => {
      const { script } = sessionsOf("en").find((entry) => entry.id === id)!;
      const read = () => plain(script.screen(80, 0)).match(/(\d+m \d+s|\d+s) [·•]/)![1];
      const first = read();
      vi.setSystemTime(1_000_000 + 5000);
      const later = read();
      vi.setSystemTime(1_000_000);
      return [first, later];
    };
    const [first, later] = elapsed("preview-merchant-session");
    expect(later).not.toBe(first);
    const bases = new Set(["preview-merchant-session", "preview-customer-session", "preview-server-session", "preview-kitchen-session"].map((id) => elapsed(id)[0]));
    expect(bases.size).toBeGreaterThan(1);
  });

  it("stands still without motion: no tick, a fixed timer and spinner", () => {
    const { scenario, output } = createPreviewScenario("overview", "en", false);
    expect(scenario).toBeDefined();
    const script = output("preview-merchant-session");
    if (typeof script !== "object") throw new Error("no script");
    expect(script.tick).toBeUndefined();
    const first = script.screen(80, 0);
    vi.setSystemTime(1_000_000 + 60_000);
    expect(script.screen(80, 0)).toBe(first);
    expect(first).not.toContain("Checking sold-out");
  });

  it("shows each agent, status and marker", () => {
    const screens = Object.fromEntries(sessionsOf("en").map(({ id, script }) => [id, plain(script.screen(80, 0))]));
    // Claude Code: working tail, idle with a background task, a question waiting for the user.
    expect(screens["preview-merchant-session"]).toContain("Welcome to Claude Code");
    expect(screens["preview-merchant-session"]).toMatch(/Checking… \(\d+m \d+s · esc to interrupt\)/);
    expect(screens["preview-merchant-session"]).toContain("╭");
    expect(screens["preview-review"]).toContain("1 background task: pnpm test --watch");
    expect(screens["preview-review"]).not.toContain("esc to interrupt");
    expect(screens["preview-order-checks"]).toContain("Can meal deals be combined with member discounts?");
    expect(screens["preview-order-checks"]).toContain("❯ 1. Keep them separate");
    expect(screens["preview-order-checks"]).toContain("Enter to select");
    // Codex.
    expect(screens["preview-server-session"]).toContain("OpenAI Codex");
    expect(screens["preview-server-session"]).toMatch(/Checking \(\d+m \d+s • esc to interrupt\)/);
    expect(screens["preview-admin-session"]).toContain("Awaiting feedback");
    expect(screens["preview-admin-session"]).not.toContain("esc to interrupt");
    expect(screens["preview-admin-session"]).toContain("› Ask Codex");
    // Grok Build.
    expect(screens["preview-customer-session"]).toContain("Grok Build");
    expect(screens["preview-customer-session"]).toMatch(/Checking… \d+m \d+s · esc to cancel/);
    // Nothing says it is a demo any more, and the coordinator keeps its progress summary.
    for (const screen of Object.values(screens)) expect(screen).not.toContain("Interactive demo");
    expect(screens["preview-coordinator"]).toContain("48 sample checks");
  });

  it("writes a tick that rewrites only the tail", () => {
    const session: TerminalSession = {
      id: "x", agent: "claude", status: "working", cwd: "/demo", copy: { prompt: "p", verb: "Doing", done: [], live: [{ say: ["next"] }] },
    };
    const script = createTerminalScript(session, { startedAt: Date.now(), animate: true });
    script.screen(60, 0);
    expect(script.tick!(60, 1)).toMatch(/^\x1b\[6A\r\x1b\[J\r\n\x1b/);
    vi.setSystemTime(1_000_000 + liveSchedule("x", 1)[0]);
    const bytes = script.tick!(60, 2);
    expect(bytes).toMatch(/^\x1b\[6A\r\x1b\[J\r\n.*next/);
    expect(bytes.endsWith("? for shortcuts\x1b[0m")).toBe(true);
  });
});
