// @vitest-environment jsdom
import { KeyRound } from "lucide-react";
import React from "react";
import { describe, expect, it, vi } from "vitest";

import type { ActionMenuEntry, ActionMenuItem, ActionMenuSubmenu } from "../components/ActionMenu";
import { SessionContextLabel } from "../components/SessionMetadata";
import { consoleOf, sessionOf } from "../gallery/fixtures/builders";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import type { Account, Project, Session } from "../protocol";
import { archiveSubmenu, consoleMenu, type MenuPlacement, projectMenu, sessionMenu } from "./menus";
import type { SidebarHandlers } from "./types";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const openIn = (menu: ActionMenuEntry[]) => menu.find((entry) => typeof entry === "object" && entry.label === "Open in") as ActionMenuSubmenu;
const account = (id: string, agent: Account["agent"]): Account => ({ id, agent, name: id, config_dir: `/home/${id}` });

function submenu(session: Session, accounts: Account[], handlers = {} as SidebarHandlers) {
  const entry = sessionMenu(t, handlers, session, accounts, undefined).find((candidate) => typeof candidate === "object" && "items" in candidate);
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

  it("disables an account whose quota is used up at the time given, with why, and never the current one", () => {
    const spent = (accountId: string | null) => ({
      agent: "claude" as const, account: accountId, fetched_at: 0, plan: null, unavailable: null,
      windows: [{ kind: "session" as const, label: null, used_percent: 100, resets_at: 7_200_000, window_minutes: 300 }],
    });
    const menu = (quotas: ReturnType<typeof spent>[], now: number) => {
      const entry = sessionMenu(t, {} as SidebarHandlers, project(), [account("w", "claude")], undefined, { quota: { quotas, now } })
        .find((candidate) => typeof candidate === "object" && "items" in candidate) as ActionMenuSubmenu;
      return entry.items.slice(0, 2) as { reason?: { state: string; detail?: string } }[];
    };
    const [current, other] = menu([spent("w"), spent(null)], 0);
    expect(current.reason).toBeUndefined();
    expect(other.reason).toEqual({ state: "Used up", detail: "resets in 2h 0m" });
    expect(menu([spent("w")], 7_200_000)[1].reason).toBeUndefined();
  });

  it("disables an account whose sharing change runs, with why, whatever its quota, and never the current one", () => {
    const sharing = new Map([["w", { account: "w", progress: { share: true, phase: "changing" as const, done: 1, total: 3, item: "skills" } }]]);
    const entry = sessionMenu(t, {} as SidebarHandlers, project(), [account("w", "claude")], undefined, { sharing })
      .find((candidate) => typeof candidate === "object" && "items" in candidate) as ActionMenuSubmenu;
    const [current, other] = entry.items as { reason?: { state: string }; description?: string }[];
    expect(current.reason).toBeUndefined();
    expect(other.reason).toEqual({ state: "Changing sharing" });
    expect(other.description).toBe("Unavailable while its sharing is being changed");
    const free = sessionMenu(t, {} as SidebarHandlers, project(), [account("w", "claude")], undefined, { sharing: new Map() })
      .find((candidate) => typeof candidate === "object" && "items" in candidate) as ActionMenuSubmenu;
    expect((free.items[1] as { reason?: unknown }).reason).toBeUndefined();
    const onto = sessionMenu(t, {} as SidebarHandlers, project({ account_id: "w", config_dir: "/home/w" }), [account("w", "claude")], undefined, { sharing })
      .find((candidate) => typeof candidate === "object" && "items" in candidate) as ActionMenuSubmenu;
    expect((onto.items[1] as { selected: boolean; reason?: unknown }).selected).toBe(true);
    expect((onto.items[1] as { reason?: unknown }).reason).toBeUndefined();
  });

  it("disables every other account while the session's own account is changing its sharing", () => {
    const sharing = new Map([["w", { account: "w", progress: { share: true, phase: "changing" as const, done: 1, total: 3, item: "skills" } }]]);
    const entry = sessionMenu(t, {} as SidebarHandlers, project({ account_id: "w", config_dir: "/home/w" }), [account("w", "claude")], undefined, { sharing })
      .find((candidate) => typeof candidate === "object" && "items" in candidate) as ActionMenuSubmenu;
    const [dflt, current] = entry.items as { reason?: { state: string }; selected: boolean }[];
    expect(current.selected).toBe(true);
    expect(current.reason).toBeUndefined();
    expect(dflt.reason).toEqual({ state: "Account changing sharing" });
    expect((dflt as { description?: string }).description).toBe("Unavailable while this session’s account is having its sharing changed");
  });

  it("disables Hand off with the same reason while the session's own account is changing its sharing", () => {
    const sharing = new Map([["w", { account: "w", progress: { share: true, phase: "changing" as const, done: 1, total: 3, item: "skills" } }]]);
    const handOff = (opts: { sharing?: typeof sharing }) => sessionMenu(t, {} as SidebarHandlers, project({ account_id: "w", config_dir: "/home/w" }), [account("w", "claude")], undefined, opts)
      .find((entry) => typeof entry === "object" && "label" in entry && entry.label === "Hand off") as { disabled?: boolean; reason?: { state: string }; description?: string };
    expect(handOff({ sharing }).reason).toEqual({ state: "Account changing sharing" });
    expect(handOff({ sharing }).description).toBe("Unavailable while this session’s account is having its sharing changed");
    expect(handOff({}).reason).toBeUndefined();
    expect(handOff({ sharing: new Map() }).reason).toBeUndefined();
  });

  it("disables the other accounts, Hand off and Unbind while an account switch holds the session", () => {
    const held = project({ account_id: "w", config_dir: "/home/w", bound_to: "hub", switching_account: true });
    const menu = sessionMenu(t, {} as SidebarHandlers, held, [account("w", "claude")], undefined, {});
    const entry = menu.find((candidate) => typeof candidate === "object" && "items" in candidate) as ActionMenuSubmenu;
    const [dflt, current] = entry.items as { reason?: { state: string }; selected: boolean }[];
    expect(current.reason).toBeUndefined();
    expect(dflt.reason).toEqual({ state: "Switching account" });
    const item = (label: string) => menu.find((candidate) => typeof candidate === "object" && "label" in candidate && candidate.label === label) as { disabled?: boolean; reason?: { state: string } };
    expect(item("Hand off").reason).toEqual({ state: "Switching account" });
    expect(item("Unbind").disabled).toBe(true);
  });

  it("states what remains of an account's quota as its description, and has no reason for it", () => {
    const quota = {
      agent: "claude" as const, account: "w", fetched_at: 0, plan: null, unavailable: null,
      windows: [{ kind: "session" as const, label: null, used_percent: 40, resets_at: 7_200_000, window_minutes: 300 }],
    };
    const entry = sessionMenu(t, {} as SidebarHandlers, project(), [account("w", "claude")], undefined, { quota: { quotas: [quota], now: 0 } })
      .find((candidate) => typeof candidate === "object" && "items" in candidate) as ActionMenuSubmenu;
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
  const item = (session: Session, handlers = {} as SidebarHandlers) =>
    sessionMenu(t, handlers, session, [], undefined).find((entry) => typeof entry === "object" && "onClick" in entry && entry.label === "Hand off") as { disabled?: boolean; onClick: () => void } | undefined;

  it.each([
    ["idle", false, true],
    ["waiting_user", false, true],
    ["interrupted", false, true],
    ["working", false, false],
    ["starting", false, false],
    ["idle", true, false],
  ] as const)("on a %s session (locked: %s) is enabled: %s", (status, locked, enabled) => {
    expect(item(sessionOf("s", "c", "p", "Title", status, { locked }))?.disabled ?? false).toBe(!enabled);
  });

  it("is offered for console sessions and not for an archived one, and opens the hand-off dialog", () => {
    const hub = sessionOf("h", "c", undefined, "Hub", "idle");
    expect(item(sessionOf("h", "c", undefined, "Hub", "archived"))).toBeUndefined();
    const handlers = { onOpenDialog: vi.fn() } as unknown as SidebarHandlers;
    item(hub, handlers)!.onClick();
    expect(handlers.onOpenDialog).toHaveBeenCalledWith({ kind: "hand-off", session: hub });
  });
});

describe("a session that is still starting", () => {
  it("offers Archive, never Delete, even when it was never used", () => {
    const starting = sessionOf("s", "c", "p", "Title", "starting");
    const labels = sessionMenu(t, {} as SidebarHandlers, starting, [], undefined).map((entry) => (entry === "separator" ? entry : entry.label));
    expect(labels).toContain("Archive");
    expect(labels).not.toContain("Delete");
  });
});

describe("the Colour entry", () => {
  const hub = sessionOf("h", "c", undefined, "Hub", "idle", { colour: "cyan" });
  const entry = (session: Session, handlers = {} as SidebarHandlers) => sessionMenu(t, handlers, session, [], undefined).find((candidate) => typeof candidate === "object" && candidate.label === "Colour");

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
    sessionMenu(t, {} as SidebarHandlers, session, [], undefined, { placement }).flatMap((entry) => (typeof entry === "object" ? [entry.label] : []));

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
    const entry = sessionMenu(t, handlers, hub, [], undefined).find((candidate) => typeof candidate === "object" && candidate.label === "Focus mode");
    (entry as { onClick: () => void }).onClick();
    expect(handlers.onFocus).toHaveBeenCalledWith({ consoleSession: hub });
  });
});

describe("the Archive and Delete entries", () => {
  const entry = (session: Session, hasBoundSessions = false) =>
    sessionMenu(t, {} as SidebarHandlers, session, [], undefined, { hasBoundSessions }).flatMap((item) => (typeof item === "object" && ["Archive", "Delete"].includes(item.label) ? [item.label] : []));
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

  it("puts Pin first, then groups viewing in the app, outside tools, managing the project, and removing it", () => {
    const handlers = {} as unknown as SidebarHandlers;
    const menu = projectMenu(t, handlers, { id: "p", pinned: false } as Project, [], { gitRepository: true });
    expect(menu.map((entry) => (entry === "separator" ? entry : entry.label))).toEqual([
      "Pin", "Focus mode", "Browse files", "View archive", "separator",
      "Open in", "Git", "separator",
      "Rename", "Copy project name", "Project settings", "separator",
      "Remove project",
    ]);
  });

  it("groups outside tools for every project", () => {
    const menu = projectMenu(t, {} as SidebarHandlers, { id: "p" } as Project, []);
    expect(openIn(menu).items.map((entry) => typeof entry === "object" ? entry.label : entry)).toEqual(["File manager", "Terminal"]);
    expect(labels("list")).not.toContain("Terminal");
  });

  it("opens the branded repository through the Open in submenu", () => {
    const handlers = { onOpenRepository: vi.fn() } as unknown as SidebarHandlers;
    const project = { id: "p", remote_url: "git@github.com:acme/app.git" } as Project;
    const entry = openIn(projectMenu(t, handlers, project, [])).items[2];
    expect(entry).toMatchObject({ label: "GitHub", icon: { props: { brand: "github" } } });
    (entry as { onClick: () => void }).onClick();
    expect(handlers.onOpenRepository).toHaveBeenCalledExactlyOnceWith("https://github.com/acme/app");
    expect(openIn(projectMenu(t, {} as SidebarHandlers, project, [])).items).toHaveLength(2);
    expect(openIn(projectMenu(t, handlers, { ...project, remote_url: "/tmp/repo" }, [])).items).toHaveLength(2);
  });

  it("offers Browse files only while this project's browser is not visible", () => {
    for (const visibleBrowserProjectId of [undefined, "other", "p"]) {
      const handlers = { visibleBrowserProjectId } as SidebarHandlers;
      const menu = projectMenu(t, handlers, { id: "p" } as Project, []);
      const menuLabels = menu.flatMap((entry) => typeof entry === "object" ? [entry.label] : []);
      expect(menuLabels.includes("Browse files")).toBe(visibleBrowserProjectId !== "p");
      expect(menuLabels).toContain("Open in");
    }
  });

  it("opens this project's directory in the file manager", () => {
    const project = { id: "p" } as Project;
    const handlers = { onOpenProjectDirectory: vi.fn(), visibleBrowserProjectId: "p" } as unknown as SidebarHandlers;
    const menu = projectMenu(t, handlers, project, []);
    const entry = openIn(menu).items.find((item) => typeof item === "object" && item.label === "File manager");
    (entry as { onClick: () => void }).onClick();
    expect(handlers.onOpenProjectDirectory).toHaveBeenCalledExactlyOnceWith(project);
  });

  it("puts pull and push under Git only for a git repository", () => {
    const git = (repository: boolean) => {
      const entry = projectMenu(t, {} as SidebarHandlers, { id: "p" } as Project, [], { gitRepository: repository }).find(
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
    const menu = projectMenu(t, handlers, project, [], { gitRepository: true });
    const terminal = openIn(menu).items.find((entry) => typeof entry === "object" && entry.label === "Terminal");
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
      sessionMenu(t, {} as SidebarHandlers, hub, [], undefined, { placement }).some((entry) => typeof entry === "object" && (entry.label === "Pin" || entry.label === "Unpin"));
    expect([pin("list"), pin("focused")]).toEqual([true, false]);
  });
});

describe("a console's menu", () => {
  it("lists Add project, Console settings, the archived console sessions and Delete console", () => {
    const handlers = { onOpenDialog: vi.fn(), onOpenArchive: vi.fn(), onSelectSession: vi.fn() };
    const hub = sessionOf("hub", "c", undefined, "Old hub", "archived", { role: "console" });
    const menu = consoleMenu(t, consoleOf("c", "Main"), handlers, [hub]);
    const labels = menu.map((entry) => (entry === "separator" ? entry : entry.label));
    expect(labels).toEqual(["Add project", "Console settings", "View archive", "separator", "Delete console"]);
    const archive = menu[2] as { items: { label: string; onClick: () => void }[] };
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

  it("shows each archived session's context size at the item's end, without a tooltip", () => {
    const sized = { ...roots[0], context: { tokens: 12_345 } };
    const [item] = archiveSubmenu(t, "Archive", [sized], vi.fn(), vi.fn()).items as ActionMenuItem[];
    expect(item.end).toMatchObject({ type: SessionContextLabel, props: { context: sized.context, plain: true } });
  });

  it("leaves View archive out of a project's own focus mode, which lists its archive below", () => {
    const menu = projectMenu(t, {} as SidebarHandlers, { id: "p" } as Project, roots, { placement: "focused" });
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

it("asks for confirmation before restoring project changes", () => {
  const project = { id: "p" } as Project;
  const handlers = { onOpenDialog: vi.fn() } as unknown as SidebarHandlers;
  const git = projectMenu(t, handlers, project, [], { gitRepository: true }).find((entry) => typeof entry === "object" && "items" in entry && entry.label === "Git");
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
  ] as const)("copies a %s session while %s in the %s menu", (role, status, placement) => {
    const session = sessionOf("session-id", "c", role === "project" ? "p" : undefined, "Title", status);
    const handlers = { onCopySessionId: vi.fn() } as unknown as SidebarHandlers;
    const menu = sessionMenu(t, handlers, session, [], undefined, { placement });
    const labels = menu.map((entry) => entry === "separator" ? entry : entry.label);
    if (status !== "archived") expect(labels.indexOf("Copy session ID")).toBe(labels.indexOf("Rename") + 1);
    const entry = menu.find((item) => typeof item === "object" && item.label === "Copy session ID");
    (entry as { onClick: () => void }).onClick();
    expect(handlers.onCopySessionId).toHaveBeenCalledExactlyOnceWith(session);
  });
});

it("keeps a locked live session protected and offers unlocking", () => {
  const session = sessionOf("locked", "c", "p", "Protected", "idle", { locked: true });
  const handlers = { onSetLocked: vi.fn() } as unknown as SidebarHandlers;
  const items = sessionMenu(t, handlers, session, [], undefined);
  const unlock = items.find((item) => typeof item !== "string" && item.label === "Unlock");
  const removal = items.find((item) => typeof item !== "string" && ["Archive", "Delete"].includes(item.label));
  expect(removal).toMatchObject({ disabled: true });
  expect(unlock).toBeDefined();
  (unlock as { onClick: () => void }).onClick();
  expect(handlers.onSetLocked).toHaveBeenCalledWith(session, false);
});

it("offers unbinding only on a bound session that is not archived", () => {
  const handlers = { onReleaseSession: vi.fn() } as unknown as SidebarHandlers;
  const unbind = (session: Session) => sessionMenu(t, handlers, session, [], undefined).find((item) => typeof item === "object" && item.label === "Unbind");
  expect(unbind(sessionOf("free", "c", "p", "Free", "idle"))).toBeUndefined();
  const bound = sessionOf("bound", "c", "p", "Bound", "idle", { bound_to: "hub" });
  (unbind(bound) as { onClick: () => void }).onClick();
  expect(handlers.onReleaseSession).toHaveBeenCalledExactlyOnceWith(bound);
  expect(unbind(sessionOf("new", "c", "p", "New", "starting", { bound_to: "hub" }))).toMatchObject({ disabled: true });
  expect(unbind(sessionOf("gone", "c", "p", "Gone", "archived", { bound_to: "hub" }))).toBeUndefined();
});

it.each([false, true])("offers no lock controls on archived sessions, locked=%s", (locked) => {
  const session = sessionOf("archived", "c", "p", "Archived", "archived", { locked });
  const labels = sessionMenu(t, {} as SidebarHandlers, session, [], undefined).flatMap((item) => typeof item === "object" ? [item.label] : []);
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
