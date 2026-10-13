import { describe, expect, it, vi } from "vitest";

import { consoleOf, sessionOf } from "../gallery/fixtures/builders";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import type { Account, Project, Session } from "../protocol";
import { consoleMenu, type MenuPlacement, projectMenu, sessionMenu } from "./menus";
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
  const labels = (session: Session, placement: MenuPlacement = "list") =>
    sessionMenu(t, {} as SidebarHandlers, session, [], { placement }).flatMap((entry) => (typeof entry === "object" ? [entry.label] : []));

  it.each([
    ["a console session's row", hub, "list", true],
    ["a console session's own focus mode", hub, "focused", false],
    ["an archived console session", { ...hub, status: "archived" as const }, "list", false],
    ["a project session", sessionOf("s", "c", "p", "Title", "idle"), "list", false],
  ] as const)("is offered for %s: %#", (_, session, placement, offered) => {
    expect(labels(session, placement).includes("Focus mode")).toBe(offered);
  });

  it("enters the console session's focus mode", () => {
    const handlers = { onFocus: vi.fn() } as unknown as SidebarHandlers;
    const entry = sessionMenu(t, handlers, hub, []).find((candidate) => typeof candidate === "object" && candidate.label === "Focus mode");
    (entry as { onClick: () => void }).onClick();
    expect(handlers.onFocus).toHaveBeenCalledWith({ consoleSession: hub });
  });
});

describe("a project's menu", () => {
  const labels = (placement: MenuPlacement, gitRepository = false) =>
    projectMenu(t, {} as SidebarHandlers, { id: "p" } as Project, [], { placement, gitRepository }).flatMap((entry) =>
      typeof entry === "object" ? [entry.label] : [],
    );

  it("leaves Focus mode out of any focus mode, and offers it in the project list", () => {
    expect(labels("list")).toContain("Focus mode");
    expect(labels("focused")).not.toContain("Focus mode");
    expect(labels("nested")).not.toContain("Focus mode");
  });

  it("offers Open terminal for every project", () => {
    expect(labels("list")).toContain("Open terminal");
    expect(labels("focused")).toContain("Open terminal");
  });

  it("puts sync and push under Git only for a git repository", () => {
    const git = (repository: boolean) => {
      const entry = projectMenu(t, {} as SidebarHandlers, { id: "p" } as Project, [], { gitRepository: repository }).find(
        (candidate) => typeof candidate === "object" && candidate.label === "Git",
      );
      return entry && typeof entry === "object" && "items" in entry ? entry.items.flatMap((item) => (typeof item === "object" ? [item.label] : [])) : undefined;
    };
    expect(git(true)).toEqual(["Sync repository", "Push current branch"]);
    expect(git(false)).toBeUndefined();
  });

  it("opens a terminal and pushes the current branch", () => {
    const handlers = { onOpenProjectTerminal: vi.fn(), onPushProjectBranch: vi.fn() } as unknown as SidebarHandlers;
    const project = { id: "p" } as Project;
    const menu = projectMenu(t, handlers, project, [], { gitRepository: true });
    const terminal = menu.find((entry) => typeof entry === "object" && entry.label === "Open terminal");
    (terminal as { onClick: () => void }).onClick();
    expect(handlers.onOpenProjectTerminal).toHaveBeenCalledWith(project);
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
      sessionMenu(t, {} as SidebarHandlers, hub, [], { placement }).some((entry) => typeof entry === "object" && (entry.label === "Pin" || entry.label === "Unpin"));
    expect([pin("list"), pin("focused")]).toEqual([true, false]);
  });
});

describe("the Export configuration entry", () => {
  const entries = (canExport: boolean, onOpenDialog = vi.fn()) =>
    consoleMenu(t, consoleOf("c", "Main"), onOpenDialog, canExport).filter(
      (entry): entry is Exclude<typeof entry, "separator"> => entry !== "separator",
    );

  it("is offered only where the platform can save a file", () => {
    expect(entries(true).map((entry) => entry.label)).toContain("Export configuration…");
    expect(entries(false).map((entry) => entry.label)).not.toContain("Export configuration…");
  });

  it("opens the export of this console", () => {
    const onOpenDialog = vi.fn();
    const exportItem = entries(true, onOpenDialog).find((entry) => entry.label === "Export configuration…") as { onClick: () => void };
    exportItem.onClick();
    expect(onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind: "export-configuration", console: consoleOf("c", "Main") });
  });
});
