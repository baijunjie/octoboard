import { Hand, Plus, Settings } from "lucide-react";
import React, { memo, useRef } from "react";

import { Button } from "../components/controls/Button";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { isRailPeekArea, PHONE_SLIDE_AWAY } from "../layout/paneOverlay";
import type { Console } from "../protocol";
import { statusLabel } from "../sessions/sessionLabel";
import { consoleMenu, type ConsoleMenuHandlers } from "../sidebar/menus";
import type { OpenSettings } from "../settings/settingsLauncher";
import { RailQuotaEntry } from "./RailQuotaEntry";
import { RailProcessesEntry } from "./RailProcessesEntry";
import { CountedGlyph } from "./CountedGlyph";
import type { ProcessJump } from "../process/processJump";
import { useArchivedOwnerSessions, useConsoleStatus } from "../store";
import { ActionMenu } from "../components/menus/ActionMenu";
import { AvatarStatusBadge } from "../components/status/TopStatusMarker";
import { StatusBadge } from "../components/status/StatusBadge";
import { CHROME_BUTTON_FILLS, CHROME_CURRENT_FILLS, ChromeButton } from "./ChromeButton";
import { ConsoleAvatar } from "./ConsoleAvatar";
import { NotificationsBell } from "./NotificationsBell";
import { useListMotion } from "../components/motion/useListMotion";
import { TitledControl } from "../components/hint/TitledControl";
import { FadedScroller } from "../components/scroll/scrollFade";

const NEW_CONSOLE_ID = "rail-new-console";

/** Below the `phone` breakpoint a rail that is `onDemand` leaves the row and lies over the open
 * drawer's card (`drawerClass` in `layout/paneOverlay.tsx`, whose `max-phone:` padding leaves the
 * rail's width free for it): the same inset from the window's edge, the top bar and the bottom
 * chrome as the card, so the two are one card. The rail paints nothing of its own but the groove
 * on its end edge, which is the seam between it and the sidebar; the card's surface, edge, shadow and
 * corners are all the card's. It comes in and goes out with the card, the same 240ms, easing and
 * slide in from the start edge (`PHONE_SLIDE_AWAY`), `visibility` staying on until the exit has
 * finished and then keeping the away rail out of reach. `z-41` keeps it above the card (`z-40`),
 * which comes later in the document and would otherwise paint over it. */
const ON_DEMAND_FRAME =
  "max-phone:fixed max-phone:start-(--peek-gap) max-phone:top-[calc(var(--top-chrome-height)+var(--peek-gap))] max-phone:bottom-[calc(var(--bottom-chrome-height)+var(--peek-gap))] max-phone:z-41 max-phone:groove-e max-phone:transition-[translate,opacity,visibility] max-phone:duration-240 max-phone:ease-[cubic-bezier(0.2,0,0,1)] max-phone:motion-reduce:transition-none";
const ON_DEMAND_OPEN = "max-phone:opacity-100";
const ON_DEMAND_AWAY = `max-phone:opacity-0 max-phone:invisible ${PHONE_SLIDE_AWAY}`;

/** Puts focus on the rail's New console button, by identity: the console tiles come before it. */
export function focusNewConsole(): void {
  document.getElementById(NEW_CONSOLE_ID)?.focus();
}

/** The activity badge on a 28px avatar: 14px at the bottom end corner, leaving a 2px ring.
 * Under right-to-left the corner, and so the hole, is on the left. */
const AVATAR_CUT_OUT =
  "[mask-image:radial-gradient(circle_at_24px_24px,transparent_9px,black_9.5px)] rtl:[mask-image:radial-gradient(circle_at_4px_24px,transparent_9px,black_9.5px)]";

/** One console on the rail: its avatar, marked with what is going on in it, and the current one on a
 * tile of its own. The tooltip says what the accessible name says, the console's name with the
 * activity folded in, so the badge, which is decorative, gets its words without a tooltip of its
 * own. A right-click offers the console's own actions, from a menu whose trigger is not drawn: the
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
  const sessions = useArchivedOwnerSessions(thisConsole.id);
  const tileRef = useRef<HTMLDivElement>(null);
  const label = status ? t("session.status.named", { name: isolate(thisConsole.name), status: statusLabel(t, status) }) : thisConsole.name;
  const tooltip = status ? (
    <span className="flex items-center gap-2">
      <span dir="auto" className="min-w-0 [overflow-wrap:anywhere]">{thisConsole.name}</span>
      <StatusBadge variant="soft" color={status === "waiting_user" ? "warning" : "success"} overflow="wrap">
        {statusLabel(t, status)}
      </StatusBadge>
    </span>
  ) : thisConsole.name;
  return (
    <div ref={tileRef} data-list-motion={thisConsole.id} className="flex">
      <TitledControl title={tooltip} placement="end" focusOnly={hoverOpensSidebar}>
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          aria-label={label}
          // The markup tooltip repeats the name, which TitledControl only detects for plain text.
          aria-describedby=""
          aria-current={current ? "true" : undefined}
          // Not styling: `usePanePeekPointer` (`Sidebar.tsx`) matches this to keep the floating sidebar
          // up while the pointer is on an avatar, which lies outside the card and its gap. Without it
          // the card slides away from under a pointer resting here.
          data-sidebar-peek-keep=""
          preventFocusOnPress
          onPress={onSelect}
          onHoverStart={(event) => event.pointerType === "mouse" && onHoverChange(true)}
          onHoverEnd={(event) => event.pointerType === "mouse" && onHoverChange(false)}
          className={`size-9 min-w-0 rounded-lg p-0 ${current ? CHROME_CURRENT_FILLS : CHROME_BUTTON_FILLS}`}
        >
          <span className="relative flex">
            <ConsoleAvatar name={thisConsole.name} icon={thisConsole.icon} className={`size-7 ${status ? AVATAR_CUT_OUT : ""}`} />
            <AvatarStatusBadge status={status} className="absolute -end-0.75 -bottom-0.75" />
          </span>
        </Button>
      </TitledControl>
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
  /** Below the `phone` breakpoint the rail is not in the row but comes in with the sidebar's
   * drawer. False where there is no drawer (no console): the rail is then the only way to New
   * console and Settings, so it stays in the row. */
  onDemand: boolean;
  /** The rail is out of the window for now (`onDemand`, below the `phone` breakpoint, the drawer
   * closed): out of reach too, not focusable, skipped by the region cycle and hidden from assistive
   * technology. */
  away: boolean;
}

/**
 * The left rail, the window chrome's second half beside the top bar: at the top the console
 * switcher (one avatar per console, in their order, then New console), at the bottom, pushed down,
 * the waiting count, the notifications bell (browser only, while the permission is undecided), the
 * account quota entry and Settings. It sits at the reading direction's start, like the sidebar, and
 * holds only glyphs and avatars. Below the `phone` breakpoint an `onDemand` rail leaves the row and
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
        <FadedScroller hideScrollBar className="relative flex min-h-0 flex-col items-center gap-1 overscroll-contain py-0.5">
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
          <ChromeButton label={t("rail.waiting", { count: waitingCount })} onPress={onNextWaiting} tooltipPlacement="end">
            <CountedGlyph count={waitingCount} overflowLabel={t("rail.waitingBadgeOverflow")} color="warning">
              <Hand aria-hidden="true" className="size-4 text-warning-soft-foreground" />
            </CountedGlyph>
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
