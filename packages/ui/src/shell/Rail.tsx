import { Hand, Plus, Settings } from "lucide-react";
import React, { memo, useRef } from "react";

import { joinPhrases } from "../i18n/joinPhrases";
import type { ShownStatusKind } from "../sessions/sessionStatus";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import { isRailPeekArea, PHONE_SLIDE_AWAY } from "../layout/paneOverlay";
import type { Console } from "../protocol";
import { statusLabel } from "../sessions/sessionLabel";
import { consoleMenu, type ConsoleMenuHandlers } from "../sidebar/menus";
import type { OpenSettings } from "../settings/settingsLauncher";
import { RailQuotaEntry } from "./RailQuotaEntry";
import { RailProcessesEntry } from "./RailProcessesEntry";
import { RailCountBadge } from "./RailCountBadge";
import type { ProcessJump } from "../process/processJump";
import { useArchivedOwnerSessions, useConsoleStatus, useConsoleStatusCounts } from "../store";
import { ActionMenu } from "../components/menus/ActionMenu";
import { AvatarStatusBadge } from "../components/status/TopStatusMarker";
import { CHROME_BUTTON_FILLS, CHROME_CURRENT_FILLS, ChromeButton } from "./ChromeButton";
import { ConsoleAvatar } from "./ConsoleAvatar";
import { NotificationsBell } from "./NotificationsBell";
import { useListMotion } from "../components/motion/useListMotion";
import { TitledControl } from "../components/hint/TitledControl";
import { FadedScroller } from "../components/scroll/scrollFade";
import { BadgeAnchor, Button } from "../primitives";

const TOOLTIP_STATUSES: readonly ShownStatusKind[] = ["waiting_user", "dormant", "paused", "done", "idle", "delegated", "background", "working", "starting", "switching", "interrupted"];

const NEW_CONSOLE_ID = "rail-new-console";

/** At the tablet boundary and below a rail that is `onDemand` leaves the row and lies over the open
 * drawer's card (`drawerClass` in `layout/paneOverlay.tsx`, whose `max-tablet:` padding leaves the
 * rail's width free for it): the same inset from the window's edge, the top bar and the bottom
 * chrome as the card, so the two are one card. The rail paints nothing of its own but the groove
 * on its end edge, which is the seam between it and the sidebar; the card's surface, edge, shadow and
 * corners are all the card's. It comes in and goes out with the card, the same 240ms, easing and
 * slide in from the start edge (`PHONE_SLIDE_AWAY`), `visibility` staying on until the exit has
 * finished and then keeping the away rail out of reach. `z-41` keeps it above the card (`z-40`),
 * which comes later in the document and would otherwise paint over it. */
const ON_DEMAND_FRAME =
  "max-tablet:fixed max-tablet:start-(--peek-gap) max-tablet:top-[calc(var(--top-chrome-height)+var(--peek-gap))] max-tablet:bottom-[calc(var(--bottom-chrome-height)+var(--peek-gap))] max-tablet:z-41 max-tablet:groove-e max-tablet:transition-[translate,opacity,visibility] max-tablet:duration-240 max-tablet:ease-[cubic-bezier(0.2,0,0,1)] max-tablet:motion-reduce:transition-none";
const ON_DEMAND_OPEN = "max-tablet:opacity-100";
const ON_DEMAND_AWAY = `max-tablet:opacity-0 max-tablet:invisible ${PHONE_SLIDE_AWAY}`;

/** Puts focus on the rail's New console button, by identity: the console tiles come before it. */
export function focusNewConsole(): void {
  document.getElementById(NEW_CONSOLE_ID)?.focus();
}

/** One console on the rail: its avatar, marked with what is going on in it, and the current one on a
 * tile of its own. The tooltip and accessible name count the console's sessions by shown status.
 * The decorative badge has no tooltip of its own. A right-click offers the console's own actions, from a menu whose trigger is not drawn: the
 * sidebar's header has the same menu on a visible button. */
const ConsoleTile = memo(function ConsoleTile({
  console: thisConsole,
  current,
  onSelect,
  onHoverChange,
  hoverOpensSidebar,
  handlers,
}: {
  console: Console;
  current: boolean;
  onSelect: () => void;
  /** A mouse pointer entered or left the avatar; touch and pen are ignored. */
  onHoverChange: (hovered: boolean) => void;
  /** Hovering the avatar floats the hidden sidebar in, so the tooltip then waits for keyboard
   * focus. The tooltip says more than that sidebar's header does (the activity, not only the
   * name), so a mouse user with the docked sidebar hidden never reads those words; the activity
   * is reachable there only from the session rows' own status glyphs. */
  hoverOpensSidebar: boolean;
  handlers: ConsoleMenuHandlers;
}): React.ReactElement {
  const t = useT();
  // Read here rather than handed down, so an event about a session renders only its console's tile.
  const status = useConsoleStatus(thisConsole.id);
  const counts = useConsoleStatusCounts(thisConsole.id);
  const language = useCurrentLanguage();
  const sessions = useArchivedOwnerSessions(thisConsole.id);
  const tileRef = useRef<HTMLDivElement>(null);
  const entries = TOOLTIP_STATUSES.filter((kind) => counts[kind]).map((kind) => ({
    kind,
    count: counts[kind]!,
    text: t("rail.statusCount", { status: statusLabel(t, kind), count: counts[kind]! }),
  }));
  const label = entries.length ? t("session.status.named", { name: isolate(thisConsole.name), status: joinPhrases(language, entries.map((entry) => entry.text)) }) : thisConsole.name;
  const tooltip = entries.length ? (
    <div className="flex min-w-0 flex-col gap-1.5">
      <div dir="auto" className="min-w-0 font-medium [overflow-wrap:anywhere]">{thisConsole.name}</div>
      <hr className="groove-t" />
      <dl className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-4 gap-y-0.5">
        {entries.map(({ kind, count }) => (
          <React.Fragment key={kind}>
            <dt className="min-w-0 text-app-muted [overflow-wrap:anywhere]">{statusLabel(t, kind)}</dt>
            <dd className="text-end tabular-nums">{count.toLocaleString(language)}</dd>
          </React.Fragment>
        ))}
      </dl>
    </div>
  ) : thisConsole.name;
  return (
    <div ref={tileRef} data-list-motion={thisConsole.id} className="flex px-1 pb-1">
      {/* The tile's box includes the badge's 4px overhang, keeping it inside the scroll viewport. */}
      <BadgeAnchor>
        <TitledControl title={tooltip} placement="end" focusOnly={hoverOpensSidebar}>
          <Button
            iconOnly
            size="sm"
            variant="ghost"
            label={label}
            // The markup tooltip repeats the name, which TitledControl only detects for plain text.
            describedBy=""
            current={current}
            // Not styling: `usePanePeekPointer` (`Sidebar.tsx`) matches this to keep the floating sidebar
            // up while the pointer is on an avatar, which lies outside the card and its gap. Without it
            // the card slides away from under a pointer resting here.
            data-sidebar-peek-keep=""
            focusOnPress={false}
            onPress={onSelect}
            onMouseHoverChange={onHoverChange}
            className={`size-9 min-w-0 rounded-lg p-0 ${current ? CHROME_CURRENT_FILLS : CHROME_BUTTON_FILLS}`}
          >
            <ConsoleAvatar name={thisConsole.name} icon={thisConsole.icon} className="size-7" />
          </Button>
        </TitledControl>
        <AvatarStatusBadge status={status} />
      </BadgeAnchor>
      <ActionMenu
        className="hidden"
        label={t("sidebar.console.actions", { name: isolate(thisConsole.name) })}
        tooltip={false}
        items={consoleMenu(t, thisConsole, handlers, sessions)}
        contextTargetRef={tileRef}
      />
    </div>
  );
});

/** The console menu's handlers serve the avatars' menus; `onOpenDialog` also opens New console. */
interface RailProps extends ConsoleMenuHandlers {
  consoles: Console[];
  /** The current console, which the sidebar shows unless it is previewing another. */
  currentConsoleId?: string;
  selectedSessionId?: string;
  onSelectConsole: (consoleId: string) => void;
  /** The pointer is on a console's avatar: with the docked sidebar hidden, that floats it in
   * showing this console. */
  onConsoleEnter: (consoleId: string) => void;
  onConsoleLeave: () => void;
  onRailHoverChange?: (hovered: boolean) => void;
  /** Whether the hidden sidebar can float in on hovering an avatar right now. */
  hoverOpensSidebar: boolean;
  waitingCount: number;
  onNextWaiting: () => void;
  /** Shows the Processes panel that owns a tree the global process list names; false when that project or session is gone. */
  onOpenProcesses: (target: ProcessJump) => boolean;
  onOpenSettings: () => void;
  /** Opens Settings on a section, as the quota panel's way into the accounts one. */
  onOpenSettingsAt: OpenSettings;
  /** Where focus goes when a control that held it goes away (the notifications bell). */
  focusTerminal: () => void;
  /** At the tablet boundary and below the rail is not in the row but comes in with the sidebar's
   * drawer. False where there is no drawer (no console): the rail is then the only way to New
   * console and Settings, so it stays in the row. */
  onDemand: boolean;
  /** The rail is out of the window for now (`onDemand`, at the tablet boundary and below, the drawer
   * closed): out of reach too, not focusable, skipped by the region cycle and hidden from assistive
   * technology. */
  away: boolean;
}

/**
 * The left rail, the window chrome's second half beside the top bar: at the top the console
 * switcher (one avatar per console, in their order, then New console), at the bottom, pushed down,
 * the waiting count, the notifications bell (browser only, while the permission is undecided), the
 * account quota entry and Settings. It sits at the reading direction's start, like the sidebar, and
 * holds only glyphs and avatars. At the tablet boundary and below an `onDemand` rail leaves the row and
 * comes in with the sidebar's drawer (`ON_DEMAND_FRAME`).
 */
export function Rail({
  consoles,
  currentConsoleId,
  selectedSessionId,
  onSelectConsole,
  onConsoleEnter,
  onConsoleLeave,
  onRailHoverChange,
  hoverOpensSidebar,
  onOpenDialog,
  onSelectSession,
  onOpenArchive,
  waitingCount,
  onNextWaiting,
  onOpenProcesses,
  onOpenSettings,
  onOpenSettingsAt,
  focusTerminal,
  onDemand,
  away,
}: RailProps): React.ReactElement {
  const t = useT();
  const consolesRef = useListMotion<HTMLDivElement>({ descendants: true });

  return (
    <nav
      // `data-escape-scope`: Escape on one of its controls closes an open drawer too, as on the top bar.
      data-escape-scope
      data-region="rail"
      onPointerEnter={(event) => event.pointerType === "mouse" && onRailHoverChange?.(isRailPeekArea(event.target))}
      onPointerMove={(event) => event.pointerType === "mouse" && onRailHoverChange?.(isRailPeekArea(event.target))}
      onPointerLeave={(event) => event.pointerType === "mouse" && onRailHoverChange?.(false)}
      aria-label={t("rail.label")}
      // `inert` rather than `aria-hidden` and `tabindex`: it takes the whole subtree out of the focus
      // order and the accessibility tree at once, which `invisible` does only once the exit is over.
      inert={away}
      className={`flex w-(--rail-width) shrink-0 flex-col items-center gap-1 pt-1 pb-2 select-none ${onDemand ? `${ON_DEMAND_FRAME} ${away ? ON_DEMAND_AWAY : ON_DEMAND_OPEN}` : ""}`}
    >
      <div ref={consolesRef} className="relative flex min-h-0 flex-col items-center gap-1">
        {/* More consoles than fit scroll here, the scrollbar left out: the rail is too narrow for
            one, which leaves the fade as the only thing saying there are more. */}
        <FadedScroller hideScrollBar className="relative flex min-h-0 flex-col items-center overscroll-contain py-0.5">
          {consoles.map((c) => (
            <ConsoleTile
              key={c.id}
              console={c}
              current={c.id === currentConsoleId}
              onSelect={() => onSelectConsole(c.id)}
              hoverOpensSidebar={hoverOpensSidebar}
              onHoverChange={(hovered) => (hovered ? onConsoleEnter(c.id) : onConsoleLeave())}
              handlers={{ onOpenDialog, onSelectSession, onOpenArchive }}
            />
          ))}
        </FadedScroller>
        <div data-list-motion="new-console" className="flex shrink-0">
          <ChromeButton id={NEW_CONSOLE_ID} label={t("rail.newConsole")} onPress={() => onOpenDialog({ kind: "new-console" })} tooltipPlacement="end" className="shrink-0">
            <Plus aria-hidden="true" className="size-4" />
          </ChromeButton>
        </div>
      </div>
      <div className="mt-auto flex shrink-0 flex-col items-center gap-1">
        {waitingCount > 0 && (
          <ChromeButton label={t("rail.waiting", { count: waitingCount })} onPress={onNextWaiting} tooltipPlacement="end"
            badge={<RailCountBadge count={waitingCount} overflowLabel={t("rail.waitingBadgeOverflow")} color="warning" />}>
            <Hand aria-hidden="true" className="size-4 text-warning-soft-foreground" />
          </ChromeButton>
        )}
        <RailProcessesEntry onOpen={onOpenProcesses} />
        <NotificationsBell focusTerminal={focusTerminal} />
        <RailQuotaEntry selectedSessionId={selectedSessionId} onOpenSettingsAt={onOpenSettingsAt} />
        <ChromeButton label={t("rail.settings")} onPress={onOpenSettings} tooltipPlacement="end">
          <Settings aria-hidden="true" className="size-4" />
        </ChromeButton>
      </div>
    </nav>
  );
}
