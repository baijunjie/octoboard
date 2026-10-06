import { Button } from "@heroui/react";
import { ChevronRight, Hand, PanelLeft, PanelRight, Plus, Settings } from "lucide-react";
import React, { useSyncExternalStore } from "react";

import type { ConnectionState } from "../daemon-client";
import { usePlatform } from "../platform/react";
import type { Session } from "../protocol";
import { useDaemonStore } from "../store";
import { FadeOverflow } from "./FadeOverflow";
import { StatusIcon } from "./StatusIcon";
import { TitledControl } from "./TitledControl";

/** A press on any of the bar's controls must leave keyboard focus on the terminal, so every one of
 * them is built from this. `TitledControl` gives it the native tooltip HeroUI's buttons drop. */
function BarButton({
  label,
  onPress,
  children,
  isIconOnly = true,
}: {
  label: string;
  onPress: () => void;
  children: React.ReactNode;
  isIconOnly?: boolean;
}): React.ReactElement {
  return (
    <TitledControl title={label}>
      <Button isIconOnly={isIconOnly} size="sm" variant="ghost" aria-label={label} preventFocusOnPress onPress={onPress}>
        {children}
      </Button>
    </TitledControl>
  );
}

/** The bar's frame: full window width, above everything, and the window's drag handle where the
 * platform has no native titlebar. `deep` makes every non-interactive descendant draggable, and
 * Tauri's drag script already leaves buttons alone; a double-click zooms the window like a native
 * titlebar. `data-escape-scope`: Escape on one of its controls closes an open drawer too. */
function BarFrame({ children }: { children: React.ReactNode }): React.ReactElement {
  const { windowChrome } = usePlatform();
  return (
    <header
      data-escape-scope
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

const CONNECTION_DOT: Record<ConnectionState, { label: string; className: string }> = {
  open: { label: "Connected", className: "bg-success" },
  connecting: { label: "Connecting", className: "bg-warning" },
  reconnecting: { label: "Reconnecting", className: "bg-warning" },
  closed: { label: "Disconnected", className: "bg-danger" },
};

function ConnectionDot(): React.ReactElement {
  const state = useDaemonStore((s) => s.connectionState);
  const { label, className } = CONNECTION_DOT[state];
  return (
    <TitledControl title={`Daemon: ${label}`}>
      <span role="img" aria-label={`Daemon: ${label}`} className="flex size-8 items-center justify-center">
        <span className={`size-2 rounded-full ${className}`} />
      </span>
    </TitledControl>
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
      <FadeOverflow axis="x" className="min-w-0" titleWhenClipped={names.join(" › ")}>
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
  /** Whether the sidebar is currently shown, docked or as an open drawer. */
  sidebarShown: boolean;
  onToggleSidebar: () => void;
  onNewConsole: () => void;
  selectedSession?: Session;
  waitingCount: number;
  onNextWaiting: () => void;
  /** Only a hub session has a report panel, so the toggle exists only for one. */
  hasReportPanel: boolean;
  reportShown: boolean;
  onToggleReport: () => void;
}

/**
 * The bar across the top of the window: a left segment aligned with the sidebar (sidebar toggle,
 * New console), the selected session's breadcrumb, and on the right the waiting count, the daemon
 * connection, the report panel toggle (hub session only) and Settings.
 */
export function TitleBar({
  onOpenSettings,
  sidebarWidth,
  sidebarShown,
  onToggleSidebar,
  onNewConsole,
  selectedSession,
  waitingCount,
  onNextWaiting,
  hasReportPanel,
  reportShown,
  onToggleReport,
}: TitleBarProps): React.ReactElement {
  return (
    <BarFrame>
      {/* Above the `docked` breakpoint with the sidebar shown, exactly the sidebar's width and its
          edge line, so the two read as one column; otherwise just as wide as its controls. */}
      <div
        className={`flex shrink-0 items-center ${sidebarWidth !== undefined ? "docked:w-(--bar-left-width) docked:border-r docked:border-separator" : ""}`}
        style={sidebarWidth !== undefined ? ({ "--bar-left-width": `${sidebarWidth}px` } as React.CSSProperties) : undefined}
      >
        <LeadingInset />
        <div className="flex items-center gap-1 px-2">
          <BarButton label={sidebarShown ? "Hide sessions" : "Show sessions"} onPress={onToggleSidebar}>
            <PanelLeft aria-hidden="true" className="size-4" />
          </BarButton>
          <BarButton label="New console" onPress={onNewConsole}>
            <Plus aria-hidden="true" className="size-4" />
          </BarButton>
        </div>
      </div>
      <div className="flex min-w-0 flex-1 items-center px-3">{selectedSession && <Breadcrumb session={selectedSession} />}</div>
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
        <ConnectionDot />
        {hasReportPanel && (
          <BarButton label={reportShown ? "Hide report" : "Show report"} onPress={onToggleReport}>
            <PanelRight aria-hidden="true" className="size-4" />
          </BarButton>
        )}
        <BarButton label="Settings" onPress={onOpenSettings}>
          <Settings aria-hidden="true" className="size-4" />
        </BarButton>
      </div>
    </BarFrame>
  );
}
