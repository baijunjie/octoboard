// @vitest-environment jsdom
import { KeyRound, Link, SquareActivity, SquareTerminal, Terminal } from "lucide-react";
import React from "react";
import { describe, expect, it, vi } from "vitest";

import type { ActionMenuEntry, ActionMenuItem, ActionMenuSubmenu } from "../components/menus/menuEntries";
import { SessionRelativeTime } from "../components/status/SessionMetadata";
import { accountOf, consoleOf, gitStatusOf, sessionOf } from "../fixtures/builders";
import { DEFAULT_ACCOUNT_NAME } from "../accounts/defaultAccount";
import { format } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Translate } from "../i18n/catalog";
import type { Account, GitStatus, Project, Session } from "../protocol";
import { archiveSubmenu, consoleMenu, type MenuPlacement, projectMenu, sessionMenu } from "./menus";
import { BROWSER_HOST } from "../test/hosts";
import { TEST_PROVIDERS } from "../test/testProviders";
import type { SidebarHandlers } from "./types";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const openIn = (menu: ActionMenuEntry[]) => menu.find((entry) => typeof entry === "object" && entry.label === "Open in") as ActionMenuSubmenu;
const account = (id: string, agent: Account["agent"]): Account => accountOf(id, agent, id, `/home/${id}`);

function submenu(session: Session, accounts: Account[], handlers = {} as SidebarHandlers) {
  const entry = sessionMenu(t, handlers, session, accounts, {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false }).find((candidate) => typeof candidate === "object" && "items" in candidate && candidate.label === "Switch account");
  return entry && typeof entry === "object" && "items" in entry ? entry : undefined;
}

describe("the Switch account entry", () => {
  const project = (extra: Partial<Session> = {}) => sessionOf("s", "c", "p", "Title", "idle", extra);
  const consoleSession = sessionOf("h", "c", undefined, "Hub", "idle");

  it.each([
    ["the agent has only its default account", project(), [], false],
    ["the agent has a stored account", project(), [account("w", "claude")], true],
    ["only another agent has a stored account", project(), [account("c", "codex")], false],
    ["the session is on an account that was removed, and the default is somewhere to go", project({ account_id: "gone", config_dir: "/x" }), [], true],
    ["it is a console session of an agent with a stored account", consoleSession, [account("w", "claude")], true],
    ["the session is archived", project({ status: "archived" }), [account("w", "claude")], false],
    ["the session is still starting", project({ status: "starting" }), [account("w", "claude")], false],
  ] as const)("is present exactly when there is somewhere to go: %s", (_, session, accounts, shown) => {
    expect(submenu(session, [...accounts])?.label === "Switch account").toBe(shown);
  });

  it("lists only accounts, marks the current one, and picks another through a confirmation", () => {
    const handlers = { onOpenDialog: vi.fn() } as unknown as SidebarHandlers;
    const items = submenu(project(), [account("w", "claude")], handlers)!.items;
    const [current, other] = items;

    expect(items).toHaveLength(2);

    expect([(current as { selected: boolean }).selected, (other as { selected: boolean }).selected]).toEqual([true, false]);
    (current as { onClick: () => void }).onClick();
    expect(handlers.onOpenDialog).not.toHaveBeenCalled();
    (other as { onClick: () => void }).onClick();
    expect(handlers.onOpenDialog).toHaveBeenCalledWith(expect.objectContaining({ kind: "switch-account", account: "w", accountName: "w" }));
  });

  it("lists no turned-off account but the session's own, and is absent when every other one is off", () => {
    const labels = (session: Session, accounts: Account[], disabledDefaults?: ("claude" | "codex" | "grok")[]) => {
      const entry = sessionMenu(t, {} as SidebarHandlers, session, accounts, {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, disabledDefaults })
        .find((candidate) => typeof candidate === "object" && "items" in candidate && candidate.label === "Switch account") as ActionMenuSubmenu | undefined;
      return entry?.items.map((item) => (typeof item === "object" ? item.label : item));
    };
    const off = { ...account("w", "claude"), disabled: true };
    expect(labels(project(), [off, account("v", "claude")])).toEqual(["Default", "v"]);
    expect(labels(project({ account_id: "w", config_dir: "/home/w" }), [off, account("v", "claude")], ["claude"])).toEqual(["w", "v"]);
    expect(labels(project({ account_id: "w", config_dir: "/home/w" }), [off], ["claude"])).toBeUndefined();
    expect(labels(project(), [off])).toBeUndefined();
  });

  it("disables an account whose quota is used up at the time given, with why, and never the current one", () => {
    const spent = (accountId: string | null) => ({
      agent: "claude" as const, account: accountId, fetched_at: 0, plan: null, unavailable: null,
      windows: [{ kind: "session" as const, label: null, used_percent: 100, resets_at: 7_200_000, window_minutes: 300 }], email: null,
    });
    const menu = (quotas: ReturnType<typeof spent>[], now: number) => {
      const entry = sessionMenu(t, {} as SidebarHandlers, project(), [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, quota: { quotas, now } })
        .find((candidate) => typeof candidate === "object" && "items" in candidate && candidate.label === "Switch account") as ActionMenuSubmenu;
      return entry.items.slice(0, 2) as { reason?: { state: string; detail?: string } }[];
    };
    const [current, other] = menu([spent("w"), spent(null)], 0);
    expect(current.reason).toBeUndefined();
    expect(other.reason).toEqual({ state: "Limit reached", detail: "resets in 2h 0m" });
    expect(menu([spent("w")], 7_200_000)[1].reason).toBeUndefined();
  });

  it("disables an account whose sharing change runs, without a reason, whatever its quota, and never the current one", () => {
    const sharing = new Map([["w", { account: "w", progress: { share: true, phase: "changing" as const, done: 1, total: 3, item: "skills" } }]]);
    const entry = sessionMenu(t, {} as SidebarHandlers, project(), [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, sharing })
      .find((candidate) => typeof candidate === "object" && "items" in candidate && candidate.label === "Switch account") as ActionMenuSubmenu;
    const [current, other] = entry.items as { disabled?: boolean; reason?: { state: string }; description?: string }[];
    expect(current.reason).toBeUndefined();
    expect(other.disabled).toBe(true);
    expect(other.reason).toBeUndefined();
    expect(other.description).toBeUndefined();
    const free = sessionMenu(t, {} as SidebarHandlers, project(), [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, sharing: new Map() })
      .find((candidate) => typeof candidate === "object" && "items" in candidate && candidate.label === "Switch account") as ActionMenuSubmenu;
    expect((free.items[1] as { reason?: unknown }).reason).toBeUndefined();
    const onto = sessionMenu(t, {} as SidebarHandlers, project({ account_id: "w", config_dir: "/home/w" }), [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, sharing })
      .find((candidate) => typeof candidate === "object" && "items" in candidate && candidate.label === "Switch account") as ActionMenuSubmenu;
    expect((onto.items[1] as { selected: boolean; reason?: unknown }).selected).toBe(true);
    expect((onto.items[1] as { reason?: unknown }).reason).toBeUndefined();
  });

  it("disables every other account, without a reason, while the session's own account is changing its sharing", () => {
    const sharing = new Map([["w", { account: "w", progress: { share: true, phase: "changing" as const, done: 1, total: 3, item: "skills" } }]]);
    const entry = sessionMenu(t, {} as SidebarHandlers, project({ account_id: "w", config_dir: "/home/w" }), [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, sharing })
      .find((candidate) => typeof candidate === "object" && "items" in candidate && candidate.label === "Switch account") as ActionMenuSubmenu;
    const [dflt, current] = entry.items as { disabled?: boolean; reason?: { state: string }; description?: string; selected: boolean }[];
    expect(current.selected).toBe(true);
    expect(current.reason).toBeUndefined();
    expect(dflt.disabled).toBe(true);
    expect(dflt.reason).toBeUndefined();
    expect(dflt.description).toBeUndefined();
  });

  it("disables Hand off, without a reason, while the session's own account is changing its sharing", () => {
    const sharing = new Map([["w", { account: "w", progress: { share: true, phase: "changing" as const, done: 1, total: 3, item: "skills" } }]]);
    const handoff = (opts: { sharing?: typeof sharing }) => sessionMenu(t, {} as SidebarHandlers, project({ account_id: "w", config_dir: "/home/w" }), [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, ...opts })
      .find((entry) => typeof entry === "object" && "label" in entry && entry.label === "Hand off") as { disabled?: boolean; reason?: { state: string }; description?: string };
    expect(handoff({ sharing }).disabled).toBe(true);
    expect(handoff({ sharing }).reason).toBeUndefined();
    expect(handoff({ sharing }).description).toBeUndefined();
    expect(handoff({}).disabled).toBe(false);
    expect(handoff({ sharing: new Map() }).reason).toBeUndefined();
  });

  it("disables the other accounts, Hand off and Unbind while an account switch holds the session", () => {
    const held = project({ account_id: "w", config_dir: "/home/w", bound_to: "hub" });
    const menu = sessionMenu(t, {} as SidebarHandlers, held, [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: true });
    const entry = menu.find((candidate) => typeof candidate === "object" && "items" in candidate && candidate.label === "Switch account") as ActionMenuSubmenu;
    const [dflt, current] = entry.items as { disabled?: boolean; reason?: { state: string }; selected: boolean }[];
    expect(current.reason).toBeUndefined();
    expect(dflt.disabled).toBe(true);
    expect(dflt.reason).toBeUndefined();
    const item = (label: string) => menu.find((candidate) => typeof candidate === "object" && "label" in candidate && candidate.label === label) as { disabled?: boolean; reason?: { state: string } };
    expect(item("Hand off").disabled).toBe(true);
    expect(item("Hand off").reason).toBeUndefined();
    expect(item("Unbind").disabled).toBe(true);
  });

  it("disables an account that is not logged in, saying why, and keeps the reason while an account switch holds the session", () => {
    const signedOut = {
      agent: "claude" as const, account: "v", fetched_at: 0, plan: null, windows: [],
      unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "Claude Code is not signed in" }, email: null,
    };
    const items = (session: Session, switching = false) => (sessionMenu(t, {} as SidebarHandlers, session, [account("w", "claude"), account("v", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching, quota: { quotas: [signedOut], now: 0 } })
      .find((candidate) => typeof candidate === "object" && "items" in candidate && candidate.label === "Switch account") as ActionMenuSubmenu).items as { disabled?: boolean; reason?: { state: string }; description?: string; selected: boolean }[];
    const [, , v] = items(project());
    expect(v.disabled).toBe(true);
    expect(v.reason).toEqual({ state: "Not logged in" });
    expect(v.description).toBe("Unavailable until it is logged in");

    const [dflt, current, held] = items(project({ account_id: "w", config_dir: "/home/w" }), true);
    expect(current.selected).toBe(true);
    expect(current.reason).toBeUndefined();
    expect(dflt.disabled).toBe(true);
    expect(dflt.reason).toBeUndefined();
    expect(held.disabled).toBe(true);
    expect(held.reason).toEqual({ state: "Not logged in" });
    expect(held.description).toBe("Unavailable until it is logged in");
  });

  it("states what remains of an account's quota as its description, and has no reason for it", () => {
    const quota = {
      agent: "claude" as const, account: "w", fetched_at: 0, plan: null, unavailable: null,
      windows: [{ kind: "session" as const, label: null, used_percent: 40, resets_at: 7_200_000, window_minutes: 300 }], email: null,
    };
    const entry = sessionMenu(t, {} as SidebarHandlers, project(), [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, quota: { quotas: [quota], now: 0 } })
      .find((candidate) => typeof candidate === "object" && "items" in candidate && candidate.label === "Switch account") as ActionMenuSubmenu;
    const other = entry.items[1] as { reason?: unknown; description?: string };
    expect(other.reason).toBeUndefined();
    expect(other.description).toMatch(/60%/);
  });

  it("keeps a key for a removed account's directory, in the width of a usage bar", () => {
    const items = submenu(project({ account_id: "gone", config_dir: "/x" }), [])!.items;
    const icon = (items.find((item) => typeof item === "object" && "selected" in item && item.selected) as { icon: React.ReactElement<{ className: string; children: React.ReactElement }> }).icon;
    expect(icon.props.className).toContain("w-8");
    expect(icon.props.children.type).toBe(KeyRound);
  });
});

describe("the Hand off entry", () => {
  const item = (session: Session, handlers = {} as SidebarHandlers, canHandoff = true, switching = false) =>
    sessionMenu(t, handlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, canHandoff, switching }).find((entry) => typeof entry === "object" && "onClick" in entry && entry.label === "Hand off") as { disabled?: boolean; reason?: { state: string }; onClick: () => void } | undefined;

  it.each([
    ["idle", false, undefined],
    ["waiting_user", false, "Session busy"],
    ["interrupted", false, undefined],
    ["working", false, "Session busy"],
    ["starting", false, undefined],
    ["idle", true, "Session locked"],
  ] as const)("on a %s session (locked: %s) is disabled with the reason %s", (status, locked, reason) => {
    const handoff = item(sessionOf("s", "c", "p", "Title", status, { locked }));
    expect(handoff?.disabled ?? false).toBe(reason !== undefined || status === "starting");
    expect(handoff?.reason?.state).toBe(reason);
  });

  it("leaves a session shown waiting only for a usage-limit dialog handable, as the daemon does", () => {
    const handoff = item(sessionOf("s", "c", "p", "Title", "waiting_user", { at_usage_limit: true }));
    expect(handoff?.disabled ?? false).toBe(false);
    expect(handoff?.reason).toBeUndefined();
  });

  it("still says the session is locked while an account switch holds it", () => {
    const handoff = item(sessionOf("s", "c", "p", "Title", "idle", { locked: true }), undefined, true, true);
    expect(handoff?.disabled).toBe(true);
    expect(handoff?.reason?.state).toBe("Session locked");
  });

  it("lets a lasting hold win over a brief one, so the reason shown is the one that lasts", () => {
    const sharing = new Map([["w", { account: "w", progress: { share: true, phase: "changing" as const, done: 1, total: 3, item: "skills" } }]]);
    const handoff = (session: Session, opts: { sharing?: typeof sharing; canHandoff?: boolean; switching?: boolean }) => sessionMenu(t, {} as SidebarHandlers, session, [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, ...opts })
      .find((entry) => typeof entry === "object" && "onClick" in entry && entry.label === "Hand off") as { disabled?: boolean; reason?: { state: string } };
    const onAccount = (extra: Partial<Session>) => sessionOf("s", "c", "p", "Title", "idle", extra);
    const own = { account_id: "w", config_dir: "/home/w" };
    expect(handoff(onAccount({ ...own, status: "working" }), { sharing })).toMatchObject({ disabled: true, reason: { state: "Session busy" } });
    expect(handoff(onAccount({ ...own, status: "starting" }), { canHandoff: false })).toMatchObject({ disabled: true, reason: { state: "No other agent" } });
    expect(handoff(onAccount(own), { canHandoff: false, switching: true })).toMatchObject({ disabled: true, reason: { state: "No other agent" } });
    expect(handoff(onAccount({ ...own, locked: true }), { canHandoff: false })).toMatchObject({ reason: { state: "Session locked" } });
    expect(handoff(onAccount({ ...own, status: "working" }), { canHandoff: false })).toMatchObject({ reason: { state: "Session busy" } });
    expect(handoff(onAccount(own), { sharing, switching: true }).reason).toBeUndefined();
  });

  it("says why Archive and Delete are unavailable, and leaves Unbind's brief states unexplained", () => {
    const entry = (session: Session, label: string, hasBoundSessions = true) =>
      sessionMenu(t, {} as SidebarHandlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, hasBoundSessions }).find((candidate) => typeof candidate === "object" && candidate.label === label) as { disabled?: boolean; reason?: { state: string } } | undefined;
    const locked = sessionOf("s", "c", "p", "Title", "idle", { locked: true });
    expect(entry(locked, "Archive")).toMatchObject({ disabled: true, reason: { state: "Session locked" } });
    expect(entry({ ...locked, status: "archived" }, "Delete")).toMatchObject({ disabled: true, reason: { state: "Session locked" } });
    expect(entry(sessionOf("s", "c", "p", "Title", "idle"), "Archive")?.reason).toBeUndefined();
    const bound = sessionOf("s", "c", "p", "Title", "starting", { bound_to: "h" });
    expect(entry(bound, "Unbind")).toMatchObject({ disabled: true });
    expect(entry(bound, "Unbind")?.reason).toBeUndefined();
    expect(entry({ ...bound, status: "idle" }, "Unbind")?.reason).toBeUndefined();
  });

  it("says no other agent could take the session over", () => {
    const handoff = item(sessionOf("s", "c", "p", "Title", "idle"), undefined, false);
    expect(handoff?.disabled).toBe(true);
    expect(handoff?.reason).toEqual({ state: "No other agent" });
  });

  it("is offered for console sessions and not for an archived one, and opens the handoff dialog", () => {
    const hub = sessionOf("h", "c", undefined, "Hub", "idle");
    expect(item(sessionOf("h", "c", undefined, "Hub", "archived"))).toBeUndefined();
    const handlers = { onOpenDialog: vi.fn() } as unknown as SidebarHandlers;
    item(hub, handlers)!.onClick();
    expect(handlers.onOpenDialog).toHaveBeenCalledWith({ kind: "handoff", session: hub });
  });
});

describe("the Hand off entry and the sessions under the session", () => {
  const handoff = (session: Session, childActive: boolean) =>
    sessionMenu(t, {} as SidebarHandlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, childActive }).find((entry) => typeof entry === "object" && "onClick" in entry && entry.label === "Hand off") as { disabled?: boolean; reason?: { state: string }; description?: string };

  it("is disabled, saying why, while a session under it can still report to it", () => {
    const entry = handoff(sessionOf("s", "c", "p", "Title", "idle"), true);
    expect(entry.disabled).toBe(true);
    expect(entry.reason).toEqual({ state: "Child sessions working" });
    expect(entry.description).toBe("A session under this one is still at work and may report to it, so it cannot be handed off yet.");
  });

  it("is not held back by sessions under it when none can report", () => {
    const entry = handoff(sessionOf("s", "c", "p", "Title", "idle"), false);
    expect(entry.disabled ?? false).toBe(false);
    expect(entry.reason).toBeUndefined();
  });

  it("says the session itself is busy before it says a child is", () => {
    expect(handoff(sessionOf("s", "c", "p", "Title", "working"), true).reason).toEqual({ state: "Session busy" });
    expect(handoff(sessionOf("s", "c", "p", "Title", "idle", { locked: true }), true).reason).toEqual({ state: "Session locked" });
  });

  it("treats a session stopped at its usage limit with work in the background as busy, and one without as at rest", () => {
    const stopped = { status: "waiting_user", at_usage_limit: true } as const;
    expect(handoff(sessionOf("s", "c", "p", "Title", stopped.status, { at_usage_limit: true, background_tasks: [{ id: "t" }] }), false).reason).toEqual({ state: "Session busy" });
    expect(handoff(sessionOf("s", "c", "p", "Title", stopped.status, { at_usage_limit: true }), false).reason).toBeUndefined();
  });

  it("treats a session idle with work in the background as busy", () => {
    expect(handoff(sessionOf("s", "c", "p", "Title", "idle", { background_tasks: [{ id: "t" }] }), false).reason).toEqual({ state: "Session busy" });
    expect(handoff(sessionOf("s", "c", "p", "Title", "dormant"), false).reason).toEqual({ state: "Session busy" });
  });
});

describe("the Open in the app entry", () => {
  const conversation = "0b8f1c52-3d1e-4a7b-9c41-5e6d7f8091a2";
  const app = (extra: Partial<{ label: string; seen: boolean; onOpen: () => void }> = {}) => ({ label: "Open in Claude app", seen: true, onOpen: vi.fn(), ...extra });
  const entryOf = (session: Session, nativeApp: ReturnType<typeof app> | undefined) =>
    sessionMenu(t, {} as SidebarHandlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, nativeApp }).find((entry) => typeof entry === "object" && "onClick" in entry && entry.label === "Open in Claude app") as ActionMenuItem | undefined;
  const asleep = (status: "interrupted" | "archived" = "interrupted", extra: Partial<Session> = {}) => sessionOf("s", "c", "p", "Title", status, { agent_session_id: conversation, ...extra });

  it("is not in the menu unless the caller offers it, as where the app is not installed", () => {
    expect(entryOf(asleep(), undefined)).toBeUndefined();
  });

  it.each(["interrupted", "archived"] as const)("is offered, enabled, with the letter W, for a %s session with a conversation the app can see; pressing it opens", (status) => {
    const nativeApp = app();
    const entry = entryOf(asleep(status), nativeApp)!;
    expect(entry.shortcut).toBe("W");
    expect(entry.disabled).toBeFalsy();
    expect(entry.reason).toBeUndefined();
    entry.onClick();
    expect(nativeApp.onOpen).toHaveBeenCalledOnce();
  });

  it("is labelled as the caller words it", () => {
    const menu = sessionMenu(t, {} as SidebarHandlers, asleep(), [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, nativeApp: app({ label: "Open in Codex app" }) });
    expect(menu.some((entry) => typeof entry === "object" && "onClick" in entry && entry.label === "Open in Codex app")).toBe(true);
  });

  it.each(["starting", "working", "waiting_user", "dormant", "idle"] as const)("is disabled, saying to stop the session first, for a %s session, which has a process", (status) => {
    const entry = entryOf(asleep(status as never), app())!;
    expect(entry.disabled).toBe(true);
    expect(entry.reason).toEqual({ state: "Session running" });
    expect(entry.description).toBe("Stop the session first: the agent's app and Octoboard would both write to this conversation.");
  });

  it("is disabled for a session with no conversation yet", () => {
    for (const extra of [{ has_conversation: false }, { agent_session_id: null }]) {
      const entry = entryOf(asleep("interrupted", extra), app())!;
      expect(entry.disabled).toBe(true);
      expect(entry.reason).toEqual({ state: "No conversation yet" });
    }
  });

  it("stays enabled, with a note under its label, where the default directory does not hold the conversation's latest record", () => {
    const nativeApp = app({ seen: false });
    const entry = entryOf(asleep(), nativeApp)!;
    expect(entry.disabled).toBeFalsy();
    expect(entry.reason).toBeUndefined();
    expect(entry.ariaLabel).toBe("Open in Claude app");
    expect(entry.description).toBe("The agent's app may not see this conversation: its latest record is kept in this account's own directory, not the default one.");
    expect(JSON.stringify(entry.content)).toContain("The agent's app may not see this conversation");
    entry.onClick();
    expect(nativeApp.onOpen).toHaveBeenCalledOnce();
  });

  it("sits with Reopen in the first group, before Copy session ID and Delete, in an archived session's menu", () => {
    const menu = sessionMenu(t, {} as SidebarHandlers, asleep("archived"), [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, nativeApp: app(), focusTarget: { consoleSession: sessionOf("owner", "c", undefined, "Owner", "idle") } });
    const labels = menu.map((entry) => entry === "separator" ? entry : entry.label);
    expect(labels).toEqual(["Reopen", "Open in Claude app", "separator", "Copy session ID", "separator", "Delete"]);
  });

  it("takes a letter no other entry of the menu has", () => {
    for (const status of ["interrupted", "archived"] as const) {
      const menu = sessionMenu(t, {} as SidebarHandlers, asleep(status), [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, nativeApp: app() });
      const keys = menu.filter((entry): entry is ActionMenuItem => typeof entry === "object" && "onClick" in entry && typeof entry.shortcut === "string").map((entry) => entry.shortcut);
      expect(keys.filter((key) => key === "W")).toEqual(["W"]);
      expect(new Set(keys).size).toBe(keys.length);
    }
  });
});

describe("the Change model entry", () => {
  const item = (session: Session, handlers = {} as SidebarHandlers, switching = false) =>
    sessionMenu(t, handlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching }).find((entry) => typeof entry === "object" && "onClick" in entry && entry.label === "Change model") as { disabled?: boolean; reason?: unknown; onClick: () => void } | undefined;

  it.each([
    ["idle", {}, false],
    ["working", {}, false],
    ["waiting_user", {}, false],
    ["interrupted", {}, false],
    ["starting", {}, true],
    ["idle", { providers: TEST_PROVIDERS, switching: true }, true],
  ] as const)("on a %s session (%o) is disabled: %s, with no reason", (status, extra, disabled) => {
    const entry = item(sessionOf("s", "c", "p", "Title", status), undefined, "switching" in extra);
    expect(entry?.disabled ?? false).toBe(disabled);
    expect(entry?.reason).toBeUndefined();
  });

  it.each(["list", "focused"] as const)("is above Switch account in the %s menu, is absent when archived, and opens its dialog", (placement) => {
    expect(item(sessionOf("h", "c", undefined, "Hub", "archived"))).toBeUndefined();
    const hub = sessionOf("h", "c", undefined, "Hub", "idle");
    const handlers = { onOpenDialog: vi.fn() } as unknown as SidebarHandlers;
    const menu = sessionMenu(t, handlers, hub, [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, placement });
    const labels = menu.flatMap((entry) => typeof entry === "object" ? [entry.label] : []);
    expect(labels.slice(labels.indexOf("Change model"), labels.indexOf("Hand off") + 1)).toEqual(["Change model", "Switch account", "Hand off"]);
    const changeModel = menu.find((entry) => typeof entry === "object" && "onClick" in entry && entry.label === "Change model") as ActionMenuItem;
    changeModel.onClick();
    expect(handlers.onOpenDialog).toHaveBeenCalledWith({ kind: "session-model", session: hub });
  });
});

describe("a session that is still starting", () => {
  it("offers Archive, never Delete, even when it was never used", () => {
    const starting = sessionOf("s", "c", "p", "Title", "starting");
    const labels = sessionMenu(t, {} as SidebarHandlers, starting, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false }).map((entry) => (entry === "separator" ? entry : entry.label));
    expect(labels).toContain("Archive");
    expect(labels).not.toContain("Delete");
  });
});

describe("the order of a session's action menu", () => {
  // A run of separators is one boundary: the menu draws a separator only between two items.
  const groups = (menu: ActionMenuEntry[]): string[][] => {
    const grouped: string[][] = [];
    let current: string[] = [];
    for (const entry of menu) {
      if (entry === "separator") {
        if (current.length > 0) grouped.push(current);
        current = [];
      } else current.push(entry.label);
    }
    if (current.length > 0) grouped.push(current);
    return grouped;
  };
  const focusTarget = { consoleSession: sessionOf("owner", "c", undefined, "Owner", "idle") };
  const stored = [account("w", "claude")];
  const project = (extra: Partial<Session> = {}) => sessionOf("s", "c", "p", "Title", "idle", extra);
  const hub = (extra: Partial<Session> = {}) => sessionOf("h", "c", undefined, "Hub", "idle", extra);
  const menuFor = (session: Session, placement: MenuPlacement, offeredFocus: boolean, accounts: Account[]) =>
    sessionMenu(t, {} as SidebarHandlers, session, accounts, {}, {
      host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, placement, ...(offeredFocus ? { focusTarget } : {}),
    });

  it.each<[string, Session, MenuPlacement, boolean, Account[], string[][]]>([
    ["a live project session in the list", project(), "list", true, stored, [
      ["Lock", "Pin", "Focus mode"],
      ["Copy session ID"],
      ["Change model", "Switch account", "Hand off"],
      ["Rename"],
      ["Archive"],
    ]],
    ["a live project session in its focus mode", project(), "focused", true, stored, [
      ["Lock"],
      ["Copy session ID"],
      ["Change model", "Switch account", "Hand off"],
      ["Rename"],
      ["Archive"],
    ]],
    ["a live project session with nowhere to focus", project(), "list", false, stored, [
      ["Lock", "Pin"],
      ["Copy session ID"],
      ["Change model", "Switch account", "Hand off"],
      ["Rename"],
      ["Archive"],
    ]],
    ["a live console session", hub(), "list", true, stored, [
      ["Lock", "Pin", "Focus mode"],
      ["Colour"],
      ["Copy session ID"],
      ["Change model", "Switch account", "Hand off"],
      ["Rename"],
      ["Archive"],
    ]],
    ["a live console session in its focus mode", hub(), "focused", true, stored, [
      ["Lock"],
      ["Colour"],
      ["Copy session ID"],
      ["Change model", "Switch account", "Hand off"],
      ["Rename"],
      ["Archive"],
    ]],
    ["an unused project session", project({ has_conversation: false }), "list", true, stored, [
      ["Lock", "Pin", "Focus mode"],
      ["Copy session ID"],
      ["Change model", "Switch account", "Hand off"],
      ["Rename"],
      ["Delete"],
    ]],
    ["a bound project session", project({ bound_to: "hub" }), "list", true, stored, [
      ["Lock", "Pin", "Focus mode"],
      ["Copy session ID"],
      ["Change model", "Switch account", "Hand off", "Unbind"],
      ["Rename"],
      ["Archive"],
    ]],
    ["a session that is still starting", sessionOf("s", "c", "p", "Title", "starting", { has_conversation: false }), "list", true, stored, [
      ["Lock", "Pin", "Focus mode"],
      ["Copy session ID"],
      ["Change model", "Hand off"],
      ["Rename"],
      ["Archive"],
    ]],
    ["an archived project session", project({ status: "archived" }), "list", true, stored, [
      ["Reopen"],
      ["Copy session ID"],
      ["Delete"],
    ]],
    ["an archived console session", hub({ status: "archived" }), "list", true, stored, [
      ["Reopen"],
      ["Copy session ID"],
      ["Delete"],
    ]],
  ])("groups %s", (_name, session, placement, offeredFocus, accounts, expected) => {
    expect(groups(menuFor(session, placement, offeredFocus, accounts))).toEqual(expected);
  });

  it("keeps Rename a direct action on R that opens the rename dialog", () => {
    const session = project();
    const handlers = { onOpenDialog: vi.fn() } as unknown as SidebarHandlers;
    const menu = sessionMenu(t, handlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false });
    const rename = menu.find((entry): entry is ActionMenuItem => typeof entry === "object" && entry.label === "Rename");
    expect(rename).toMatchObject({ shortcut: "R" });
    expect(rename).not.toHaveProperty("items");
    expect(rename).not.toHaveProperty("panel");
    rename!.onClick();
    expect(handlers.onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind: "rename-session", session });
  });
});


describe("the Colour entry", () => {
  const hub = sessionOf("h", "c", undefined, "Hub", "idle", { colour: "cyan" });
  const entry = (session: Session, handlers = {} as SidebarHandlers) => sessionMenu(t, handlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false }).find((candidate) => typeof candidate === "object" && candidate.label === "Colour");

  it.each([
    ["a console session's row", hub, true],
    ["an archived console session", { ...hub, status: "archived" as const }, false],
    ["a project session", sessionOf("s", "c", "p", "Title", "idle"), false],
  ] as const)("is offered for %s", (_, session, offered) => {
    expect(entry(session) !== undefined).toBe(offered);
  });

  it("is a panel of swatches that sets the session's colour and closes the menu", () => {
    const handlers = { onSetSessionColour: vi.fn() } as unknown as SidebarHandlers;
    const close = vi.fn();
    const panel = (entry(hub, handlers) as { panel: (context: object) => { props: { colour: string; onChoose: (colour: string) => void } } }).panel({ close });
    expect(panel.props.colour).toBe("cyan");
    panel.props.onChoose("red");
    expect(close).toHaveBeenCalledOnce();
    expect(handlers.onSetSessionColour).toHaveBeenCalledExactlyOnceWith(hub, "red");
  });
});

describe("the Focus mode entry", () => {
  const hub = sessionOf("h", "c", undefined, "Hub", "idle");
  const labels = (session: Session, placement: MenuPlacement = "list") =>
    sessionMenu(t, {} as SidebarHandlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, placement, focusTarget: { consoleSession: hub } }).flatMap((entry) => (typeof entry === "object" ? [entry.label] : []));

  it.each([
    ["a console session's row", hub, "list", true],
    ["a console session's own focus mode", hub, "focused", false],
    ["an archived console session", { ...hub, status: "archived" as const }, "list", false],
    ["a selected project session", sessionOf("s", "c", "p", "Title", "idle"), "list", true],
  ] as const)("is offered for %s: %#", (_, session, placement, offered) => {
    expect(labels(session, placement).includes("Focus mode")).toBe(offered);
  });

  it("enters the console session's focus mode", () => {
    const handlers = { onFocus: vi.fn() } as unknown as SidebarHandlers;
    const entry = sessionMenu(t, handlers, hub, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, focusTarget: { consoleSession: hub } }).find((candidate) => typeof candidate === "object" && candidate.label === "Focus mode");
    (entry as { onClick: () => void }).onClick();
    expect(handlers.onFocus).toHaveBeenCalledWith({ consoleSession: hub });
  });
});

describe("the Archive and Delete entries", () => {
  const entry = (session: Session, hasBoundSessions = false) =>
    sessionMenu(t, {} as SidebarHandlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, hasBoundSessions }).flatMap((item) => (typeof item === "object" && ["Archive", "Delete"].includes(item.label) ? [item.label] : []));
  const session = (extra: Partial<Session>) => sessionOf("s", "c", "p", "Title", "idle", extra);

  it.each([
    ["a session that has had a turn", "Archive", session({}), false],
    ["a session that has never been used", "Delete", session({ has_conversation: false }), false],
    ["an unused session the user opened under an owner", "Delete", session({ has_conversation: false, bound_to: "h", origin: "user" }), false],
    ["an unused owner with a bound session", "Archive", session({ has_conversation: false }), true],
    ["an archived session", "Delete", session({ status: "archived" }), false],
  ] as const)("%s gets %s", (_, label, target, hasBound) => {
    expect(entry(target, hasBound)).toEqual([label]);
  });
});

/** The handlers of the local daemon's window, which opens a project in this machine's file manager and terminal. */
function local(more: Partial<SidebarHandlers> = {}): SidebarHandlers {
  return { onOpenProjectDirectory: vi.fn(), onOpenSystemTerminal: vi.fn(), ...more } as unknown as SidebarHandlers;
}

describe("a project's menu", () => {
  const labels = (placement: MenuPlacement, gitRepository = false) =>
    projectMenu(t, local(), { id: "p" } as Project, [], { placement, gitStatus: gitRepository ? gitStatusOf("p") : undefined }).flatMap((entry) =>
      typeof entry === "object" ? [entry.label] : [],
    );

  it("leaves Focus mode out of any focus mode, and offers it in the project list", () => {
    expect(labels("list")).toContain("Focus mode");
    expect(labels("focused")).not.toContain("Focus mode");
    expect(labels("nested")).not.toContain("Focus mode");
  });

  it("puts Pin first, then groups viewing, copying and opening, managing the project, and removing it", () => {
    const handlers = local();
    const menu = projectMenu(t, handlers, { id: "p", pinned: false } as Project, [], { gitStatus: gitStatusOf("p") });
    expect(menu.map((entry) => (entry === "separator" ? entry : entry.label))).toEqual([
      "Pin", "Focus mode", "View archive", "separator",
      "Copy project name", "Copy absolute path", "separator",
      "Open in", "Open panel", "Git", "separator",
      "Rename", "Project settings", "separator",
      "Remove project",
    ]);
  });

  it("groups outside tools for every project", () => {
    const menu = projectMenu(t, local(), { id: "p" } as Project, []);
    expect(openIn(menu).items.map((entry) => typeof entry === "object" ? entry.label : entry)).toEqual(["File manager", "System terminal"]);
    expect(labels("list")).not.toContain("Terminal");
  });

  it("offers neither the file manager nor the system terminal for a daemon elsewhere, and no Open in with nothing in it", () => {
    const project = { id: "p", remote_url: "git@github.com:acme/app.git" } as Project;
    const remote = { onOpenRepository: vi.fn() } as unknown as SidebarHandlers;
    expect(openIn(projectMenu(t, remote, project, [])).items.map((entry) => typeof entry === "object" ? entry.label : entry)).toEqual(["GitHub"]);
    const labels = projectMenu(t, {} as SidebarHandlers, { id: "p" } as Project, []).flatMap((entry) => typeof entry === "object" ? [entry.label] : []);
    expect(labels).not.toContain("Open in");
  });

  it("opens the branded repository through the Open in submenu", () => {
    const handlers = local({ onOpenRepository: vi.fn() });
    const project = { id: "p", remote_url: "git@github.com:acme/app.git" } as Project;
    const entry = openIn(projectMenu(t, handlers, project, [])).items[2];
    expect(entry).toMatchObject({ label: "GitHub", icon: { props: { brand: "github" } } });
    (entry as { onClick: () => void }).onClick();
    expect(handlers.onOpenRepository).toHaveBeenCalledExactlyOnceWith("https://github.com/acme/app");
    expect(openIn(projectMenu(t, local(), project, [])).items).toHaveLength(2);
    expect(openIn(projectMenu(t, handlers, { ...project, remote_url: "/tmp/repo" }, [])).items).toHaveLength(2);
  });

  it("offers each panel in Open panel, below Open in, except those on screen for this project", () => {
    const panelItems = (visibleProjectPanels: SidebarHandlers["visibleProjectPanels"]) => {
      const menu = projectMenu(t, local({ visibleProjectPanels }), { id: "p" } as Project, []);
      const labels = menu.flatMap((entry) => typeof entry === "object" ? [entry.label] : []);
      const panel = menu.find((entry) => typeof entry === "object" && entry.label === "Open panel") as ActionMenuSubmenu | undefined;
      return { labels, items: panel?.items.flatMap((item) => typeof item === "object" ? [item.label] : []) };
    };
    expect(panelItems(undefined).items).toEqual(["Terminal", "Files", "Processes"]);
    expect(panelItems({ project: "other", kinds: ["files", "terminal"] }).items).toEqual(["Terminal", "Files", "Processes"]);
    expect(panelItems({ project: "p", kinds: ["files"] }).items).toEqual(["Terminal", "Processes"]);
    expect(panelItems({ project: "p", kinds: ["terminal", "report"] }).items).toEqual(["Files", "Processes"]);
    expect(panelItems({ project: "p", kinds: ["files", "processes"] }).items).toEqual(["Terminal"]);
    expect(panelItems({ project: "p", kinds: ["files", "processes", "terminal"] }).items).toBeUndefined();
    const { labels } = panelItems(undefined);
    expect(labels.indexOf("Open in") + 1).toBe(labels.indexOf("Open panel"));
    expect(labels).not.toContain("Browse files");
  });

  it("opens a panel of the project with its icon, the in-app Terminal bordered", () => {
    const handlers = { onOpenProjectPanel: vi.fn() } as unknown as SidebarHandlers;
    const project = { id: "p" } as Project;
    const panel = projectMenu(t, handlers, project, []).find((entry) => typeof entry === "object" && entry.label === "Open panel") as ActionMenuSubmenu;
    const [terminal, files, processes] = panel.items as { label: string; icon: unknown; onClick: () => void }[];
    files.onClick();
    processes.onClick();
    terminal.onClick();
    expect(vi.mocked(handlers.onOpenProjectPanel).mock.calls).toEqual([[project, "files"], [project, "processes"], [project, "terminal"]]);
    expect(processes.icon).toBe(SquareActivity);
    expect(terminal.icon).toBe(SquareTerminal);
  });

  it("opens this project's directory in the file manager", () => {
    const project = { id: "p" } as Project;
    const handlers = { onOpenProjectDirectory: vi.fn(), visibleProjectPanels: { project: "p", kinds: ["files"] } } as unknown as SidebarHandlers;
    const menu = projectMenu(t, handlers, project, []);
    const entry = openIn(menu).items.find((item) => typeof item === "object" && item.label === "File manager");
    (entry as { onClick: () => void }).onClick();
    expect(handlers.onOpenProjectDirectory).toHaveBeenCalledExactlyOnceWith(project);
  });

  it("puts pull and push under Git only for a git repository", () => {
    const git = (repository: boolean) => {
      const entry = projectMenu(t, local(), { id: "p" } as Project, [], { gitStatus: repository ? gitStatusOf("p") : undefined }).find(
        (candidate) => typeof candidate === "object" && candidate.label === "Git",
      );
      return entry && typeof entry === "object" && "items" in entry ? entry.items.flatMap((item) => (typeof item === "object" ? [item.label] : [])) : undefined;
    };
    expect(git(true)).toEqual(["Pull current branch", "Push current branch", "Switch branch", "Restore file changes"]);
    expect(git(false)).toBeUndefined();
  });

  it("opens the system terminal and pushes the current branch", () => {
    const handlers = { onOpenSystemTerminal: vi.fn(), onPushProjectBranch: vi.fn() } as unknown as SidebarHandlers;
    const project = { id: "p" } as Project;
    const menu = projectMenu(t, handlers, project, [], { gitStatus: gitStatusOf("p") });
    const terminal = openIn(menu).items.find((entry) => typeof entry === "object" && entry.label === "System terminal");
    expect(terminal).toMatchObject({ icon: Terminal });
    (terminal as { onClick: () => void }).onClick();
    expect(handlers.onOpenSystemTerminal).toHaveBeenCalledWith(project);
    const git = menu.find((entry) => typeof entry === "object" && "items" in entry && entry.label === "Git");
    const push = (git as { items: { label: string; onClick: () => void }[] }).items.find((item) => item.label === "Push current branch");
    push?.onClick();
    expect(handlers.onPushProjectBranch).toHaveBeenCalledWith(project);
  });

  it("leaves Pin out of the focused project's own menu only", () => {
    const pin = (placement: MenuPlacement) => labels(placement).some((label) => label === "Pin" || label === "Unpin");
    expect([pin("list"), pin("nested"), pin("focused")]).toEqual([true, true, false]);
  });
});

describe("a console session's Pin entry", () => {
  const hub = sessionOf("h", "c", undefined, "Hub", "idle");
  it("is left out of its own focus mode's menu only", () => {
    const pin = (placement: MenuPlacement) =>
      sessionMenu(t, {} as SidebarHandlers, hub, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, placement }).some((entry) => typeof entry === "object" && (entry.label === "Pin" || entry.label === "Unpin"));
    expect([pin("list"), pin("focused")]).toEqual([true, false]);
  });
});

describe("a console's menu", () => {
  it("groups project and archive actions before settings and deletion", () => {
    const handlers = { onOpenDialog: vi.fn(), onOpenArchive: vi.fn(), onSelectSession: vi.fn() };
    const hub = sessionOf("hub", "c", undefined, "Old hub", "archived", { kind: "console" });
    const menu = consoleMenu(t, consoleOf("c", "Main"), handlers, [hub]);
    const labels = menu.map((entry) => (entry === "separator" ? entry : entry.label));
    expect(labels).toEqual(["Add project", "View archive", "separator", "Console settings", "separator", "Delete console"]);
    const archive = menu[1] as { items: { label: string; onClick: () => void }[] };
    expect(archive.items.map((item) => item.label)).toEqual(["Old hub", "View all"]);
    archive.items[1].onClick();
    expect(handlers.onOpenArchive).toHaveBeenCalledWith({ console: "c" });
  });
});

describe("archive previews in menus", () => {
  const live = sessionOf("owner", "c", "p", "Live owner", "idle");
  const archivedOwner = sessionOf("archived-owner", "c", "p", "Archived owner", "archived", { ended_at: 0 });
  const child = sessionOf("child", "c", "p", "Live owner's child", "archived", { bound_to: live.id, ended_at: 100 });
  const nested = sessionOf("nested", "c", "p", "Nested child", "archived", { bound_to: archivedOwner.id, ended_at: 200 });
  const roots = Array.from({ length: 10 }, (_, index) => sessionOf(`root-${index}`, "c", "p", `Root ${index}`, "archived", { ended_at: index + 1 }));

  it.each(["list", "nested"] as const)("a project's %s menu shows the archive's ten newest rows and keeps the full archive reachable", (placement) => {
    const handlers = { onOpenArchive: vi.fn(), onSelectSession: vi.fn() } as unknown as SidebarHandlers;
    const archived = [...roots, archivedOwner, child, nested];
    const menu = projectMenu(t, handlers, { id: "p", console_id: "c" } as Project, archived, { placement });
    const archive = menu.find((entry) => typeof entry === "object" && "items" in entry && entry.label === "View archive");
    const items = (archive as { items: { label: string; onClick: () => void }[] }).items;
    expect(items.map((item) => item.label)).toEqual([
      "Live owner's child", "Root 9", "Root 8", "Root 7", "Root 6", "Root 5", "Root 4", "Root 3", "Root 2", "Root 1", "View all",
    ]);
    items[0].onClick();
    expect(handlers.onSelectSession).toHaveBeenCalledWith(child);
    items[10].onClick();
    expect(handlers.onOpenArchive).toHaveBeenCalledWith({ console: "c", project: "p" });
  });

  it("shows each archived session's archive time at the item's end", () => {
    const sized = { ...roots[0], context: { tokens: 12_345 } };
    const [item] = archiveSubmenu(t, "Archive", [sized], vi.fn(), vi.fn()).items as ActionMenuItem[];
    expect(item.end).toMatchObject({ type: SessionRelativeTime, props: { session: sized, touchPopover: false } });
  });

  it("leaves View archive out of a project's own focus mode, which lists its archive below", () => {
    const menu = projectMenu(t, local(), { id: "p" } as Project, roots, { placement: "focused" });
    expect(menu.some((entry) => typeof entry === "object" && entry.label === "View archive")).toBe(false);
  });

  it.each([
    ["a child whose owner is not in the archive", [nested], ["Nested child", "View all"]],
    ["one parent", [roots[0]], ["Root 0", "View all"]],
    ["empty", [], ["No archived sessions"]],
  ])("keeps navigation without a misleading empty message: %s", (_name, archived, labels) => {
    const items = archiveSubmenu(t, "Archive", archived as Session[], vi.fn(), vi.fn()).items;
    expect(items.map((item) => typeof item === "object" ? item.label : item)).toEqual(labels);
  });
});

it.each([[false, false], [true, true]])("with pushing %s, the project's Push item is disabled: %s", (pushing, disabled) => {
  const handlers = {} as unknown as SidebarHandlers;
  const git = projectMenu(t, handlers, { id: "p" } as Project, [], { gitStatus: gitStatusOf("p", { activity: pushing ? "pushing" : "idle" }) }).find((entry) => typeof entry === "object" && "items" in entry && entry.label === "Git");
  const items = (git as { items: { label: string; disabled?: boolean }[] }).items;
  expect(items.find((item) => item.label === "Push current branch")?.disabled ?? false).toBe(disabled);
  expect(items.find((item) => item.label === "Pull current branch")?.disabled ?? false).toBe(false);
});

it("asks for confirmation before restoring project changes", () => {
  const project = { id: "p" } as Project;
  const handlers = { onOpenDialog: vi.fn() } as unknown as SidebarHandlers;
  const git = projectMenu(t, handlers, project, [], { gitStatus: gitStatusOf("p") }).find((entry) => typeof entry === "object" && "items" in entry && entry.label === "Git");
  const entry = (git as { items: { label: string; onClick: () => void }[] }).items.find((item) => item.label === "Restore file changes");
  entry!.onClick();
  expect(handlers.onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind: "restore-project-changes", project });
});


describe("the Copy session ID entry", () => {
  it.each([
    ["project", "idle", "list"],
    ["console", "idle", "focused"],
    ["console", "starting", "list"],
    ["project", "archived", "nested"],
  ] as const)("copies a %s session while %s in the %s menu", (kind, status, placement) => {
    const session = sessionOf("session-id", "c", kind === "project" ? "p" : undefined, "Title", status);
    const handlers = { onCopySessionId: vi.fn() } as unknown as SidebarHandlers;
    const menu = sessionMenu(t, handlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, placement });
    const labels = menu.map((entry) => entry === "separator" ? entry : entry.label);
    const copyIndex = labels.indexOf("Copy session ID");
    // An archived session's menu leads with Reopen, so Copy session ID sits alone between two separators there too.
    if (status === "archived") expect(labels[0]).toBe("Reopen");
    expect(labels[copyIndex - 1]).toBe("separator");
    expect(labels[copyIndex + 1]).toBe("separator");
    const entry = menu.find((item) => typeof item === "object" && item.label === "Copy session ID");
    (entry as { onClick: () => void }).onClick();
    expect(handlers.onCopySessionId).toHaveBeenCalledExactlyOnceWith(session);
  });
});

it("keeps a locked live session protected and offers unlocking", () => {
  const session = sessionOf("locked", "c", "p", "Protected", "idle", { locked: true });
  const handlers = { onSetLocked: vi.fn() } as unknown as SidebarHandlers;
  const items = sessionMenu(t, handlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false });
  const unlock = items.find((item) => typeof item !== "string" && item.label === "Unlock");
  const removal = items.find((item) => typeof item !== "string" && ["Archive", "Delete"].includes(item.label));
  expect(removal).toMatchObject({ disabled: true });
  expect(unlock).toBeDefined();
  (unlock as { onClick: () => void }).onClick();
  expect(handlers.onSetLocked).toHaveBeenCalledWith(session, false);
});

it("offers unbinding only on a bound session that is not archived", () => {
  const handlers = { onOpenDialog: vi.fn() } as unknown as SidebarHandlers;
  const unbind = (session: Session) => sessionMenu(t, handlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false }).find((item) => typeof item === "object" && item.label === "Unbind");
  expect(unbind(sessionOf("free", "c", "p", "Free", "idle"))).toBeUndefined();
  const bound = sessionOf("bound", "c", "p", "Bound", "idle", { bound_to: "hub" });
  (unbind(bound) as { onClick: () => void }).onClick();
  expect(handlers.onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind: "unbind-session", session: bound });
  expect(unbind(sessionOf("new", "c", "p", "New", "starting", { bound_to: "hub" }))).toMatchObject({ disabled: true });
  expect(unbind(sessionOf("gone", "c", "p", "Gone", "archived", { bound_to: "hub" }))).toBeUndefined();
});

describe("the Bind to entry", () => {
  const free = sessionOf("free", "c", "p", "Free", "idle");
  const hub = sessionOf("hub", "c", undefined, "Hub", "idle");
  const coordinator = sessionOf("coord", "c", "p", "Coordinator", "idle", { agent: "codex" });
  const menuOf = (session: Session, bindOwners: { console: Session[]; project: Session[] } | undefined, handlers = {} as SidebarHandlers, switching = false) =>
    sessionMenu(t, handlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching, bindOwners });
  const bindOf = (menu: ActionMenuEntry[]) => menu.find((entry) => typeof entry === "object" && entry.label === "Bind to");
  const labelsOf = (menu: ActionMenuEntry[]) => menu.map((entry) => (entry === "separator" ? entry : entry.label));

  it("is offered where Unbind would be, right after Hand off", () => {
    const labels = labelsOf(menuOf(free, { console: [hub], project: [] }));
    expect(labels.indexOf("Bind to")).toBe(labels.indexOf("Hand off") + 1);
    expect(labels).not.toContain("Unbind");
  });

  it("is a submenu with the Link icon", () => {
    const entry = bindOf(menuOf(free, { console: [hub], project: [] })) as ActionMenuSubmenu;
    expect(entry.items).toBeInstanceOf(Array);
    expect(entry.icon).toBe(Link);
  });

  it("is not offered when the menu is given no owners", () => {
    expect(bindOf(menuOf(free, undefined))).toBeUndefined();
  });

  it("is not offered on a bound session that is given no owners, which offers Unbind instead", () => {
    const bound = sessionOf("bound", "c", "p", "Bound", "idle", { bound_to: "hub" });
    const menu = menuOf(bound, undefined);
    expect(bindOf(menu)).toBeUndefined();
    expect(labelsOf(menu)).toContain("Unbind");
  });

  it("groups the owners under a Console sessions and a Project sessions heading, with a separator between the groups", () => {
    const entry = bindOf(menuOf(free, { console: [hub], project: [coordinator] })) as ActionMenuSubmenu;
    expect(entry.items.map((item) => (item === "separator" ? item : item.label))).toEqual(["Console sessions", "Hub", "separator", "Project sessions", "Coordinator"]);
    expect(entry.items[0]).toMatchObject({ heading: true });
    expect(entry.items[3]).toMatchObject({ heading: true });
  });

  it("shows only the heading of the group that has owners, with no separator", () => {
    const only = (owners: { console: Session[]; project: Session[] }) => (bindOf(menuOf(free, owners)) as ActionMenuSubmenu).items.map((item) => (item === "separator" ? item : item.label));
    expect(only({ console: [hub], project: [] })).toEqual(["Console sessions", "Hub"]);
    expect(only({ console: [], project: [coordinator] })).toEqual(["Project sessions", "Coordinator"]);
  });

  it("keeps the owners in the order given", () => {
    const second = sessionOf("hub2", "c", undefined, "Second", "idle");
    const entry = bindOf(menuOf(free, { console: [second, hub], project: [] })) as ActionMenuSubmenu;
    expect(entry.items.slice(1).map((item) => (item as ActionMenuItem).label)).toEqual(["Second", "Hub"]);
  });

  it("binds the session to the owner that is chosen, showing the owner's agent", () => {
    const handlers = { onBindSession: vi.fn() } as unknown as SidebarHandlers;
    const entry = bindOf(menuOf(free, { console: [], project: [coordinator] }, handlers)) as ActionMenuSubmenu;
    const item = entry.items[1] as ActionMenuItem;
    expect(item.icon).toBeTruthy();
    expect(item).toHaveProperty("content");
    item.onClick();
    expect(handlers.onBindSession).toHaveBeenCalledExactlyOnceWith(free, coordinator);
  });

  it("names each owner for assistive technology by its title and its agent with the account it runs under", () => {
    const plain = sessionOf("hub2", "c", undefined, "Second hub", "idle");
    const onAccount = sessionOf("hub3", "c", undefined, "Third hub", "idle", { account_id: "w", config_dir: "/home/w" });
    const menu = sessionMenu(t, {} as SidebarHandlers, free, [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, bindOwners: { console: [plain, onAccount], project: [] } });
    const items = (bindOf(menu) as ActionMenuSubmenu).items.filter((item): item is ActionMenuItem => typeof item === "object" && "onClick" in item);
    const agent = format("en", "provider.claude.label");
    const agentOn = (name: string) => format("en", "session.agentAccount", { agent: isolate(agent), account: isolate(name) });
    const withAccount = agentOn("w");
    expect(items[0]!.ariaLabel).toBe(format("en", "sidebar.session.bindOwnerName", { title: isolate("Second hub"), agent: isolate(agentOn(DEFAULT_ACCOUNT_NAME)) }));
    expect(items[1]!.ariaLabel).toBe(format("en", "sidebar.session.bindOwnerName", { title: isolate("Third hub"), agent: isolate(withAccount) }));
  });

  it("is a submenu of a single disabled item when no session can own it", () => {
    const entry = bindOf(menuOf(free, { console: [], project: [] })) as ActionMenuSubmenu;
    expect(entry.items).toHaveLength(1);
    expect(entry.items[0]).toMatchObject({ label: "No possible owners", disabled: true });
  });

  it("is a disabled plain item, not a submenu, while the session is still starting", () => {
    const entry = bindOf(menuOf(sessionOf("new", "c", "p", "New", "starting"), { console: [hub], project: [] }));
    expect(entry).toMatchObject({ label: "Bind to", disabled: true });
    expect(entry).not.toHaveProperty("items");
  });

  it("is a disabled plain item, not a submenu, while an account switch holds the session", () => {
    const entry = bindOf(menuOf(free, { console: [hub], project: [] }, {} as SidebarHandlers, true));
    expect(entry).toMatchObject({ label: "Bind to", disabled: true });
    expect(entry).not.toHaveProperty("items");
  });
});

it.each([false, true])("offers no lock controls on archived sessions, locked=%s", (locked) => {
  const session = sessionOf("archived", "c", "p", "Archived", "archived", { locked });
  const labels = sessionMenu(t, {} as SidebarHandlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false }).flatMap((item) => typeof item === "object" ? [item.label] : []);
  expect(labels).not.toContain("Lock");
  expect(labels).not.toContain("Unlock");
  expect(labels).toContain("Reopen");
  expect(labels).toContain("Delete");
});


it("keeps composed archive names raw for matching while retaining their visible overflow presentation", () => {
  const session = sessionOf("archived", "c", "p", "مراجعة Work", "archived", { terminal_title: "Task 中文" });
  const entry = archiveSubmenu(t, "Archive", [session], vi.fn(), vi.fn()).items[0];
  expect(typeof entry).toBe("object");
  if (typeof entry !== "object" || !("onClick" in entry)) throw new Error("Expected archived session action");
  expect(entry.label).toBe("Task 中文｜مراجعة Work");
  expect(entry.label).not.toMatch(/[\u2066-\u2069]/);
  expect(entry.content).toBeTruthy();
});

describe("menu-only letters on direct actions", () => {
  const direct = (menu: ActionMenuEntry[]) => menu.filter((entry): entry is ActionMenuItem => typeof entry === "object" && "onClick" in entry);
  const letters = (menu: ActionMenuEntry[]) => direct(menu).map(({ label, shortcut }) => [label, shortcut]);
  const noNestedLetters = (menu: ActionMenuEntry[]) => {
    for (const entry of menu) {
      if (typeof entry !== "object" || "onClick" in entry || "note" in entry) continue;
      expect(entry).not.toHaveProperty("shortcut");
      if ("items" in entry) {
        for (const item of entry.items) if (typeof item === "object") expect(item).not.toHaveProperty("shortcut");
        noNestedLetters(entry.items);
      }
    }
  };
  const uniqueLetters = (menu: ActionMenuEntry[]) => {
    const keys = direct(menu).filter((entry) => !entry.disabled && !entry.reason).map((entry) => entry.shortcut);
    expect(keys.every((key) => typeof key === "string" && /^[A-Z]$/.test(key))).toBe(true);
    expect(new Set(keys).size).toBe(keys.length);
  };

  it.each(["list", "focused", "nested"] as const)("keeps project letters unique in the %s menu, pinned or not", (placement) => {
    for (const pinned of [false, true]) {
      const project = { id: "p", pinned } as Project;
      const menu = projectMenu(t, local(), project, [], { placement, gitStatus: gitStatusOf("p") });
      expect(letters(menu)).toEqual([
        ...(placement !== "focused" ? [[pinned ? "Unpin" : "Pin", "P"]] : []),
        ...(placement === "list" ? [["Focus mode", "F"]] : []),
        ["Copy project name", "C"], ["Copy absolute path", "A"],
        ["Rename", "R"], ["Project settings", "E"], ["Remove project", "D"],
      ]);
      noNestedLetters(menu);
      uniqueLetters(menu);
    }
  });

  it("gives a console's direct actions letters and leaves its archive submenu unbound", () => {
    const menu = consoleMenu(t, consoleOf("c", "Main"), { onOpenDialog: vi.fn(), onSelectSession: vi.fn(), onOpenArchive: vi.fn() }, []);
    expect(letters(menu)).toEqual([["Add project", "A"], ["Console settings", "E"], ["Delete console", "D"]]);
    noNestedLetters(menu);
    uniqueLetters(menu);
  });

  it.each(["list", "focused"] as const)("keeps session leaves bound and Colour and Switch account unbound in %s", (placement) => {
    const session = sessionOf("hub", "c", undefined, "Hub", "idle");
    const menu = sessionMenu(t, {} as SidebarHandlers, session, [account("work", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, placement });
    expect(menu.filter((entry) => typeof entry === "object" && ("items" in entry || "panel" in entry)).map((entry) => typeof entry === "object" && entry.label)).toEqual(["Colour", "Switch account"]);
    noNestedLetters(menu);
    uniqueLetters(menu);
  });
});

const gitItems = (status: GitStatus | undefined) => {
  const git = projectMenu(t, {} as unknown as SidebarHandlers, { id: "p" } as Project, [], { gitStatus: status }).find((entry) => typeof entry === "object" && "items" in entry && entry.label === "Git") as { items: ActionMenuItem[] } | undefined;
  return git?.items;
};

it.each([
  [{ branch: "main", detached: false, upstream: null }, "No upstream"],
  [{ branch: "abc1234", detached: true, upstream: null }, "Not on a branch"],
] as const)("disables the project's Pull item with its reason for the git status %j", (extra, state) => {
  const items = gitItems(gitStatusOf("p", extra))!;
  const pull = items.find((item) => item.label === "Pull current branch")!;
  expect(pull).toMatchObject({ disabled: true, reason: { state } });
  expect(pull.description).toBeTruthy();
  expect(items.find((item) => item.label === "Push current branch")!.disabled ?? false).toBe(false);
});

it.each([
  ["a branch with an upstream", gitStatusOf("p")],
  ["a status that has not read the branch", gitStatusOf("p", { branch: null, upstream: null })],
])("leaves the project's Pull item enabled for %s", (_, status) => {
  expect(gitItems(status)!.find((item) => item.label === "Pull current branch")!.disabled ?? false).toBe(false);
});

it("offers no Git submenu without a git status, or for a project that is not a repository", () => {
  expect(gitItems(undefined)).toBeUndefined();
  expect(gitItems(gitStatusOf("p", { repository: false, branch: null, upstream: null }))).toBeUndefined();
});

describe("the menu of a session in a status this client does not know", () => {
  const nativeApp = { label: "Open in Claude app", seen: true, onOpen: () => {} };
  const labelsOf = (session: Session) => sessionMenu(t, {} as SidebarHandlers, session, [account("w", "claude")], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false, nativeApp, bindOwners: { console: [sessionOf("hub", "c", undefined, "Hub", "idle")], project: [] } })
    .flatMap((entry) => (typeof entry === "object" && "label" in entry ? [entry.label] : []));

  it("offers no Change model, Switch account, Hand off, Open in app, Bind to or Unbind, which depend on the status", () => {
    const labels = labelsOf(sessionOf("s", "c", "p", "Title", "from_a_newer_daemon", { bound_to: "hub" }));
    for (const label of ["Change model", "Switch account", "Hand off", "Open in Claude app", "Bind to", "Unbind"]) expect(labels).not.toContain(label);
  });

  it("keeps what does not depend on the status", () => {
    const labels = labelsOf(sessionOf("s", "c", "p", "Title", "from_a_newer_daemon", { bound_to: "hub" }));
    for (const label of ["Lock", "Rename", "Archive", "Copy session ID"]) expect(labels).toContain(label);
  });

  it("offers them all on a known status, so the omission above is the status's", () => {
    const labels = labelsOf(sessionOf("s", "c", "p", "Title", "interrupted", { bound_to: "hub" }));
    for (const label of ["Change model", "Switch account", "Hand off", "Open in Claude app", "Bind to", "Unbind"]) expect(labels).toContain(label);
  });
});

describe("the menu of a session of a kind this client does not know", () => {
  const labelsOf = (session: Session) => sessionMenu(t, {} as SidebarHandlers, session, [], {}, { host: BROWSER_HOST, providers: TEST_PROVIDERS, switching: false })
    .flatMap((entry) => (typeof entry === "object" && "label" in entry ? [entry.label] : []));

  it("offers no Colour, which is a console session's own, and keeps the actions of any session", () => {
    const labels = labelsOf({ ...sessionOf("s", "c", undefined, "Title", "idle"), kind: "from_a_newer_daemon" });
    expect(labels).not.toContain("Colour");
    for (const label of ["Lock", "Rename", "Archive", "Copy session ID"]) expect(labels).toContain(label);
  });

  it("offers Colour for a console session, so the omission above is the kind's", () => {
    expect(labelsOf(sessionOf("s", "c", undefined, "Title", "idle"))).toContain("Colour");
  });
});
