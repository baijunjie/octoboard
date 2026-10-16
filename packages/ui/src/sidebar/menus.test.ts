import { describe, expect, it, vi } from "vitest";

import { consoleOf, sessionOf } from "../gallery/fixtures/builders";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import type { Account, Project, Session } from "../protocol";
import { archiveSubmenu, consoleMenu, type MenuPlacement, projectMenu, sessionMenu } from "./menus";
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

describe("the Colour entry", () => {
  const hub = sessionOf("h", "c", undefined, "Hub", "idle", { colour: "cyan" });
  const entry = (session: Session, handlers = {} as SidebarHandlers) => sessionMenu(t, handlers, session, []).find((candidate) => typeof candidate === "object" && candidate.label === "Colour");

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

describe("the Archive and Delete entries", () => {
  const entry = (session: Session, hasBoundSessions = false) =>
    sessionMenu(t, {} as SidebarHandlers, session, [], { hasBoundSessions }).flatMap((item) => (typeof item === "object" && ["Archive", "Delete"].includes(item.label) ? [item.label] : []));
  const session = (extra: Partial<Session>) => sessionOf("s", "c", "p", "Title", "idle", extra);

  it.each([
    ["a session that has had a turn", "Archive", session({}), false],
    ["a session that has never been used", "Delete", session({ has_conversation: false }), false],
    ["an unused session bound to an owner", "Archive", session({ has_conversation: false, bound_to: "h" }), false],
    ["an unused owner with a bound session", "Archive", session({ has_conversation: false }), true],
    ["an archived session", "Delete", session({ status: "archived" }), false],
  ] as const)("%s gets %s", (_, label, target, hasBound) => {
    expect(entry(target, hasBound)).toEqual([label]);
  });
});

describe("a project's menu", () => {
  const labels = (placement: MenuPlacement, gitRepository = false) =>
    projectMenu(t, {} as SidebarHandlers, { id: "p" } as Project, [], [], { placement, gitRepository }).flatMap((entry) =>
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

  it.each(["list", "focused", "nested"] as const)("offers Browse files only while this project's browser is not visible in the %s menu", (placement) => {
    for (const visibleBrowserProjectId of [undefined, "other", "p"]) {
      const handlers = { visibleBrowserProjectId } as SidebarHandlers;
      const menu = projectMenu(t, handlers, { id: "p" } as Project, [], [], { placement });
      const menuLabels = menu.flatMap((entry) => typeof entry === "object" ? [entry.label] : []);
      expect(menuLabels.includes("Browse files")).toBe(visibleBrowserProjectId !== "p");
      expect(menuLabels).toContain("Open in file manager");
    }
  });

  it("opens this project's directory in the file manager", () => {
    const project = { id: "p" } as Project;
    const handlers = { onOpenProjectDirectory: vi.fn(), visibleBrowserProjectId: "p" } as unknown as SidebarHandlers;
    const menu = projectMenu(t, handlers, project, [], []);
    const entry = menu.find((item) => typeof item === "object" && item.label === "Open in file manager");
    (entry as { onClick: () => void }).onClick();
    expect(handlers.onOpenProjectDirectory).toHaveBeenCalledExactlyOnceWith(project);
  });

  it("puts pull and push under Git only for a git repository", () => {
    const git = (repository: boolean) => {
      const entry = projectMenu(t, {} as SidebarHandlers, { id: "p" } as Project, [], [], { gitRepository: repository }).find(
        (candidate) => typeof candidate === "object" && candidate.label === "Git",
      );
      return entry && typeof entry === "object" && "items" in entry ? entry.items.flatMap((item) => (typeof item === "object" ? [item.label] : [])) : undefined;
    };
    expect(git(true)).toEqual(["Pull current branch", "Push current branch", "Switch branch", "Restore file changes"]);
    expect(git(false)).toBeUndefined();
  });

  it("opens a terminal and pushes the current branch", () => {
    const handlers = { onOpenProjectTerminal: vi.fn(), onPushProjectBranch: vi.fn() } as unknown as SidebarHandlers;
    const project = { id: "p" } as Project;
    const menu = projectMenu(t, handlers, project, [], [], { gitRepository: true });
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

describe("a console's menu", () => {
  it("lists Add project, Edit console, the archived console sessions and Delete console", () => {
    const handlers = { onOpenDialog: vi.fn(), onOpenArchive: vi.fn(), onSelectSession: vi.fn() };
    const hub = sessionOf("hub", "c", undefined, "Old hub", "archived", { role: "console" });
    const projectSession = sessionOf("work", "c", "p", "Project work", "archived");
    const menu = consoleMenu(t, consoleOf("c", "Main"), handlers, [hub, projectSession]);
    const labels = menu.map((entry) => (entry === "separator" ? entry : entry.label));
    expect(labels).toEqual(["Add project", "Edit console", "Archived console sessions", "separator", "Delete console"]);
    const archive = menu[2] as { items: { label: string; onClick: () => void }[] };
    expect(archive.items.map((item) => item.label)).toEqual(["Old hub", "View all"]);
    archive.items[1].onClick();
    expect(handlers.onOpenArchive).toHaveBeenCalledWith({ console: "c" });
  });
});

describe("archive previews in menus", () => {
  const live = sessionOf("owner", "c", "p", "Live owner", "idle");
  const child = sessionOf("child", "c", "p", "Hidden child", "archived", { bound_to: live.id });
  const roots = Array.from({ length: 6 }, (_, index) => sessionOf(`root-${index}`, "c", "p", `Root ${index}`, "archived", { ended_at: index }));

  it.each(["list", "focused", "nested"] as const)("a project's %s menu shows five parents and keeps the full archive reachable", (placement) => {
    const handlers = { onOpenArchive: vi.fn(), onSelectSession: vi.fn() } as unknown as SidebarHandlers;
    const archived = [...roots, child];
    const menu = projectMenu(t, handlers, { id: "p", console_id: "c" } as Project, archived, [...archived, live], { placement });
    const archive = menu.find((entry) => typeof entry === "object" && "items" in entry && entry.label === "View archive");
    const items = (archive as { items: { label: string; onClick: () => void }[] }).items;
    expect(items.map((item) => item.label)).toEqual(["Root 5", "Root 4", "Root 3", "Root 2", "Root 1", "View all"]);
    items[0].onClick();
    expect(handlers.onSelectSession).toHaveBeenCalledWith(roots[5]);
    items[5].onClick();
    expect(handlers.onOpenArchive).toHaveBeenCalledWith({ console: "c", project: "p" });
  });

  it.each([
    ["only a child", [child], [child, live], ["View all"]],
    ["one parent", [roots[0]], [roots[0]], ["Root 0", "View all"]],
    ["empty", [], [], ["No archived sessions"]],
  ])("keeps navigation without a misleading empty message: %s", (_name, archived, context, labels) => {
    const items = archiveSubmenu(t, "Archive", archived as Session[], context as Session[], vi.fn(), vi.fn()).items;
    expect(items.map((item) => typeof item === "object" ? item.label : item)).toEqual(labels);
  });
});

it("asks for confirmation before restoring project changes", () => {
  const project = { id: "p" } as Project;
  const handlers = { onOpenDialog: vi.fn() } as unknown as SidebarHandlers;
  const git = projectMenu(t, handlers, project, [], [], { gitRepository: true }).find((entry) => typeof entry === "object" && "items" in entry && entry.label === "Git");
  const entry = (git as { items: { label: string; onClick: () => void }[] }).items.find((item) => item.label === "Restore file changes");
  entry!.onClick();
  expect(handlers.onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind: "restore-project-changes", project });
});
