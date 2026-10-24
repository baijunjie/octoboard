import { CircleHelp } from "lucide-react";
import React from "react";

import { Button } from "./Button";
import { TitledControl } from "./TitledControl";

/** A small muted "?" that carries an explanation, to take a line of explanatory text out of a row. It is an
 * icon-only control, so it is a tab stop (see "Tooltips on icon-only controls" in
 * `docs/product/labels-and-tooltips.md`), and it is a `TitledControl` whose text is the explanation:
 *
 * - Resting a mouse on it, or focusing it from the keyboard, shows the explanation in a tooltip, which
 *   assistive technology is given as the control's description while it is open.
 * - A touch or pen press opens the same explanation in a popover drawn like the tooltip (`touchPopover`), which
 *   a touch screen needs as it has no hover. An outside tap or Escape closes it, and focus goes back to the "?".
 *   A mouse press opens nothing: the tooltip is what a mouse has.
 *
 * `label` is the control's accessible name and has to say what it explains. The explanation is text, or rich
 * content such as a `<Message>` with literals in chips; the tooltip and the popover show the same content. */
export function HelpTip({ label, children }: { label: string; children: React.ReactNode }): React.ReactElement {
  return (
    <TitledControl title={children} touchPopover>
      <Button isIconOnly size="xs" variant="ghost" aria-label={label} className="shrink-0 -my-0.5 rounded-full text-app-muted">
        <CircleHelp className="size-3.5" aria-hidden />
      </Button>
    </TitledControl>
  );
}
