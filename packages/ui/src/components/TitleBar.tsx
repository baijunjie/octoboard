import { Badge, Button, Tooltip } from "@heroui/react";
import {
  Bell,
  ChevronRight,
  Hand,
  PanelLeft,
  PanelLeftDashed,
  PanelRight,
  PanelRightDashed,
  Plus,
  RefreshCw,
  Settings,
  Unplug,
} from "lucide-react";
import React, { useSyncExternalStore } from "react";

import type { ConnectionState } from "../daemon-client";
import { useT } from "../i18n/react";
import type { PlainMessageKey } from "../i18n/catalog";
import { PANE_ID } from "../layout/paneOverlay";
import { useNotificationPermission } from "../lifecycle/useNotificationPermission";
import { usePlatform } from "../platform/react";
import type { Session } from "../protocol";
import { useDaemonStore } from "../store";
import type { TerminalProblem } from "../terminal/TerminalPane";
import { FadeOverflow } from "./FadeOverflow";
import { useFocusHandoff } from "./useFocusHandoff";
import { StatusIcon } from "./StatusIcon";
import { TitledControl } from "./TitledControl";

/** A press on any of the bar's controls must leave keyboard focus on the terminal, so every one of
 * them is built from this. `TitledControl` gives it the tooltip. */
function BarButton({
  label,
  onPress,
  children,
  isIconOnly = true,
  onMouseHoverChange,
  expanded,
  controls,
}: {
  label: string;
  onPress: () => void;
  children: React.ReactNode;
  isIconOnly?: boolean;
  /** For a button that shows or hides a region: whether the region is shown, as `aria-expanded`;
   * `controls` is the region's id. */
  expanded?: boolean;
  controls?: string;
  /** Called as a mouse pointer enters or leaves the button; touch and pen are ignored, as the
   * edge hot zones ignore them (`useHover` filters out touch only). */
  onMouseHoverChange?: (hovered: boolean) => void;
}): React.ReactElement {
  return (
    <TitledControl title={label}>
      <Button
        isIconOnly={isIconOnly}
        size="sm"
        variant="ghost"
        aria-label={label}
        aria-expanded={expanded}
        aria-controls={controls}
        preventFocusOnPress
        onPress={onPress}
        onHoverStart={(event) => event.pointerType === "mouse" && onMouseHoverChange?.(true)}
        onHoverEnd={(event) => event.pointerType === "mouse" && onMouseHoverChange?.(false)}
      >
        {children}
      </Button>
    </TitledControl>
  );
}

/** Where a top bar glyph's badge sits: centred on the glyph's top right corner, the count and the
 * dot alike, so they line up across the bar. The corner stays the physical top right under a
 * right-to-left language, since HeroUI's `Badge` has only physical placements. Its `top-right`
 * placement only pushes a badge a quarter of its size past the corner, which leaves a count badge
 * covering most of a 16px glyph; `translate` stacks with that placement's own `transform`, adding
 * the other quarter. */
const BADGE_PLACEMENT = "pointer-events-none translate-x-1/4 -translate-y-1/4";

/** The invitation to turn notifications on, where the platform wants the ask to come from a user
 * gesture (a browser) and the answer is still undecided; gone once the answer is given, whichever
 * it is. The dot only draws the eye, the label carries the meaning. The Settings dialog's
 * Notifications section is the other place to see and change the answer.
 *
 * Pressed from the keyboard, the bell holds focus until the answer removes it, and focus would then
 * fall to `<body>`; so once the answer is in, focus that was lost is handed on (`useFocusHandoff`).
 * Not before the answer: while the browser's own prompt is up, keys must not reach the agent. */
function NotificationsBell({ focusTerminal }: { focusTerminal: () => void }): React.ReactElement | null {
  const t = useT();
  const { status, request } = useNotificationPermission();
  const held = useFocusHandoff(focusTerminal);
  if (!request || status !== "undecided") return null;
  return (
    <span className="contents" {...held}>
      <BarButton label={t("titleBar.notifications.enable")} onPress={() => void request()}>
        <Badge.Anchor>
          <Bell aria-hidden="true" className="size-4" />
          <Badge aria-hidden="true" color="accent" variant="primary" size="sm" className={`${BADGE_PLACEMENT} min-h-2.5 min-w-2.5`} />
        </Badge.Anchor>
      </BarButton>
    </span>
  );
}

/** The bar's frame: full window width, above everything, and the window's drag handle where the
 * platform has no native titlebar. `deep` makes every non-interactive descendant draggable, and
 * Tauri's drag script already leaves buttons alone; a double-click zooms the window like a native
 * titlebar. `data-escape-scope`: Escape on one of its controls closes an open drawer too.
 *
 * It is a plain `header` rather than HeroUI's `Toolbar`, which would make its buttons one roving
 * tab stop; every control here has to be a tab stop of its own. */
function BarFrame({ children }: { children: React.ReactNode }): React.ReactElement {
  const { windowChrome } = usePlatform();
  const inset = useWindowControlsInset();
  return (
    <header
      data-escape-scope
      data-region="topbar"
      data-tauri-drag-region={windowChrome ? "deep" : undefined}
      className="flex h-(--title-bar-height) shrink-0 items-stretch border-b border-separator bg-surface select-none rtl:pl-(--window-controls-inset)"
      style={{ "--window-controls-inset": `${inset}px` } as React.CSSProperties}
    >
      {children}
    </header>
  );
}

const noSubscribe = () => () => {};
const noInset = () => 0;

/** The inset the window controls cover, which changes as they come and go (fullscreen); 0 without
 * them. The macOS traffic lights sit at the window's physical left whatever the reading direction,
 * so the clear space is physical too: under left-to-right a spacer at the start of the bar's first
 * segment (`LeftInset`), under right-to-left, where that segment is on the right, the bar's own
 * left padding (`rtl:pl-*` in `BarFrame`). */
function useWindowControlsInset(): number {
  const { windowChrome } = usePlatform();
  return useSyncExternalStore(windowChrome?.subscribe ?? noSubscribe, windowChrome?.leftInset ?? noInset);
}

function LeftInset(): React.ReactElement | null {
  const { windowChrome } = usePlatform();
  return windowChrome ? <div className="w-(--window-controls-inset) shrink-0 rtl:hidden" /> : null;
}

/** The bar with nothing in it, for the screens that have no session UI yet (starting up, failed to
 * start): it still keeps the window's controls clear and makes the window draggable. */
export function BareTitleBar(): React.ReactElement {
  return (
    <BarFrame>
      <LeftInset />
    </BarFrame>
  );
}

const DAEMON_PROBLEM: Record<Exclude<ConnectionState, "open">, { label: PlainMessageKey; color: "warning" | "danger" }> = {
  connecting: { label: "titleBar.daemon.connecting", color: "warning" },
  reconnecting: { label: "titleBar.daemon.reconnecting", color: "warning" },
  closed: { label: "titleBar.daemon.disconnected", color: "danger" },
};

const TERMINAL_PROBLEM: Record<TerminalProblem["state"], { label: PlainMessageKey; color: "warning" | "danger" }> = {
  reconnecting: { label: "titleBar.terminal.reconnecting", color: "warning" },
  disconnected: { label: "titleBar.terminal.disconnected", color: "danger" },
};

/** A connection problem as a glyph, its label as accessible name and tooltip: a turning arrow
 * while attempts are under way, an unplugged cord once they are spent, so color is never the only
 * cue. The label is visually hidden text rather than the glyph's own `aria-label`, which a live
 * region may not announce.
 *
 * A deliberate exception to `TitledControl`'s rule against `Tooltip.Trigger`: this is an
 * indicator, not a button, so it has no react-aria-components control to hang the tooltip on,
 * which `TitledControl` needs as its child. The trigger is focusable, which keeps the label
 * reachable from the keyboard, and a mouse press leaves focus where it was like the bar's buttons. */
function ConnectionIcon({ label, color }: { label: string; color: "warning" | "danger" }): React.ReactElement {
  const Icon = color === "danger" ? Unplug : RefreshCw;
  return (
    <Tooltip>
      <Tooltip.Trigger
        className="flex size-7 items-center justify-center rounded-md outline-none focus-visible:ring-2 focus-visible:ring-focus"
        // As `Scrim` and `PaneResizeHandle`: a press must not focus the trigger and take focus off the terminal.
        onMouseDown={(event) => event.preventDefault()}
      >
        <Icon
          aria-hidden="true"
          className={color === "danger" ? "size-4 text-danger" : "size-4 text-warning motion-safe:animate-spin-slow"}
        />
        <span className="sr-only">{label}</span>
      </Tooltip.Trigger>
      <Tooltip.Content className="pointer-events-none">{label}</Tooltip.Content>
    </Tooltip>
  );
}

/** The one indicator of connection trouble, empty while everything is healthy: the daemon's own
 * state first, since the terminal's socket cannot be better than the daemon behind it, then the
 * selected session's terminal. The daemon's Retry lives on `ConnectionBanner`; the terminal's
 * Reconnect is here, once automatic attempts are spent. The terminal's state is the only thing the
 * status region announces, and it is always mounted, as a live region has to exist before its
 * content changes to be announced; the daemon's state is announced by `ConnectionBanner`, so its
 * icon sits outside the region and is not read out a second time. A focused indicator that goes
 * away with the problem hands focus on (`useFocusHandoff`). */
function ConnectionStatus({
  terminalProblem,
  focusTerminal,
}: {
  terminalProblem?: TerminalProblem;
  focusTerminal: () => void;
}): React.ReactElement {
  const t = useT();
  const held = useFocusHandoff(focusTerminal);
  const daemonState = useDaemonStore((s) => s.connectionState);
  const daemonProblem = daemonState !== "open" ? DAEMON_PROBLEM[daemonState] : undefined;
  const problem = daemonProblem ? undefined : terminalProblem && TERMINAL_PROBLEM[terminalProblem.state];
  const reconnect =
    daemonState === "open" && terminalProblem?.state === "disconnected" ? terminalProblem.reconnect : undefined;
  return (
    <span className="contents" {...held}>
      {daemonProblem && <ConnectionIcon label={t(daemonProblem.label)} color={daemonProblem.color} />}
      <span role="status" className="flex items-center">
        {problem && <ConnectionIcon label={t(problem.label)} color={problem.color} />}
      </span>
      {reconnect && (
        <BarButton isIconOnly={false} label={t("titleBar.terminal.reconnectLabel")} onPress={reconnect}>
          {t("titleBar.terminal.reconnect")}
        </BarButton>
      )}
    </span>
  );
}

/** Console › Project › session title, with the session's status icon beside it; a console session
 * has no project, so its trail is just the console and its own title. The trail fades at its end
 * edge when too long, the icon always stays. */
function Breadcrumb({ session }: { session: Session }): React.ReactElement {
  const consoleName = useDaemonStore((s) => s.consoles.get(session.console_id)?.name);
  const projectName = useDaemonStore((s) => (session.project_id ? s.projects.get(session.project_id)?.name : undefined));
  const trail = session.role === "console" ? [consoleName, session.title] : [consoleName, projectName, session.title];
  return (
    <Trail names={trail.map((part) => part ?? "…")}>
      <StatusIcon status={session.status} />
    </Trail>
  );
}

/** A trail of names, the last one emphasised; it fades at its end edge when too long, and what
 * follows it (a status icon) always stays. */
function Trail({ names, children }: { names: string[]; children?: React.ReactNode }): React.ReactElement {
  return (
    <div className="flex min-w-0 items-center gap-1.5 text-sm">
      <FadeOverflow className="min-w-0" titleWhenClipped={names.join(" › ")}>
        <span className="inline-flex items-center gap-1.5">
          {names.map((name, index) => (
            <React.Fragment key={index}>
              {index > 0 && <ChevronRight aria-hidden="true" className="size-3.5 shrink-0 text-muted rtl:-scale-x-100" />}
              <span dir="auto" className={index === names.length - 1 ? "font-medium" : "text-muted"}>
                {name}
              </span>
            </React.Fragment>
          ))}
        </span>
      </FadeOverflow>
      {children}
    </div>
  );
}

interface TitleBarProps {
  onOpenSettings: () => void;
  /** The width of the docked sidebar when it is shown, which the start segment then matches so the
   * sidebar visually extends up into the bar; `undefined` while it is hidden. */
  sidebarWidth?: number;
  /** Whether the sidebar is currently shown, docked or as an open drawer; a hidden sidebar that is
   * only floating in on hover does not count, since pressing the toggle docks it. */
  sidebarShown: boolean;
  onToggleSidebar: () => void;
  /** The pointer is on the sidebar toggle: with the docked sidebar hidden, that floats it in. */
  onSidebarToggleEnter: () => void;
  onSidebarToggleLeave: () => void;
  onNewConsole: () => void;
  selectedSession?: Session;
  /** What the content area shows instead of the selected session's terminal (the archive view),
   * as the breadcrumb's names; it then takes the session's place in the bar. */
  viewTrail?: string[];
  /** What is wrong with the selected session's terminal connection, if anything. */
  terminalProblem?: TerminalProblem;
  waitingCount: number;
  onNextWaiting: () => void;
  /** Only a console session has a report panel, so the toggle exists only for one. */
  hasReportPanel: boolean;
  /** Whether the report panel is shown, docked or as an open drawer; hidden but floating in on
   * hover does not count, as with the sidebar. */
  reportShown: boolean;
  onToggleReport: () => void;
  /** The pointer is on the report toggle: with the docked report panel hidden, that floats it in. */
  onReportToggleEnter: () => void;
  onReportToggleLeave: () => void;
  /** Where focus goes when a control that held it goes away (the notifications bell, a connection
   * indicator). */
  focusTerminal: () => void;
}

/**
 * The bar across the top of the window: a start segment aligned with the sidebar (sidebar toggle,
 * New console), the selected session's breadcrumb, and at the end the waiting count, the
 * connection trouble indicator (nothing while healthy), the notifications bell (browser only, while
 * the permission is undecided), the report panel toggle (console session only) and Settings.
 */
export function TitleBar({
  onOpenSettings,
  sidebarWidth,
  sidebarShown,
  onToggleSidebar,
  onSidebarToggleEnter,
  onSidebarToggleLeave,
  onNewConsole,
  selectedSession,
  viewTrail,
  terminalProblem,
  waitingCount,
  onNextWaiting,
  hasReportPanel,
  reportShown,
  onToggleReport,
  onReportToggleEnter,
  onReportToggleLeave,
  focusTerminal,
}: TitleBarProps): React.ReactElement {
  const t = useT();
  return (
    <BarFrame>
      {/* Above the `docked` breakpoint with the sidebar shown, exactly the sidebar's width and its
          edge line, so the two read as one column; otherwise just as wide as its controls. */}
      <div
        className={`flex shrink-0 items-center ${sidebarWidth !== undefined ? "docked:w-(--bar-start-width) docked:border-e docked:border-separator" : ""}`}
        style={
          sidebarWidth !== undefined ? ({ "--bar-start-width": `${sidebarWidth}px` } as React.CSSProperties) : undefined
        }
      >
        <LeftInset />
        <div className="flex items-center gap-1 px-2">
          <BarButton
            label={sidebarShown ? t("titleBar.sidebar.hide") : t("titleBar.sidebar.show")}
            onPress={onToggleSidebar}
            expanded={sidebarShown}
            controls={PANE_ID.sidebar}
            onMouseHoverChange={(hovered) => (hovered ? onSidebarToggleEnter() : onSidebarToggleLeave())}
          >
            {sidebarShown ? (
              <PanelLeft aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            ) : (
              <PanelLeftDashed aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            )}
          </BarButton>
          <BarButton label={t("titleBar.newConsole")} onPress={onNewConsole}>
            <Plus aria-hidden="true" className="size-4" />
          </BarButton>
        </div>
      </div>
      <div className="flex min-w-0 flex-1 items-center px-3">
        {viewTrail ? <Trail names={viewTrail} /> : selectedSession && <Breadcrumb session={selectedSession} />}
      </div>
      <div className="flex shrink-0 items-center gap-1 px-2">
        {waitingCount > 0 && (
          <BarButton label={t("titleBar.waiting", { count: waitingCount })} onPress={onNextWaiting}>
            <Badge.Anchor>
              <Hand aria-hidden="true" className="size-4 text-warning" />
              <Badge aria-hidden="true" color="warning" variant="primary" size="sm" className={BADGE_PLACEMENT}>
                {waitingCount > 99 ? t("titleBar.waitingBadgeOverflow") : waitingCount}
              </Badge>
            </Badge.Anchor>
          </BarButton>
        )}
        <ConnectionStatus terminalProblem={terminalProblem} focusTerminal={focusTerminal} />
        <NotificationsBell focusTerminal={focusTerminal} />
        {hasReportPanel && (
          <BarButton
            label={reportShown ? t("titleBar.report.hide") : t("titleBar.report.show")}
            onPress={onToggleReport}
            expanded={reportShown}
            controls={PANE_ID.report}
            onMouseHoverChange={(hovered) => (hovered ? onReportToggleEnter() : onReportToggleLeave())}
          >
            {reportShown ? (
              <PanelRight aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            ) : (
              <PanelRightDashed aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            )}
          </BarButton>
        )}
        <BarButton label={t("titleBar.settings")} onPress={onOpenSettings}>
          <Settings aria-hidden="true" className="size-4" />
        </BarButton>
      </div>
    </BarFrame>
  );
}
