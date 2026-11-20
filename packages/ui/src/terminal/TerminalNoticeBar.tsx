import React from "react";

import { Button, Surface } from "../primitives";

/**
 * The bar that floats over a terminal whose process has ended and says so, with the one button that
 * deals with it. It does not cover the terminal: the last output stays on screen, and that output is
 * what says why. The sentence is hidden from assistive technology (the caller speaks it from a
 * `StatusAnnouncer`, so nothing is said twice) and the button stays reachable.
 */
export function TerminalNoticeBar({
  ref,
  sentence,
  action,
  actionLabel,
  variant,
  compact = false,
  onPress,
}: {
  ref?: React.Ref<HTMLDivElement>;
  sentence: React.ReactNode;
  action: string;
  /** The button's accessible name where it says more than `action` (which it must contain, for speech input). */
  actionLabel?: string;
  variant: "primary" | "secondary";
  /** One line however narrow the terminal: the bar takes at most the terminal's width less its margins, the sentence may
   * hold a part that clips (`FadeOverflow`) and the button is the extra-small one. Off, the bar is as it always was. */
  compact?: boolean;
  onPress: () => void;
}): React.ReactElement {
  return (
    <Surface ref={ref} className={`floating-surface pointer-events-auto flex items-center gap-3 rounded-lg px-3 py-2 text-sm shadow-lg ${compact ? "max-w-[calc(100%-2rem)]" : ""}`}>
      <span aria-hidden="true" className={compact ? "flex min-w-0 items-center whitespace-pre text-app-muted" : "text-app-muted"}>
        {sentence}
      </span>
      {compact
        ? (
          <span className="shrink-0">
            <Button size="xs" variant={variant} label={actionLabel} focusOnPress={false} onPress={onPress}>
              {action}
            </Button>
          </span>
        )
        : (
          <Button size="sm" variant={variant} focusOnPress={false} onPress={onPress}>
            {action}
          </Button>
        )}
    </Surface>
  );
}

/** Where the notice bar floats over a terminal: along its bottom edge, or its top where it must keep the last rows clear, centred. The slot stays mounted
 * while the bar comes and goes, and lets pointer events through to the terminal around the bar. */
export function TerminalNoticeSlot({ edge = "bottom", children }: { edge?: "top" | "bottom"; children?: React.ReactNode }): React.ReactElement {
  return <div className={`pointer-events-none absolute inset-x-0 flex justify-center ${edge === "top" ? "top-4" : "bottom-4"}`}>{children}</div>;
}
