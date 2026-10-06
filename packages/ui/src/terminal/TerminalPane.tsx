import { Button, Chip } from "@heroui/react";
import React, { useCallback, useEffect, useRef, useState } from "react";

import "@xterm/xterm/css/xterm.css";
import { isDormant as isDormantStatus, isLive, type Session } from "../protocol";
import { useDaemon } from "../store";
import { useOctoboardTheme } from "../theme";
import { TerminalController, type TermStatus } from "./TerminalController";
import { XTERM_THEMES } from "./xtermThemes";

interface TerminalPaneProps {
  /** The session whose terminal should be shown, or `undefined` when nothing is selected yet. */
  session?: Session;
  onResume: (sessionId: string) => void;
  /** Whether the sidebar drawer is currently open, for the leading toggle shown only below the
   * `docked` breakpoint (`App.tsx` holds the state; both overlays live there). */
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
  /** Whether a report panel exists at all right now — only a hub session has one, so its toggle
   * is meaningless (and hidden) without one. */
  hasReportPanel: boolean;
  reportOpen: boolean;
  onToggleReport: () => void;
}

/** Reconnect backoff after a dropped-but-still-live session's socket closes: doubles each attempt,
 * capped, and stops after `MAX_AUTO_RECONNECT_ATTEMPTS` so a session that keeps failing to connect
 * does not retry forever — a manual "Reconnect" button covers that case instead. The attempt
 * counter only resets once a connection has *stayed* open for `STABLE_CONNECTION_MS` — resetting it
 * on every `open` would mean a connection that opens and drops again within a few seconds (a
 * backpressure drop recurs roughly that often under sustained output) always sees attempt 0, so the
 * delay would never grow past the base value and the cap would never engage. */
const MAX_AUTO_RECONNECT_ATTEMPTS = 5;
const BASE_RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_DELAY_MS = 8000;
const STABLE_CONNECTION_MS = 10000;

/**
 * React boundary around `TerminalController`: one controller instance for the life of the
 * component (not per session — see the controller's own doc comment for why session/socket/status
 * /focus must change together), mounted into a plain `div` it owns directly rather than through
 * React's own DOM diffing, since xterm.js manages that subtree itself.
 */
export function TerminalPane({
  session,
  onResume,
  sidebarOpen,
  onToggleSidebar,
  hasReportPanel,
  reportOpen,
  onToggleReport,
}: TerminalPaneProps): React.ReactElement {
  const { terminalUrl } = useDaemon();
  const { resolved: colorTheme } = useOctoboardTheme();
  const containerRef = useRef<HTMLDivElement>(null);
  const controllerRef = useRef<TerminalController | undefined>(undefined);
  const [status, setStatus] = useState<TermStatus>("closed");
  const reconnectAttemptsRef = useRef(0);
  const stableTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // What the controller's theme currently is: the construction below reads it for the initial
  // value, and the `colorTheme` effect further down both reads and updates it. One ref rather than
  // two so the two can never claim different things about the same instance.
  const appliedColorThemeRef = useRef(colorTheme);

  const handleStatusChange = useCallback((s: TermStatus) => {
    if (stableTimerRef.current) {
      clearTimeout(stableTimerRef.current);
      stableTimerRef.current = undefined;
    }
    if (s === "open") {
      stableTimerRef.current = setTimeout(() => {
        reconnectAttemptsRef.current = 0;
      }, STABLE_CONNECTION_MS);
    }
    setStatus(s);
  }, []);

  useEffect(() => {
    const controller = new TerminalController({ onStatusChange: handleStatusChange }, appliedColorThemeRef.current);
    controllerRef.current = controller;
    if (containerRef.current) controller.mount(containerRef.current);

    const resizeObserver = new ResizeObserver(() => {
      controller.fit();
      controller.resize(controller.term.cols, controller.term.rows);
    });
    if (containerRef.current) resizeObserver.observe(containerRef.current);

    return () => {
      if (stableTimerRef.current) clearTimeout(stableTimerRef.current);
      resizeObserver.disconnect();
      controller.dispose();
    };
    // `handleStatusChange` is stable (its own deps are empty); this effect is mount/unmount-only.
  }, []);

  // Recolours the running terminal in place rather than recreating it — the instance above is
  // mount-only, per its own comment, and `setColorTheme` is exactly the escape hatch for a change
  // that must still reach it. Skipped when `colorTheme` already matches what the controller has:
  // without this check, the constructor's own initial theme and this effect's first run (which
  // land in the same task as `mount()`'s `term.open()`) would apply the identical colours twice,
  // the second time via `setColorTheme`, which drives a theme-service and renderer refresh — the
  // same class of operation `TerminalController.mount()` defers `fit`/`reset` around because xterm
  // throws from its own scheduled work when they run in that task.
  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller || colorTheme === appliedColorThemeRef.current) return;
    appliedColorThemeRef.current = colorTheme;
    controller.setColorTheme(colorTheme);
  }, [colorTheme]);

  const isDormant = session ? isDormantStatus(session.status) : false;
  // Whether the session's socket should currently be up at all, per the session's own stored
  // status rather than the controller's last-known socket outcome — a reattach can land on
  // `not_running` by losing a race against the session genuinely ending, so only the authoritative
  // `Session` record decides between offering Resume (truly dormant) and Reconnect (still live,
  // just not connected right now).
  const sessionIsLive = session ? isLive(session.status) : false;

  const attachCurrent = useCallback(
    (userInitiated: boolean) => {
      const controller = controllerRef.current;
      if (!controller || !session) return;
      controller.attach(session.id, terminalUrl(session.id), userInitiated);
    },
    // Keyed on the id alone, so only `session.id` may be read in here: anything else would go
    // stale, since a status change does not produce a new callback.
    [session?.id, terminalUrl],
  );

  useEffect(() => {
    reconnectAttemptsRef.current = 0;
  }, [session?.id]);

  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller) return;
    // A dormant session is resumed by the caller before it is ever attached to directly — the
    // daemon closes the terminal socket immediately for a session with no running process
    // (`daemon/PROTOCOL.md`), so connecting here would just bounce. Once the resume's
    // `session_upserted` broadcast flips the status, this effect re-runs and attaches for real.
    if (!session || isDormant) {
      // Keeps the ended session's last output on screen; `detach` itself checks that the output
      // belongs to this session, so selecting a different dormant session clears as before.
      controller.detach({ keepScreenFor: session?.id });
      return;
    }
    // Only skip when a connection to this exact session is already in flight or open — any other
    // status for this same session id means selecting it (again) should attach for real, not no-op.
    const alreadyAttached =
      controller.currentSessionId === session.id &&
      (controller.currentStatus === "connecting" || controller.currentStatus === "open");
    if (alreadyAttached) return;
    attachCurrent(true);
  }, [session, isDormant, attachCurrent]);

  // Reconnects a session that is still live (per its own `Session` record) after its socket landed
  // on `closed` or `not_running` — the daemon drops a client that stops draining for a few seconds
  // (an occluded window, a large output burst) and documents that the client is expected to
  // reconnect and get the ring buffer replayed, not treat the drop as the session having ended; and
  // a reattach that itself fails to connect (`not_running`) needs the same retry, not a dead end.
  // Never user-initiated: a background heal must not pull focus out of whatever the user is
  // typing into.
  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller || !session) return;
    if (isDormant || !sessionIsLive) return;
    if (status !== "closed" && status !== "not_running") return;
    if (controller.currentSessionId !== session.id) return;
    if (reconnectAttemptsRef.current >= MAX_AUTO_RECONNECT_ATTEMPTS) return;
    const delay = Math.min(BASE_RECONNECT_DELAY_MS * 2 ** reconnectAttemptsRef.current, MAX_RECONNECT_DELAY_MS);
    const timer = setTimeout(() => {
      reconnectAttemptsRef.current += 1;
      attachCurrent(false);
    }, delay);
    return () => clearTimeout(timer);
    // Deliberately keyed on the session's id rather than its record: a status broadcast arriving
    // for this session would otherwise tear the pending timer down and start it over, so under a
    // working agent the reconnect could be postponed indefinitely.
  }, [status, session?.id, isDormant, sessionIsLive, attachCurrent]);

  const reconnectNow = () => {
    reconnectAttemptsRef.current = 0;
    attachCurrent(true);
  };

  // Whether to offer each affordance is keyed off the session's own status, not the controller's
  // raw socket outcome — see `sessionIsLive`'s comment above for why `not_running` cannot mean
  // "offer Resume" on its own.
  const offerResume = session && isDormant;
  const offerReconnect = session && !isDormant && sessionIsLive && (status === "closed" || status === "not_running");

  return (
    // The background must match the active xterm theme's (`XTERM_THEMES`, read here rather than
    // duplicated as a literal), so the padding around the terminal is not a different colour —
    // a plain Tailwind class cannot do this since the colour now changes at runtime with the
    // theme, not at build time. Above the `docked` breakpoint the 520px basis and floor are the
    // report panel's counterpart: with a 0 basis free space stays positive at any window wider
    // than the panel's own basis, flexbox never leaves the grow phase, and the panel's shrink
    // factor is never consulted. 520px is 55 columns at ~9.2px/column off a real agent CLI (the
    // container's padding eats the rest). Below the breakpoint the sidebar and the report panel
    // are overlays rather than row siblings (`App.tsx`), so this pane is the row's only content
    // and takes a much smaller floor instead: 382px is 40 columns at the same ~9.2px/column plus
    // the same padding allowance, under which the terminal stops being usable at all, so
    // `overflow-x-auto` on the row (`App.tsx`) scrolls rather than squeezing it further.
    //
    // `data-escape-scope`: one of the origins `App.tsx`'s capture-phase Escape listener closes a
    // drawer for — xterm.js holds keyboard focus here most of the time and would otherwise
    // swallow the key before that listener's own bubble-phase alternative ever saw it.
    <div
      data-escape-scope
      className="relative flex min-h-0 min-w-[382px] flex-[1_1_382px] flex-col docked:min-w-[520px] docked:flex-[1_1_520px]"
      style={{ backgroundColor: XTERM_THEMES[colorTheme].background }}
    >
      {/* `z-[35]` clears the narrow-mode scrim (`Scrim.tsx`, `z-30`) so this header's own toggle
          stays pressable while a drawer dims the screen — not the drawers themselves (`z-40` in
          `Sidebar.tsx`/`ReportPanel.tsx`): both now start below this header rather than at the
          viewport top, so they no longer overlap it at all. */}
      <div className="relative z-[35] flex h-(--pane-header-height) shrink-0 items-center gap-2 border-b border-separator bg-surface px-3">
        {/* Shown only below the `docked` breakpoint, via the variant rather than a width check in
            JS, so there is no hydration/resize flicker. Above it the sidebar and the report panel
            already sit in the row, and these would be redundant. */}
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          aria-label={sidebarOpen ? "Hide sessions" : "Show sessions"}
          preventFocusOnPress
          className="docked:hidden"
          onPress={onToggleSidebar}
        >
          ☰
        </Button>
        {/* With nothing selected there is no connection to have a status: the pane's own
            placeholder says what to do, and a red "Disconnected" next to it reads as a fault. */}
        {session && (
          <Chip size="sm" variant="soft" color={isDormant ? "default" : STATUS_COLOR[status]}>
            {formatStatus(status, isDormant)}
          </Chip>
        )}
        {offerReconnect && (
          <Button size="sm" variant="secondary" onPress={reconnectNow}>
            Reconnect
          </Button>
        )}
        {offerResume && (
          <Button size="sm" variant="primary" onPress={() => onResume(session.id)}>
            Resume
          </Button>
        )}
        {hasReportPanel && (
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            aria-label={reportOpen ? "Hide report" : "Show report"}
            preventFocusOnPress
            className="ml-auto docked:hidden"
            onPress={onToggleReport}
          >
            ▤
          </Button>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-hidden p-1" ref={containerRef} />
      {!session && (
        <div className="absolute inset-x-0 bottom-0 top-(--pane-header-height) flex items-center justify-center bg-background text-muted">
          Select a session to view its terminal.
        </div>
      )}
    </div>
  );
}

const STATUS_COLOR: Record<TermStatus, "default" | "success" | "warning" | "danger"> = {
  connecting: "warning",
  open: "success",
  closed: "danger",
  not_running: "danger",
};

function formatStatus(status: TermStatus, isDormant?: boolean): string {
  if (isDormant) return "Not running";
  switch (status) {
    case "connecting":
      return "Connecting…";
    case "open":
      return "Connected";
    case "closed":
      return "Disconnected";
    case "not_running":
      // Reached here only for a session the `Session` record still calls live (see
      // `sessionIsLive`) — a reattach simply failed to connect this time, not proof the process is
      // gone, so it reads the same as a plain drop rather than echoing the dormant wording below.
      return "Disconnected";
  }
}
