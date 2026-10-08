import { describe, expect, it, vi } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import type { Account, Project, Session } from "../protocol";
import { projectMenu, sessionMenu } from "./menus";
import type { SidebarHandlers } from "./types";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const account = (id: string, agent: Account["agent"]): Account => ({ id, agent, name: id, config_dir: `/home/${id}` });

function submenu(session: Session, accounts: Account[], handlers = {} as SidebarHandlers) {
  const entry = sessionMenu(t, handlers, session, accounts).find((candidate) => typeof candidate === "object" && "items" in candidate);
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
  ] as const)("is present exactly when there is somewhere to go: %s", (_, session, accounts, shown) => {
    expect(submenu(session, [...accounts])?.label === "Switch account").toBe(shown);
  });

  it("marks the current account without making it actionable, picks another through a confirmation, and ends in Settings", () => {
    const handlers = { onOpenDialog: vi.fn(), onOpenSettings: vi.fn() } as unknown as SidebarHandlers;
    const items = submenu(project(), [account("w", "claude")], handlers)!.items;
    const [current, other, separator, manage] = items;

    expect([separator, (current as { selected: boolean }).selected, (other as { selected: boolean }).selected]).toEqual(["separator", true, false]);
    (current as { onClick: () => void }).onClick();
    expect(handlers.onOpenDialog).not.toHaveBeenCalled();
    (other as { onClick: () => void }).onClick();
    expect(handlers.onOpenDialog).toHaveBeenCalledWith(expect.objectContaining({ kind: "switch-account", account: "w", accountName: "w" }));
    (manage as { onClick: () => void }).onClick();
    expect(handlers.onOpenSettings).toHaveBeenCalledWith("accounts");
  });
});

describe("the Focus mode entry", () => {
  const hub = sessionOf("h", "c", undefined, "Hub", "idle");
  const labels = (session: Session, inFocus = false) =>
    sessionMenu(t, {} as SidebarHandlers, session, [], { inFocus }).flatMap((entry) => (typeof entry === "object" ? [entry.label] : []));

  it.each([
    ["a console session's row", hub, false, true],
    ["a console session's own focus mode", hub, true, false],
    ["an archived console session", { ...hub, status: "archived" as const }, false, false],
    ["a project session", sessionOf("s", "c", "p", "Title", "idle"), false, false],
  ])("is offered for %s: %#", (_, session, inFocus, offered) => {
    expect(labels(session, inFocus).includes("Focus mode")).toBe(offered);
  });

  it("enters the console session's focus mode", () => {
    const handlers = { onFocus: vi.fn() } as unknown as SidebarHandlers;
    const entry = sessionMenu(t, handlers, hub, []).find((candidate) => typeof candidate === "object" && candidate.label === "Focus mode");
    (entry as { onClick: () => void }).onClick();
    expect(handlers.onFocus).toHaveBeenCalledWith({ consoleSession: hub });
  });
});

describe("a project's Focus mode entry", () => {
  const labels = (inFocus: boolean) =>
    projectMenu(t, {} as SidebarHandlers, { id: "p" } as Project, [], { inFocus }).flatMap((entry) => (typeof entry === "object" ? [entry.label] : []));

  it("is left out of any focus mode's project menu, and offered in the project list's", () => {
    expect(labels(false)).toContain("Focus mode");
    expect(labels(true)).not.toContain("Focus mode");
  });
});
