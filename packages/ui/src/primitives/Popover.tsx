import { Popover as HeroPopover } from "@heroui/react";
import React from "react";
import { Dialog as AriaDialog, Popover as AriaPopover } from "react-aria-components";

import { type DataAttributes, definedProps } from "./props";

/** Where a floating element opens relative to what it belongs to: the side, then the alignment along it. The start and
 * end sides follow the reading direction. */
export type OverlayPlacement = "top" | "bottom" | "start" | "end" | "bottom end" | "end bottom";

/** HeroUI's popover around a trigger: `children` are the trigger, a layer `Button` that opens it on a press and says
 * that it does, and its `PopoverContent`. Given `open` the caller holds the state, told of a change by `onOpenChange`. */
export function Popover({ open, onOpenChange, children }: {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  children: React.ReactNode;
}): React.ReactElement {
  return <HeroPopover {...definedProps({ isOpen: open, onOpenChange })}>{children as React.ReactElement}</HeroPopover>;
}

/** The floating part of a `Popover`: HeroUI's popover content holding its dialog, named `label`, which holds
 * `children`. `className` styles the floating element, `dialogClassName` the dialog inside it. */
export function PopoverContent({ placement, label, className, dialogClassName, children, ...data }: DataAttributes & {
  placement?: OverlayPlacement;
  label: string;
  className?: string;
  dialogClassName?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroPopover.Content {...data} {...definedProps({ placement, className })}>
      <HeroPopover.Dialog aria-label={label} {...definedProps({ className: dialogClassName })}>
        {children as React.ReactElement}
      </HeroPopover.Dialog>
    </HeroPopover.Content>
  );
}

/** React Aria's bare popover, with none of HeroUI's look: an element floating against `anchor`, which the caller opens
 * and draws (`className`). It is modal unless `nonModal`: a press outside or Escape closes it (`onOpenChange`), and
 * focus goes back to what held it. `children` is its content, or a `PopoverDialog`; with neither, `label` names it.
 * Inside a `Popover` it is that popover's floating part, the trigger then wired to it. */
export function FloatingPopover({ open, onOpenChange, anchor, nonModal, placement, offset, label, className, children, ...data }: DataAttributes & {
  open: boolean;
  onOpenChange?: (open: boolean) => void;
  anchor: React.RefObject<Element | null>;
  nonModal?: boolean;
  placement?: OverlayPlacement;
  offset?: number;
  label?: string;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <AriaPopover
      {...data}
      isOpen={open}
      triggerRef={anchor}
      {...definedProps({ onOpenChange, isNonModal: nonModal, placement, offset, "aria-label": label, className })}
    >
      {children}
    </AriaPopover>
  );
}

/** React Aria's bare dialog, the content of a `FloatingPopover` that holds focus: named `label`, it contains focus while
 * its popover is modal. */
export function PopoverDialog({ id, label, className, children }: {
  id?: string;
  label: string;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return <AriaDialog {...definedProps({ id, className })} aria-label={label}>{children}</AriaDialog>;
}
