import { Badge, Button } from "@heroui/react";
import { Hand, PanelRight, PanelRightDashed, Plus, Settings } from "lucide-react";
import React, { useMemo, useRef } from "react";

import type { DialogRequest } from "../dialogs/dialogRequest";
import { useT } from "../i18n/react";
import { PANE_ID } from "../layout/paneOverlay";
import type { Console, Session } from "../protocol";
import { activityLabelKey } from "../sessionLabel";
import { consoleMenu } from "../sidebar/menus";
import { type Activity, consoleActivity } from "../sidebar/order";
import type { TerminalProblem } from "../terminal/TerminalPane";
import { ActionMenu } from "./ActionMenu";
import { CHROME_BUTTON_FILLS, CHROME_CURRENT_FILLS, ChromeButton } from "./ChromeButton";
import { ConnectionStatus } from "./ConnectionStatus";
import { ConsoleAvatar } from "./ConsoleAvatar";
import { NotificationsBell } from "./NotificationsBell";
import { ActivityBadge } from "./StatusIcon";
import { TitledControl } from "./TitledControl";

/** The badges on the rail are cut out of what they sit on: the glyph or avatar under one is masked
 * away in a ring around it, so the badge reads as separate from it and the ring shows whatever is
 * behind (the native material, the browser's chrome colour, a hover or selected tint) rather than a
 * colour of its own. Each mask is the hole for one badge geometry, so it is written next to the
 * classes placing that badge.
 *
 * The waiting count is as large as the 16px glyph itself, so centred on the glyph's top right corner
 * it would hide a quarter of it; it sits above the glyph's top right, 4px past it along the diagonal,
 * so the glyph keeps its shape. The pill is 16px high with no padding (`*:px-0` takes away the
 * label's own `px-0.5`, which would make two digits a 19px pill); two semibold 11px digits measure
 * up to 15px, so up to two digits it is a 16px circle, and the overflow label ("99+", about 26px)
 * is a pill of its own 28px width, centred 2px nearer the glyph so that its end stays clear of the
 * content panel's edge. Its hole is the two end circles, each 1.5px wider than the
 * pill's own ends, as one mask layer each: the layers are intersected, so what is cut out is their
 * union. */
const COUNT_PLACEMENT = "pointer-events-none translate-x-[calc(25%+4px)] -translate-y-[calc(25%+4px)]";
const COUNT_OVERFLOW_PLACEMENT = "pointer-events-none translate-x-[calc(25%+2px)] -translate-y-[calc(25%+4px)]";
const COUNT_CUT_OUT = "[mask-image:radial-gradient(circle_at_18px_-8px,transparent_9.5px,black_10px)]";
const COUNT_OVERFLOW_PILL = "w-7";
const COUNT_OVERFLOW_CUT_OUT =
  "[mask-image:radial-gradient(circle_at_10px_-8px,transparent_9.5px,black_10px),radial-gradient(circle_at_22px_-8px,transparent_9.5px,black_10px)] [mask-composite:intersect]";
/** The activity badge on a 28px avatar: 14px, at the avatar's bottom end corner and 3px past it,
 * so it stays inside the current console's tile, leaving a 2px ring. Under right-to-left the
 * corner, and so the hole, is on the left. */
const AVATAR_CUT_OUT =
  "[mask-image:radial-gradient(circle_at_24px_24px,transparent_9px,black_9.5px)] rtl:[mask-image:radial-gradient(circle_at_4px_24px,transparent_9px,black_9.5px)]";

/** One console on the rail: its avatar, marked with what is going on in it, and the current one on a
 * tile of its own. The tooltip is the console's name; the accessible name adds the activity. A
 * right-click offers the console's own actions, from a menu whose trigger is not drawn: the
 * sidebar's header has the same menu on a visible button. */
function ConsoleTile({
  console: thisConsole,
  activity,
  current,
  onSelect,
  onOpenDialog,
}: {
  console: Console;
  activity: Activity;
  current: boolean;
  onSelect: () => void;
  onOpenDialog: (dialog: DialogRequest) => void;
}): React.ReactElement {
  const t = useT();
  const tileRef = useRef<HTMLDivElement>(null);
  const activityKey = activityLabelKey(activity);
  return (
    <div ref={tileRef} className="flex">
      <TitledControl title={thisConsole.name} placement="end">
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          aria-label={activityKey ? t(activityKey, { name: thisConsole.name }) : thisConsole.name}
          aria-current={current ? "true" : undefined}
          preventFocusOnPress
          onPress={onSelect}
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
        label={t("sidebar.console.actions", { name: thisConsole.name })}
        tooltip={false}
        items={consoleMenu(t, thisConsole, onOpenDialog)}
        contextTargetRef={tileRef}
      />
    </div>
  );
}

interface RailProps {
  consoles: Console[];
  sessions: Session[];
  /** The console the sidebar shows. */
  currentConsoleId?: string;
  onSelectConsole: (consoleId: string) => void;
  onOpenDialog: (dialog: DialogRequest) => void;
  waitingCount: number;
  onNextWaiting: () => void;
  /** What is wrong with the selected session's terminal connection, if anything. */
  terminalProblem?: TerminalProblem;
  /** Only a console session has a report panel, so the toggle exists only for one. */
  hasReportPanel: boolean;
  /** Whether the report panel is shown, docked or as an open drawer; hidden but floating in on
   * hover does not count, as with the sidebar. */
  reportShown: boolean;
  onToggleReport: () => void;
  /** The pointer is on the report toggle: with the docked report panel hidden, that floats it in. */
  onReportToggleEnter: () => void;
  onReportToggleLeave: () => void;
  onOpenSettings: () => void;
  /** Where focus goes when a control that held it goes away (the notifications bell, a connection
   * indicator). */
  focusTerminal: () => void;
}

/**
 * The left rail, the window chrome's second half beside the top bar: at the top the console
 * switcher (one avatar per console, in their order, then New console), at the bottom, pushed down,
 * the waiting count, the connection trouble indicator (nothing while healthy), the notifications
 * bell (browser only, while the permission is undecided), the report panel toggle (console session
 * only) and Settings. It sits at the reading direction's start, like the sidebar, and holds only
 * glyphs and avatars.
 */
export function Rail({
  consoles,
  sessions,
  currentConsoleId,
  onSelectConsole,
  onOpenDialog,
  waitingCount,
  onNextWaiting,
  terminalProblem,
  hasReportPanel,
  reportShown,
  onToggleReport,
  onReportToggleEnter,
  onReportToggleLeave,
  onOpenSettings,
  focusTerminal,
}: RailProps): React.ReactElement {
  const t = useT();
  const overflow = waitingCount > 99;
  const activities = useMemo(() => {
    const byConsole = new Map<string, Session[]>();
    for (const session of sessions) {
      const group = byConsole.get(session.console_id);
      if (group) group.push(session);
      else byConsole.set(session.console_id, [session]);
    }
    return new Map(consoles.map((c) => [c.id, consoleActivity(byConsole.get(c.id) ?? [])]));
  }, [consoles, sessions]);

  return (
    <nav
      // `data-escape-scope`: Escape on one of its controls closes an open drawer too, as on the top bar.
      data-escape-scope
      data-region="rail"
      aria-label={t("rail.label")}
      className="flex w-(--rail-width) shrink-0 flex-col items-center gap-1 pt-1 pb-2 select-none"
    >
      <div className="flex min-h-0 flex-col items-center gap-1">
        {/* More consoles than fit scroll here, the scrollbar left out: the rail is too narrow for one. */}
        <div className="flex min-h-0 flex-col items-center gap-1 overflow-y-auto overscroll-contain py-0.5 [scrollbar-width:none]">
          {consoles.map((c) => (
            <ConsoleTile
              key={c.id}
              console={c}
              activity={activities.get(c.id)}
              current={c.id === currentConsoleId}
              onSelect={() => onSelectConsole(c.id)}
              onOpenDialog={onOpenDialog}
            />
          ))}
        </div>
        <ChromeButton label={t("rail.newConsole")} onPress={() => onOpenDialog({ kind: "new-console" })} tooltipPlacement="end" className="shrink-0">
          <Plus aria-hidden="true" className="size-4" />
        </ChromeButton>
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
                className={`${overflow ? COUNT_OVERFLOW_PLACEMENT : COUNT_PLACEMENT} border-0 text-[11px] leading-none font-semibold *:px-0 ${overflow ? COUNT_OVERFLOW_PILL : ""}`}
              >
                {overflow ? t("rail.waitingBadgeOverflow") : waitingCount}
              </Badge>
            </Badge.Anchor>
          </ChromeButton>
        )}
        <ConnectionStatus terminalProblem={terminalProblem} focusTerminal={focusTerminal} />
        <NotificationsBell focusTerminal={focusTerminal} />
        {hasReportPanel && (
          <ChromeButton
            label={reportShown ? t("rail.report.hide") : t("rail.report.show")}
            onPress={onToggleReport}
            expanded={reportShown}
            controls={PANE_ID.report}
            onMouseHoverChange={(hovered) => (hovered ? onReportToggleEnter() : onReportToggleLeave())}
            tooltipPlacement="end"
          >
            {reportShown ? (
              <PanelRight aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            ) : (
              <PanelRightDashed aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            )}
          </ChromeButton>
        )}
        <ChromeButton label={t("rail.settings")} onPress={onOpenSettings} tooltipPlacement="end">
          <Settings aria-hidden="true" className="size-4" />
        </ChromeButton>
      </div>
    </nav>
  );
}
