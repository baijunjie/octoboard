import { TriangleAlert } from "lucide-react";
import React from "react";

import { Button, Surface } from "../primitives";

/** The one button that deals with what the bar says. */
interface BarAction {
  action: string;
  /** The button's accessible name where it says more than `action` (which it must contain, for speech input). */
  actionLabel?: string;
  variant: "primary" | "secondary";
  onPress: () => void;
  controls?: never;
}

/** The controls the bar ends with in place of its one button, each a compact (`xs`) control that takes
 * `focusOnPress={false}`. */
interface BarControls {
  controls: React.ReactNode;
  action?: never;
  actionLabel?: never;
  variant?: never;
  onPress?: never;
}

/**
 * The bar that floats over a terminal and says something about it, with what deals with it. It does not cover the
 * terminal: the output stays on screen. Its plain kind says a terminal's process has ended, the last output being what
 * says why, or that something is under way (the uploads of dropped files); its `warning` kind says the session runs
 * without something it would have (a standing notice), and leads with the warning triangle so that kind is not told by
 * colour alone. The sentence is hidden from assistive technology (the caller speaks it from a `StatusAnnouncer`, so
 * nothing is said twice) and the controls stay reachable.
 */
export function TerminalNoticeBar({
  ref,
  sentence,
  kind = "plain",
  compact = false,
  ...end
}: {
  ref?: React.Ref<HTMLDivElement>;
  sentence: React.ReactNode;
  kind?: "plain" | "warning";
  /** One line however narrow the terminal: the bar takes at most the terminal's width less its margins, the sentence may
   * hold a part that clips (`FadeOverflow`) and the button is the extra-small one. Off, the bar is as it always was. */
  compact?: boolean;
} & (BarAction | BarControls)): React.ReactElement {
  const actions = end.controls !== undefined
    ? <span className="flex shrink-0 items-center gap-1">{end.controls}</span>
    : compact
      ? (
        <span className="shrink-0">
          <Button size="xs" variant={end.variant} label={end.actionLabel} focusOnPress={false} onPress={end.onPress}>
            {end.action}
          </Button>
        </span>
      )
      : (
        <Button size="sm" variant={end.variant} focusOnPress={false} onPress={end.onPress}>
          {end.action}
        </Button>
      );
  return (
    <Surface ref={ref} className={`floating-surface pointer-events-auto flex items-center gap-3 rounded-lg px-3 py-2 text-sm shadow-lg ${compact ? "max-w-[calc(100%-2rem)]" : ""}`}>
      {kind === "warning" && <TriangleAlert className="size-4 shrink-0 text-warning-soft-foreground" />}
      <span aria-hidden="true" className={compact ? "flex min-w-0 items-center whitespace-pre text-app-muted" : "text-app-muted"}>
        {sentence}
      </span>
      {actions}
    </Surface>
  );
}

/** Where the notice bars float over a terminal: along its bottom edge, or its top where they must keep the last rows clear, centred. The slot stays
 * mounted while the bars come and go, and lets pointer events through to the terminal around them. */
export function TerminalNoticeSlot({ edge = "bottom", children }: { edge?: "top" | "bottom"; children?: React.ReactNode }): React.ReactElement {
  // A column, so that bars sharing the slot (a standing notice above the uploads of dropped files) stack, centred.
  return <div className={`pointer-events-none absolute inset-x-0 flex flex-col items-center gap-2 ${edge === "top" ? "top-4" : "bottom-4"}`}>{children}</div>;
}
