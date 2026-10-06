import { Archive, Focus, List, Pencil, Pin, PinOff, Settings2, Trash2 } from "lucide-react";

import type { ActionMenuEntry, ActionMenuItem, ActionMenuSubmenu } from "../components/ActionMenu";
import { AgentIcon } from "../components/AgentIcon";
import type { Translate } from "../i18n/catalog";
import type { Project, Session } from "../protocol";
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

/** The project's actions, in the project row and in focus mode's header (which leaves out "Focus
 * mode", being in it). */
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
    ...(inFocus ? [] : [{ label: t("sidebar.project.focus"), icon: Focus, end: <FocusShortcutKbd />, onClick: () => handlers.onFocusProject(project.id) }]),
    archiveSubmenu(t, t("sidebar.project.archive"), archived, handlers.onSelectSession, () =>
      handlers.onOpenArchive({ console: project.console_id, project: project.id }),
    ),
    "separator",
    { label: t("sidebar.project.remove"), icon: Trash2, onClick: () => handlers.onOpenDialog({ kind: "delete-project", project }), destructive: true },
  ];
}

/** A session's actions: pin, rename and archive; an archived session (listed in focus mode) offers
 * deleting instead. Resuming or reopening is no item: selecting an interrupted session resumes it,
 * and an archived one is reopened by typing to it or from the archive view. */
export function sessionMenu(t: Translate, handlers: SidebarHandlers, session: Session): ActionMenuEntry[] {
  const archived = session.status === "archived";
  return [
    ...(archived ? [] : [pinItem(t, session.pinned, () => handlers.onSetPinned({ session }, !session.pinned))]),
    { label: t("sidebar.session.rename"), icon: Pencil, onClick: () => handlers.onOpenDialog({ kind: "rename-session", session }) },
    "separator",
    archived
      ? { label: t("sidebar.session.delete"), icon: Trash2, onClick: () => handlers.onOpenDialog({ kind: "delete-session", session }), destructive: true }
      : { label: t("sidebar.session.archive"), icon: Archive, onClick: () => handlers.onOpenDialog({ kind: "archive-session", session }) },
  ];
}
