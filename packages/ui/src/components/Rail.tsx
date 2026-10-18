import { Badge, Button } from "@heroui/react";
import { Hand, Plus, Settings } from "lucide-react";
import React, { memo, useRef } from "react";

import { isolate } from "../i18n/isolate";
import { textDirection } from "../i18n/languages";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Console } from "../protocol";
import { activityLabelKey } from "../sessionLabel";
import { consoleMenu, type ConsoleMenuHandlers } from "../sidebar/menus";
import type { SettingsSectionId } from "../settings/SettingsDialog";
import { RailQuotaEntry } from "../quota/RailQuotaEntry";
import { useArchivedOwnerSessions, useConsoleActivity } from "../sessions/useSessions";
import { ActionMenu } from "./ActionMenu";
import { ActivityBadge } from "./ActivityMarker";
import { CHROME_BUTTON_FILLS, CHROME_CURRENT_FILLS, ChromeButton } from "./ChromeButton";
import { ConsoleAvatar } from "./ConsoleAvatar";
import { NotificationsBell } from "./NotificationsBell";
import { useListMotion } from "./useListMotion";
import { TitledControl } from "./TitledControl";
import { FadedScroller } from "./scrollFade";

const NEW_CONSOLE_ID = "rail-new-console";

/** Puts focus on the rail's New console button, by identity: the console tiles come before it. */
export function focusNewConsole(): void {
  document.getElementById(NEW_CONSOLE_ID)?.focus();
}

/** The badges on the rail are cut out of what they sit on: the glyph or avatar under one is masked
 * away in a ring around it, so the badge reads as separate from it and the ring shows whatever is
 * behind (the native material, the browser's chrome colour, a hover or selected tint) rather than a
 * colour of its own. Each mask is the hole for one badge geometry, so it is written next to the
 * classes placing that badge.
 *
 * The waiting count is as large as the 16px glyph itself, so centred on the glyph's top end corner
 * it would hide a quarter of it; it sits above that corner, 4px past it along the diagonal, so the
 * glyph keeps its shape. The pill is 16px high with no padding (`*:px-0` takes away the
 * label's own `px-0.5`, which would make two digits a 19px pill); two semibold 11px digits measure
 * up to 15px, so up to two digits it is a 16px circle, and the overflow label ("99+", about 26px)
 * is a pill of its own 28px width, centred 2px nearer the glyph so that its end stays clear of the
 * content panel's edge. Its hole is the two end circles, each 1.5px wider than the
 * pill's own ends, as one mask layer each: the layers are intersected, so what is cut out is their
 * union.
 *
 * The count is a decoration of the layout rather than a physical thing, so under right-to-left it
 * moves to the glyph's other top corner, the one that still points into the content: the rail is at
 * the window's start side, which under right-to-left is its right edge, so left at the glyph's
 * physical top right the overflow pill would run into that edge.
 * HeroUI's `Badge` has only physical placements, so the corner is picked by `placement`, and the
 * offsets and the mask coordinates (measured across the 16px glyph) are mirrored to match it. */
const COUNT_PLACEMENT = "pointer-events-none translate-x-[calc(25%+4px)] -translate-y-[calc(25%+4px)] rtl:-translate-x-[calc(25%+4px)]";
const COUNT_OVERFLOW_PLACEMENT = "pointer-events-none translate-x-[calc(25%+2px)] -translate-y-[calc(25%+4px)] rtl:-translate-x-[calc(25%+2px)]";
const COUNT_CUT_OUT =
  "[mask-image:radial-gradient(circle_at_18px_-8px,transparent_9.5px,black_10px)] rtl:[mask-image:radial-gradient(circle_at_-2px_-8px,transparent_9.5px,black_10px)]";
const COUNT_OVERFLOW_PILL = "w-7";
const COUNT_OVERFLOW_CUT_OUT =
  "[mask-image:radial-gradient(circle_at_10px_-8px,transparent_9.5px,black_10px),radial-gradient(circle_at_22px_-8px,transparent_9.5px,black_10px)] [mask-composite:intersect] rtl:[mask-image:radial-gradient(circle_at_6px_-8px,transparent_9.5px,black_10px),radial-gradient(circle_at_-6px_-8px,transparent_9.5px,black_10px)]";
/** The activity badge on a 28px avatar: 14px, at the avatar's bottom end corner and 3px past it,
 * so it stays inside the current console's tile, leaving a 2px ring. Under right-to-left the
 * corner, and so the hole, is on the left. */
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
  const activity = useConsoleActivity(thisConsole.id);
  const sessions = useArchivedOwnerSessions(thisConsole.id);
  const tileRef = useRef<HTMLDivElement>(null);
  const activityKey = activityLabelKey(activity);
  const label = activityKey ? t(activityKey, { name: isolate(thisConsole.name) }) : thisConsole.name;
  return (
    <div ref={tileRef} data-list-motion={thisConsole.id} className="flex">
      <TitledControl title={label} placement="end" focusOnly={hoverOpensSidebar}>
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          aria-label={label}
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
            <ConsoleAvatar name={thisConsole.name} icon={thisConsole.icon} className={`size-7 ${activity ? AVATAR_CUT_OUT : ""}`} />
            <ActivityBadge activity={activity} className="absolute -end-0.75 -bottom-0.75" />
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
  onSelectConsole: (consoleId: string) => void;
  /** The pointer is on a console's avatar: with the docked sidebar hidden, that floats it in
   * showing this console. */
  onConsoleEnter: (consoleId: string) => void;
  onConsoleLeave: () => void;
  /** Whether the hidden sidebar can float in on hovering an avatar right now. */
  hoverOpensSidebar: boolean;
  waitingCount: number;
  onNextWaiting: () => void;
  onOpenSettings: () => void;
  /** Opens Settings on a section, as the quota panel's way into the accounts one. */
  onOpenSettingsAt: (section: SettingsSectionId) => void;
  /** Where focus goes when a control that held it goes away (the notifications bell). */
  focusTerminal: () => void;
}

/**
 * The left rail, the window chrome's second half beside the top bar: at the top the console
 * switcher (one avatar per console, in their order, then New console), at the bottom, pushed down,
 * the waiting count, the notifications bell (browser only, while the permission is undecided), the
 * account quota entry and Settings. It sits at the reading direction's start, like the sidebar, and
 * holds only glyphs and avatars.
 */
export function Rail({
  consoles,
  currentConsoleId,
  onSelectConsole,
  onConsoleEnter,
  onConsoleLeave,
  hoverOpensSidebar,
  onOpenDialog,
  onSelectSession,
  onOpenArchive,
  waitingCount,
  onNextWaiting,
  onOpenSettings,
  onOpenSettingsAt,
  focusTerminal,
}: RailProps): React.ReactElement {
  const t = useT();
  const rtl = textDirection(useCurrentLanguage()) === "rtl";
  const overflow = waitingCount > 99;
  const consolesRef = useListMotion<HTMLDivElement>({ descendants: true });

  return (
    <nav
      // `data-escape-scope`: Escape on one of its controls closes an open drawer too, as on the top bar.
      data-escape-scope
      data-region="rail"
      aria-label={t("rail.label")}
      className="flex w-(--rail-width) shrink-0 flex-col items-center gap-1 pt-1 pb-2 select-none"
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
            <Badge.Anchor>
              <Hand aria-hidden="true" className={`size-4 text-warning-glyph ${overflow ? COUNT_OVERFLOW_CUT_OUT : COUNT_CUT_OUT}`} />
              <Badge
                aria-hidden="true"
                color="warning"
                variant="primary"
                size="sm"
                placement={rtl ? "top-left" : "top-right"}
                // The overflow label is a number followed by a sign, which reads in that order in
                // any language; left to the surrounding direction it would come out as "+99".
                dir="ltr"
                className={`${overflow ? COUNT_OVERFLOW_PLACEMENT : COUNT_PLACEMENT} border-0 text-[11px] leading-none font-semibold *:px-0 ${overflow ? COUNT_OVERFLOW_PILL : ""}`}
              >
                {overflow ? t("rail.waitingBadgeOverflow") : waitingCount}
              </Badge>
            </Badge.Anchor>
          </ChromeButton>
        )}
        <NotificationsBell focusTerminal={focusTerminal} />
        <RailQuotaEntry onOpenSettingsAt={onOpenSettingsAt} />
        <ChromeButton label={t("rail.settings")} onPress={onOpenSettings} tooltipPlacement="end">
          <Settings aria-hidden="true" className="size-4" />
        </ChromeButton>
      </div>
    </nav>
  );
}
