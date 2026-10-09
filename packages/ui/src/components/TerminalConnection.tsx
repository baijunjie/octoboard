import { Tooltip } from "@heroui/react";
import { Plug, RefreshCw, Unplug } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import type { TerminalProblem } from "../terminal/TerminalPane";
import { ChromeButton } from "./ChromeButton";

/** The turning arrow while automatic attempts are under way: a glyph with its label as accessible
 * name and tooltip, nothing to press. It is an `img`, as `Tooltip.Trigger` would otherwise make it
 * a `button` (its `role` comes before the props spread over it).
 *
 * A deliberate exception to `TitledControl`'s rule against `Tooltip.Trigger`: this is an
 * indicator, not a button, so it has no react-aria-components control to hang the tooltip on,
 * which `TitledControl` needs as its child. The trigger is focusable, which keeps the label
 * reachable from the keyboard, and a mouse press leaves focus where it was like the chrome's
 * buttons. It is the size of an icon-only small `ChromeButton`, so the two states do not jump. */
function ReconnectingIcon({ label }: { label: string }): React.ReactElement {
  return (
    <Tooltip>
      <Tooltip.Trigger
        role="img"
        aria-label={label}
        className="flex size-9 shrink-0 items-center justify-center rounded-3xl outline-none focus-visible:ring-2 focus-visible:ring-focus md:size-8"
        // As `Scrim` and `PaneResizeHandle`: a press must not focus the trigger and take focus off the terminal.
        onMouseDown={(event) => event.preventDefault()}
      >
        <RefreshCw aria-hidden="true" className="size-4 text-warning-glyph motion-safe:animate-spin-slow" />
      </Tooltip.Trigger>
      <Tooltip.Content className="pointer-events-none">{label}</Tooltip.Content>
    </Tooltip>
  );
}

/** The selected session's terminal connection, shown only while something is wrong with it, at the
 * end of the breadcrumb beside the session's status icon. While automatic attempts run it is a
 * turning arrow; once they are spent it is an unplugged cord, red, that becomes the plug under the
 * pointer, on keyboard focus and while pressed, and reconnects when pressed (the arrow then takes
 * over). The glyphs differ by shape as well as color, so color is never the only cue. The state is
 * announced through a status region that is always mounted, as a live region has to exist before
 * its content changes to be announced. Whoever mounts it hands focus on when it goes away
 * (`useFocusHandoff`). */
export function TerminalConnection({ problem }: { problem?: TerminalProblem }): React.ReactElement {
  const t = useT();
  const reconnecting = problem?.state === "reconnecting" ? t("terminalConnection.reconnecting") : undefined;
  const disconnected = problem?.state === "disconnected" ? t("terminalConnection.disconnected") : undefined;
  return (
    <span className="contents">
      <span role="status" className="sr-only">
        {reconnecting ?? disconnected}
      </span>
      {reconnecting && <ReconnectingIcon label={reconnecting} />}
      {problem?.state === "disconnected" && (
        <ChromeButton label={t("terminalConnection.reconnect")} onPress={problem.reconnect} className="group shrink-0">
          <Unplug
            aria-hidden="true"
            className="size-4 text-danger-glyph group-hover:hidden group-focus-visible:hidden group-data-[pressed=true]:hidden"
          />
          <Plug aria-hidden="true" className="hidden size-4 group-hover:block group-focus-visible:block group-data-[pressed=true]:block" />
        </ChromeButton>
      )}
    </span>
  );
}
