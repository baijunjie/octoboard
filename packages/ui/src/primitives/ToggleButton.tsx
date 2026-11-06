import { ToggleButton as HeroToggleButton, ToggleButtonGroup as HeroToggleButtonGroup } from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";
import { baseButtonProps, type ButtonBaseProps } from "./Button";

/** HeroUI's toggle button: a button that stays pressed while it is on. On its own it is on while `selected`, told of
 * presses by `onSelectedChange`; in a `ToggleButtonGroup` it is the group's option keyed `id` instead, and the group
 * holds the selection. `ghost` is HeroUI's borderless toggle; `default` (the default) is its filled one. `size` takes
 * HeroUI's sizes; the project's `xs` is not offered for a toggle. */
export function ToggleButton({ variant, size, selected, onSelectedChange, className, ...rest }: Omit<ButtonBaseProps, "size"> & {
  variant?: "default" | "ghost";
  size?: "sm" | "md" | "lg";
  selected?: boolean;
  onSelectedChange?: (selected: boolean) => void;
}): React.ReactElement {
  const disabled = useDisabled(rest.disabled);
  return (
    <HeroToggleButton
      {...baseButtonProps({ ...rest, disabled })}
      variant={variant}
      size={size}
      isSelected={selected}
      onChange={onSelectedChange}
      className={className}
    />
  );
}

/** HeroUI's toggle button group as a segmented control: exactly one of its `ToggleButton`s is on at a time, keyed by
 * their `id`s, and pressing the one that is on leaves it on. With `selectedKey` the selection is the caller's, told of
 * changes by `onSelectionChange`; with `defaultSelectedKey` it is the group's own. `label` names the group for
 * assistive technology. */
export function ToggleButtonGroup({ label, selectedKey, defaultSelectedKey, onSelectionChange, className, children }: {
  label: string;
  selectedKey?: string;
  defaultSelectedKey?: string;
  onSelectionChange?: (key: string) => void;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroToggleButtonGroup
      aria-label={label}
      isDisabled={useDisabled()}
      selectionMode="single"
      disallowEmptySelection
      selectedKeys={selectedKey === undefined ? undefined : [selectedKey]}
      defaultSelectedKeys={defaultSelectedKey === undefined ? undefined : [defaultSelectedKey]}
      onSelectionChange={onSelectionChange && ((keys) => {
        const [key] = keys;
        if (key !== undefined) onSelectionChange(String(key));
      })}
      className={className}
    >
      {children}
    </HeroToggleButtonGroup>
  );
}
