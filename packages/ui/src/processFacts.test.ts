import { expect, it } from "vitest";

import { format } from "./i18n/catalog";
import type { Translate } from "./i18n/catalog";
import type { Language } from "./i18n/languages";
import { processFacts } from "./processFacts";
import type { ProcessTree } from "./protocol";

const NOW = Date.UTC(2026, 9, 28, 12, 0, 0);
const translate = (language: Language): Translate => ((key: string, ...args: unknown[]) => format(language, key as never, args[0] as never)) as Translate;
const tree = (extra: Partial<ProcessTree> = {}): ProcessTree => ({
  id: "1:1", command: "vite", cwd: null, cwd_missing: false, kept: false, started_at: NOW - 2 * 3600_000, process_count: 1, memory_bytes: 3_400_000, ports: [], ...extra,
});

it("gives memory and age, then the ports and the process count only when there are any", () => {
  expect(processFacts(translate("en"), "en", tree(), NOW)).toBe("3.4 MB of memory, started 2 hours ago");
  expect(processFacts(translate("en"), "en", tree({ ports: [8080], process_count: 2 }), NOW)).toBe("3.4 MB of memory, listening on port \u20688080\u2069, started 2 hours ago, 2 processes");
});

it("says a tree that has just started so", () => {
  expect(processFacts(translate("en"), "en", tree({ started_at: NOW - 10_000 }), NOW)).toBe("3.4 MB of memory, started just now");
});

it("is worded in the viewing language", () => {
  expect(processFacts(translate("zh-Hans"), "zh-Hans", tree({ ports: [3000, 5173], process_count: 4 }), NOW)).toContain("监听端口");
});

it("joins several ports with the language's own word, each isolated, so they are not another fact of the line", () => {
  const ports = (language: Language) => processFacts(translate(language), language, tree({ ports: [3000, 5173, 24678] }), NOW);
  expect(ports("en")).toContain("listening on ports \u20683000\u2069, \u20685173\u2069, and \u206824678\u2069, started");
  expect(ports("zh-Hans")).toMatch(/监听端口 \u20683000\u2069(、|和)/);
});
