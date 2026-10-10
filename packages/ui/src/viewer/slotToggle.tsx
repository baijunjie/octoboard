// A choice between a few options, drawn into one of the header's slots: the shape the view controls
// that pick among values share.
import { ToggleButton, ToggleButtonGroup } from "@heroui/react";
import React, { useContext } from "react";
import { createPortal } from "react-dom";

import { ControlsSlot, type Controls } from "./controlsSlot";

/** A choice between a few options, drawn as a group of toggle buttons into one of the header's
 * slots (`ControlsSlot`): the one shape the view controls that pick among values share. It draws
 * nothing where there is no header. */
export function SlotToggle<Value extends string>({
  slot,
  label,
  value,
  options,
  onChange,
}: {
  slot: Exclude<keyof Controls, "claimWrap">;
  label: string;
  value: Value;
  options: readonly { id: Value; label: string }[];
  onChange: (value: Value) => void;
}): React.ReactElement | null {
  const element = useContext(ControlsSlot)?.[slot];
  if (!element) return null;
  return createPortal(
    <ToggleButtonGroup
      aria-label={label}
      size="sm"
      selectionMode="single"
      disallowEmptySelection
      selectedKeys={[value]}
      onSelectionChange={(keys) => {
        const [picked] = [...keys];
        const option = options.find((candidate) => candidate.id === picked);
        if (option) onChange(option.id);
      }}
    >
      {options.map((option) => (
        <ToggleButton key={option.id} id={option.id}>
          {option.label}
        </ToggleButton>
      ))}
    </ToggleButtonGroup>,
    element,
  );
}
