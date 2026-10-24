import { CircleHelp } from "lucide-react";
import React from "react";

import { Button } from "./Button";
import { Hint } from "./Hint";

/** A small muted "?" that carries an explanation, to take a line of explanatory text out of a row. It is an
 * icon-only control, so it is a tab stop (see "The help tip" in `docs/product/labels-and-tooltips.md`), and it
 * opens the explanation in a popover (`Hint` of kind `popover`), as a status mark does, so the explanation can
 * be selected and copied with the mouse:
 *
 * - A mouse resting on it opens the popover without moving focus, and it stays open while the pointer
 *   moves into it; a mouse click opens nothing.
 * - A touch or pen tap, the keyboard (Enter, Space) and assistive technology open it with focus inside.
 *
 * `label` is the control's accessible name and has to say what it explains; it also names the popover. The
 * explanation is text, or rich content such as a `<Message>` with literals in chips. */
export function HelpTip({ label, children }: { label: string; children: React.ReactNode }): React.ReactElement {
  return (
    <Hint
      kind="popover"
      label={label}
      trigger={({ onMouseHoverChange: _, expanded: __, popupId: ___, ...wiring }) => (
        <Button {...wiring} isIconOnly size="xs" variant="ghost" aria-label={label} className="shrink-0 -my-0.5 rounded-full text-app-muted">
          <CircleHelp className="size-3.5" aria-hidden />
        </Button>
      )}
    >
      {() => children}
    </Hint>
  );
}
