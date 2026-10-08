import { Archive, ArrowLeftRight, Focus, KeyRound, List, Pencil, Pin, PinOff, Settings2, Trash2 } from "lucide-react";

import { switchEntries } from "../accountChoices";
import type { ActionMenuEntry, ActionMenuItem, ActionMenuSubmenu } from "../components/ActionMenu";
import { AgentIcon } from "../components/AgentIcon";
import { PathText } from "../components/PathText";
import type { Translate } from "../i18n/catalog";
import type { Account, Project, Session } from "../protocol";
import { FocusShortcutKbd } from "./focusShortcut";
import type { SidebarHandlers } from "./types";

/** How many archived sessions a "View archive" submenu lists before "View all". */
const ARCHIVE_SUBMENU_COUNT = 5;

/** "View archive": the newest `ARCHIVE_SUBMENU_COUNT` archived sessions, each reopening its
 * session, then "View all", which opens the archive view. `archived` is newest first. */
export function archiveSubmenu(
  t: Translate,
  label: string,
  archived: Session[],
  onSelect: (session: Session) => void,
  onViewAll: () => void,
): ActionMenuSubmenu {
  const items: ActionMenuItem[] = archived.slice(0, ARCHIVE_SUBMENU_COUNT).map((session) => ({
    label: session.title,
    icon: <AgentIcon agent={session.agent} />,
    onClick: () => onSelect(session),
  }));
  if (items.length === 0) {
    items.push({ label: t("sidebar.archive.empty"), icon: Archive, onClick: () => {}, disabled: true });
  }
  return {
    label,
    icon: Archive,
    items: [...items, ...(archived.length > 0 ? [{ label: t("sidebar.archive.viewAll", { count: archived.length }), icon: List, onClick: onViewAll }] : [])],
  };
}

function pinItem(t: Translate, pinned: boolean, onToggle: () => void): ActionMenuItem {
  return pinned
    ? { label: t("sidebar.unpin"), icon: PinOff, onClick: onToggle }
    : { label: t("sidebar.pin"), icon: Pin, onClick: onToggle };
}

/** The project's actions, in the project row and in a focus mode (the project's own header, and a
 * console session's project headings), which leaves out "Focus mode": the project's own is in it
 * already, and a console session's has no project focus mode to enter from there. */
export function projectMenu(
  t: Translate,
  handlers: SidebarHandlers,
  project: Project,
  archived: Session[],
  { inFocus = false }: { inFocus?: boolean } = {},
): ActionMenuEntry[] {
  return [
    pinItem(t, project.pinned, () => handlers.onSetPinned({ project }, !project.pinned)),
    { label: t("sidebar.project.rename"), icon: Pencil, onClick: () => handlers.onOpenDialog({ kind: "rename-project", project }) },
    { label: t("sidebar.project.edit"), icon: Settings2, onClick: () => handlers.onOpenDialog({ kind: "edit-project", project }) },
    ...(inFocus ? [] : [{ label: t("sidebar.focus.enter"), icon: Focus, end: <FocusShortcutKbd />, onClick: () => handlers.onFocus({ project }) }]),
    archiveSubmenu(t, t("sidebar.project.archive"), archived, handlers.onSelectSession, () =>
      handlers.onOpenArchive({ console: project.console_id, project: project.id }),
    ),
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

/** A session's actions: pin, rename, focus mode, switch account and archive; an archived session
 * (listed in focus mode) offers deleting instead. Resuming or reopening is no item: selecting an
 * interrupted session resumes it, and an archived one is reopened by typing to it or from the
 * archive view. Shared by a project session's row and a console session's row
 * (`ConsoleSessionsSection` in `Sidebar.tsx`) — a console session cannot archive itself (see "The
 * console session's tools" in `docs/product/hub-orchestration.md`), so this is the only way to
 * archive one, and the only place to switch its account. Focus mode is offered for a console
 * session alone, and not from its own focus mode's header (`inFocus`). */
export function sessionMenu(
  t: Translate,
  handlers: SidebarHandlers,
  session: Session,
  accounts: Account[],
  { inFocus = false }: { inFocus?: boolean } = {},
): ActionMenuEntry[] {
  const archived = session.status === "archived";
  const switchAccount = archived ? undefined : switchAccountSubmenu(t, handlers, session, accounts);
  const focusable = session.role === "console" && !archived && !inFocus;
  return [
    ...(archived ? [] : [pinItem(t, session.pinned, () => handlers.onSetPinned({ session }, !session.pinned))]),
    { label: t("sidebar.session.rename"), icon: Pencil, onClick: () => handlers.onOpenDialog({ kind: "rename-session", session }) },
    ...(focusable ? [{ label: t("sidebar.focus.enter"), icon: Focus, end: <FocusShortcutKbd />, onClick: () => handlers.onFocus({ consoleSession: session }) }] : []),
    ...(switchAccount ? [switchAccount] : []),
    "separator",
    archived
      ? { label: t("sidebar.session.delete"), icon: Trash2, onClick: () => handlers.onOpenDialog({ kind: "delete-session", session }), destructive: true }
      : { label: t("sidebar.session.archive"), icon: Archive, onClick: () => handlers.onOpenDialog({ kind: "archive-session", session }) },
  ];
}
