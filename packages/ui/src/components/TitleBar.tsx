import { Button, Chip } from "@heroui/react";
import {
  Bell,
  ChevronRight,
  Hand,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRightClose,
  PanelRightOpen,
  Plus,
  Settings,
} from "lucide-react";
import React, { useEffect, useRef, useSyncExternalStore } from "react";

import type { ConnectionState } from "../daemon-client";
import { PANE_ID } from "../layout/paneOverlay";
import { useNotificationPermission } from "../lifecycle/useNotificationPermission";
import { usePlatform } from "../platform/react";
import type { Session } from "../protocol";
import { useDaemonStore } from "../store";
import type { TerminalProblem } from "../terminal/TerminalPane";
import { FadeOverflow } from "./FadeOverflow";
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

/** The invitation to turn notifications on, where the platform wants the ask to come from a user
 * gesture (a browser) and the answer is still undecided; gone once the answer is given, whichever
 * it is. The dot only draws the eye, the label carries the meaning. The Settings dialog's
 * Notifications section is the other place to see and change the answer.
 *
 * Pressed from the keyboard, the bell holds focus until the answer removes it, and focus would then
 * fall to `<body>`; so once the answer is in, focus that was lost goes to the terminal, or to the
 * bar's first control when there is no terminal to take it. Not before the answer: while the
 * browser's own prompt is up, keys must not reach the agent. */
function NotificationsBell({ focusTerminal }: { focusTerminal: () => void }): React.ReactElement | null {
  const { status, request } = useNotificationPermission();
  // Whether the bell held focus last. A focused element that is removed fires no blur, so this is
  // still true when the answer removes the bell from under focus; a blur of the whole window (the
  // browser's own prompt taking focus) leaves the bell the active element and keeps it too. Focus
  // moved by script while the window is unfocused fires nothing either, so the hand-off also needs
  // focus to have actually fallen to `<body>`.
  const holdsFocus = useRef(false);
  useEffect(() => {
    if (status === "undecided" || !holdsFocus.current) return;
    holdsFocus.current = false;
    if (document.activeElement !== document.body) return;
    focusTerminal();
    if (document.activeElement === document.body) {
      document.querySelector<HTMLElement>("[data-region=topbar] button")?.focus();
    }
  }, [status, focusTerminal]);
  if (!request || status !== "undecided") return null;
  return (
    <span
      className="contents"
      onFocus={() => (holdsFocus.current = true)}
      onBlur={(event) => {
        if (document.activeElement !== event.target) holdsFocus.current = false;
      }}
    >
      <BarButton label="Turn on notifications" onPress={() => void request()}>
        <Bell aria-hidden="true" className="size-4" />
        <span aria-hidden="true" className="absolute top-1.5 right-1.5 size-2 rounded-full bg-accent" />
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
  return (
    <header
      data-escape-scope
      data-region="topbar"
      data-tauri-drag-region={windowChrome ? "deep" : undefined}
      className="flex h-(--title-bar-height) shrink-0 items-stretch border-b border-separator bg-surface select-none"
    >
      {children}
    </header>
  );
}

const noSubscribe = () => () => {};
const noInset = () => 0;

/** The inset the window controls cover at the leading edge, which changes as they come and go
 * (fullscreen); nothing without them. */
function LeadingInset(): React.ReactElement | null {
  const { windowChrome } = usePlatform();
  const inset = useSyncExternalStore(windowChrome?.subscribe ?? noSubscribe, windowChrome?.leadingInset ?? noInset);
  return windowChrome ? <div className="shrink-0" style={{ width: inset }} /> : null;
}

/** The bar with nothing in it, for the screens that have no session UI yet (starting up, failed to
 * start): it still keeps the window's controls clear and makes the window draggable. */
export function BareTitleBar(): React.ReactElement {
  return (
    <BarFrame>
      <LeadingInset />
    </BarFrame>
  );
}

const DAEMON_PROBLEM: Record<Exclude<ConnectionState, "open">, { label: string; color: "warning" | "danger" }> = {
  connecting: { label: "Connecting…", color: "warning" },
  reconnecting: { label: "Reconnecting…", color: "warning" },
  closed: { label: "Disconnected", color: "danger" },
};

const TERMINAL_PROBLEM: Record<TerminalProblem["state"], { label: string; color: "warning" | "danger" }> = {
  reconnecting: { label: "Terminal reconnecting…", color: "warning" },
  disconnected: { label: "Terminal disconnected", color: "danger" },
};

/** The one indicator of connection trouble, empty while everything is healthy: the daemon's own
 * state first, since the terminal's socket cannot be better than the daemon behind it, then the
 * selected session's terminal. The daemon's Retry lives on `ConnectionBanner`; the terminal's
 * Reconnect is here, once automatic attempts are spent. The terminal's state is the only thing the
 * status region announces, and it is always mounted, as a live region has to exist before its
 * content changes to be announced; the daemon's state is announced by `ConnectionBanner`, so its
 * chip sits outside the region and is not read out a second time. */
function ConnectionStatus({ terminalProblem }: { terminalProblem?: TerminalProblem }): React.ReactElement {
  const daemonState = useDaemonStore((s) => s.connectionState);
  const daemonProblem = daemonState !== "open" ? DAEMON_PROBLEM[daemonState] : undefined;
  const problem = daemonProblem ? undefined : terminalProblem && TERMINAL_PROBLEM[terminalProblem.state];
  const reconnect =
    daemonState === "open" && terminalProblem?.state === "disconnected" ? terminalProblem.reconnect : undefined;
  return (
    <>
      {daemonProblem && (
        <Chip size="sm" variant="soft" color={daemonProblem.color}>
          {daemonProblem.label}
        </Chip>
      )}
      <span role="status" className="flex items-center">
        {problem && (
          <Chip size="sm" variant="soft" color={problem.color}>
            {problem.label}
          </Chip>
        )}
      </span>
      {reconnect && (
        <BarButton isIconOnly={false} label="Reconnect terminal" onPress={reconnect}>
          Reconnect
        </BarButton>
      )}
    </>
  );
}

/** Console › Project › session title, with the session's status icon beside it; a hub session
 * has no project and is the console's "Hub" as in the tree. The trail fades at the right edge when
 * too long, the icon always stays. */
function Breadcrumb({ session }: { session: Session }): React.ReactElement {
  const consoleName = useDaemonStore((s) => s.consoles.get(session.console_id)?.name);
  const projectName = useDaemonStore((s) => (session.project_id ? s.projects.get(session.project_id)?.name : undefined));
  const trail = session.role === "hub" ? [consoleName, "Hub"] : [consoleName, projectName, session.title];
  const names = trail.map((part) => part ?? "…");
  return (
    <div className="flex min-w-0 items-center gap-1.5 text-sm">
      <FadeOverflow className="min-w-0" titleWhenClipped={names.join(" › ")}>
        <span className="inline-flex items-center gap-1.5">
          {names.map((name, index) => (
            <React.Fragment key={index}>
              {index > 0 && <ChevronRight aria-hidden="true" className="size-3.5 shrink-0 text-muted" />}
              <span className={index === names.length - 1 ? "font-medium" : "text-muted"}>{name}</span>
            </React.Fragment>
          ))}
        </span>
      </FadeOverflow>
      <StatusIcon status={session.status} />
    </div>
  );
}

interface TitleBarProps {
  onOpenSettings: () => void;
  /** The width of the docked sidebar when it is shown, which the left segment then matches so the
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
  /** What is wrong with the selected session's terminal connection, if anything. */
  terminalProblem?: TerminalProblem;
  waitingCount: number;
  onNextWaiting: () => void;
  /** Only a hub session has a report panel, so the toggle exists only for one. */
  hasReportPanel: boolean;
  /** Whether the report panel is shown, docked or as an open drawer; hidden but floating in on
   * hover does not count, as with the sidebar. */
  reportShown: boolean;
  onToggleReport: () => void;
  /** The pointer is on the report toggle: with the docked report panel hidden, that floats it in. */
  onReportToggleEnter: () => void;
  onReportToggleLeave: () => void;
  /** Where focus goes when a control that held it goes away (the notifications bell). */
  focusTerminal: () => void;
}

/**
 * The bar across the top of the window: a left segment aligned with the sidebar (sidebar toggle,
 * New console), the selected session's breadcrumb, and on the right the waiting count, the
 * connection trouble indicator (nothing while healthy), the notifications bell (browser only, while
 * the permission is undecided), the report panel toggle (hub session only) and Settings.
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
  return (
    <BarFrame>
      {/* Above the `docked` breakpoint with the sidebar shown, exactly the sidebar's width and its
          edge line, so the two read as one column; otherwise just as wide as its controls. */}
      <div
        className={`flex shrink-0 items-center ${sidebarWidth !== undefined ? "docked:w-(--bar-left-width) docked:border-r docked:border-separator" : ""}`}
        style={
          sidebarWidth !== undefined ? ({ "--bar-left-width": `${sidebarWidth}px` } as React.CSSProperties) : undefined
        }
      >
        <LeadingInset />
        <div className="flex items-center gap-1 px-2">
          <BarButton
            label={sidebarShown ? "Hide sessions" : "Show sessions"}
            onPress={onToggleSidebar}
            expanded={sidebarShown}
            controls={PANE_ID.sidebar}
            onMouseHoverChange={(hovered) => (hovered ? onSidebarToggleEnter() : onSidebarToggleLeave())}
          >
            {sidebarShown ? (
              <PanelLeftClose aria-hidden="true" className="size-4" />
            ) : (
              <PanelLeftOpen aria-hidden="true" className="size-4" />
            )}
          </BarButton>
          <BarButton label="New console" onPress={onNewConsole}>
            <Plus aria-hidden="true" className="size-4" />
          </BarButton>
        </div>
      </div>
      <div className="flex min-w-0 flex-1 items-center px-3">
        {selectedSession && <Breadcrumb session={selectedSession} />}
      </div>
      <div className="flex shrink-0 items-center gap-1 px-2">
        {waitingCount > 0 && (
          <BarButton
            isIconOnly={false}
            label={`${waitingCount} ${waitingCount === 1 ? "session is" : "sessions are"} waiting for you. Go to the next one`}
            onPress={onNextWaiting}
          >
            <Hand aria-hidden="true" className="size-4 text-warning" />
            {waitingCount}
          </BarButton>
        )}
        <ConnectionStatus terminalProblem={terminalProblem} />
        <NotificationsBell focusTerminal={focusTerminal} />
        {hasReportPanel && (
          <BarButton
            label={reportShown ? "Hide report" : "Show report"}
            onPress={onToggleReport}
            expanded={reportShown}
            controls={PANE_ID.report}
            onMouseHoverChange={(hovered) => (hovered ? onReportToggleEnter() : onReportToggleLeave())}
          >
            {reportShown ? (
              <PanelRightClose aria-hidden="true" className="size-4" />
            ) : (
              <PanelRightOpen aria-hidden="true" className="size-4" />
            )}
          </BarButton>
        )}
        <BarButton label="Settings" onPress={onOpenSettings}>
          <Settings aria-hidden="true" className="size-4" />
        </BarButton>
      </div>
    </BarFrame>
  );
}
