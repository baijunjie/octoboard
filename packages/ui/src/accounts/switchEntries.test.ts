import { describe, expect, it } from "vitest";

import { handoffBlock, switchEntries } from "./switchEntries";
import { sessionOf } from "../fixtures/builders";
import type { Translate } from "../i18n/catalog";
import type { AccountSharing } from "../protocol";
import { accounts, NOW, usedUp, t } from "./accountChoicesTestSupport";

describe("switchEntries and a running sharing change", () => {
  it("marks the account whose change runs, and no other, a removed account's directory never", () => {
    const sharing = new Map<string, AccountSharing>([["w", { account: "w", progress: { share: false, phase: "changing" as const, done: 0, total: 2 } }], ["gone", { account: "gone", progress: { share: false, phase: "changing" as const, done: 0, total: 2 } }]]);
    const entries = switchEntries(sessionOf("s", "c-1", "p-1", "Title", "idle", { account_id: "gone", config_dir: "/home/old" }), accounts, t, {}, undefined, sharing);
    expect(entries.map((entry) => [entry.name, entry.changingSharing])).toEqual([["Default", false], ["Work", true], ["/home/old", false]]);
  });
});

describe("switchEntries", () => {
  const session = (extra = {}) => sessionOf("s", "c-1", "p-1", "Title", "idle", extra);
  const summary = (entries: ReturnType<typeof switchEntries>) =>
    entries.map((entry) => `${entry.name}${entry.current ? " (current)" : ""}`);

  it.each([
    ["the default account first, then the agent's stored accounts, marking the one it is on", {}, ["Default (current)", "Work"]],
    ["a stored account as current", { account_id: "w", config_dir: "/home/w" }, ["Default", "Work (current)"]],
    ["a removed account as current, named by the directory the session recorded", { account_id: "gone", config_dir: "/home/old" }, ["Default", "Work", "/home/old (current)"]],
    ["a removed account that recorded no directory as nothing to name", { account_id: "gone" }, ["Default", "Work"]],
    ["only the session's own agent's accounts", { agent: "grok" }, ["Default (current)"]],
  ] as const)("lists %s", (_, extra, expected) => {
    expect(summary(switchEntries(session(extra), accounts, t, {}))).toEqual(expected);
  });

  it("marks an account whose quota is used up at the time given, and not once it has reset", () => {
    const exhausted = (now: number) => switchEntries(session(), accounts, t, {}, { quotas: [usedUp("claude", "w")], now }).map((entry) => entry.exhaustedBy?.kind);
    expect(exhausted(NOW)).toEqual([undefined, "session"]);
    expect(exhausted(NOW + 120_000)).toEqual([undefined, undefined]);
  });
});

describe("handoffBlock", () => {
  const t = ((key: string) => key) as Translate;
  const base = { status: "idle", locked: false, switching_account: false } as const;

  it("says a session still starting is not ready, not that it is mid-turn", () => {
    expect(handoffBlock(t, { ...base, status: "starting" }, false)).toEqual({ detail: "accountChoice.handoff.starting" });
    expect(handoffBlock(t, { ...base, status: "working" }, false)?.detail).toBe("accountChoice.handoff.working");
    expect(handoffBlock(t, base, false)).toBeUndefined();
  });
});
