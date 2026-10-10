import { ArrowLeft, ArrowRight, ChevronRight, PanelLeft, PanelLeftDashed } from "lucide-react";
import React, { useSyncExternalStore } from "react";

import { PANE_ID } from "../layout/paneOverlay";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import type { Session } from "../protocol";
import { useDaemonStore } from "../store";
import { ChromeButton } from "./ChromeButton";
import { FadeOverflow } from "./FadeOverflow";
import { StatusIcon } from "./StatusIcon";

/** The bar's frame: full window width, above everything, painting nothing of its own (the window's
 * background shows, `--window-background` in `style.css`), and the window's drag handle where the
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
      className="flex h-(--title-bar-height) shrink-0 items-stretch select-none rtl:pl-(--window-controls-inset)"
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

/** Console › Project › session title, with the session's status icon; a console session has no
 * project, so its trail is just the console and its own title. The trail fades at its end edge when
 * too long, the icon always stays. */
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
 * follows it (the status icon) always stays. It sits on the window chrome, so the names before the
 * last take `--chrome-muted`, not the panel's `--muted` (`style.css`). */
function Trail({ names, children }: { names: string[]; children?: React.ReactNode }): React.ReactElement {
  return (
    <div className="flex min-w-0 items-center gap-1.5 text-sm">
      <FadeOverflow className="min-w-0" titleWhenClipped={names.join(" › ")}>
        <span className="inline-flex items-center gap-1.5">
          {names.map((name, index) => (
            <React.Fragment key={index}>
              {index > 0 && <ChevronRight aria-hidden="true" className="size-3.5 shrink-0 text-chrome-muted rtl:-scale-x-100" />}
              <span dir="auto" className={index === names.length - 1 ? "font-medium" : "text-chrome-muted"}>
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
  /** Whether the sidebar is currently shown, docked or as an open drawer; a hidden sidebar that is
   * only floating in on hover (from a console's avatar on the rail) does not count, since pressing
   * the toggle docks it. */
  sidebarShown: boolean;
  onToggleSidebar: () => void;
  canGoBack: boolean;
  canGoForward: boolean;
  onBack: () => void;
  onForward: () => void;
  selectedSession?: Session;
  /** What the content area shows instead of the selected session's terminal (the archive view),
   * as the breadcrumb's names; it then takes the session's place in the bar. */
  viewTrail?: string[];
}

/**
 * The bar across the top of the window, on the window chrome like the rail below it: Back and
 * Forward, the sidebar toggle, and the selected session's breadcrumb with its status icon. Nothing
 * sits at its end but the window's drag area: the other window controls are on the rail
 * (`Rail.tsx`). Above the `docked` breakpoint, the start segment tracks the docked sidebar's clip
 * (`.title-bar-start`), so the breadcrumb stays over the main area while that column eases open or
 * closed.
 */
export function TitleBar({
  sidebarShown,
  onToggleSidebar,
  canGoBack,
  canGoForward,
  onBack,
  onForward,
  selectedSession,
  viewTrail,
}: TitleBarProps): React.ReactElement {
  const t = useT();
  return (
    <BarFrame>
      {/* Above the `docked` breakpoint, as wide as the rail plus the sidebar's clip, and never
          narrower than these buttons. The breadcrumb after it stays over the main area while the
          column eases; below the breakpoint the class does not apply. */}
      <div className="title-bar-start flex shrink-0 items-center">
        <LeftInset />
        <div className="flex items-center gap-1 px-2">
          <ChromeButton label={t("titleBar.back")} onPress={onBack} isDisabled={!canGoBack}>
            <ArrowLeft aria-hidden="true" className="size-4 rtl:-scale-x-100" />
          </ChromeButton>
          <ChromeButton label={t("titleBar.forward")} onPress={onForward} isDisabled={!canGoForward}>
            <ArrowRight aria-hidden="true" className="size-4 rtl:-scale-x-100" />
          </ChromeButton>
          <ChromeButton
            label={sidebarShown ? t("titleBar.sidebar.hide") : t("titleBar.sidebar.show")}
            onPress={onToggleSidebar}
            expanded={sidebarShown}
            controls={PANE_ID.sidebar}
          >
            {sidebarShown ? (
              <PanelLeft aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            ) : (
              <PanelLeftDashed aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            )}
          </ChromeButton>
        </div>
      </div>
      <div className="flex min-w-0 flex-1 items-center px-3">
        {viewTrail ? <Trail names={viewTrail} /> : selectedSession && <Breadcrumb session={selectedSession} />}
      </div>
    </BarFrame>
  );
}
