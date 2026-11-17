import { describe, expect, it } from "vitest";

import { handoffBlock, switchEntries } from "./switchEntries";
import { sessionOf } from "../fixtures/builders";
import type { Translate } from "../i18n/catalog";
import type { AccountSharing, Session } from "../protocol";
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
  const base = { status: "idle", locked: false } as const;
  const free = { switching: false, ownChanging: false };

  it("says a session still starting is not ready, not that it is mid-turn", () => {
    expect(handoffBlock(t, { ...base, status: "starting" }, free)).toEqual({ detail: "accountChoice.handoff.starting" });
    expect(handoffBlock(t, { ...base, status: "working" }, free)?.detail).toBe("accountChoice.handoff.working");
    expect(handoffBlock(t, base, free)).toBeUndefined();
  });

  const busy = { detail: "accountChoice.handoff.working", reason: { state: "accountChoice.handoff.working.reason" } };
  const childActive = { detail: "accountChoice.handoff.childActive", reason: { state: "accountChoice.handoff.childActive.reason" } };

  it.each<[string, Partial<Session>]>([
    ["working", { status: "working" }],
    ["waiting for the user", { status: "waiting_user" }],
    ["showing its background agents", { status: "dormant" }],
    ["idle with work in the background", { background_tasks: [{ id: "t" }] }],
    ["waiting at a usage limit with work in the background", { status: "waiting_user", at_usage_limit: true, background_tasks: [{ id: "t" }] }],
  ])("is blocked, with a reason, by a session that is %s", (_name, extra) => {
    expect(handoffBlock(t, { ...base, ...extra }, free)).toEqual(busy);
  });

  it("does not block a session waiting only for a usage-limit dialog, or one idle with nothing running", () => {
    expect(handoffBlock(t, { ...base, status: "waiting_user", at_usage_limit: true }, free)).toBeUndefined();
    expect(handoffBlock(t, { ...base, background_tasks: [] }, free)).toBeUndefined();
  });

  it("is blocked, with a reason, by a child that can still report, unless none can", () => {
    expect(handoffBlock(t, base, { ...free, childActive: true })).toEqual(childActive);
    expect(handoffBlock(t, base, { ...free, childActive: false })).toBeUndefined();
  });

  it("says a starting session with a session at work under it is busy, as the daemon refuses it, not that the child is", () => {
    const starting = { ...base, status: "starting" as const };
    expect(handoffBlock(t, starting, { ...free, childActive: true })).toEqual(busy);
    expect(handoffBlock(t, starting, { switching: true, ownChanging: true, childActive: true })).toEqual(busy);
  });

  it("puts the lasting holds first: locked, then busy, then a child at work, ahead of the brief ones", () => {
    const everything = { switching: true, ownChanging: true, childActive: true };
    expect(handoffBlock(t, { ...base, locked: true, status: "working" }, everything)?.reason?.state).toBe("accountChoice.sessionLocked.reason");
    expect(handoffBlock(t, { ...base, status: "working" }, everything)).toEqual(busy);
    expect(handoffBlock(t, base, everything)).toEqual(childActive);
    expect(handoffBlock(t, base, { ...everything, childActive: false })).toEqual({ detail: "accountChoice.switching.detail" });
    expect(handoffBlock(t, base, { switching: false, ownChanging: true, childActive: false })).toEqual({ detail: "accountChoice.ownSharingChange.detail" });
    expect(handoffBlock(t, { ...base, status: "starting" }, { ...everything, switching: false, ownChanging: false, childActive: false })).toEqual({ detail: "accountChoice.handoff.starting" });
  });
});
