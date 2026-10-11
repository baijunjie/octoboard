import React, { useContext } from "react";
import { createPortal } from "react-dom";

import { IconToggle } from "../components/IconToggle";
import { ControlsSlot, type Controls } from "./controlsSlot";

/** A two-state choice drawn as an `IconToggle` into one of the header's slots (`ControlsSlot`): the
 * one shape the view controls that portal into the header share. It draws nothing where there is
 * no header. `label` says what pressing does and `children` is the icon of the current state. */
export function SlotToggle({
  slot,
  label,
  isSelected,
  onChange,
  children,
}: {
  slot: Exclude<keyof Controls, "claimWrap">;
  label: string;
  isSelected: boolean;
  onChange: (selected: boolean) => void;
  children: React.ReactNode;
}): React.ReactElement | null {
  const element = useContext(ControlsSlot)?.[slot];
  if (!element) return null;
  return createPortal(
    <IconToggle label={label} isSelected={isSelected} onChange={onChange}>
      {children}
    </IconToggle>,
    element,
  );
}
