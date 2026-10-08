import { Tooltip } from "@heroui/react";
import { Plug, RefreshCw, Unplug } from "lucide-react";
import React from "react";

import type { ConnectionState } from "../daemon-client";
import { useT } from "../i18n/react";
import type { PlainMessageKey } from "../i18n/catalog";
import { useDaemonStore } from "../store";
import type { TerminalProblem } from "../terminal/TerminalPane";
import { ChromeButton } from "./ChromeButton";
import { useFocusHandoff } from "./useFocusHandoff";

const DAEMON_PROBLEM: Record<Exclude<ConnectionState, "open">, { label: PlainMessageKey; color: "warning" | "danger" }> = {
  connecting: { label: "rail.daemon.connecting", color: "warning" },
  reconnecting: { label: "rail.daemon.reconnecting", color: "warning" },
  closed: { label: "rail.daemon.disconnected", color: "danger" },
};

const TERMINAL_PROBLEM: Record<TerminalProblem["state"], { label: PlainMessageKey; color: "warning" | "danger" }> = {
  reconnecting: { label: "rail.terminal.reconnecting", color: "warning" },
  disconnected: { label: "rail.terminal.disconnected", color: "danger" },
};

/** A connection problem as a glyph, its label as accessible name and tooltip: a turning arrow
 * while attempts are under way, an unplugged cord once they are spent, so color is never the only
 * cue. The label is visually hidden text rather than the glyph's own `aria-label`, which a live
 * region may not announce.
 *
 * A deliberate exception to `TitledControl`'s rule against `Tooltip.Trigger`: this is an
 * indicator, not a button, so it has no react-aria-components control to hang the tooltip on,
 * which `TitledControl` needs as its child. The trigger is focusable, which keeps the label
 * reachable from the keyboard, and a mouse press leaves focus where it was like the chrome's buttons. */
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
          className={color === "danger" ? "size-4 text-danger-glyph" : "size-4 text-warning-glyph motion-safe:animate-spin-slow"}
        />
        <span className="sr-only">{label}</span>
      </Tooltip.Trigger>
      <Tooltip.Content placement="end" className="pointer-events-none">{label}</Tooltip.Content>
    </Tooltip>
  );
}

/** The one indicator of connection trouble, empty while everything is healthy: the daemon's own
 * state first, since the terminal's socket cannot be better than the daemon behind it, then the
 * selected session's terminal. The daemon's Retry lives on `ConnectionBanner`; the terminal's
 * Reconnect is here, once automatic attempts are spent, as a button whose label is its tooltip. The
 * terminal's state is the only thing the status region announces, and it is always mounted, as a
 * live region has to exist before its content changes to be announced; the daemon's state is
 * announced by `ConnectionBanner`, so its icon sits outside the region and is not read out a second
 * time. A focused indicator that goes away with the problem hands focus on (`useFocusHandoff`). */
export function ConnectionStatus({
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
        <ChromeButton label={t("rail.terminal.reconnectLabel")} onPress={reconnect} tooltipPlacement="end">
          <Plug aria-hidden="true" className="size-4" />
        </ChromeButton>
      )}
    </span>
  );
}
