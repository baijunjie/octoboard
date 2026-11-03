import { Surface } from "@heroui/react";
import React from "react";

import { Button } from "../components/controls/Button";

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
  variant,
  onPress,
}: {
  ref?: React.Ref<HTMLDivElement>;
  sentence: string;
  action: string;
  variant: "primary" | "secondary";
  onPress: () => void;
}): React.ReactElement {
  return (
    <Surface ref={ref} className="floating-surface pointer-events-auto flex items-center gap-3 rounded-lg px-3 py-2 text-sm shadow-lg">
      <span aria-hidden="true" className="text-app-muted">
        {sentence}
      </span>
      <Button size="sm" variant={variant} preventFocusOnPress onPress={onPress}>
        {action}
      </Button>
    </Surface>
  );
}

/** Where the notice bar floats over a terminal: along its bottom edge, centred. The slot stays mounted
 * while the bar comes and goes, and lets pointer events through to the terminal around the bar. */
export function TerminalNoticeSlot({ children }: { children?: React.ReactNode }): React.ReactElement {
  return <div className="pointer-events-none absolute inset-x-0 bottom-4 flex justify-center">{children}</div>;
}
