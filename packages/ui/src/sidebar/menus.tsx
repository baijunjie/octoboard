import { Archive, ArrowLeftRight, Copy, Download, Focus, Folder, FolderOpen, FolderPlus, GitBranch, KeyRound, List, Palette, Pencil, Pin, PinOff, RotateCcw, Settings2, Terminal, Trash2, Undo2, Upload } from "lucide-react";

import { switchEntries } from "../accountChoices";
import type { ActionMenuEntry, ActionMenuItem, ActionMenuPanel, ActionMenuSubmenu } from "../components/ActionMenu";
import { AgentIcon } from "../components/AgentIcon";
import { PathText } from "../components/PathText";
import type { Translate } from "../i18n/catalog";
import type { Account, Console, Project, Session } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { FocusShortcutKbd } from "./focusShortcut";
import { archivePreviewSessions } from "./order";
import { SessionColourSwatches } from "./SessionColourSwatches";
import type { SidebarHandlers } from "./types";

/** "View archive": the five newest archived parent sessions, each selecting its session, then
 * "View all", which opens the full archive including children. `sessions` includes live owners. */
export function archiveSubmenu(
  t: Translate,
  label: string,
  archived: Session[],
  sessions: Session[],
  onSelect: (session: Session) => void,
  onViewAll: () => void,
): ActionMenuSubmenu {
  const items: ActionMenuItem[] = archivePreviewSessions(archived, sessions).map((session) => ({
    label: sessionDisplayTitle(t, session),
    icon: <AgentIcon agent={session.agent} />,
    onClick: () => onSelect(session),
  }));
  if (archived.length === 0) {
    items.push({ label: t("sidebar.archive.empty"), icon: Archive, onClick: () => {}, disabled: true });
  }
  return {
    label,
    icon: Archive,
    items: [...items, ...(archived.length > 0 ? [{ label: t("sidebar.archive.viewAll"), icon: List, iconClassName: "rtl:-scale-x-100", onClick: onViewAll }] : [])],
  };
}

function pinItem(t: Translate, pinned: boolean, onToggle: () => void): ActionMenuItem {
  return pinned
    ? { label: t("sidebar.unpin"), icon: PinOff, onClick: onToggle }
    : { label: t("sidebar.pin"), icon: Pin, onClick: onToggle };
}

/** What a console's menu acts through. */
export type ConsoleMenuHandlers = Pick<SidebarHandlers, "onOpenDialog" | "onSelectSession" | "onOpenArchive">;

/** A console's actions, in the sidebar's header and on the console's avatar in the rail: among
 * them the console's archived console sessions, which `archived` lists (the callers pass exactly
 * those, in no particular order). */
export function consoleMenu(
  t: Translate,
  console: Console,
  handlers: ConsoleMenuHandlers,
  archived: Session[],
): ActionMenuEntry[] {
  return [
    { label: t("sidebar.console.addProject"), icon: FolderPlus, onClick: () => handlers.onOpenDialog({ kind: "new-project", console }) },
    { label: t("sidebar.console.edit"), icon: Pencil, onClick: () => handlers.onOpenDialog({ kind: "edit-console", console }) },
    archiveSubmenu(t, t("sidebar.archive.consoleSessions"), archived, archived, handlers.onSelectSession, () =>
      handlers.onOpenArchive({ console: console.id }),
    ),
    "separator",
    { label: t("sidebar.console.delete"), icon: Trash2, onClick: () => handlers.onOpenDialog({ kind: "delete-console", console }), destructive: true },
  ];
}

/** Where a menu is shown, which decides what it leaves out: `list` is the item's row in the
 * sidebar's lists, with everything; `focused` is the header of the item's own focus mode, which
 * has no "Focus mode" to enter and no Pin / Unpin (the item in focus is not pinned from there);
 * `nested` is a project's heading inside a console session's focus mode, which has no project
 * focus mode to enter from there. */
export type MenuPlacement = "list" | "focused" | "nested";

/** The project's actions, in the project row and in a focus mode (the project's own header, and a
 * console session's project headings); `placement` says which. Pin / Unpin comes first, as in a
 * session's menu; then the actions are grouped as viewing the project inside the app (focus mode,
 * files, archive), opening it in outside tools, managing the project itself, and removing it.
 * Git is a submenu, offered only while the project is known to be a git repository
 * (`gitRepository`, from its git status): pull the current branch, push the current branch, switch
 * to a local branch through its own submenu, and restore tracked file changes after confirmation. */
export function projectMenu(
  t: Translate,
  handlers: SidebarHandlers,
  project: Project,
  archived: Session[],
  sessions: Session[],
  { placement = "list", gitRepository = false }: { placement?: MenuPlacement; gitRepository?: boolean } = {},
): ActionMenuEntry[] {
  return [
    ...(placement === "focused" ? [] : [pinItem(t, project.pinned, () => handlers.onSetPinned({ project }, !project.pinned))]),
    ...(placement === "list" ? [{ label: t("sidebar.focus.enter"), icon: Focus, end: <FocusShortcutKbd />, onClick: () => handlers.onFocus({ project }) }] : []),
    ...(handlers.visibleBrowserProjectId === project.id
      ? []
      : [{ label: t("sidebar.project.browse"), icon: FolderOpen, onClick: () => handlers.onBrowseProject(project) }]),
    archiveSubmenu(t, t("sidebar.project.archive"), archived, sessions, handlers.onSelectSession, () =>
      handlers.onOpenArchive({ console: project.console_id, project: project.id }),
    ),
    "separator",
    { label: t("sidebar.project.openDirectory"), icon: Folder, onClick: () => handlers.onOpenProjectDirectory(project) },
    { label: t("sidebar.project.openTerminal"), icon: Terminal, onClick: () => handlers.onOpenProjectTerminal(project) },
    ...(gitRepository
      ? [
        {
          label: t("sidebar.project.git"),
          icon: GitBranch,
          items: [
            { label: t("sidebar.project.pullBranch"), icon: Download, onClick: () => handlers.onPullProjectBranch(project) },
            { label: t("sidebar.project.pushBranch"), icon: Upload, onClick: () => handlers.onPushProjectBranch(project) },
            { label: t("sidebar.project.switchBranch"), icon: ArrowLeftRight, items: [], loadItems: () => handlers.onLoadProjectBranches(project) },
            "separator" as const,
            { label: t("sidebar.project.restoreChanges"), icon: Undo2, onClick: () => handlers.onOpenDialog({ kind: "restore-project-changes", project }), destructive: true },
          ],
        },
      ]
      : []),
    "separator",
    { label: t("sidebar.project.rename"), icon: Pencil, onClick: () => handlers.onOpenDialog({ kind: "rename-project", project }) },
    ...(handlers.onCopyProjectName
      ? [{ label: t("sidebar.project.copyName"), icon: Copy, onClick: () => handlers.onCopyProjectName!(project) }]
      : []),
    { label: t("sidebar.project.edit"), icon: Settings2, onClick: () => handlers.onOpenDialog({ kind: "edit-project", project }) },
    "separator",
    { label: t("sidebar.project.remove"), icon: Trash2, onClick: () => handlers.onOpenDialog({ kind: "delete-project", project }), destructive: true },
  ];
}

/** "Switch account": the session's agent's accounts, the one the session is on marked as current
 * and doing nothing, and last a way into Settings' Agent accounts section. Absent when the agent
 * has no other account to go to — hidden rather than disabled, as there is nothing the user could
 * do with it. */
function switchAccountSubmenu(
  t: Translate,
  handlers: SidebarHandlers,
  session: Session,
  accounts: Account[],
): ActionMenuSubmenu | undefined {
  const entries = switchEntries(session, accounts, t);
  if (entries.length < 2) return undefined;
  return {
    label: t("sidebar.session.switchAccount"),
    icon: ArrowLeftRight,
    items: [
      ...entries.map(({ account, name, current, isPath }) => ({
        label: name,
        // A directory is a path, so it reads left to right and shows its end, as it does elsewhere.
        // `block` gives the span the label's width to clip against; inline, it never overflows.
        ...(isPath ? { content: <PathText path={name} as="span" className="block" />, ariaLabel: name } : {}),
        icon: KeyRound,
        selected: current,
        onClick: current
          ? () => {}
          : () => handlers.onOpenDialog({ kind: "switch-account", session, account, accountName: name }),
      })),
      "separator" as const,
      { label: t("sidebar.session.manageAccounts"), icon: Settings2, onClick: () => handlers.onOpenSettings("accounts") },
    ],
  };
}

/** Whether a session's menu offers Delete in place of Archive: it is archived already, or it has
 * never been used — no conversation on the agent's side (no turn taken, none resumed at open),
 * bound to no owner and no session bound to it — and is deleted outright. One bound to an owner
 * keeps Archive even before its first prompt lands, since it reports to that owner, and so does an
 * owner that already has a session under it. One still starting keeps Archive too: the daemon
 * refuses to delete it, and archiving it works. */
export function offersDelete(session: Session, hasBoundSessions: boolean): boolean {
  if (session.status === "starting") return false;
  return session.status === "archived" || (!session.has_conversation && !session.bound_to && !hasBoundSessions);
}

/** The palette of a console session's colour, as a panel beside the menu. Choosing a swatch applies
 * it at once and closes the menu. */
function colourSubmenu(t: Translate, handlers: SidebarHandlers, session: Session): ActionMenuPanel {
  return {
    label: t("sidebar.session.colour"),
    icon: Palette,
    panel: ({ close, ...focus }) => (
      <SessionColourSwatches
        colour={session.colour}
        onChoose={(colour) => {
          close();
          handlers.onSetSessionColour(session, colour);
        }}
        onClose={close}
        {...focus}
      />
    ),
  };
}

/** A session's actions: pin, rename, colour (a console session's), focus mode, switch account and
 * archive; an archived session (listed in focus mode) and one never used offer deleting instead.
 * `hasBoundSessions` says whether any session is bound to this one, which makes it used. Resuming
 * an interrupted session happens on selection; an archived session offers Reopen and Delete. Shared by a
 * project session's row and a console session's row (`ConsoleSessionsSection` in `Sidebar.tsx`) —
 * a console session cannot archive itself (see "The console session's tools" in `docs/product/hub-orchestration.md`), so
 * this is the only way to archive one, and the only place to switch its account. Focus mode is
 * offered for a console session alone; its own focus mode's header (`placement` `focused`) leaves
 * out Focus mode and Pin / Unpin. */
export function sessionMenu(
  t: Translate,
  handlers: SidebarHandlers,
  session: Session,
  accounts: Account[],
  { placement = "list", hasBoundSessions = false }: { placement?: MenuPlacement; hasBoundSessions?: boolean } = {},
): ActionMenuEntry[] {
  const focused = placement === "focused";
  const archived = session.status === "archived";
  // The daemon refuses to switch the account of a session that is still starting (see `offersDelete`).
  const switchAccount = archived || session.status === "starting" ? undefined : switchAccountSubmenu(t, handlers, session, accounts);
  const focusable = session.role === "console" && !archived && !focused;
  return [
    ...(archived ? [{ label: t("sidebar.session.reopen"), icon: RotateCcw, onClick: () => handlers.onReopenSession(session) }] : []),
    ...(archived || focused ? [] : [pinItem(t, session.pinned, () => handlers.onSetPinned({ session }, !session.pinned))]),
    ...(!archived ? [{ label: t("sidebar.session.rename"), icon: Pencil, onClick: () => handlers.onOpenDialog({ kind: "rename-session", session }) }] : []),
    ...(session.role === "console" && !archived ? [colourSubmenu(t, handlers, session)] : []),
    ...(focusable ? [{ label: t("sidebar.focus.enter"), icon: Focus, end: <FocusShortcutKbd />, onClick: () => handlers.onFocus({ consoleSession: session }) }] : []),
    ...(switchAccount ? [switchAccount] : []),
    "separator",
    offersDelete(session, hasBoundSessions)
      ? { label: t("sidebar.session.delete"), icon: Trash2, onClick: () => handlers.onOpenDialog({ kind: "delete-session", session }), destructive: true }
      : { label: t("sidebar.session.archive"), icon: Archive, onClick: () => handlers.onArchiveSession(session) },
  ];
}
