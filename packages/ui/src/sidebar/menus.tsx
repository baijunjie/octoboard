import { Archive, ArrowLeftRight, Copy, Download, Focus, Forward, Folder, FolderOpen, FolderPlus, GitBranch, KeyRound, List, Lock, LockOpen, Palette, Pencil, Pin, PinOff, RotateCcw, Settings2, Terminal, Trash2, Undo2, Unlink, Upload, SquareArrowOutUpRight } from "lucide-react";

import { handoffBlock, switchEntries, switchEntryBlock, type QuotaReadings } from "../accountChoices";
import type { ActionMenuEntry, ActionMenuItem, ActionMenuPanel, ActionMenuSubmenu } from "../components/ActionMenu";
import { ExternalLinkIcon } from "../components/ExternalLinkIcon";
import { RepositoryIcon } from "../components/RepositoryIcon";
import { repositoryLink } from "../repositoryLink";
import { FadeOverflow } from "../components/FadeOverflow";
import { SessionRelativeTime } from "../components/SessionMetadata";
import { AgentIcon } from "../components/AgentIcon";
import { PathText } from "../components/PathText";
import type { Translate } from "../i18n/catalog";
import type { Account, AccountSharing, Agent, Console, Project, Session, Settings } from "../protocol";
import { accountUsage } from "../quota/accountUsage";
import { sessionChoiceTitle, sessionDisplayTitle } from "../sessionLabel";
import { ShortcutKbd } from "../shortcuts/ShortcutKbd";
import { archivePreviewSessions, isUnusedSession } from "./order";
import { SessionColourSwatches } from "./SessionColourSwatches";
import type { ArchiveScope, SidebarHandlers } from "./types";

/** "View archive": the archive's preview (`archivePreviewSessions`), each item selecting its session
 * and showing its archive time, then "View all", which opens the full archive. */
export function archiveSubmenu(
  t: Translate,
  label: string,
  archived: Session[],
  onSelect: (session: Session) => void,
  onViewAll: () => void,
): ActionMenuSubmenu {
  const items: ActionMenuItem[] = archivePreviewSessions(archived).map((session) => ({
    label: sessionChoiceTitle(t, session),
    content: <FadeOverflow as="span" dir="auto" className="block" titleWhenClipped={sessionDisplayTitle(t, session)}>{sessionDisplayTitle(t, session)}</FadeOverflow>,
    icon: <AgentIcon agent={session.agent} />,
    // A press on the item selects the session, so a tap opens no popover of the dates.
    end: session.ended_at != null ? <SessionRelativeTime session={session} touchPopover={false} /> : undefined,
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
    { label: t("sidebar.console.edit"), icon: Settings2, onClick: () => handlers.onOpenDialog({ kind: "edit-console", console }) },
    archiveSubmenu(t, t("sidebar.archive.consoleSessions"), archived, handlers.onSelectSession, () =>
      handlers.onOpenArchive({ console: console.id }),
    ),
    "separator",
    { label: t("sidebar.console.delete"), icon: Trash2, onClick: () => handlers.onOpenDialog({ kind: "delete-console", console }), destructive: true },
  ];
}

/** Where a menu is shown, which decides what it leaves out: `list` is the item's row in the
 * sidebar's lists, with everything; `focused` is the item's own focus mode: the project's header,
 * the console session's own card. It has no "Focus mode" to enter and no Pin / Unpin (the item in
 * focus is not pinned from there), and the project's header no "View archive", which its focus mode
 * lists below; `nested` is a project's heading inside a console session's focus mode, which has no
 * project focus mode to enter from there. */
export type MenuPlacement = "list" | "focused" | "nested";

/** The project's actions, in the project row and in a focus mode (the project's own header, and a
 * console session's project headings); `placement` says which. Pin / Unpin comes first, as in a
 * session's menu; then the actions are grouped as viewing the project inside the app (focus mode,
 * files, archive), opening it in outside tools, managing the project itself, and removing it.
 * "View archive" previews `archived` and its "View all" opens `archiveScope`. By default both are the
 * project's archive; a console session's focus mode passes the project's share of that console
 * session's archive and opens the whole of it.
 * Git is a submenu, offered only while the project is known to be a git repository
 * (`gitRepository`, from its git status): pull the current branch, push the current branch, switch
 * to a local branch through its own submenu, and restore tracked file changes after confirmation. */
export function projectMenu(
  t: Translate,
  handlers: SidebarHandlers,
  project: Project,
  archived: Session[],
  {
    placement = "list",
    gitRepository = false,
    archiveScope = { console: project.console_id, project: project.id },
  }: { placement?: MenuPlacement; gitRepository?: boolean; archiveScope?: ArchiveScope } = {},
): ActionMenuEntry[] {
  const repository = repositoryLink(project.remote_url);
  return [
    ...(placement === "focused" ? [] : [pinItem(t, project.pinned, () => handlers.onSetPinned({ project }, !project.pinned))]),
    ...(placement === "list" ? [{ label: t("sidebar.focus.enter"), icon: Focus, end: <ShortcutKbd command="focusMode" />, onClick: () => handlers.onFocus({ project }) }] : []),
    ...(handlers.visibleBrowserProjectId === project.id
      ? []
      : [{ label: t("sidebar.project.browse"), icon: FolderOpen, onClick: () => handlers.onBrowseProject(project) }]),
    ...(placement === "focused"
      ? []
      : [archiveSubmenu(t, t("sidebar.project.archive"), archived, handlers.onSelectSession, () => handlers.onOpenArchive(archiveScope))]),
    "separator",
    {
      label: t("common.openIn"), icon: SquareArrowOutUpRight,
      items: [
        { label: t("common.fileManager"), icon: Folder, onClick: () => handlers.onOpenProjectDirectory(project) },
        { label: t("common.terminal"), icon: Terminal, onClick: () => handlers.onOpenProjectTerminal(project) },
        ...(repository && handlers.onOpenRepository ? [{
          label: repository.host, icon: <RepositoryIcon brand={repository.brand} />, end: <ExternalLinkIcon />,
          onClick: () => handlers.onOpenRepository!(repository.url),
        }] : []),
      ],
    },
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
    { label: t("sidebar.project.copyName"), icon: Copy, onClick: () => handlers.onCopyProjectName(project) },
    { label: t("sidebar.project.edit"), icon: Settings2, onClick: () => handlers.onOpenDialog({ kind: "edit-project", project }) },
    "separator",
    { label: t("sidebar.project.remove"), icon: Trash2, onClick: () => handlers.onOpenDialog({ kind: "delete-project", project }), destructive: true },
  ];
}

/** "Switch account": the session's agent's accounts, the one the session is on marked as current
 * and doing nothing. An account whose quota is used up or whose sharing change runs is unavailable, with why under
 * its name; so is every account while the session's own account is changing, and every account but the current one
 * while an account switch holds the session. A turned-off account is not listed, but the session's own stays. Absent
 * when the agent has no other account to go to that is turned on — hidden rather than disabled, as there is nothing
 * the user could do with it. */
function switchAccountSubmenu(
  t: Translate,
  handlers: SidebarHandlers,
  session: Session,
  accounts: Account[],
  defaultNames: Settings["default_account_names"],
  readings: QuotaReadings | undefined,
  sharing: Map<string, AccountSharing> | undefined,
  disabledDefaults: Agent[] | undefined,
): ActionMenuSubmenu | undefined {
  const entries = switchEntries(session, accounts, t, defaultNames, readings, sharing, disabledDefaults)
    .filter((entry) => entry.current || !entry.disabled);
  if (entries.length < 2) return undefined;
  const ownChanging = entries.some((entry) => entry.current && entry.changingSharing);
  const switching = session.switching_account === true;
  return {
    label: t("sidebar.session.switchAccount"),
    icon: ArrowLeftRight,
    items: entries.map(({ account, name, current, isPath, exhaustedBy, quota, changingSharing }) => {
      // A usage bar as the icon, the remainder said to assistive technology, and for one
      // used up the short reason under its name. The current account is never offered, so it is
      // not marked used up; a removed account's directory has no quota.
      const usage = accountUsage(t, session.agent, quota, current ? undefined : exhaustedBy, readings?.now ?? 0);
      // Nothing can be switched onto an account whose sharing change runs, nor away from one, whatever its quota.
      const unavailable = switchEntryBlock(t, { current, changingSharing }, { switching, ownChanging });
      const { icon, detail, reason } = unavailable ? { icon: usage.icon, ...unavailable } : usage;
      return {
        label: name,
        // A directory is a path, so it reads left to right and shows its end, as it does elsewhere.
        // `block` gives the span the label's width to clip against; inline, it never overflows.
        ...(isPath ? { content: <PathText path={name} as="span" className="block" />, ariaLabel: name } : {}),
        // The key sits in the bar's width, so that the labels line up.
        icon: isPath ? <span aria-hidden="true" className="flex w-8 shrink-0 justify-center"><KeyRound className="size-4 text-app-muted" /></span> : icon,
        reason,
        description: detail,
        selected: current,
        onClick: current
          ? () => {}
          : () => handlers.onOpenDialog({ kind: "switch-account", session, account, accountName: name }),
      };
    }),
  };
}

/** "Hand off": offered for a session that is not archived. Disabled while the session is in the
 * middle of a turn or still starting, which the daemon refuses (`session_working`), while it
 * is locked, as Archive is, and while no other agent could take it over (`hasTarget`); and, with the reason
 * shown, while the session's own account is changing its sharing (`ownChanging`), which the daemon refuses, and
 * while an account switch holds the session (see `handoffBlock`). */
function handOffItem(t: Translate, handlers: SidebarHandlers, session: Session, hasTarget: boolean, ownChanging: boolean): ActionMenuItem {
  const block = handoffBlock(t, session, ownChanging);
  return {
    ...(block?.reason ? { description: block.detail, reason: block.reason } : {}),
    label: t("sidebar.session.handOff"),
    icon: Forward,
    iconClassName: "rtl:-scale-x-100",
    onClick: () => handlers.onOpenDialog({ kind: "hand-off", session }),
    disabled: !hasTarget || block !== undefined,
  };
}

/** Whether a session's menu offers Delete in place of Archive: it is archived already, or it has
 * never been used (see `isUnusedSession`) and is deleted outright. One still starting keeps
 * Archive: the daemon refuses to delete it, and archiving it works (`isUnusedSession` rejects it). */
export function offersDelete(session: Session, hasBoundSessions: boolean): boolean {
  return session.status === "archived" || isUnusedSession(session, hasBoundSessions);
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

/** A session's actions: pin, lock, rename, colour (a console session's), focus mode, switch account, hand off,
 * unbind (a bound session's, disabled while it is still starting, which the daemon refuses) and archive;
 * an archived session (listed in focus mode) and one never used offer deleting instead.
 * `hasBoundSessions` says whether any session is bound to this one, which makes it used. Resuming
 * an interrupted session happens on selection; an archived session offers Reopen and Delete. Shared by a
 * project session's row and a console session's row (`ConsoleSessionsSection` in `Sidebar.tsx`) —
 * this is also where the user switches a console session's account. Focus mode is
 * offered for a console session alone; its own card in its focus mode (`placement` `focused`)
 * leaves out Focus mode and Pin / Unpin. `canHandOff` is whether another agent could take a session over.
 * `quota` is what the Switch account entries are judged against, for the accounts used up, and `sharing` for the
 * accounts whose sharing change runs, and `disabledDefaults` the agents whose default account is turned off. */
export function sessionMenu(
  t: Translate,
  handlers: SidebarHandlers,
  session: Session,
  accounts: Account[],
  defaultNames: Settings["default_account_names"],
  { placement = "list", hasBoundSessions = false, canHandOff = true, quota, sharing, disabledDefaults }: {
    placement?: MenuPlacement;
    hasBoundSessions?: boolean;
    canHandOff?: boolean;
    quota?: QuotaReadings;
    sharing?: Map<string, AccountSharing>;
    disabledDefaults?: Agent[];
  } = {},
): ActionMenuEntry[] {
  const focused = placement === "focused";
  const archived = session.status === "archived";
  // The daemon refuses to switch the account of a session that is still starting.
  const switchAccount = archived || session.status === "starting" ? undefined : switchAccountSubmenu(t, handlers, session, accounts, defaultNames, quota, sharing, disabledDefaults);
  const focusable = session.role === "console" && !archived && !focused;
  return [
    ...(!archived ? [{ label: t(session.locked ? "sidebar.session.unlock" : "sidebar.session.lock"), icon: session.locked ? LockOpen : Lock, end: <ShortcutKbd command="lock" />, onClick: () => handlers.onSetLocked(session, !session.locked) }] : []),
    ...(archived ? [{ label: t("sidebar.session.reopen"), icon: RotateCcw, onClick: () => handlers.onReopenSession(session) }] : []),
    ...(archived || focused ? [] : [{ ...pinItem(t, session.pinned, () => handlers.onSetPinned({ session }, !session.pinned)), end: <ShortcutKbd command="pin" /> }]),
    ...(focusable ? [{ label: t("sidebar.focus.enter"), icon: Focus, end: <ShortcutKbd command="focusMode" />, onClick: () => handlers.onFocus({ consoleSession: session }) }] : []),
    ...(!archived ? ["separator" as const] : []),
    ...(!archived ? [{ label: t("sidebar.session.rename"), icon: Pencil, onClick: () => handlers.onOpenDialog({ kind: "rename-session", session }) }] : []),
    {
      label: t("sidebar.session.copyId"),
      ariaLabel: t("sidebar.session.copyId"),
      content: (
        <span className="flex min-w-0 max-w-full flex-col">
          <span>{t("sidebar.session.copyId")}</span>
          {/* HeroUI's label ignores pointer events; the ID needs its own hover target. */}
          <span aria-hidden="true" data-marquee-scope className="pointer-events-auto min-w-0">
            <FadeOverflow as="span" dir="ltr" className="block font-mono text-xs text-app-muted">{session.id}</FadeOverflow>
          </span>
        </span>
      ),
      description: session.id,
      icon: Copy,
      end: <ShortcutKbd command="copyId" />,
      onClick: () => handlers.onCopySessionId(session),
    },
    ...(session.role === "console" && !archived ? [colourSubmenu(t, handlers, session)] : []),
    ...(!archived ? ["separator" as const] : []),
    ...(switchAccount ? [switchAccount] : []),
    ...(archived ? [] : [handOffItem(t, handlers, session, canHandOff, sharing?.get(session.account_id ?? "")?.progress !== undefined)]),
    ...(session.bound_to && !archived ? [{ label: t("sidebar.session.unbind"), icon: Unlink, disabled: session.status === "starting" || session.switching_account === true, onClick: () => handlers.onOpenDialog({ kind: "unbind-session", session }) }] : []),
    "separator",
    offersDelete(session, hasBoundSessions)
      ? { label: t("sidebar.session.delete"), icon: Trash2, onClick: () => handlers.onOpenDialog({ kind: "delete-session", session }), destructive: true, disabled: session.locked }
      : { label: t("sidebar.session.archive"), icon: Archive, end: <ShortcutKbd command="archive" />, onClick: () => handlers.onArchiveSession(session), disabled: session.locked },
  ];
}
