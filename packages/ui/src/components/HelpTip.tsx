import { Button, Popover } from "@heroui/react";
import { CircleHelp } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";
import { useFocusVisible } from "react-aria";

import { TitledControl } from "./TitledControl";

/** A small muted "?" that carries an explanation, to take a line of explanatory text out of a row. It is an
 * icon-only control, so it is a tab stop (see "Tooltips on icon-only controls" in
 * `docs/product/labels-and-tooltips.md`) and works two ways:
 *
 * - Resting on it, or focusing it from the keyboard, shows the explanation in a tooltip, which assistive
 *   technology is given as the control's description while it is open.
 * - Pressing it (a click, a tap, Enter or Space) opens the same explanation in a popover that stays until
 *   an outside press, Escape or another press of the "?" closes it, which is what a touch screen has, as
 *   it has no hover. The popover takes focus and gives it back to the "?" on closing. While it is open the
 *   tooltip is held back, so the two never stack, and it stays held back after the popover closes if it
 *   would otherwise reopen at once, that is when the pointer rests on the "?" or the keyboard is in use (focus
 *   coming back would show it, and a second Escape would then close the dialog around it instead), until focus
 *   or the pointer leaves the "?".
 *
 * `label` is the control's accessible name and has to say what it explains. */
export function HelpTip({ label, children }: { label: string; children: string }): React.ReactElement {
  const [open, setOpen] = useState(false);
  const [justClosed, setJustClosed] = useState(false);
  const { isFocusVisible } = useFocusVisible();
  const [hovered, setHovered] = useState(false);
  const release = () => setJustClosed(false);
  const button = useRef<HTMLButtonElement>(null);
  // Where the pointer last was while the popover was open, which `onHoverEnd` cannot tell: the popover's
  // underlay takes the pointer off the "?" as it opens, and what is under the pointer at closing is the question.
  const pointer = useRef<{ x: number; y: number } | null>(null);
  useEffect(() => {
    if (!open) return;
    const track = (event: PointerEvent) => void (pointer.current = { x: event.clientX, y: event.clientY });
    document.addEventListener("pointermove", track);
    return () => document.removeEventListener("pointermove", track);
  }, [open]);
  return (
    <Popover
      isOpen={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (next) {
          pointer.current = null;
          return;
        }
        // The underlay and the modal around the "?" hide it from hit testing, so where the pointer last was while
        // the popover was open is compared with its box; a pointer that never moved leaves `hovered` as it was.
        // What would reopen the tooltip at once, the pointer on the "?" or keyboard focus on it, is decided at
        // render, since the keyboard modality is not settled yet while an Escape is being handled. A pointer that
        // has left the "?" holds nothing back: its next hover releases the hold (`onHoverStart`).
        let pointerOn = hovered;
        if (pointer.current && button.current) {
          const box = button.current.getBoundingClientRect();
          const { x, y } = pointer.current;
          pointerOn = x >= box.left && x <= box.right && y >= box.top && y <= box.bottom;
        }
        setHovered(pointerOn);
        setJustClosed(true);
      }}
    >
      <TitledControl title={children} isDisabled={open || (justClosed && (hovered || isFocusVisible))}>
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          ref={button}
          aria-label={label}
          className="row-icon-control -my-0.5 rounded-full text-app-muted"
          onBlur={release}
          onHoverStart={() => {
            if (open) return;
            // A hover that starts after the pointer had left is a fresh rest, not the one the popover closed on.
            if (!hovered) release();
            setHovered(true);
          }}
          onHoverEnd={() => {
            // The popover's underlay takes the pointer off the "?" as it opens, so what was true when it opened stands.
            if (open) return;
            setHovered(false);
            release();
          }}
        >
          <CircleHelp className="size-3.5" aria-hidden />
        </Button>
      </TitledControl>
      <Popover.Content placement="top" className="max-w-64">
        <Popover.Dialog aria-label={label} className="p-3 text-sm">{children}</Popover.Dialog>
      </Popover.Content>
    </Popover>
  );
}
